//! The reader against databases the keepass crate writes (an implementation of its own), every
//! cipher, inner stream, compression and key derivation, and headers made by hand for the limits.

use keepass::config::{CompressionConfig, DatabaseConfig, InnerCipherConfig, KdfConfig, OuterCipherConfig};
use keepass::db::{Entry as KpEntry, fields};
use keepass::{Database, DatabaseKey};
use lockra_otp::{Algorithm, Digits, OtpKind, Period};

use super::*;
use crate::item::{Item, RejectReason};
use crate::login::{App, Login, LoginField};

const PASSWORD: &str = "correct horse battery";

fn config(cipher: OuterCipherConfig, inner: InnerCipherConfig, compression: CompressionConfig, kdf: KdfConfig) -> DatabaseConfig {
    let mut config = DatabaseConfig::default();
    config.outer_cipher_config = cipher;
    config.inner_cipher_config = inner;
    config.compression_config = compression;
    config.kdf_config = kdf;
    config
}

/// AES-256, gzip, ChaCha20 for the protected values, a quick AES-KDF.
fn quick() -> DatabaseConfig {
    config(OuterCipherConfig::AES256, InnerCipherConfig::ChaCha20, CompressionConfig::GZip, KdfConfig::Aes { rounds: 16 })
}

/// A database: an entry at the root, one in a group with every kind of string and an earlier
/// version in its history, one in a nested group with KeePass 2's own code fields, and one in the
/// recycle bin.
fn sample(config: DatabaseConfig) -> Database {
    let mut db = Database::with_config(config);
    db.root_mut().add_entry().edit(|e| {
        e.set_unprotected(fields::TITLE, "Router");
        e.set_unprotected(fields::USERNAME, "admin");
        e.set_protected(fields::PASSWORD, "kp-pass-1");
        e.set_unprotected(fields::URL, "http://192.168.1.1");
        e.set_unprotected(fields::NOTES, "In the hall cupboard & <labelled>");
    });
    {
        let mut root = db.root_mut();
        let mut work = root.add_group();
        work.name = "Work".into();
        work.add_entry().edit(|e| {
            e.set_unprotected(fields::TITLE, "GitHub");
            e.set_unprotected(fields::USERNAME, "octocat");
            e.set_protected(fields::PASSWORD, "kp-old-pass");
            // The version before: its protected values come first in the stream.
            let earlier: KpEntry = (**e).clone();
            e.history.get_or_insert_with(Default::default).add_entry(earlier);
            e.set_protected(fields::PASSWORD, "kp-pass-2");
            e.set_unprotected(fields::URL, "https://github.com/login");
            e.set_unprotected("KP2A_URL", "https://gist.github.com");
            e.set_protected(fields::OTP, "otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP&issuer=GitHub");
            e.set_protected("PIN", "4242");
            e.set_unprotected("Customer", "C-77");
            e.set_unprotected("Empty", "");
            e.tags.push("dev".into());
            e.tags.push("work".into());
        });
        let mut servers = work.add_group();
        servers.name = "Servers".into();
        servers.add_entry().edit(|e| {
            e.set_unprotected(fields::TITLE, "Build");
            e.set_unprotected(fields::USERNAME, "ci");
            e.set_protected(fields::PASSWORD, "kp-pass-3");
            e.set_protected("TimeOtp-Secret-Base32", "GEZDGNBVGY3TQOJQ");
            e.set_unprotected("TimeOtp-Length", "8");
            e.set_unprotected("TimeOtp-Period", "60");
            e.set_unprotected("TimeOtp-Algorithm", "HMAC-SHA-256");
        });
    }
    let bin = {
        let mut root = db.root_mut();
        let mut bin = root.add_group();
        bin.name = "Recycle Bin".into();
        bin.add_entry().edit(|e| {
            e.set_unprotected(fields::TITLE, "Deleted");
            e.set_protected(fields::PASSWORD, "kp-gone");
        });
        bin.id().uuid()
    };
    db.meta.recyclebin_uuid = Some(bin);
    db.meta.recyclebin_enabled = Some(true);
    db
}

fn save(db: &Database, key: DatabaseKey) -> Vec<u8> {
    let mut out = Vec::new();
    db.save(&mut out, key).unwrap();
    out
}

fn with_password() -> DatabaseKey {
    DatabaseKey::new().with_password(PASSWORD)
}

fn logins(found: Vec<Found>) -> Vec<Login> {
    let mut logins: Vec<Login> = found
        .into_iter()
        .map(|f| match f {
            Found::Login(login) => *login,
            Found::Left { .. } => panic!("{f:?}"),
        })
        .collect();
    logins.sort_by(|a, b| a.title.cmp(&b.title));
    logins
}

#[test]
fn entries_come_with_their_groups_fields_and_codes_and_not_from_the_recycle_bin() {
    let bytes = save(&sample(quick()), with_password());
    assert_eq!(bytes[..8], MAGIC);
    check(&bytes).unwrap();
    let found = logins(read(&bytes, PASSWORD, None).unwrap());
    let titles: Vec<&str> = found.iter().map(|l| l.title.as_str()).collect();
    assert_eq!(titles, ["Build", "GitHub", "Router"]);
    let [build, github, router] = &found[..] else { panic!() };
    assert_eq!((router.app, router.folder.as_deref(), router.password.as_str()), (App::KeePass, None, "kp-pass-1"));
    assert_eq!(router.notes.as_str(), "In the hall cupboard & <labelled>");
    assert_eq!((github.folder.as_deref(), github.username.as_str(), github.password.as_str()), (Some("Work"), "octocat", "kp-pass-2"));
    let urls: Vec<&str> = github.urls.iter().map(|u| u.url.as_str()).collect();
    assert_eq!(urls, ["https://github.com/login", "https://gist.github.com"]);
    assert_eq!(github.tags, ["dev", "work"]);
    let mut fields = github.fields.clone();
    fields.sort_by(|a, b| a.label.cmp(&b.label));
    assert_eq!(
        fields,
        [
            LoginField { label: "Customer".into(), value: "C-77".to_owned().into(), hidden: false },
            LoginField { label: "PIN".into(), value: "4242".to_owned().into(), hidden: true },
        ]
    );
    assert!(matches!(&github.totp, Some(Item::Account(code)) if code.issuer == "GitHub" && code.secret.as_slice() == b"Hello!\xde\xad\xbe\xef"));
    assert_eq!(build.folder.as_deref(), Some("Work/Servers"));
    let Some(Item::Account(code)) = &build.totp else { panic!("{:?}", build.totp) };
    assert_eq!((code.kind, code.digits, code.algorithm), (OtpKind::Totp { period: Period::new(60).unwrap() }, Digits::EIGHT, Algorithm::Sha256));
    assert_eq!((code.issuer.as_str(), code.account.as_str(), code.secret.as_slice()), ("Build", "ci", &b"1234567890"[..]));
}

#[test]
fn every_cipher_inner_stream_compression_and_key_derivation() {
    let argon2d = KdfConfig::Argon2 { iterations: 2, memory: 64 * 1024, parallelism: 2, version: argon2::Version::Version13 };
    let argon2id = KdfConfig::Argon2id { iterations: 1, memory: 32 * 1024, parallelism: 1, version: argon2::Version::Version10 };
    let configs = [
        config(OuterCipherConfig::Twofish, InnerCipherConfig::Salsa20, CompressionConfig::None, KdfConfig::Aes { rounds: 3 }),
        config(OuterCipherConfig::ChaCha20, InnerCipherConfig::Plain, CompressionConfig::GZip, argon2d),
        config(OuterCipherConfig::AES256, InnerCipherConfig::Salsa20, CompressionConfig::GZip, argon2id),
    ];
    for config in configs {
        let described = format!("{config:?}");
        let bytes = save(&sample(config), with_password());
        let found = logins(read(&bytes, PASSWORD, None).unwrap_or_else(|e| panic!("{described}: {e}")));
        let passwords: Vec<&str> = found.iter().map(|l| l.password.as_str()).collect();
        assert_eq!(passwords, ["kp-pass-3", "kp-pass-2", "kp-pass-1"], "{described}");
    }
}

#[test]
fn a_key_file_with_the_password_or_alone() {
    let key_file = b"any file at all can be a key file".to_vec();
    let both = save(&sample(quick()), with_password().with_keyfile(&mut key_file.as_slice()).unwrap());
    let key = KeyFile::from_bytes(&key_file).unwrap();
    assert_eq!(logins(read(&both, PASSWORD, Some(&key)).unwrap()).len(), 3);
    // Without it, or with another file, or with another password: not opened.
    let other = KeyFile::from_bytes(b"another file").unwrap();
    assert_eq!(read(&both, PASSWORD, None).unwrap_err(), KdbxError::WrongKey);
    assert_eq!(read(&both, PASSWORD, Some(&other)).unwrap_err(), KdbxError::WrongKey);
    assert_eq!(read(&both, "wrong password", Some(&key)).unwrap_err(), KdbxError::WrongKey);
    // A database the key file alone opens: the password is left empty.
    let alone = save(&sample(quick()), DatabaseKey::new().with_keyfile(&mut key_file.as_slice()).unwrap());
    assert_eq!(logins(read(&alone, "", Some(&key)).unwrap()).len(), 3);
    assert_eq!(read(&alone, PASSWORD, Some(&key)).unwrap_err(), KdbxError::WrongKey);
    // An empty password is a password of its own when there is no key file.
    let empty = save(&sample(quick()), DatabaseKey::new().with_password(""));
    assert_eq!(logins(read(&empty, "", None).unwrap()).len(), 3);
}

#[test]
fn a_wrong_password_and_damage_are_told_apart() {
    let bytes = save(&sample(quick()), with_password());
    assert_eq!(read(&bytes, "not it", None).unwrap_err(), KdbxError::WrongKey);
    // A changed header fails its hash; a changed block, its HMAC; a cut file, its length.
    let mut header = bytes.clone();
    let header_len = super::header(&bytes).unwrap().bytes.len();
    header[header_len - 6] ^= 1;
    assert_eq!(read(&header, PASSWORD, None).unwrap_err(), KdbxError::Damaged);
    let mut block = bytes.clone();
    let last = block.len() - 40;
    block[last] ^= 1;
    assert_eq!(read(&block, PASSWORD, None).unwrap_err(), KdbxError::Damaged);
    assert_eq!(read(&bytes[..bytes.len() - 100], PASSWORD, None).unwrap_err(), KdbxError::Damaged);
    assert_eq!(read(&bytes[..header_len + 20], PASSWORD, None).unwrap_err(), KdbxError::Damaged);
}

#[test]
fn a_payload_expands_only_to_the_limit() {
    let mut db = Database::with_config(quick());
    db.root_mut().add_entry().edit(|e| e.set_unprotected(fields::NOTES, "a".repeat(256 * 1024)));
    let gzipped = save(&db, with_password());
    assert!(gzipped.len() < 16 * 1024, "{}", gzipped.len());
    assert_eq!(read_within(&gzipped, PASSWORD, None, 64 * 1024).unwrap_err(), KdbxError::TooLarge);
    assert_eq!(read_within(&gzipped, PASSWORD, None, 1024 * 1024).unwrap().len(), 1);
    let mut plain =
        Database::with_config(config(OuterCipherConfig::AES256, InnerCipherConfig::ChaCha20, CompressionConfig::None, KdfConfig::Aes { rounds: 1 }));
    plain.root_mut().add_entry().edit(|e| e.set_unprotected(fields::NOTES, "a".repeat(256 * 1024)));
    assert_eq!(read_within(&save(&plain, with_password()), PASSWORD, None, 64 * 1024).unwrap_err(), KdbxError::TooLarge);
}

/// A KDBX 4 header made by hand: AES-256, gzip, and the key derivation's dictionary of `kdf`.
fn header_with(major: u16, kdf: &[(&str, u8, Vec<u8>)]) -> Vec<u8> {
    let mut dictionary = vec![0x00, 0x01];
    for (name, kind, value) in kdf {
        dictionary.push(*kind);
        dictionary.extend_from_slice(&u32::try_from(name.len()).unwrap().to_le_bytes());
        dictionary.extend_from_slice(name.as_bytes());
        dictionary.extend_from_slice(&u32::try_from(value.len()).unwrap().to_le_bytes());
        dictionary.extend_from_slice(value);
    }
    dictionary.push(0);
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&major.to_le_bytes());
    for (id, data) in
        [(2u8, CIPHER_AES256.to_vec()), (3, 1u32.to_le_bytes().to_vec()), (4, vec![7; 32]), (7, vec![9; 16]), (11, dictionary), (0, b"\r\n\r\n".to_vec())]
    {
        out.push(id);
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&data);
    }
    out
}

fn argon2(memory: u64, iterations: u64, lanes: u32, version: u32) -> Vec<(&'static str, u8, Vec<u8>)> {
    vec![
        ("$UUID", 0x42, KDF_ARGON2ID.to_vec()),
        ("S", 0x42, vec![1; 32]),
        ("M", 0x05, memory.to_le_bytes().to_vec()),
        ("I", 0x05, iterations.to_le_bytes().to_vec()),
        ("P", 0x04, lanes.to_le_bytes().to_vec()),
        ("V", 0x04, version.to_le_bytes().to_vec()),
    ]
}

#[test]
fn the_key_derivation_is_bounded_before_any_work() {
    let aes = |rounds: u64| vec![("$UUID", 0x42, KDF_AES.to_vec()), ("R", 0x05, rounds.to_le_bytes().to_vec()), ("S", 0x42, vec![3; 32])];
    assert_eq!(check(&header_with(4, &aes(MAX_AES_ROUNDS))), Ok(()));
    assert_eq!(check(&header_with(4, &aes(MAX_AES_ROUNDS + 1))), Err(KdbxError::TooCostly));
    assert_eq!(check(&header_with(4, &aes(u64::MAX))), Err(KdbxError::TooCostly));
    assert_eq!(check(&header_with(4, &argon2(MAX_ARGON2_MEMORY, 64, 8, 0x13))), Ok(()));
    for (memory, iterations, lanes) in [
        (MAX_ARGON2_MEMORY + 1024, 1, 1),
        (1 << 20, MAX_ARGON2_ITERATIONS + 1, 1),
        (1 << 20, 1, MAX_ARGON2_PARALLELISM + 1),
        // Each within its own limit, together past the work allowed.
        (MAX_ARGON2_MEMORY, 65, 1),
        (u64::MAX, u64::MAX, u32::MAX),
    ] {
        assert_eq!(check(&header_with(4, &argon2(memory, iterations, lanes, 0x13))), Err(KdbxError::TooCostly), "{memory} {iterations} {lanes}");
    }
    // Parameters Argon2 itself refuses, and a version it does not know.
    assert_eq!(check(&header_with(4, &argon2(64 * 1024, 0, 1, 0x13))), Err(KdbxError::Damaged));
    assert_eq!(check(&header_with(4, &argon2(4 * 1024, 1, 1, 0x13))), Err(KdbxError::Damaged));
    assert_eq!(check(&header_with(4, &argon2(64 * 1024, 1, 1, 0x11))), Err(KdbxError::Unsupported));
}

#[test]
fn only_kdbx_4_with_known_parts_is_read() {
    let good = argon2(64 * 1024, 1, 1, 0x13);
    assert_eq!(check(&header_with(3, &good)), Err(KdbxError::Unsupported), "KDBX 3.1");
    assert_eq!(check(b"not a database"), Err(KdbxError::Unsupported));
    assert_eq!(check(&MAGIC), Err(KdbxError::Unsupported));
    let unknown = vec![("$UUID", 0x42, vec![0; 16])];
    assert_eq!(check(&header_with(4, &unknown)), Err(KdbxError::Unsupported));
    // A later dictionary version, a cut dictionary, a number of the wrong width.
    let mut later = header_with(4, &good);
    let at = later.windows(2).position(|w| w == [0x00, 0x01]).unwrap();
    later[at + 1] = 0x02;
    assert_eq!(check(&later), Err(KdbxError::Unsupported));
    let narrow = vec![("$UUID", 0x42, KDF_AES.to_vec()), ("R", 0x05, vec![1, 0, 0, 0]), ("S", 0x42, vec![3; 32])];
    assert_eq!(check(&header_with(4, &narrow)), Err(KdbxError::Damaged));
    let unseeded = vec![("$UUID", 0x42, KDF_AES.to_vec()), ("R", 0x04, 1u32.to_le_bytes().to_vec())];
    assert_eq!(check(&header_with(4, &unseeded)), Err(KdbxError::Damaged));
    let header = header_with(4, &good);
    assert_eq!(check(&header[..header.len() - 3]), Err(KdbxError::Damaged));
    // The header alone, without its hash, its HMAC or a block.
    assert_eq!(read(&header, PASSWORD, None).unwrap_err(), KdbxError::Damaged);
}

#[test]
fn codes_of_every_kind_keepass_keeps() {
    let code = |strings: &[(&str, &str)]| {
        let strings: Vec<(String, Zeroizing<String>)> = strings.iter().map(|(k, v)| ((*k).to_owned(), Zeroizing::new((*v).to_owned()))).collect();
        xml::code_of_for_tests(&strings)
    };
    let auth = |item: Option<Item>| match item {
        Some(Item::Account(auth)) => auth,
        other => panic!("{other:?}"),
    };
    let keeotp = auth(code(&[("otp", "key=JBSWY3DPEHPK3PXP&size=8&step=60&otpHashMode=Sha512")]));
    assert_eq!((keeotp.digits, keeotp.kind, keeotp.algorithm), (Digits::EIGHT, OtpKind::Totp { period: Period::new(60).unwrap() }, Algorithm::Sha512));
    let hotp = auth(code(&[("HmacOtp-Secret-Hex", "31 32 33 34 35 36 37 38 39 30"), ("HmacOtp-Counter", "7")]));
    assert_eq!((hotp.kind, hotp.secret.as_slice()), (OtpKind::Hotp { counter: 7 }, &b"1234567890"[..]));
    let text = auth(code(&[("TimeOtp-Secret", "12345678901234567890")]));
    assert_eq!(text.secret.as_slice(), b"12345678901234567890");
    let base64 = auth(code(&[("TimeOtp-Secret-Base64", "MTIzNDU2Nzg5MA==")]));
    assert_eq!(base64.secret.as_slice(), b"1234567890");
    let legacy = auth(code(&[("TOTP Seed", "GEZDGNBVGY3TQOJQ"), ("TOTP Settings", "30;7")]));
    assert_eq!(legacy.digits, Digits::new(7).unwrap());
    let rejected = |strings: &[(&str, &str)]| match code(strings) {
        Some(Item::Rejected { reason, .. }) => reason,
        other => panic!("{other:?}"),
    };
    assert_eq!(rejected(&[("TOTP Seed", "GEZDGNBVGY3TQOJQ"), ("TOTP Settings", "30;S")]), RejectReason::UnknownType);
    assert_eq!(rejected(&[("otp", "key=JBSWY3DPEHPK3PXP&type=Hotp")]), RejectReason::UnknownType);
    assert_eq!(rejected(&[("otp", "size=6")]), RejectReason::NotOtpauth);
    assert_eq!(rejected(&[("otp", "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&period=0")]), RejectReason::InvalidPeriod);
    assert_eq!(rejected(&[("TimeOtp-Secret-Base32", "not base32!")]), RejectReason::InvalidSecret);
    assert_eq!(rejected(&[("TimeOtp-Secret-Base32", "GEZDGNBV"), ("TimeOtp-Algorithm", "HMAC-MD5")]), RejectReason::UnknownAlgorithm);
    assert_eq!(rejected(&[("TimeOtp-Secret-Base32", "GEZDGNBV"), ("TimeOtp-Length", "12")]), RejectReason::UnsupportedDigits);
    assert_eq!(rejected(&[("TimeOtp-Secret-Base32", "GEZDGNBV"), ("TimeOtp-Period", "0")]), RejectReason::InvalidPeriod);
    assert_eq!(rejected(&[("HmacOtp-Secret", "abc"), ("HmacOtp-Counter", "-1")]), RejectReason::InvalidCounter);
    assert_eq!(rejected(&[("TimeOtp-Secret", "")]), RejectReason::EmptySecret);
    assert_eq!(code(&[("Other", "x")]), None);
}

#[test]
fn xml_damage_is_refused() {
    let mut stream = InnerStream::Plain;
    for xml in [
        "<KeePassFile><Root><Group><Entry><String><Key>Title</Key><Value>&nope;</Value></String></Entry></Group></Root></KeePassFile>",
        "<KeePassFile><Root><Group><Entry><String><Key>Title</Key><Value Protected=\"True\">!!!</Value></String></Entry></Group></Root></KeePassFile>",
        "<KeePassFile><Root><Group><Entry>",
        "<KeePassFile><Root><Group><Entry></Group></Root></KeePassFile>",
    ] {
        assert_eq!(xml::logins(xml.as_bytes(), &mut stream).unwrap_err(), KdbxError::Damaged, "{xml}");
    }
    // An entry outside every group is no entry KeePass shows.
    assert!(xml::logins(b"<KeePassFile><Root><Entry></Entry></Root></KeePassFile>", &mut stream).unwrap().is_empty());
    let deep = format!("<KeePassFile><Root>{}", "<Group>".repeat(300));
    assert_eq!(xml::logins(deep.as_bytes(), &mut stream).unwrap_err(), KdbxError::Damaged);
    // Character references and CDATA read as text; an empty protected value takes no stream.
    let fine = "<KeePassFile><Root><Group><Name>DB</Name><Entry><String><Key>Title</Key><Value>A&#38;B &#x263A; <![CDATA[<x>]]></Value></String>\
        <String><Key>Password</Key><Value Protected=\"True\"/></String></Entry></Group></Root></KeePassFile>";
    let found = xml::logins(fine.as_bytes(), &mut stream).unwrap();
    let Found::Login(login) = &found[0] else { panic!() };
    assert_eq!((login.title.as_str(), login.password.as_str(), login.folder.as_deref()), ("A&B \u{263a} <x>", "", None));
}
