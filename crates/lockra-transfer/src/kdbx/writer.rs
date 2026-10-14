//! Logins written as a KeePass database (docs/formats.md §11), KDBX 4.1 as the keepass crate
//! writes it: AES-256, gzip, ChaCha20 for the protected values, the key derived with Argon2id
//! at the cost given. Each login is an entry under the group of its folder; its code is the
//! `otp` string KeePassXC keeps, an `otpauth://` URI.

use std::collections::{BTreeSet, HashMap};

use keepass::config::{DatabaseConfig, KdfConfig};
use keepass::db::{GroupId, fields};
use keepass::{Database, DatabaseKey};
use zeroize::Zeroizing;

use crate::item::Item;
use crate::login::Login;

/// What the database and its root group are called.
const NAME: &str = "Lockra";
/// KeePass's own strings of an entry, and the code strings Lockra reads: no field takes one of
/// these names.
const RESERVED: [&str; 8] = [fields::TITLE, fields::USERNAME, fields::PASSWORD, fields::URL, fields::NOTES, fields::OTP, "TOTP Seed", "TOTP Settings"];
/// The beginnings of the names Lockra (and KeePassXC) read as more URLs or as a code.
const RESERVED_PREFIXES: [&str; 3] = ["KP2A_URL", "TimeOtp-", "HmacOtp-"];

/// What Argon2id spends on a database's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Argon2Cost {
    /// Memory, KiB.
    pub memory_kib: u32,
    /// Passes.
    pub iterations: u32,
    /// Lanes.
    pub lanes: u32,
}

/// The database could not be written (the keepass crate refused its parts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the KeePass database could not be written")]
pub struct WriteError;

/// `logins` as a KeePass database under `password`, its key derived at `cost`. The key
/// derivation runs here: call it off the async runtime.
pub fn write(logins: &[Login], password: &str, cost: Argon2Cost) -> Result<Zeroizing<Vec<u8>>, WriteError> {
    let mut config = DatabaseConfig::default();
    config.kdf_config = KdfConfig::Argon2id {
        iterations: u64::from(cost.iterations),
        memory: u64::from(cost.memory_kib) * 1024,
        parallelism: cost.lanes,
        version: argon2::Version::Version13,
    };
    let mut db = Database::with_config(config);
    db.meta.database_name = Some(NAME.to_owned());
    db.root_mut().name = NAME.to_owned();
    let mut groups: HashMap<String, GroupId> = HashMap::new();
    for login in logins {
        let group = group_of(&mut db, &mut groups, login.folder.as_deref());
        let Some(mut group) = db.group_mut(group) else { return Err(WriteError) };
        group.add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, login.title.as_str());
            entry.set_unprotected(fields::USERNAME, login.username.as_str());
            entry.set_protected(fields::PASSWORD, login.password.as_str());
            let mut urls = login.urls.iter().map(|u| u.url.as_str());
            if let Some(first) = urls.next() {
                entry.set_unprotected(fields::URL, first);
            }
            // KeePassXC's and KeePass2Android's further URLs: KP2A_URL, KP2A_URL_1, ….
            for (index, url) in urls.enumerate() {
                let key = if index == 0 { "KP2A_URL".to_owned() } else { format!("KP2A_URL_{index}") };
                entry.set_unprotected(key, url);
            }
            if !login.notes.is_empty() {
                entry.set_unprotected(fields::NOTES, login.notes.as_str());
            }
            if let Some(Item::Account(auth)) = &login.totp {
                entry.set_protected(fields::OTP, auth.to_uri().as_str());
            }
            let mut taken: BTreeSet<String> = RESERVED.iter().map(|name| (*name).to_owned()).collect();
            for field in &login.fields {
                let name = field_name(&field.label, &taken);
                taken.insert(name.clone());
                if field.hidden {
                    entry.set_protected(name, field.value.as_str());
                } else {
                    entry.set_unprotected(name, field.value.as_str());
                }
            }
            entry.tags.clone_from(&login.tags);
        });
    }
    let mut out = Zeroizing::new(Vec::new());
    db.save(&mut *out, DatabaseKey::new().with_password(password)).map_err(|_| WriteError)?;
    Ok(out)
}

/// The group of `folder` (a path such as `Work/Servers`), made as needed: the root for none.
fn group_of(db: &mut Database, groups: &mut HashMap<String, GroupId>, folder: Option<&str>) -> GroupId {
    let mut parent = db.root().id();
    let mut path = String::new();
    for name in folder.unwrap_or_default().split('/').map(str::trim).filter(|name| !name.is_empty()) {
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str(name);
        parent = match groups.get(&path) {
            Some(&id) => id,
            None => {
                let id = match db.group_mut(parent) {
                    Some(mut group) => {
                        let mut child = group.add_group();
                        child.name = name.to_owned();
                        child.id()
                    }
                    None => parent,
                };
                groups.insert(path.clone(), id);
                id
            }
        };
    }
    parent
}

/// A field's name in its entry: its label (`Field` for none), escaped with `_` where KeePass or
/// Lockra would read it as more URLs or a code, and numbered where an entry has the name already.
fn field_name(label: &str, taken: &BTreeSet<String>) -> String {
    let label = label.trim();
    let base = if label.is_empty() {
        "Field".to_owned()
    } else if RESERVED_PREFIXES.iter().any(|prefix| label.starts_with(prefix)) {
        format!("_{label}")
    } else {
        label.to_owned()
    };
    if !taken.contains(&base) {
        return base;
    }
    (2..).map(|n| format!("{base} ({n})")).find(|name| !taken.contains(name)).unwrap_or(base)
}

#[cfg(test)]
mod tests;
