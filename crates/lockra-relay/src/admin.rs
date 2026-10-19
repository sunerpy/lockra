//! The commands for a running relay (`lockra-relay new-code`, `codes`, `drop-code`, `spaces`,
//! `admit`, `revoke`): the request and its answer are one line of JSON each, over a Unix socket in
//! the relay's directory, `<data>/lockra-relay-v1/admin.sock`. Only the relay's user (and root)
//! may open it, and nothing of it is reachable over the network. The relay process is the only
//! writer of the admission file, whatever the commands ask.

use std::fmt::Write as _;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::admission;
use crate::api::App;

/// The admin socket's name in the relay's directory.
pub(crate) const SOCKET: &str = "admin.sock";
/// A request is a line of a few hundred bytes.
#[cfg(unix)]
const MAX_REQUEST_BYTES: u64 = 64 * 1024;
/// How long a command waits for the relay.
#[cfg(unix)]
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// What a command asks the relay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdminRequest {
    /// A registration code, valid for `expires_secs`, with the administrator's note.
    NewCode { note: String, expires_secs: u64 },
    /// The codes not used yet.
    Codes,
    /// Cancel a code.
    DropCode { id: String },
    /// The spaces: admitted, served, and on disk without being served.
    Spaces,
    /// Serve a space on disk that no code admitted.
    Admit { space: Uuid, note: String },
    /// Stop serving an admitted space.
    Revoke { space: Uuid },
}

/// The relay's answer to a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdminAnswer {
    /// A new code: the only time its letters are shown.
    Code { id: String, code: String, expires_at: u64 },
    /// The codes not used yet.
    Codes { codes: Vec<CodeInfo> },
    /// The spaces.
    Spaces { registration: String, spaces: Vec<SpaceInfo>, unserved: Vec<UnservedInfo> },
    /// Done.
    Done,
    /// Not done, and why.
    Refused { message: String },
}

/// A code not used yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeInfo {
    pub id: String,
    pub note: String,
    pub created_at: u64,
    pub expires_at: u64,
}

/// A space the relay serves or admits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpaceInfo {
    pub id: Uuid,
    /// Listed in `--allowed-spaces` (else admitted by a code or `admit`, or open registration's).
    pub listed: bool,
    pub note: String,
    /// When a code or `admit` admitted it.
    pub admitted_at: Option<u64>,
    /// Its files are read in: devices, bytes and the last use are known.
    pub served: bool,
    pub devices: usize,
    pub bytes: u64,
    pub last_used: Option<u64>,
}

/// A space on disk the relay does not serve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnservedInfo {
    pub id: Uuid,
    pub last_used: Option<u64>,
}

/// The relay's answer to `request`.
pub(crate) async fn answer(app: &App, request: AdminRequest) -> AdminAnswer {
    let store = &app.store;
    let gate = store.gate();
    let now = admission::now();
    let done = |result: Result<(), String>| result.map_or_else(|message| AdminAnswer::Refused { message }, |()| AdminAnswer::Done);
    match request {
        AdminRequest::NewCode { note, expires_secs } => {
            let gate = std::sync::Arc::clone(gate);
            match tokio::task::spawn_blocking(move || gate.new_code(note, expires_secs, now)).await.unwrap_or_else(|e| Err(e.to_string())) {
                Ok(made) => {
                    tracing::info!("a registration code was made");
                    AdminAnswer::Code { id: made.id, code: made.code, expires_at: made.expires_at }
                }
                Err(message) => AdminAnswer::Refused { message },
            }
        }
        AdminRequest::Codes => AdminAnswer::Codes {
            codes: gate
                .codes(now)
                .into_iter()
                .map(|(id, code)| CodeInfo { id, note: code.note, created_at: code.created_at, expires_at: code.expires_at })
                .collect(),
        },
        AdminRequest::DropCode { id } => {
            let gate = std::sync::Arc::clone(gate);
            done(tokio::task::spawn_blocking(move || gate.drop_code(&id, now)).await.unwrap_or_else(|e| Err(e.to_string())))
        }
        AdminRequest::Spaces => {
            let mut spaces: Vec<SpaceInfo> = gate
                .admitted()
                .into_iter()
                .map(|(id, admitted)| {
                    let served = store.served(id);
                    SpaceInfo {
                        id,
                        listed: admitted.is_none(),
                        note: admitted.as_ref().map(|a| a.note.clone()).unwrap_or_default(),
                        admitted_at: admitted.map(|a| a.admitted_at),
                        served: served.is_some(),
                        devices: served.map_or(0, |s| s.devices),
                        bytes: served.map_or(0, |s| s.bytes),
                        last_used: served.and_then(|s| seconds(s.touched)),
                    }
                })
                .collect();
            if gate.is_open() {
                // Open registration admits every space it serves.
                for id in store.served_ids() {
                    if let Some(served) = store.served(id) {
                        spaces.push(SpaceInfo {
                            id,
                            listed: false,
                            note: String::new(),
                            admitted_at: None,
                            served: true,
                            devices: served.devices,
                            bytes: served.bytes,
                            last_used: seconds(served.touched),
                        });
                    }
                }
                spaces.sort_by_key(|space| space.id);
            }
            match store.unserved().await {
                Ok(unserved) => AdminAnswer::Spaces {
                    registration: gate.name().to_owned(),
                    spaces,
                    unserved: unserved.into_iter().map(|(id, touched)| UnservedInfo { id, last_used: seconds(touched) }).collect(),
                },
                Err(e) => AdminAnswer::Refused { message: format!("the spaces on disk could not be read: {e}") },
            }
        }
        AdminRequest::Admit { space, note } => {
            let result = store.admit(space, note).await;
            if result.is_ok() {
                tracing::info!("a space was admitted by command");
            }
            done(result)
        }
        AdminRequest::Revoke { space } => {
            let result = store.revoke(space).await;
            if result.is_ok() {
                tracing::info!("a space was revoked by command");
            }
            done(result)
        }
    }
}

fn seconds(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH).ok().map(|since| since.as_secs()).filter(|s| *s > 0)
}

/// Ask the relay serving `data_dir` (as `lockra-relay COMMAND` does).
pub fn request(data_dir: &Path, request: &AdminRequest) -> Result<AdminAnswer, String> {
    #[cfg(unix)]
    {
        use std::io::{BufRead as _, BufReader, Read as _, Write as _};
        use std::os::unix::net::UnixStream;
        let path = data_dir.join(crate::store::ROOT).join(SOCKET);
        let mut stream = UnixStream::connect(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
                format!("no relay serves {} (no {}); start it, or give its --data", data_dir.display(), path.display())
            }
            std::io::ErrorKind::PermissionDenied => format!("{} is the relay's: run this as its user or as root", path.display()),
            _ => format!("{}: {e}", path.display()),
        })?;
        stream.set_read_timeout(Some(TIMEOUT)).and_then(|()| stream.set_write_timeout(Some(TIMEOUT))).map_err(|e| e.to_string())?;
        let mut line = serde_json::to_vec(request).map_err(|e| e.to_string())?;
        line.push(b'\n');
        stream.write_all(&line).map_err(|e| format!("the relay did not take the command: {e}"))?;
        let mut answer = String::new();
        BufReader::new(stream.take(MAX_REQUEST_BYTES)).read_line(&mut answer).map_err(|e| format!("the relay did not answer: {e}"))?;
        serde_json::from_str(&answer).map_err(|_| "the relay's answer is not one this command reads (another version?)".to_owned())
    }
    #[cfg(not(unix))]
    {
        let _ = (data_dir, request);
        Err("the relay's commands need a Unix socket: Linux or macOS".into())
    }
}

/// `answer` as the command prints it, or why it failed.
pub fn render(answer: &AdminAnswer) -> Result<String, String> {
    let mut out = String::new();
    match answer {
        AdminAnswer::Code { id, code, expires_at } => {
            let _ = writeln!(out, "{code}");
            let _ =
                writeln!(out, "Registration code {id}: it admits one new sync space until {}. Enter it under the relay's address in Lockra.", utc(*expires_at));
        }
        AdminAnswer::Codes { codes } if codes.is_empty() => out.push_str("No code waits to be used.\n"),
        AdminAnswer::Codes { codes } => {
            let _ = writeln!(out, "{:<8}  {:<20}  NOTE", "ID", "EXPIRES");
            for code in codes {
                let _ = writeln!(out, "{:<8}  {:<20}  {}", code.id, utc(code.expires_at), code.note);
            }
        }
        AdminAnswer::Spaces { registration, spaces, unserved } => {
            let _ = writeln!(out, "Registration: {registration}");
            if spaces.is_empty() {
                out.push_str("No space is served.\n");
            } else {
                let _ = writeln!(out, "{:<36}  {:<10}  {:>7}  {:>9}  {:<20}  NOTE", "SPACE", "ADMITTED", "DEVICES", "BYTES", "LAST USED");
                for space in spaces {
                    let how =
                        if space.listed { "listed".to_owned() } else { space.admitted_at.map_or_else(|| "open".to_owned(), |at| utc(at)[..10].to_owned()) };
                    let (devices, bytes) = if space.served { (space.devices.to_string(), space.bytes.to_string()) } else { ("-".into(), "-".into()) };
                    let _ = writeln!(
                        out,
                        "{:<36}  {:<10}  {:>7}  {:>9}  {:<20}  {}",
                        space.id,
                        how,
                        devices,
                        bytes,
                        space.last_used.map_or_else(|| "-".to_owned(), utc),
                        space.note
                    );
                }
            }
            if !unserved.is_empty() {
                let _ = writeln!(out, "\nOn disk, not served (lockra-relay admit SPACE_ID serves one):");
                for space in unserved {
                    let _ = writeln!(out, "{:<36}  last used {}", space.id, space.last_used.map_or_else(|| "-".to_owned(), utc));
                }
            }
        }
        AdminAnswer::Done => out.push_str("Done.\n"),
        AdminAnswer::Refused { message } => return Err(message.clone()),
    }
    Ok(out)
}

/// Unix seconds as `2026-10-27 08:00 UTC`.
fn utc(secs: u64) -> String {
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX / 2);
    let (hour, minute) = ((secs % 86_400) / 3600, (secs % 3600) / 60);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// The admin socket: its listener, bound at start, and its loop.
#[cfg(unix)]
pub(crate) mod socket {
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
    use tokio::net::{UnixListener, UnixStream};
    use tokio::sync::watch;
    use tokio::task::JoinSet;

    use super::{AdminAnswer, AdminRequest, MAX_REQUEST_BYTES, TIMEOUT, answer};
    use crate::api::App;

    /// The socket at `path`, for this relay alone: another relay serving the same directory is
    /// refused; a socket left by one that stopped is replaced.
    pub(crate) fn bind(path: &Path) -> io::Result<UnixListener> {
        if path.exists() {
            if std::os::unix::net::UnixStream::connect(path).is_ok() {
                return Err(io::Error::new(io::ErrorKind::AddrInUse, format!("another relay serves this data directory ({} answers)", path.display())));
            }
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path).map_err(|e| io::Error::new(e.kind(), format!("the admin socket {}: {e}", path.display())))?;
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(listener)
    }

    /// Answer the commands until the relay stops; the socket goes with it.
    pub(crate) async fn serve(listener: UnixListener, path: PathBuf, app: Arc<App>, mut stopped: watch::Receiver<bool>) {
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                _ = stopped.changed() => break,
                accepted = listener.accept() => match accepted {
                    Ok((stream, _)) => {
                        let app = Arc::clone(&app);
                        connections.spawn(async move { connection(stream, &app).await });
                    }
                    Err(error) => {
                        tracing::warn!(%error, "accepting a command failed");
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                },
            }
            while connections.try_join_next().is_some() {}
        }
        drop(listener);
        connections.abort_all();
        let _ = std::fs::remove_file(&path);
    }

    async fn connection(stream: UnixStream, app: &App) {
        let (read, mut write) = stream.into_split();
        let mut line = String::new();
        let read = tokio::time::timeout(TIMEOUT, BufReader::new(read.take(MAX_REQUEST_BYTES)).read_line(&mut line)).await;
        let reply = match read {
            Ok(Ok(_)) => match serde_json::from_str::<AdminRequest>(&line) {
                Ok(request) => answer(app, request).await,
                Err(_) => AdminAnswer::Refused { message: "not a command this relay reads (another version?)".into() },
            },
            _ => return,
        };
        let mut text = serde_json::to_vec(&reply).unwrap_or_default();
        text.push(b'\n');
        let _ = tokio::time::timeout(TIMEOUT, write.write_all(&text)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_utc_calendar_dates() {
        assert_eq!(utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(utc(1_792_454_400 + 8 * 3600 + 5 * 60), "2026-10-20 08:05 UTC");
        assert_eq!(utc(4_107_542_399), "2100-02-28 23:59 UTC");
    }

    #[test]
    fn answers_print_for_people() {
        let code = AdminAnswer::Code { id: "1a2b3c4d".into(), code: "ABCD-EFGH-IJKL-MNOP-QRST-UVWX".into(), expires_at: 1_792_454_400 };
        let text = render(&code).unwrap();
        assert!(text.starts_with("ABCD-EFGH-IJKL-MNOP-QRST-UVWX\n") && text.contains("1a2b3c4d") && text.contains("2026-10-20 00:00 UTC"), "{text}");
        assert_eq!(render(&AdminAnswer::Codes { codes: vec![] }).unwrap(), "No code waits to be used.\n");
        let codes =
            render(&AdminAnswer::Codes { codes: vec![CodeInfo { id: "1a2b3c4d".into(), note: "phone".into(), created_at: 1, expires_at: 86_400 }] }).unwrap();
        assert!(codes.contains("1a2b3c4d  1970-01-02 00:00 UTC  phone"), "{codes}");
        let id = Uuid::nil();
        let spaces = render(&AdminAnswer::Spaces {
            registration: "code".into(),
            spaces: vec![
                SpaceInfo { id, listed: true, note: String::new(), admitted_at: None, served: false, devices: 0, bytes: 0, last_used: None },
                SpaceInfo {
                    id,
                    listed: false,
                    note: "laptop".into(),
                    admitted_at: Some(86_400),
                    served: true,
                    devices: 2,
                    bytes: 4096,
                    last_used: Some(90_000),
                },
                SpaceInfo { id, listed: false, note: String::new(), admitted_at: None, served: true, devices: 1, bytes: 1, last_used: None },
            ],
            unserved: vec![UnservedInfo { id, last_used: Some(0) }],
        })
        .unwrap();
        for part in ["Registration: code", "listed", "1970-01-02", "laptop", "4096", "open", "On disk, not served"] {
            assert!(spaces.contains(part), "{part}: {spaces}");
        }
        assert!(render(&AdminAnswer::Spaces { registration: "open".into(), spaces: vec![], unserved: vec![] }).unwrap().contains("No space is served."));
        assert_eq!(render(&AdminAnswer::Done).unwrap(), "Done.\n");
        assert_eq!(render(&AdminAnswer::Refused { message: "no".into() }).unwrap_err(), "no");
    }

    #[test]
    fn requests_and_answers_are_one_json_line_each() {
        let request = AdminRequest::NewCode { note: "phone".into(), expires_secs: 60 };
        let text = serde_json::to_string(&request).unwrap();
        assert_eq!(text, r#"{"op":"new_code","note":"phone","expires_secs":60}"#);
        assert_eq!(serde_json::from_str::<AdminRequest>(&text).unwrap(), request);
        assert!(serde_json::from_str::<AdminRequest>(r#"{"op":"delete_everything"}"#).is_err());
        let answer = AdminAnswer::Refused { message: "x".into() };
        assert_eq!(serde_json::from_str::<AdminAnswer>(&serde_json::to_string(&answer).unwrap()).unwrap(), answer);
    }

    #[cfg(unix)]
    #[test]
    fn a_command_without_a_relay_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let error = request(dir.path(), &AdminRequest::Codes).unwrap_err();
        assert!(error.contains("no relay serves"), "{error}");
    }
}
