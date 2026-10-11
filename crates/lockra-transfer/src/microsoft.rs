//! Microsoft Authenticator: the `accounts` table of its `PhoneFactor` SQLite database.
//!
//! Microsoft Authenticator has no export. On a rooted Android phone the database sits at
//! `/data/data/com.azure.authenticator/databases/PhoneFactor`, in WAL mode: recent changes live in
//! `PhoneFactor-wal` until a checkpoint, so both files are read together. `account_type` 0 is a
//! third-party TOTP account with a Base32 secret and six digits, 1 is a personal Microsoft account
//! with a Base64 secret and eight digits; work and school accounts carry no portable secret.
//! Newer app versions may keep only `encrypted_oath_secret_key`, which cannot be read off the phone.

use std::fs;
use std::path::Path;

use lockra_otp::{Algorithm, Digits, OtpAuth, OtpKind, Period, base32};
use rusqlite::{Connection, OpenFlags};
use zeroize::Zeroizing;

use crate::encoding::decode_base64;
use crate::item::{Incompatible, Item, RejectReason, display_label};

/// The first 16 bytes of every SQLite database.
pub const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";

const TYPE_TOTP: i64 = 0;
const TYPE_MICROSOFT: i64 = 1;
const REQUIRED: [&str; 4] = ["name", "username", "oath_secret_key", "account_type"];

/// Why the database could not be read at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PhoneFactorError {
    /// A SQLite database, but not Microsoft Authenticator's (no `accounts` table with the columns).
    #[error("not a Microsoft Authenticator database")]
    NotPhoneFactor,
    /// The bytes are not a readable SQLite database.
    #[error("the database cannot be read")]
    Unreadable,
}

/// Whether `bytes` starts like a SQLite write-ahead log (`-wal` file).
pub fn is_wal(bytes: &[u8]) -> bool {
    matches!(bytes.get(..4), Some([0x37, 0x7f, 0x06, 0x82 | 0x83]))
}

/// The accounts of a PhoneFactor database. The files are copied into a private temporary
/// directory first: SQLite writes the shared-memory index and may checkpoint the log, and the
/// user's copies must stay exactly as they are. The directory is removed before this returns.
pub fn read(database: &[u8], wal: Option<&[u8]>) -> Result<Vec<Item>, PhoneFactorError> {
    if !database.starts_with(SQLITE_MAGIC) {
        return Err(PhoneFactorError::Unreadable);
    }
    let dir = tempfile::tempdir().map_err(|_| PhoneFactorError::Unreadable)?;
    let path = dir.path().join("PhoneFactor");
    fs::write(&path, database).map_err(|_| PhoneFactorError::Unreadable)?;
    if let Some(wal) = wal {
        fs::write(dir.path().join("PhoneFactor-wal"), wal).map_err(|_| PhoneFactorError::Unreadable)?;
    }
    let items = read_path(&path);
    drop(dir);
    items
}

fn read_path(path: &Path) -> Result<Vec<Item>, PhoneFactorError> {
    let unreadable = |_| PhoneFactorError::Unreadable;
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(unreadable)?;
    let columns: Vec<String> = {
        let mut statement = connection.prepare("PRAGMA table_info(accounts)").map_err(unreadable)?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1)).map_err(unreadable)?;
        rows.collect::<Result<_, _>>().map_err(unreadable)?
    };
    if REQUIRED.iter().any(|c| !columns.iter().any(|have| have == c)) {
        return Err(PhoneFactorError::NotPhoneFactor);
    }
    let encrypted = if columns.iter().any(|c| c == "encrypted_oath_secret_key") { "encrypted_oath_secret_key" } else { "NULL" };
    let order = if columns.iter().any(|c| c == "ux_position") { "ux_position, rowid" } else { "rowid" };
    let sql = format!("SELECT name, username, oath_secret_key, account_type, {encrypted} FROM accounts ORDER BY {order}");
    let mut statement = connection.prepare(&sql).map_err(unreadable)?;
    let rows = statement
        .query_map([], |row| {
            Ok(Row {
                name: row.get(0)?,
                username: row.get(1)?,
                secret: row.get::<_, Option<String>>(2)?.map(Zeroizing::new),
                account_type: row.get(3)?,
                encrypted: row.get::<_, Option<String>>(4)?.is_some_and(|e| !e.trim().is_empty()),
            })
        })
        .map_err(unreadable)?;
    rows.map(|row| row.map(to_item)).collect::<Result<_, _>>().map_err(unreadable)
}

struct Row {
    name: Option<String>,
    username: Option<String>,
    secret: Option<Zeroizing<String>>,
    account_type: Option<i64>,
    encrypted: bool,
}

fn to_item(row: Row) -> Item {
    let issuer = row.name.unwrap_or_default().trim().to_owned();
    let account = row.username.unwrap_or_default().trim().to_owned();
    let reject = |reason| Item::Rejected { label: display_label(&issuer, &account), line: None, reason };
    let digits = match row.account_type {
        Some(TYPE_TOTP) => Digits::SIX,
        Some(TYPE_MICROSOFT) => Digits::EIGHT,
        _ => return reject(RejectReason::UnsupportedAccountType),
    };
    let text = row.secret.filter(|s| !s.trim().is_empty());
    let Some(text) = text else {
        return reject(if row.encrypted { RejectReason::EncryptedSecret } else { RejectReason::EmptySecret });
    };
    let secret = if digits == Digits::SIX { base32::decode(&text).ok() } else { decode_base64(&text).filter(|s| !s.is_empty()) };
    let Some(secret) = secret else {
        return reject(RejectReason::InvalidSecret);
    };
    Item::Account(OtpAuth { kind: OtpKind::Totp { period: Period::THIRTY }, algorithm: Algorithm::Sha1, digits, secret, issuer, account })
}

/// `Ok` when Microsoft Authenticator computes the same codes after scanning the account's
/// standard `otpauth://` QR code: it adds accounts as SHA1, six digits, 30 seconds, whatever the
/// URI says.
pub fn exportable(auth: &OtpAuth) -> Result<(), Incompatible> {
    match auth.kind {
        OtpKind::Hotp { .. } => Err(Incompatible::HotpNotSupported),
        OtpKind::Totp { period } if period != Period::THIRTY => Err(Incompatible::PeriodNot30),
        OtpKind::Totp { .. } if auth.algorithm != Algorithm::Sha1 => Err(Incompatible::AlgorithmNotSha1),
        OtpKind::Totp { .. } if auth.digits != Digits::SIX => Err(Incompatible::DigitsNot6),
        OtpKind::Totp { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;

    /// The `accounts` table as Microsoft Authenticator wrote it in 2025.
    const SCHEMA: &str = "CREATE TABLE accounts (_id INTEGER PRIMARY KEY AUTOINCREMENT, group_key TEXT, name TEXT, username TEXT, paws_url TEXT, \
        oath_secret_key TEXT, oath_enabled INTEGER, cid TEXT, cached_pin TEXT, ngc_ski TEXT, aad_user_id TEXT, aad_tenant_id TEXT, \
        account_type INTEGER, account_capability INTEGER, ux_position INTEGER, is_totp_code_shown INTEGER, \
        encrypted_oath_secret_key TEXT NOT NULL DEFAULT '', mfa_pin_encryption_key_alias TEXT, identity_provider TEXT, \
        aad_ngc_totp_enabled INTEGER, aad_authority TEXT, restore_capability INTEGER, has_password INTEGER, \
        aad_security_defaults_policy_enabled INTEGER, phone_app_detail_id TEXT, replication_scope TEXT, activated_device_token TEXT, \
        routing_hint TEXT, tenant_country_code TEXT, data_boundary TEXT, puid TEXT)";

    struct Fixture {
        _dir: tempfile::TempDir,
        connection: Connection,
        path: std::path::PathBuf,
    }

    /// A database in WAL mode with automatic checkpoints off: until `checkpoint`, rows live only
    /// in the `-wal` file, as on a phone that has not checkpointed yet.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("PhoneFactor");
        let connection = Connection::open(&path).unwrap();
        connection.pragma_update(None, "journal_mode", "WAL").unwrap();
        connection.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        Fixture { _dir: dir, connection, path }
    }

    fn insert(f: &Fixture, position: i64, name: &str, user: &str, secret: Option<&str>, account_type: i64, encrypted: &str) {
        f.connection
            .execute(
                "INSERT INTO accounts (name, username, oath_secret_key, account_type, ux_position, encrypted_oath_secret_key) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![name, user, secret, account_type, position, encrypted],
            )
            .unwrap();
    }

    fn files(f: &Fixture) -> (Vec<u8>, Vec<u8>) {
        let wal = fs::read(f.path.with_file_name("PhoneFactor-wal")).unwrap_or_default();
        (fs::read(&f.path).unwrap(), wal)
    }

    #[test]
    fn reads_every_account_type_from_the_log() {
        let f = fixture();
        insert(&f, 1, "GitHub", "octocat", Some("JBSWY3DPEHPK3PXP"), 0, "");
        insert(&f, 2, "Microsoft", "me@outlook.com", Some("SGVsbG8h3q2+7w=="), 1, "");
        insert(&f, 3, "Contoso", "worker@contoso.com", None, 2, "opaque");
        insert(&f, 4, "Locked", "x", None, 0, "ZW5jcnlwdGVk");
        insert(&f, 5, "Empty", "y", Some("  "), 0, "");
        insert(&f, 6, "Broken", "z", Some("not base32!"), 0, "");
        let (database, wal) = files(&f);
        assert!(is_wal(&wal), "the rows must still be in the log for this test to mean anything");
        let items = read(&database, Some(&wal)).unwrap();
        assert_eq!(items.len(), 6);
        let Item::Account(github) = &items[0] else { panic!("{:?}", items[0]) };
        assert_eq!((github.issuer.as_str(), github.account.as_str(), github.digits), ("GitHub", "octocat", Digits::SIX));
        assert_eq!(github.secret.as_slice(), b"Hello!\xde\xad\xbe\xef");
        let Item::Account(msa) = &items[1] else { panic!("{:?}", items[1]) };
        assert_eq!((msa.digits, msa.algorithm, msa.kind), (Digits::EIGHT, Algorithm::Sha1, OtpKind::Totp { period: Period::THIRTY }));
        assert_eq!(msa.secret.as_slice(), b"Hello!\xde\xad\xbe\xef");
        let reasons: Vec<_> = items[2..].iter().map(|i| if let Item::Rejected { reason, .. } = i { *reason } else { panic!("{i:?}") }).collect();
        assert_eq!(reasons, [RejectReason::UnsupportedAccountType, RejectReason::EncryptedSecret, RejectReason::EmptySecret, RejectReason::InvalidSecret]);
        assert_eq!(items[2], Item::Rejected { label: "Contoso: worker@contoso.com".into(), line: None, reason: RejectReason::UnsupportedAccountType });
    }

    #[test]
    fn without_the_log_the_uncheckpointed_rows_are_missing() {
        let f = fixture();
        insert(&f, 1, "GitHub", "octocat", Some("JBSWY3DPEHPK3PXP"), 0, "");
        let (database, _) = files(&f);
        assert!(read(&database, None).is_err() || read(&database, None).unwrap().is_empty());
    }

    #[test]
    fn a_checkpointed_database_reads_on_its_own_and_the_inputs_stay_untouched() {
        let f = fixture();
        insert(&f, 1, "GitHub", "octocat", Some("JBSWY3DPEHPK3PXP"), 0, "");
        f.connection.pragma_update(None, "wal_checkpoint", "TRUNCATE").unwrap();
        let (database, _) = files(&f);
        let before = database.clone();
        assert_eq!(read(&database, None).unwrap().len(), 1);
        assert_eq!(database, before);
    }

    #[test]
    fn an_older_schema_without_the_encrypted_column_still_reads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE accounts (name TEXT, username TEXT, oath_secret_key TEXT, account_type INTEGER)").unwrap();
        connection.execute("INSERT INTO accounts VALUES ('A', 'b', NULL, 0)", []).unwrap();
        drop(connection);
        let items = read(&fs::read(&path).unwrap(), None).unwrap();
        assert_eq!(items, [Item::Rejected { label: "A: b".into(), line: None, reason: RejectReason::EmptySecret }]);
    }

    #[test]
    fn other_databases_and_garbage_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("other.db");
        Connection::open(&path).unwrap().execute_batch("CREATE TABLE notes (body TEXT)").unwrap();
        assert_eq!(read(&fs::read(&path).unwrap(), None).unwrap_err(), PhoneFactorError::NotPhoneFactor);
        assert_eq!(read(b"definitely not sqlite", None).unwrap_err(), PhoneFactorError::Unreadable);
        let mut damaged = SQLITE_MAGIC.to_vec();
        damaged.extend_from_slice(&[0xff; 200]);
        assert_eq!(read(&damaged, None).unwrap_err(), PhoneFactorError::Unreadable);
        assert!(!is_wal(b"\x37\x7f\x06\x84"));
        assert!(is_wal(b"\x37\x7f\x06\x82rest"));
    }

    #[test]
    fn export_compatibility_follows_what_the_app_computes() {
        let base = OtpAuth {
            kind: OtpKind::Totp { period: Period::THIRTY },
            algorithm: Algorithm::Sha1,
            digits: Digits::SIX,
            secret: Zeroizing::new(vec![1; 10]),
            issuer: "x".into(),
            account: "y".into(),
        };
        assert_eq!(exportable(&base), Ok(()));
        assert_eq!(exportable(&OtpAuth { digits: Digits::EIGHT, ..base.clone() }), Err(Incompatible::DigitsNot6));
        assert_eq!(exportable(&OtpAuth { algorithm: Algorithm::Sha256, ..base.clone() }), Err(Incompatible::AlgorithmNotSha1));
        assert_eq!(exportable(&OtpAuth { kind: OtpKind::Totp { period: Period::new(60).unwrap() }, ..base.clone() }), Err(Incompatible::PeriodNot30));
        assert_eq!(exportable(&OtpAuth { kind: OtpKind::Hotp { counter: 0 }, ..base }), Err(Incompatible::HotpNotSupported));
    }
}
