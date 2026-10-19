//! Which spaces the relay serves (`--registration`): every one (open), the listed ones (closed),
//! or the listed ones and those a registration code admitted (code).
//!
//! A registration code is 15 random bytes the relay made (`lockra-relay new-code`), shown as 24
//! base32 letters in groups of four. It admits one new space, until it expires: the first request
//! of a space the relay does not know carries it (`Lockra-Registration`), and the relay keeps the
//! space's id with the SHA-256 of its access token from then on, as a closed relay keeps the
//! listed ones. The relay keeps a code as its SHA-256 alone.
//!
//! `<data>/lockra-relay-v1/admission.json` keeps the admitted spaces and the codes not used yet.
//! Only the relay process writes it (the commands reach it over its admin socket), each change
//! through a temporary file that takes the name once it is on disk.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use data_encoding::{BASE32_NOPAD, HEXLOWER};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::config::Admission;
use crate::store::write_atomic;

/// The file that keeps the admitted spaces and the codes, in the relay's root directory.
pub(crate) const FILE: &str = "admission.json";
/// The random bytes of a code.
const CODE_BYTES: usize = 15;
/// The format of [`FILE`].
const VERSION: u32 = 1;

/// What the relay answers a space's request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Served (open registration, a listed or admitted space and its token).
    Allowed,
    /// A space the relay does not know, which a registration code may admit.
    Unknown,
    /// Not served: unlisted on a closed relay, or another token than the space's.
    Forbidden,
}

/// What a registration code did for a space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Redeemed {
    /// The space is admitted (now, or by the request that came first).
    Admitted,
    /// The space is admitted with another token.
    Forbidden,
    /// No such code, used, or expired; or the relay takes no codes.
    BadCode,
}

/// A space a code (or `lockra-relay admit`) admitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Admitted {
    access: [u8; 32],
    /// The administrator's note, from the code or the command.
    pub note: String,
    /// When, Unix seconds.
    pub admitted_at: u64,
    /// The code that admitted it (its id), none when `admit` did.
    pub code: Option<String>,
}

/// A code not used yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pending {
    hash: [u8; 32],
    /// The administrator's note.
    pub note: String,
    /// Unix seconds.
    pub created_at: u64,
    /// Unix seconds; the code admits nothing from then on.
    pub expires_at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Registry {
    spaces: BTreeMap<Uuid, Admitted>,
    codes: BTreeMap<String, Pending>,
}

impl Registry {
    /// Without the codes that expired by `now`.
    fn prune(&mut self, now: u64) {
        self.codes.retain(|_, code| code.expires_at > now);
    }
}

/// [`FILE`] as it is written.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    version: u32,
    #[serde(default)]
    spaces: BTreeMap<Uuid, WireSpace>,
    #[serde(default)]
    codes: BTreeMap<String, WireCode>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSpace {
    access: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    note: String,
    admitted_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCode {
    hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    note: String,
    created_at: u64,
    expires_at: u64,
}

/// A code just made: the only time its letters are shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewCode {
    pub id: String,
    pub code: String,
    pub expires_at: u64,
}

/// The admission of the relay's spaces.
pub(crate) struct Gate {
    admission: Admission,
    path: PathBuf,
    registry: Mutex<Registry>,
}

impl Gate {
    /// The admission `admission` sets, with what `root/admission.json` keeps for code registration
    /// (nothing yet when it is not there). A file the relay cannot read stops it: it would serve
    /// the wrong spaces otherwise.
    pub(crate) fn open(root: &Path, admission: Admission) -> io::Result<Self> {
        let path = root.join(FILE);
        let registry = match &admission {
            Admission::Code(_) => match fs::read(&path) {
                Ok(bytes) => parse(&bytes).map_err(|message| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {message}", path.display())))?,
                Err(e) if e.kind() == io::ErrorKind::NotFound => Registry::default(),
                Err(e) => return Err(e),
            },
            Admission::Open | Admission::Closed(_) => Registry::default(),
        };
        Ok(Self { admission, path, registry: Mutex::new(registry) })
    }

    /// Whether every space is served.
    pub(crate) fn is_open(&self) -> bool {
        matches!(self.admission, Admission::Open)
    }

    /// The registration, as `--registration` names it.
    pub(crate) fn name(&self) -> &'static str {
        self.admission.name()
    }

    /// Whether space `id` may be served at all (a space on disk is read in only then).
    pub(crate) fn includes(&self, id: Uuid) -> bool {
        match &self.admission {
            Admission::Open => true,
            Admission::Closed(listed) => listed.contains_key(&id),
            Admission::Code(listed) => listed.contains_key(&id) || self.registry.lock().spaces.contains_key(&id),
        }
    }

    /// What a request of space `id` with the token whose SHA-256 is `access` gets.
    pub(crate) fn verdict(&self, id: Uuid, access: &[u8; 32]) -> Verdict {
        let by = |expected: &[u8; 32]| if same(expected, access) { Verdict::Allowed } else { Verdict::Forbidden };
        match &self.admission {
            Admission::Open => Verdict::Allowed,
            Admission::Closed(listed) => listed.get(&id).map_or(Verdict::Forbidden, by),
            Admission::Code(listed) => match listed.get(&id) {
                Some(expected) => by(expected),
                None => self.registry.lock().spaces.get(&id).map_or(Verdict::Unknown, |admitted| by(&admitted.access)),
            },
        }
    }

    /// Whether the request is served as it is.
    pub(crate) fn allows(&self, id: Uuid, access: &[u8; 32]) -> bool {
        self.verdict(id, access) == Verdict::Allowed
    }

    /// Admit space `id`, bound to `access`, with the code `text` as of `now` (Unix seconds); the
    /// code is used up. A space admitted meanwhile (a request of it that came first) is not
    /// admitted again: its token decides.
    pub(crate) fn redeem(&self, id: Uuid, access: &[u8; 32], text: &str, now: u64) -> io::Result<Redeemed> {
        let Admission::Code(listed) = &self.admission else { return Ok(Redeemed::BadCode) };
        let mut registry = self.registry.lock();
        if let Some(expected) = listed.get(&id).or_else(|| registry.spaces.get(&id).map(|a| &a.access)) {
            return Ok(if same(expected, access) { Redeemed::Admitted } else { Redeemed::Forbidden });
        }
        let Some(hash) = parse_code(text).map(|bytes| code_hash(&bytes)) else { return Ok(Redeemed::BadCode) };
        let id_of_code = code_id(&hash);
        let valid = registry.codes.get(&id_of_code).is_some_and(|code| same(&code.hash, &hash) && code.expires_at > now);
        if !valid {
            return Ok(Redeemed::BadCode);
        }
        let mut next = registry.clone();
        let note = next.codes.remove(&id_of_code).map(|code| code.note).unwrap_or_default();
        next.spaces.insert(id, Admitted { access: *access, note, admitted_at: now, code: Some(id_of_code) });
        next.prune(now);
        self.save(&next)?;
        *registry = next;
        tracing::info!("a registration code admitted a sync space");
        Ok(Redeemed::Admitted)
    }

    /// A new code, admitting one space until `now + lifetime` (seconds), with `note`.
    pub(crate) fn new_code(&self, note: String, lifetime: u64, now: u64) -> Result<NewCode, String> {
        self.code_registration()?;
        let mut registry = self.registry.lock();
        let (bytes, hash, id) = loop {
            let mut bytes = [0u8; CODE_BYTES];
            getrandom::fill(&mut bytes).map_err(|e| format!("no random bytes for a code: {e}"))?;
            let hash = code_hash(&bytes);
            let id = code_id(&hash);
            if !registry.codes.contains_key(&id) {
                break (bytes, hash, id);
            }
        };
        let expires_at = now.saturating_add(lifetime);
        let mut next = registry.clone();
        next.prune(now);
        next.codes.insert(id.clone(), Pending { hash, note, created_at: now, expires_at });
        self.save(&next).map_err(|e| self.unwritten(&e))?;
        *registry = next;
        Ok(NewCode { id, code: show_code(&bytes), expires_at })
    }

    /// The codes not used yet that have not expired by `now`, by id.
    pub(crate) fn codes(&self, now: u64) -> Vec<(String, Pending)> {
        self.registry.lock().codes.iter().filter(|(_, code)| code.expires_at > now).map(|(id, code)| (id.clone(), code.clone())).collect()
    }

    /// Cancel the code `id`.
    pub(crate) fn drop_code(&self, id: &str, now: u64) -> Result<(), String> {
        self.code_registration()?;
        let mut registry = self.registry.lock();
        if !registry.codes.get(id).is_some_and(|code| code.expires_at > now) {
            return Err(format!("no code {id} waits to be used"));
        }
        let mut next = registry.clone();
        next.codes.remove(id);
        next.prune(now);
        self.save(&next).map_err(|e| self.unwritten(&e))?;
        *registry = next;
        Ok(())
    }

    /// Admit space `id`, whose files on disk say `access`, without a code.
    pub(crate) fn admit(&self, id: Uuid, access: &[u8; 32], note: String, now: u64) -> Result<(), String> {
        let listed = self.code_registration()?;
        if listed.contains_key(&id) {
            return Err(format!("space {id} is listed in --allowed-spaces already"));
        }
        let mut registry = self.registry.lock();
        if registry.spaces.contains_key(&id) {
            return Err(format!("space {id} is admitted already"));
        }
        let mut next = registry.clone();
        next.prune(now);
        next.spaces.insert(id, Admitted { access: *access, note, admitted_at: now, code: None });
        self.save(&next).map_err(|e| self.unwritten(&e))?;
        *registry = next;
        Ok(())
    }

    /// Stop admitting space `id`.
    pub(crate) fn revoke(&self, id: Uuid, now: u64) -> Result<(), String> {
        let listed = self.code_registration()?;
        if listed.contains_key(&id) {
            return Err(format!("space {id} is listed in --allowed-spaces: remove it there"));
        }
        let mut registry = self.registry.lock();
        if !registry.spaces.contains_key(&id) {
            return Err(format!("space {id} is not admitted"));
        }
        let mut next = registry.clone();
        next.spaces.remove(&id);
        next.prune(now);
        self.save(&next).map_err(|e| self.unwritten(&e))?;
        *registry = next;
        Ok(())
    }

    /// The listed spaces (`None`: listed in the settings) and the admitted ones, by id.
    pub(crate) fn admitted(&self) -> BTreeMap<Uuid, Option<Admitted>> {
        let mut all: BTreeMap<Uuid, Option<Admitted>> = self.admission.listed().keys().map(|id| (*id, None)).collect();
        for (id, admitted) in &self.registry.lock().spaces {
            all.entry(*id).or_insert_with(|| Some(admitted.clone()));
        }
        all
    }

    /// The listed spaces of code registration, or why there is none.
    fn code_registration(&self) -> Result<&BTreeMap<Uuid, [u8; 32]>, String> {
        match &self.admission {
            Admission::Code(listed) => Ok(listed),
            other => Err(format!("this relay's registration is {}: codes and admissions are for --registration code", other.name())),
        }
    }

    fn unwritten(&self, error: &io::Error) -> String {
        format!("{} could not be written: {error}", self.path.display())
    }

    fn save(&self, registry: &Registry) -> io::Result<()> {
        let wire = Wire {
            version: VERSION,
            spaces: registry
                .spaces
                .iter()
                .map(|(id, a)| (*id, WireSpace { access: HEXLOWER.encode(&a.access), note: a.note.clone(), admitted_at: a.admitted_at, code: a.code.clone() }))
                .collect(),
            codes: registry
                .codes
                .iter()
                .map(|(id, c)| {
                    (id.clone(), WireCode { hash: HEXLOWER.encode(&c.hash), note: c.note.clone(), created_at: c.created_at, expires_at: c.expires_at })
                })
                .collect(),
        };
        let mut text = serde_json::to_vec_pretty(&wire).map_err(io::Error::other)?;
        text.push(b'\n');
        write_atomic(&self.path, &text)
    }
}

/// The registry `bytes` hold.
fn parse(bytes: &[u8]) -> Result<Registry, String> {
    let wire: Wire = serde_json::from_slice(bytes).map_err(|e| format!("not an admission file ({e})"))?;
    if wire.version != VERSION {
        return Err(format!("an admission file of format {}, this relay reads {VERSION}", wire.version));
    }
    let hash = |text: &str| HEXLOWER.decode(text.as_bytes()).ok().and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok());
    let mut registry = Registry::default();
    for (id, space) in wire.spaces {
        let access = hash(&space.access).ok_or_else(|| format!("space {id} has no lowercase 64-digit access SHA-256"))?;
        registry.spaces.insert(id, Admitted { access, note: space.note, admitted_at: space.admitted_at, code: space.code });
    }
    for (id, code) in wire.codes {
        let hash = hash(&code.hash).filter(|hash| code_id(hash) == id).ok_or_else(|| format!("code {id} has no SHA-256 of its own"))?;
        registry.codes.insert(id, Pending { hash, note: code.note, created_at: code.created_at, expires_at: code.expires_at });
    }
    Ok(registry)
}

/// The bytes of a code as it was shown, typed or pasted: letters in either case, the hyphens and
/// spaces between them left out.
pub(crate) fn parse_code(text: &str) -> Option<[u8; CODE_BYTES]> {
    let letters: String = text.chars().filter(|c| !matches!(c, '-' | ' ')).map(|c| c.to_ascii_uppercase()).collect();
    BASE32_NOPAD.decode(letters.as_bytes()).ok().and_then(|bytes| <[u8; CODE_BYTES]>::try_from(bytes.as_slice()).ok())
}

/// A code as it is shown: 24 base32 letters in six groups of four.
fn show_code(bytes: &[u8; CODE_BYTES]) -> String {
    let letters = BASE32_NOPAD.encode(bytes);
    let groups: Vec<&str> = (0..letters.len()).step_by(4).filter_map(|at| letters.get(at..(at + 4).min(letters.len()))).collect();
    groups.join("-")
}

fn code_hash(bytes: &[u8; CODE_BYTES]) -> [u8; 32] {
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&Sha256::digest(bytes));
    hash
}

/// A code's id, for listing and cancelling it: the first four bytes of its SHA-256 in hex.
fn code_id(hash: &[u8; 32]) -> String {
    HEXLOWER.encode(&hash[..4])
}

/// The same hash: every byte compared, whatever the first difference.
fn same(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |different, (x, y)| different | (x ^ y)) == 0
}

/// Now, in Unix seconds.
pub(crate) fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 24 * 60 * 60;

    fn access(n: u8) -> [u8; 32] {
        [n; 32]
    }

    fn code_gate(dir: &Path, listed: &[(Uuid, [u8; 32])]) -> Gate {
        Gate::open(dir, Admission::Code(Box::new(listed.iter().copied().collect()))).unwrap()
    }

    #[test]
    fn a_code_admits_one_space_once_until_it_expires() {
        let dir = tempfile::tempdir().unwrap();
        let gate = code_gate(dir.path(), &[]);
        let (space, other) = (Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(gate.verdict(space, &access(1)), Verdict::Unknown);
        let made = gate.new_code("phone".into(), 7 * DAY, 1000).unwrap();
        assert_eq!(made.expires_at, 1000 + 7 * DAY);
        assert_eq!(made.code.len(), 29, "six groups of four letters: {}", made.code);
        assert!(made.code.split('-').all(|group| group.len() == 4 && group.bytes().all(|b| b.is_ascii_uppercase() || (b'2'..=b'7').contains(&b))));
        assert_eq!(gate.codes(1000).len(), 1);

        // Typed in lower case, without the hyphens: the same code.
        let typed = made.code.replace('-', " ").to_lowercase();
        assert_eq!(gate.redeem(space, &access(1), &typed, 2000).unwrap(), Redeemed::Admitted);
        assert_eq!(gate.verdict(space, &access(1)), Verdict::Allowed);
        assert_eq!(gate.verdict(space, &access(2)), Verdict::Forbidden);
        assert!(gate.includes(space) && !gate.includes(other));
        // Used: it admits no other space; the space itself keeps its token whatever is sent.
        assert_eq!(gate.redeem(other, &access(3), &made.code, 2001).unwrap(), Redeemed::BadCode);
        assert_eq!(gate.redeem(space, &access(1), "anything", 2001).unwrap(), Redeemed::Admitted);
        assert_eq!(gate.redeem(space, &access(9), &made.code, 2001).unwrap(), Redeemed::Forbidden);
        assert!(gate.codes(2001).is_empty());
        let admitted = gate.admitted();
        let entry = admitted[&space].as_ref().unwrap();
        assert_eq!((entry.note.as_str(), entry.admitted_at, entry.code.as_deref()), ("phone", 2000, Some(made.id.as_str())));

        // Expired, malformed, or another relay's: nothing.
        let late = gate.new_code(String::new(), DAY, 3000).unwrap();
        assert_eq!(gate.redeem(other, &access(3), &late.code, 3000 + DAY).unwrap(), Redeemed::BadCode);
        assert!(gate.codes(3000 + DAY).is_empty());
        for bad in ["", "not a code", "AAAA-AAAA-AAAA-AAAA-AAAA-AAAA", &made.code[..20]] {
            assert_eq!(gate.redeem(other, &access(3), bad, 3001).unwrap(), Redeemed::BadCode, "{bad}");
        }
        assert_eq!(gate.verdict(other, &access(3)), Verdict::Unknown);
    }

    #[test]
    fn the_registry_survives_a_restart_and_only_valid_files_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let space = Uuid::new_v4();
        let (kept, dropped) = {
            let gate = code_gate(dir.path(), &[]);
            let used = gate.new_code("laptop".into(), DAY, 100).unwrap();
            gate.redeem(space, &access(1), &used.code, 200).unwrap();
            let kept = gate.new_code("tablet".into(), DAY, 300).unwrap();
            let dropped = gate.new_code(String::new(), DAY, 300).unwrap();
            gate.drop_code(&dropped.id, 400).unwrap();
            assert!(gate.drop_code(&dropped.id, 400).unwrap_err().contains("no code"));
            (kept, dropped)
        };
        let text = fs::read_to_string(dir.path().join(FILE)).unwrap();
        assert!(!text.contains(&kept.code) && !text.contains(&kept.code.replace('-', "")), "a code is kept as its hash: {text}");
        let gate = code_gate(dir.path(), &[]);
        assert_eq!(gate.verdict(space, &access(1)), Verdict::Allowed);
        assert_eq!(gate.codes(400).iter().map(|(id, code)| (id.clone(), code.note.clone())).collect::<Vec<_>>(), [(kept.id.clone(), "tablet".to_owned())]);
        assert_eq!(gate.redeem(Uuid::new_v4(), &access(2), &dropped.code, 500).unwrap(), Redeemed::BadCode);
        assert_eq!(gate.redeem(Uuid::new_v4(), &access(2), &kept.code, 500).unwrap(), Redeemed::Admitted);

        // Closed and open relays do not read it.
        assert!(Gate::open(dir.path(), Admission::Open).unwrap().codes(400).is_empty());
        assert_eq!(Gate::open(dir.path(), Admission::Closed(Box::default())).unwrap().verdict(space, &access(1)), Verdict::Forbidden);

        // A file it cannot read stops the relay rather than serve the wrong spaces.
        for bad in [
            "not json".to_owned(),
            r#"{"version": 2}"#.to_owned(),
            r#"{"version": 1, "extra": true}"#.to_owned(),
            format!(r#"{{"version": 1, "spaces": {{"{space}": {{"access": "short", "admitted_at": 1}}}}}}"#),
            format!(r#"{{"version": 1, "codes": {{"00000000": {{"hash": "{}", "created_at": 1, "expires_at": 2}}}}}}"#, "ab".repeat(32)),
        ] {
            fs::write(dir.path().join(FILE), &bad).unwrap();
            let error = Gate::open(dir.path(), Admission::Code(Box::default())).err().unwrap();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{bad}");
        }
    }

    #[test]
    fn admitting_and_revoking_by_command() {
        let dir = tempfile::tempdir().unwrap();
        let (listed, space) = (Uuid::new_v4(), Uuid::new_v4());
        let gate = code_gate(dir.path(), &[(listed, access(5))]);
        assert_eq!(gate.verdict(listed, &access(5)), Verdict::Allowed);
        assert_eq!(gate.verdict(listed, &access(6)), Verdict::Forbidden);
        assert!(gate.admit(listed, &access(5), String::new(), 1).unwrap_err().contains("--allowed-spaces"));
        assert!(gate.revoke(listed, 1).unwrap_err().contains("--allowed-spaces"));
        gate.admit(space, &access(1), "old phone".into(), 10).unwrap();
        assert!(gate.admit(space, &access(1), String::new(), 11).unwrap_err().contains("already"));
        assert_eq!(gate.verdict(space, &access(1)), Verdict::Allowed);
        assert_eq!(
            gate.admitted().into_iter().map(|(id, a)| (id, a.map(|a| a.note))).collect::<BTreeMap<_, _>>(),
            BTreeMap::from([(listed, None), (space, Some("old phone".to_owned()))])
        );
        gate.revoke(space, 12).unwrap();
        assert!(gate.revoke(space, 12).unwrap_err().contains("not admitted"));
        assert_eq!(gate.verdict(space, &access(1)), Verdict::Unknown);
        // A redeemed space that is listed too keeps the listed hash.
        let made = gate.new_code(String::new(), DAY, 20).unwrap();
        assert_eq!(gate.redeem(listed, &access(6), &made.code, 21).unwrap(), Redeemed::Forbidden);
        assert_eq!(gate.codes(21).len(), 1, "a refused space uses no code");
    }

    #[test]
    fn open_and_closed_relays_take_no_codes() {
        let dir = tempfile::tempdir().unwrap();
        let space = Uuid::new_v4();
        let open = Gate::open(dir.path(), Admission::Open).unwrap();
        assert!(open.is_open() && open.includes(space));
        assert_eq!(open.verdict(space, &access(1)), Verdict::Allowed);
        assert_eq!(open.name(), "open");
        let closed = Gate::open(dir.path(), Admission::Closed(Box::new(BTreeMap::from([(space, access(1))])))).unwrap();
        assert_eq!(
            (closed.verdict(space, &access(1)), closed.verdict(space, &access(2)), closed.verdict(Uuid::new_v4(), &access(1))),
            (Verdict::Allowed, Verdict::Forbidden, Verdict::Forbidden)
        );
        assert_eq!(closed.redeem(Uuid::new_v4(), &access(1), "x", 1).unwrap(), Redeemed::BadCode);
        for gate in [&open, &closed] {
            assert!(gate.new_code(String::new(), DAY, 1).unwrap_err().contains("--registration code"));
            assert!(gate.drop_code("00000000", 1).is_err());
            assert!(gate.admit(Uuid::new_v4(), &access(1), String::new(), 1).is_err());
            assert!(gate.revoke(space, 1).is_err());
        }
        assert!(!dir.path().join(FILE).exists(), "nothing written");
    }

    #[test]
    fn an_unwritable_file_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let gate = Gate::open(&dir.path().join("missing"), Admission::Code(Box::default())).unwrap();
        assert!(gate.new_code(String::new(), DAY, 1).unwrap_err().contains("could not be written"));
        assert!(gate.codes(1).is_empty());
        assert!(gate.admit(Uuid::new_v4(), &access(1), String::new(), 1).unwrap_err().contains("could not be written"));
        assert!(gate.admitted().is_empty());
    }

    #[test]
    fn codes_read_back_as_they_were_shown() {
        let bytes = [0xA5; CODE_BYTES];
        let shown = show_code(&bytes);
        assert_eq!(parse_code(&shown), Some(bytes));
        assert_eq!(parse_code(&shown.to_lowercase().replace('-', "")), Some(bytes));
        assert_eq!(parse_code(&format!(" {shown} ")), Some(bytes));
        assert_eq!(parse_code("11111111"), None);
        assert!(now() > 1_700_000_000);
    }
}
