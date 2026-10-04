//! Handing the keychain entries a running build holds to the build an update installs (macOS;
//! docs/security.md, "Keychain items across updates"; [`crate::per_build`]).
//!
//! Before the updater replaces the running bundle, the old build starts the staged new build with
//! [`HANDOFF_ARG`] and one end of a socket pair as its stdin, sends what its keychain store knows
//! ([`Entries`]) and waits for one byte back. The new build reads the hand-over first thing in the
//! process, stores it in items of its own, reads them back, and only then acknowledges. Each side
//! first checks that the other process is Lockra signed with the release certificate
//! ([`PeerCheck`]), so the secrets only ever go from one release to the next. Nothing is written to
//! disk, and nothing goes through the command line or the environment.
//!
//! The protocol is plain Unix sockets, so its tests run on Linux too; only the checks of the code
//! signature are macOS ([`crate::keychain_handoff`]).

use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use crate::per_build::Entries;

/// The argument that starts a staged build in hand-over mode: it takes the hand-over from its
/// stdin, stores it, acknowledges and exits without opening a window.
pub const HANDOFF_ARG: &str = "--lockra-keychain-handoff";

const MAGIC: &[u8; 8] = b"LOCKRAH1";
const ACK: u8 = 0x06;
/// Limits far above what a hand-over holds (an entry per vault, each a few dozen bytes).
const MAX_ENTRIES: usize = 256;
const MAX_NAME: usize = 256;
const MAX_VALUE: usize = 64 * 1024;
const MAX_PAYLOAD: usize = 1024 * 1024;

/// Why a hand-over was not taken or not delivered.
#[derive(Debug, thiserror::Error)]
pub enum HandoffError {
    /// The process at the other end is not Lockra signed with the release certificate.
    #[error("the other process is not Lockra signed with the release certificate")]
    NotTrusted,
    /// The bytes are not a hand-over this build reads.
    #[error("malformed hand-over: {0}")]
    Malformed(&'static str),
    /// The socket failed or timed out.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Checks the process at the other end of a hand-over.
pub trait PeerCheck {
    /// `true` when process `pid` is Lockra signed with the release certificate.
    fn trusted(&self, pid: u32) -> bool;
}

/// The wire form of `entries` (without the length prefix [`send`] adds).
pub fn encode(entries: &Entries) -> Result<Zeroizing<Vec<u8>>, HandoffError> {
    if entries.len() > MAX_ENTRIES {
        return Err(HandoffError::Malformed("too many entries"));
    }
    let mut out = Zeroizing::new(Vec::with_capacity(64));
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&u32::try_from(entries.len()).map_err(|_| HandoffError::Malformed("too many entries"))?.to_be_bytes());
    for (name, value) in entries {
        if name.is_empty() || name.len() > MAX_NAME {
            return Err(HandoffError::Malformed("entry name"));
        }
        out.extend_from_slice(&u16::try_from(name.len()).map_err(|_| HandoffError::Malformed("entry name"))?.to_be_bytes());
        out.extend_from_slice(name.as_bytes());
        match value {
            Some(value) => {
                if value.len() > MAX_VALUE {
                    return Err(HandoffError::Malformed("entry value"));
                }
                out.push(1);
                out.extend_from_slice(&u32::try_from(value.len()).map_err(|_| HandoffError::Malformed("entry value"))?.to_be_bytes());
                out.extend_from_slice(value.as_bytes());
            }
            None => out.push(0),
        }
    }
    Ok(out)
}

/// The wire form read back; anything short, long or out of bounds is refused whole.
pub fn decode(bytes: &[u8]) -> Result<Entries, HandoffError> {
    let mut rest = bytes;
    if take(&mut rest, MAGIC.len())? != MAGIC {
        return Err(HandoffError::Malformed("not a hand-over"));
    }
    let count = u32::from_be_bytes(array(take(&mut rest, 4)?)) as usize;
    if count > MAX_ENTRIES {
        return Err(HandoffError::Malformed("too many entries"));
    }
    let mut entries = Entries::with_capacity(count);
    for _ in 0..count {
        let len = usize::from(u16::from_be_bytes(array(take(&mut rest, 2)?)));
        if len == 0 || len > MAX_NAME {
            return Err(HandoffError::Malformed("entry name"));
        }
        let name = std::str::from_utf8(take(&mut rest, len)?).map_err(|_| HandoffError::Malformed("entry name"))?.to_owned();
        let value = match take(&mut rest, 1)?[0] {
            0 => None,
            1 => {
                let len = u32::from_be_bytes(array(take(&mut rest, 4)?)) as usize;
                if len > MAX_VALUE {
                    return Err(HandoffError::Malformed("entry value"));
                }
                let text = std::str::from_utf8(take(&mut rest, len)?).map_err(|_| HandoffError::Malformed("entry value"))?;
                Some(Zeroizing::new(text.to_owned()))
            }
            _ => return Err(HandoffError::Malformed("entry state")),
        };
        entries.push((name, value));
    }
    if !rest.is_empty() {
        return Err(HandoffError::Malformed("trailing bytes"));
    }
    Ok(entries)
}

fn take<'a>(rest: &mut &'a [u8], n: usize) -> Result<&'a [u8], HandoffError> {
    if rest.len() < n {
        return Err(HandoffError::Malformed("truncated"));
    }
    let (head, tail) = rest.split_at(n);
    *rest = tail;
    Ok(head)
}

fn array<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    out.copy_from_slice(bytes);
    out
}

/// The old build's side: check the new build's process `peer`, send `entries` and wait up to
/// `timeout` for its acknowledgement.
pub fn send(stream: &mut UnixStream, entries: &Entries, check: &dyn PeerCheck, peer: u32, timeout: Duration) -> Result<(), HandoffError> {
    if !check.trusted(peer) {
        return Err(HandoffError::NotTrusted);
    }
    let payload = encode(entries)?;
    stream.set_write_timeout(Some(timeout))?;
    stream.set_read_timeout(Some(timeout))?;
    let len = u32::try_from(payload.len()).map_err(|_| HandoffError::Malformed("too long"))?;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(&payload)?;
    let mut ack = [0u8; 1];
    stream.read_exact(&mut ack)?;
    if ack[0] != ACK {
        return Err(HandoffError::Malformed("acknowledgement"));
    }
    Ok(())
}

/// A received hand-over not yet acknowledged. The staged build keeps it while it stores and reads
/// back every entry; dropping it sends no acknowledgement, so the old build learns the hand-over
/// did not happen.
pub struct PendingReceive {
    stream: UnixStream,
    entries: Entries,
}

impl PendingReceive {
    /// The entries the authenticated peer sent.
    pub fn entries(&self) -> &Entries {
        &self.entries
    }

    /// Tell the sender the entries are stored.
    pub fn acknowledge(mut self) -> Result<(), HandoffError> {
        self.stream.write_all(&[ACK])?;
        Ok(())
    }
}

/// The new build's side: check the old build's process `peer` (the parent) and read the hand-over,
/// without acknowledging it. Nothing is read from a peer that fails the check.
pub fn receive_pending(stream: &mut UnixStream, check: &dyn PeerCheck, peer: u32, timeout: Duration) -> Result<PendingReceive, HandoffError> {
    if !check.trusted(peer) {
        return Err(HandoffError::NotTrusted);
    }
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let mut len = [0u8; 4];
    stream.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_PAYLOAD {
        return Err(HandoffError::Malformed("too long"));
    }
    let mut payload = Zeroizing::new(vec![0u8; len]);
    stream.read_exact(&mut payload)?;
    let entries = decode(&payload)?;
    Ok(PendingReceive { stream: stream.try_clone()?, entries })
}

/// [`receive_pending`] on the socket this process was given as its stdin, from its parent.
pub fn take_pending_from_stdin(check: &dyn PeerCheck, timeout: Duration) -> Result<PendingReceive, HandoffError> {
    let mut stream = UnixStream::from(std::io::stdin().as_fd().try_clone_to_owned()?);
    receive_pending(&mut stream, check, std::os::unix::process::parent_id(), timeout)
}

/// Why [`start_preinstall`] did not hand over.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// The new build was not started at all.
    #[error("the staged build could not be started: {0}")]
    NotStarted(std::io::Error),
    /// The new build was started, but the hand-over failed; it is stopped and reaped.
    #[error("the staged build was stopped after the hand-over failed: {0}")]
    HandOverFailed(HandoffError),
}

/// The old build's side: start `exe` with `args` and [`HANDOFF_ARG`], a socket as its stdin, and
/// hand it `entries` once `check` trusts it. The caller waits for the child to exit: a zero status
/// means the entries are stored.
pub fn start_preinstall<I, S>(exe: &Path, args: I, entries: &Entries, check: &dyn PeerCheck, timeout: Duration) -> Result<Child, StartError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let (mut ours, theirs) = UnixStream::pair().map_err(StartError::NotStarted)?;
    let mut child = Command::new(exe).args(args).arg(HANDOFF_ARG).stdin(Stdio::from(OwnedFd::from(theirs))).spawn().map_err(StartError::NotStarted)?;
    // The new process can be checked only once it runs the staged executable.
    let deadline = Instant::now() + timeout;
    while !check.trusted(child.id()) {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(StartError::HandOverFailed(HandoffError::Io(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    format!("the staged build exited with {status} before the hand-over"),
                ))));
            }
            Ok(None) => {}
            Err(error) => {
                stop_child(&mut child);
                return Err(StartError::HandOverFailed(HandoffError::Io(error)));
            }
        }
        if Instant::now() >= deadline {
            stop_child(&mut child);
            return Err(StartError::HandOverFailed(HandoffError::NotTrusted));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    match send(&mut ours, entries, check, child.id(), timeout) {
        Ok(()) => Ok(child),
        Err(error) => {
            stop_child(&mut child);
            Err(StartError::HandOverFailed(error))
        }
    }
}

fn stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Expand an update package (the `.app.tar.gz` the updater verified) into `stage`, and find the
/// executable named `name` of the one `.app` it must hold.
pub fn stage_update(package: &[u8], stage: &Path, name: &OsStr) -> Result<PathBuf, String> {
    let decoder = flate2::read::GzDecoder::new(package);
    tar::Archive::new(decoder).unpack(stage).map_err(|e| format!("the verified update could not be expanded: {e}"))?;
    let top = fs::read_dir(stage).map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    let [app] = top.as_slice() else { return Err(format!("the verified update holds {} entries, not one app", top.len())) };
    let is_app = app.path().extension().is_some_and(|ext| ext == "app") && app.file_type().is_ok_and(|kind| kind.is_dir());
    if !is_app {
        return Err("the verified update does not hold an app".into());
    }
    let executable = app.path().join("Contents/MacOS").join(name);
    let regular = fs::symlink_metadata(&executable).map_err(|e| format!("the staged executable is missing: {e}"))?.file_type().is_file();
    if !regular {
        return Err("the staged executable is not a regular file".into());
    }
    Ok(executable)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORT: Duration = Duration::from_millis(300);
    const LONG: Duration = Duration::from_secs(10);

    struct Pids(Vec<u32>);

    impl PeerCheck for Pids {
        fn trusted(&self, pid: u32) -> bool {
            self.0.contains(&pid)
        }
    }

    struct AnyPid;

    impl PeerCheck for AnyPid {
        fn trusted(&self, _pid: u32) -> bool {
            true
        }
    }

    fn entries() -> Entries {
        vec![
            ("2b1c0000-0000-4000-8000-00000000000a".to_owned(), Some(Zeroizing::new("device key one".to_owned()))),
            ("2b1c0000-0000-4000-8000-00000000000b".to_owned(), Some(Zeroizing::new("密钥".to_owned()))),
            ("2b1c0000-0000-4000-8000-00000000000c".to_owned(), None),
        ]
    }

    fn same(a: &Entries, b: &Entries) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.0 == y.0 && x.1.as_deref() == y.1.as_deref())
    }

    /// A shell that answers as a staged build does, once it was started with the hand-over argument
    /// (`sh -c script arg` makes the argument `$0`).
    const STAGED: &str = "test \"$0\" = --lockra-keychain-handoff || exit 3; head -c 4 >/dev/null; printf '\\006' >&0; cat >/dev/null";

    #[test]
    fn entries_and_absent_entries_travel_intact() {
        let (mut old, mut new) = UnixStream::pair().unwrap();
        let sent = entries();
        let sender = std::thread::spawn(move || send(&mut old, &sent, &Pids(vec![2]), 2, LONG));
        let pending = receive_pending(&mut new, &Pids(vec![1]), 1, LONG).unwrap();
        assert!(same(pending.entries(), &entries()));
        pending.acknowledge().unwrap();
        sender.join().unwrap().unwrap();
    }

    /// The old build must not install until the new build has stored every entry: receiving and
    /// acknowledging are two steps.
    #[test]
    fn a_pending_hand_over_acknowledges_only_when_the_receiver_commits() {
        let (mut old, mut new) = UnixStream::pair().unwrap();
        let sent = entries();
        let sender = std::thread::spawn(move || {
            old.set_write_timeout(Some(LONG)).unwrap();
            old.set_read_timeout(Some(SHORT)).unwrap();
            let payload = encode(&sent).unwrap();
            old.write_all(&u32::try_from(payload.len()).unwrap().to_be_bytes()).unwrap();
            old.write_all(&payload).unwrap();
            let mut ack = [0u8; 1];
            assert!(old.read_exact(&mut ack).is_err(), "nothing acknowledges before the entries are stored");
            old.set_read_timeout(Some(LONG)).unwrap();
            old.read_exact(&mut ack).unwrap();
            assert_eq!(ack, [ACK]);
        });
        let pending = receive_pending(&mut new, &Pids(vec![1]), 1, LONG).unwrap();
        std::thread::sleep(SHORT + Duration::from_millis(50));
        pending.acknowledge().unwrap();
        sender.join().unwrap();
    }

    /// The new build takes nothing from a parent that fails the check, so never acknowledges: the
    /// sender gives up after its timeout.
    #[test]
    fn a_parent_that_fails_the_check_hands_over_nothing() {
        let (mut old, mut new) = UnixStream::pair().unwrap();
        let sent = entries();
        let sender = std::thread::spawn(move || send(&mut old, &sent, &Pids(vec![2]), 2, SHORT));
        assert!(matches!(receive_pending(&mut new, &Pids(vec![]), 1, LONG), Err(HandoffError::NotTrusted)));
        assert!(matches!(sender.join().unwrap(), Err(HandoffError::Io(_))), "no acknowledgement");
    }

    /// The old build sends nothing to a process that fails the check.
    #[test]
    fn a_child_that_fails_the_check_is_sent_nothing() {
        let (mut old, mut new) = UnixStream::pair().unwrap();
        assert!(matches!(send(&mut old, &entries(), &Pids(vec![]), 2, SHORT), Err(HandoffError::NotTrusted)));
        drop(old);
        let mut rest = Vec::new();
        new.read_to_end(&mut rest).unwrap();
        assert!(rest.is_empty(), "nothing was written");
    }

    #[test]
    fn malformed_hand_overs_are_refused_whole() {
        let good = encode(&entries()).unwrap();
        assert!(same(&decode(&good).unwrap(), &entries()));
        assert!(matches!(decode(&good[..good.len() - 1]), Err(HandoffError::Malformed("truncated"))));
        let mut long = good.to_vec();
        long.push(0);
        assert!(matches!(decode(&long), Err(HandoffError::Malformed("trailing bytes"))));
        let mut magic = good.to_vec();
        magic[0] = b'X';
        assert!(matches!(decode(&magic), Err(HandoffError::Malformed("not a hand-over"))));
        let mut state = encode(&vec![("k".to_owned(), None)]).unwrap().to_vec();
        *state.last_mut().unwrap() = 9;
        assert!(matches!(decode(&state), Err(HandoffError::Malformed("entry state"))));
        let mut not_text = encode(&vec![("k".to_owned(), Some(Zeroizing::new("ab".to_owned())))]).unwrap().to_vec();
        *not_text.last_mut().unwrap() = 0xff;
        assert!(matches!(decode(&not_text), Err(HandoffError::Malformed("entry value"))));
        assert!(matches!(encode(&vec![(String::new(), None)]), Err(HandoffError::Malformed("entry name"))));
        assert!(matches!(encode(&vec![("k".to_owned(), Some(Zeroizing::new("x".repeat(MAX_VALUE + 1))))]), Err(HandoffError::Malformed("entry value"))));
        let many: Entries = (0..=MAX_ENTRIES).map(|i| (format!("k{i}"), None)).collect();
        assert!(matches!(encode(&many), Err(HandoffError::Malformed("too many entries"))));
        let mut count = good.to_vec();
        count[8..12].copy_from_slice(&u32::try_from(MAX_ENTRIES + 1).unwrap().to_be_bytes());
        assert!(matches!(decode(&count), Err(HandoffError::Malformed("too many entries"))));
    }

    #[test]
    fn a_hand_over_longer_than_the_limit_is_not_read() {
        let (mut old, mut new) = UnixStream::pair().unwrap();
        old.write_all(&u32::try_from(MAX_PAYLOAD + 1).unwrap().to_be_bytes()).unwrap();
        assert!(matches!(receive_pending(&mut new, &Pids(vec![1]), 1, SHORT), Err(HandoffError::Malformed("too long"))));
    }

    #[test]
    fn start_hands_the_entries_to_the_process_it_starts() {
        let refused = start_preinstall(Path::new("/bin/sh"), ["-c", STAGED], &entries(), &Pids(vec![]), SHORT);
        assert!(matches!(refused, Err(StartError::HandOverFailed(HandoffError::NotTrusted))), "an untrusted process is sent nothing");
        let mut child = start_preinstall(Path::new("/bin/sh"), ["-c", STAGED], &entries(), &AnyPid, LONG).unwrap();
        assert!(child.wait().unwrap().success());
        assert!(matches!(start_preinstall(Path::new("/nonexistent/lockra"), [""; 0], &entries(), &AnyPid, SHORT), Err(StartError::NotStarted(_))));
    }

    #[test]
    fn start_waits_for_the_new_process_to_become_checkable() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct ReadyAfter(AtomicUsize);
        impl PeerCheck for ReadyAfter {
            fn trusted(&self, _pid: u32) -> bool {
                self.0.fetch_add(1, Ordering::SeqCst) >= 2
            }
        }

        let check = ReadyAfter(AtomicUsize::new(0));
        let mut child = start_preinstall(Path::new("/bin/sh"), ["-c", STAGED], &entries(), &check, LONG).unwrap();
        assert!(check.0.load(Ordering::SeqCst) >= 4, "readiness retries, then send checks once more");
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn a_staged_process_that_exits_first_is_an_error() {
        struct Never;
        impl PeerCheck for Never {
            fn trusted(&self, _pid: u32) -> bool {
                false
            }
        }
        assert!(matches!(
            start_preinstall(Path::new("/bin/sh"), ["-c", "exit 0"], &entries(), &Never, LONG),
            Err(StartError::HandOverFailed(HandoffError::Io(_)))
        ));
    }

    #[test]
    fn an_untrusted_staged_process_is_stopped_and_reaped() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let script = format!("echo $$ > '{}'; sleep 30", pid_file.display());
        assert!(matches!(
            start_preinstall(Path::new("/bin/sh"), ["-c", script.as_str()], &entries(), &Pids(vec![]), SHORT),
            Err(StartError::HandOverFailed(HandoffError::NotTrusted))
        ));
        let pid = std::fs::read_to_string(pid_file).unwrap();
        let alive =
            Command::new("/bin/kill").args(["-0", pid.trim()]).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|status| status.success());
        assert!(!alive, "the refused child must not survive the failed hand-over");
    }

    /// A `.tar.gz` holding `files` (path, contents; a path ending in `/` is a directory).
    fn package(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
        for (path, contents) in files {
            let mut header = tar::Header::new_gnu();
            if path.ends_with('/') {
                header.set_entry_type(tar::EntryType::Directory);
                header.set_size(0);
                header.set_mode(0o755);
                header.set_cksum();
                builder.append_data(&mut header, path, &[][..]).unwrap();
            } else {
                header.set_size(contents.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                builder.append_data(&mut header, path, *contents).unwrap();
            }
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn an_update_package_stages_its_one_app() {
        let app = package(&[
            ("Lockra.app/", b""),
            ("Lockra.app/Contents/MacOS/lockra-desktop", b"\xcf\xfa\xed\xfe"),
            ("Lockra.app/Contents/Info.plist", b"<plist/>"),
        ]);
        let stage = tempfile::tempdir().unwrap();
        let exe = stage_update(&app, stage.path(), OsStr::new("lockra-desktop")).unwrap();
        assert_eq!(exe, stage.path().join("Lockra.app/Contents/MacOS/lockra-desktop"));

        let wrong = |files: &[(&str, &[u8])]| stage_update(&package(files), tempfile::tempdir().unwrap().path(), OsStr::new("lockra-desktop")).unwrap_err();
        assert!(wrong(&[("Lockra.app/Contents/MacOS/lockra-desktop", b"x"), ("Other.app/Contents/MacOS/x", b"x")]).contains("2 entries"));
        assert!(wrong(&[("Lockra/Contents/MacOS/lockra-desktop", b"x")]).contains("does not hold an app"));
        assert!(wrong(&[("Lockra.app/Contents/MacOS/other", b"x")]).contains("missing"));
        assert!(wrong(&[("Lockra.app/Contents/MacOS/lockra-desktop/", b"")]).contains("not a regular file"));
        assert!(stage_update(b"not a tar.gz", tempfile::tempdir().unwrap().path(), OsStr::new("x")).unwrap_err().contains("could not be expanded"));
    }

    #[test]
    fn a_wrong_acknowledgement_is_an_error() {
        let (mut old, mut new) = UnixStream::pair().unwrap();
        let reader = std::thread::spawn(move || {
            let mut len = [0u8; 4];
            new.read_exact(&mut len).unwrap();
            let mut payload = vec![0u8; u32::from_be_bytes(len) as usize];
            new.read_exact(&mut payload).unwrap();
            new.write_all(&[0]).unwrap();
        });
        assert!(matches!(send(&mut old, &entries(), &Pids(vec![2]), 2, LONG), Err(HandoffError::Malformed("acknowledgement"))));
        reader.join().unwrap();
    }
}
