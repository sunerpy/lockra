//! The XML inside a KDBX 4 payload: groups of entries, each a set of strings (Title, UserName,
//! Password, URL, Notes and the user's own), some of them protected. Read as events, so the depth
//! of the groups costs no stack: the protected values are unmasked in document order, the
//! entries' histories and the recycle bin's included, as the inner stream runs through them all.

use lockra_otp::{Algorithm, Digits, OtpAuth, OtpKind, Period, base32, uri};
use quick_xml::events::{BytesStart, Event};
use zeroize::Zeroizing;

use super::{InnerStream, KdbxError};
use crate::item::{Item, RejectReason, display_label};
use crate::login::{App, Found, Login, account, named, tags};

/// The deepest nesting of elements read.
const MAX_DEPTH: usize = 256;

/// A group as the parse meets it.
struct Group {
    uuid: Option<Vec<u8>>,
    name: String,
    parent: Option<usize>,
}

/// An entry as the parse meets it: its strings, in order, and the group it is in.
struct Entry {
    group: usize,
    strings: Vec<(String, Zeroizing<String>, bool)>,
    tags: String,
}

/// What the text being read belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    GroupUuid,
    GroupName,
    EntryTags,
    StringKey,
    StringValue,
    RecycleBinEnabled,
    RecycleBinUuid,
    /// A protected value elsewhere (a history's entry): unmasked, to keep the stream in step,
    /// then dropped.
    Elsewhere,
}

/// The entries of `xml`, not those in the recycle bin, as logins.
pub(super) fn logins(xml: &[u8], stream: &mut InnerStream) -> Result<Vec<Found>, KdbxError> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut path: Vec<String> = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    let mut open_groups: Vec<usize> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    let mut entry: Option<Entry> = None;
    let mut key = String::new();
    let mut text = Zeroizing::new(String::new());
    let mut capture: Option<(Capture, bool)> = None;
    let mut bin_enabled = true;
    let mut bin: Option<Vec<u8>> = None;
    let mut buf = Vec::new();
    loop {
        let event = reader.read_event_into(&mut buf).map_err(|_| KdbxError::Damaged)?;
        match event {
            Event::Start(start) => {
                let name = start.name().as_ref().to_owned();
                if path.len() >= MAX_DEPTH {
                    return Err(KdbxError::Damaged);
                }
                path.push(name);
                let in_history = path.iter().any(|p| p == "History");
                match path_kind(&path) {
                    Some(PathKind::Group) => {
                        groups.push(Group { uuid: None, name: String::new(), parent: open_groups.last().copied() });
                        open_groups.push(groups.len() - 1);
                    }
                    Some(PathKind::Entry) if !in_history => {
                        let group = *open_groups.last().ok_or(KdbxError::Damaged)?;
                        entry = Some(Entry { group, strings: Vec::new(), tags: String::new() });
                    }
                    _ => {}
                }
                let protected = is_protected(&start);
                let target = match path_kind(&path) {
                    Some(PathKind::GroupUuid) => Some(Capture::GroupUuid),
                    Some(PathKind::GroupName) => Some(Capture::GroupName),
                    Some(PathKind::EntryTags) if !in_history => Some(Capture::EntryTags),
                    Some(PathKind::StringKey) if !in_history => Some(Capture::StringKey),
                    Some(PathKind::StringValue) if !in_history => Some(Capture::StringValue),
                    Some(PathKind::RecycleBinEnabled) => Some(Capture::RecycleBinEnabled),
                    Some(PathKind::RecycleBinUuid) => Some(Capture::RecycleBinUuid),
                    _ if protected => Some(Capture::Elsewhere),
                    _ => None,
                };
                capture = target.map(|t| (t, protected));
                text.clear();
            }
            Event::Empty(start) => {
                // `<Value Protected="True"/>`: an empty value, which takes nothing of the stream.
                let name = start.name().as_ref().to_owned();
                path.push(name);
                let in_history = path.iter().any(|p| p == "History");
                if path_kind(&path) == Some(PathKind::StringValue)
                    && !in_history
                    && let Some(entry) = entry.as_mut()
                {
                    entry.strings.push((std::mem::take(&mut key), Zeroizing::new(String::new()), is_protected(&start)));
                }
                path.pop();
            }
            Event::Text(content) => {
                if capture.is_some() {
                    text.push_str(&content.xml10_content());
                }
            }
            Event::CData(content) => {
                if capture.is_some() {
                    text.push_str(&content.xml10_content());
                }
            }
            Event::GeneralRef(reference) => {
                if capture.is_some() {
                    let character = match reference.resolve_char_ref().map_err(|_| KdbxError::Damaged)? {
                        Some(character) => character,
                        None => match &*reference {
                            "amp" => '&',
                            "lt" => '<',
                            "gt" => '>',
                            "quot" => '"',
                            "apos" => '\'',
                            _ => return Err(KdbxError::Damaged),
                        },
                    };
                    text.push(character);
                }
            }
            Event::End(_) => {
                if let Some((target, protected)) = capture.take() {
                    let value = if protected { unmask(&text, stream)? } else { std::mem::take(&mut text) };
                    match target {
                        Capture::GroupUuid => {
                            if let Some(&open) = open_groups.last() {
                                groups[open].uuid = uuid_of(&value);
                            }
                        }
                        Capture::GroupName => {
                            if let Some(&open) = open_groups.last() {
                                groups[open].name = value.trim().to_owned();
                            }
                        }
                        Capture::EntryTags => {
                            if let Some(entry) = entry.as_mut() {
                                entry.tags = value.to_string();
                            }
                        }
                        Capture::StringKey => key = value.to_string(),
                        Capture::StringValue => {
                            if let Some(entry) = entry.as_mut() {
                                entry.strings.push((std::mem::take(&mut key), value, protected));
                            }
                        }
                        Capture::RecycleBinEnabled => bin_enabled = !value.trim().eq_ignore_ascii_case("false"),
                        Capture::RecycleBinUuid => bin = uuid_of(&value),
                        Capture::Elsewhere => {}
                    }
                    text.clear();
                }
                match path_kind(&path) {
                    Some(PathKind::Group) => {
                        open_groups.pop();
                    }
                    Some(PathKind::Entry) if !path[..path.len() - 1].iter().any(|p| p == "History") => entries.extend(entry.take()),
                    _ => {}
                }
                path.pop();
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if !path.is_empty() {
        return Err(KdbxError::Damaged);
    }
    let bin = bin.filter(|uuid| bin_enabled && uuid.iter().any(|b| *b != 0));
    let in_bin = |mut group: Option<usize>| {
        while let Some(index) = group {
            if bin.is_some() && groups[index].uuid == bin {
                return true;
            }
            group = groups[index].parent;
        }
        false
    };
    Ok(entries.into_iter().filter(|e| !in_bin(Some(e.group))).map(|e| login_of(e, &groups)).collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathKind {
    Group,
    GroupUuid,
    GroupName,
    Entry,
    EntryTags,
    StringKey,
    StringValue,
    RecycleBinEnabled,
    RecycleBinUuid,
}

/// What the element at the end of `path` is, where it matters.
fn path_kind(path: &[String]) -> Option<PathKind> {
    let names: Vec<&str> = path.iter().map(String::as_str).collect();
    match names.as_slice() {
        ["KeePassFile", "Meta", "RecycleBinEnabled"] => return Some(PathKind::RecycleBinEnabled),
        ["KeePassFile", "Meta", "RecycleBinUUID"] => return Some(PathKind::RecycleBinUuid),
        ["KeePassFile", "Root", rest @ ..] => {
            // Groups nest; an entry sits in a group, its history's entries in the entry.
            let groups = rest.iter().take_while(|n| **n == "Group").count();
            if groups == 0 {
                return None;
            }
            let tail = &rest[groups..];
            return match tail {
                [] => Some(PathKind::Group),
                ["UUID"] => Some(PathKind::GroupUuid),
                ["Name"] => Some(PathKind::GroupName),
                ["Entry"] | ["Entry", "History", "Entry"] => Some(PathKind::Entry),
                ["Entry", "Tags"] | ["Entry", "History", "Entry", "Tags"] => Some(PathKind::EntryTags),
                ["Entry", "String", "Key"] | ["Entry", "History", "Entry", "String", "Key"] => Some(PathKind::StringKey),
                ["Entry", "String", "Value"] | ["Entry", "History", "Entry", "String", "Value"] => Some(PathKind::StringValue),
                _ => None,
            };
        }
        _ => {}
    }
    None
}

fn is_protected(start: &BytesStart<'_>) -> bool {
    start.attributes().flatten().any(|a| a.key.as_ref() == "Protected" && a.value.eq_ignore_ascii_case("true"))
}

/// A protected value: Base64 of the text XORed with the inner stream's next bytes.
fn unmask(text: &str, stream: &mut InnerStream) -> Result<Zeroizing<String>, KdbxError> {
    let compact: Zeroizing<String> = Zeroizing::new(text.chars().filter(|c| !c.is_ascii_whitespace()).collect());
    let mut bytes = Zeroizing::new(data_encoding::BASE64.decode(compact.as_bytes()).map_err(|_| KdbxError::Damaged)?);
    stream.apply(&mut bytes)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| KdbxError::Damaged)?;
    Ok(Zeroizing::new(text.to_owned()))
}

/// A UUID as KeePass writes it: Base64 of its 16 bytes.
fn uuid_of(text: &str) -> Option<Vec<u8>> {
    data_encoding::BASE64.decode(text.trim().as_bytes()).ok().filter(|bytes| bytes.len() == 16)
}

/// The strings that hold one-time codes: KeePassXC's `otp`, KeePass 2's `TimeOtp-*` and
/// `HmacOtp-*`, and the older `TOTP Seed` and `TOTP Settings`.
fn is_code_string(key: &str) -> bool {
    key == "otp" || key.starts_with("TimeOtp-") || key.starts_with("HmacOtp-") || key == "TOTP Seed" || key == "TOTP Settings"
}

fn login_of(entry: Entry, groups: &[Group]) -> Found {
    let mut login = Login::new(App::KeePass);
    let mut path = Vec::new();
    let mut group = Some(entry.group);
    while let Some(index) = group {
        // The root group is the database itself: its name is no folder.
        if groups[index].parent.is_some() {
            path.push(groups[index].name.clone());
        }
        group = groups[index].parent;
    }
    path.reverse();
    login.folder = Some(path.join("/")).filter(|p| !p.is_empty());
    login.tags = tags(&entry.tags);
    let mut codes: Vec<(String, Zeroizing<String>)> = Vec::new();
    // The entry's URL first, then KeePassXC's and KeePass2Android's additional ones by name (the
    // strings of an entry have no order of their own).
    let mut more_urls: Vec<(String, Zeroizing<String>)> = Vec::new();
    for (key, value, protected) in entry.strings {
        match key.as_str() {
            "Title" => login.title = value.trim().to_owned(),
            "UserName" => login.username = value.to_string(),
            "Password" => login.password = value,
            "URL" => login.urls.insert(0, crate::login::LoginUrl::plain(&value)),
            "Notes" => login.notes = value,
            _ if key.starts_with("KP2A_URL") => more_urls.push((key, value)),
            _ if is_code_string(&key) => codes.push((key, value)),
            _ => login.add_field(&key, &value, protected),
        }
    }
    login.urls.retain(|url| !url.url.is_empty());
    more_urls.sort_by_cached_key(|(key, _)| (url_rank(key), key.clone()));
    for (_, url) in more_urls {
        login.add_url(&url, None);
    }
    login.totp = code_of(&codes, &login.title, &login.username);
    Found::Login(Box::new(login))
}

/// Where a further URL comes: `KP2A_URL` first, then `KP2A_URL_1`, `KP2A_URL_2`, … by number
/// (so that the tenth comes after the ninth), then any other name.
fn url_rank(key: &str) -> u64 {
    match key.strip_prefix("KP2A_URL") {
        Some("") => 0,
        Some(rest) => rest.strip_prefix('_').and_then(|n| n.parse().ok()).unwrap_or(u64::MAX),
        None => u64::MAX,
    }
}

/// The one-time code of an entry, from the strings that hold one.
fn code_of(codes: &[(String, Zeroizing<String>)], title: &str, username: &str) -> Option<Item> {
    let get = |name: &str| codes.iter().find(|(key, _)| key == name).map(|(_, value)| value.trim().to_owned());
    let rejected = |reason| Some(Item::Rejected { label: display_label(title.trim(), username.trim()), line: None, reason });
    if let Some(otp) = get("otp").filter(|o| !o.is_empty()) {
        if otp.to_ascii_lowercase().starts_with("otpauth://") {
            return Some(match uri::parse(&otp) {
                Ok(auth) => Item::Account(named(auth, title, username)),
                Err(error) => Item::Rejected { label: display_label(title.trim(), username.trim()), line: None, reason: RejectReason::from(&error) },
            });
        }
        // KeeOtp's `key=…&size=6&step=30&otpHashMode=Sha1`.
        return Some(
            keeotp(&otp, title, username)
                .map_or_else(|reason| Item::Rejected { label: display_label(title.trim(), username.trim()), line: None, reason }, Item::Account),
        );
    }
    for (prefix, hotp) in [("TimeOtp-", false), ("HmacOtp-", true)] {
        let secret = match secret_of(codes, prefix) {
            None => continue,
            Some(Err(reason)) => return rejected(reason),
            Some(Ok(secret)) => secret,
        };
        let kind = if hotp {
            match get("HmacOtp-Counter").map(|c| c.parse::<u64>()) {
                None => OtpKind::Hotp { counter: 0 },
                Some(Ok(counter)) => OtpKind::Hotp { counter },
                Some(Err(_)) => return rejected(RejectReason::InvalidCounter),
            }
        } else {
            match get("TimeOtp-Period").map(|p| p.parse::<u32>().ok().and_then(|p| Period::new(p).ok())) {
                None => OtpKind::Totp { period: Period::THIRTY },
                Some(Some(period)) => OtpKind::Totp { period },
                Some(None) => return rejected(RejectReason::InvalidPeriod),
            }
        };
        let digits = match get(&format!("{prefix}Length")).map(|d| d.parse::<u8>().ok().and_then(|d| Digits::new(d).ok())) {
            None => Digits::SIX,
            Some(Some(digits)) => digits,
            Some(None) => return rejected(RejectReason::UnsupportedDigits),
        };
        let algorithm = match get("TimeOtp-Algorithm").filter(|_| !hotp).as_deref() {
            None | Some("HMAC-SHA-1") => Algorithm::Sha1,
            Some("HMAC-SHA-256") => Algorithm::Sha256,
            Some("HMAC-SHA-512") => Algorithm::Sha512,
            Some(_) => return rejected(RejectReason::UnknownAlgorithm),
        };
        let mut auth = account(secret, title, username);
        auth.kind = kind;
        auth.digits = digits;
        auth.algorithm = algorithm;
        return Some(Item::Account(auth));
    }
    if let Some(seed) = get("TOTP Seed").filter(|s| !s.is_empty()) {
        let Ok(secret) = base32::decode(&seed) else { return rejected(RejectReason::InvalidSecret) };
        let mut auth = account(secret, title, username);
        // `30;6`, or `30;S` for Steam's codes.
        if let Some(settings) = get("TOTP Settings") {
            let (period, digits) = settings.split_once(';').unwrap_or((settings.as_str(), "6"));
            let Some(period) = period.trim().parse::<u32>().ok().and_then(|p| Period::new(p).ok()) else { return rejected(RejectReason::InvalidPeriod) };
            let Some(digits) = digits.trim().parse::<u8>().ok().and_then(|d| Digits::new(d).ok()) else {
                return rejected(if digits.trim() == "S" { RejectReason::UnknownType } else { RejectReason::UnsupportedDigits });
            };
            auth.kind = OtpKind::Totp { period };
            auth.digits = digits;
        }
        return Some(Item::Account(auth));
    }
    None
}

/// [`code_of`] for an entry `Title` of user `user`.
#[cfg(test)]
pub(super) fn code_of_for_tests(codes: &[(String, Zeroizing<String>)]) -> Option<Item> {
    code_of(codes, "Title", "user")
}

/// KeePass 2's secret, in whichever of its encodings the entry keeps it.
fn secret_of(codes: &[(String, Zeroizing<String>)], prefix: &str) -> Option<Result<Zeroizing<Vec<u8>>, RejectReason>> {
    let get = |suffix: &str| codes.iter().find(|(key, _)| key.strip_prefix(prefix) == Some(suffix)).map(|(_, value)| value.trim().to_owned());
    let invalid = |_| RejectReason::InvalidSecret;
    let secret = if let Some(text) = get("Secret-Base32") {
        base32::decode(&text).map_err(invalid)
    } else if let Some(text) = get("Secret-Hex") {
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        data_encoding::HEXUPPER_PERMISSIVE.decode(compact.as_bytes()).map(Zeroizing::new).map_err(|_| RejectReason::InvalidSecret)
    } else if let Some(text) = get("Secret-Base64") {
        data_encoding::BASE64.decode(text.as_bytes()).map(Zeroizing::new).map_err(|_| RejectReason::InvalidSecret)
    } else {
        Ok(Zeroizing::new(get("Secret")?.into_bytes()))
    };
    Some(secret.and_then(|s| if s.is_empty() { Err(RejectReason::EmptySecret) } else { Ok(s) }))
}

/// KeeOtp's form of a code: `key=BASE32&size=6&step=30&otpHashMode=Sha1&type=Totp`.
fn keeotp(text: &str, title: &str, username: &str) -> Result<OtpAuth, RejectReason> {
    let mut auth: Option<OtpAuth> = None;
    let mut digits = Digits::SIX;
    let mut period = Period::THIRTY;
    let mut algorithm = Algorithm::Sha1;
    for pair in text.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "key" => auth = Some(account(base32::decode(value).map_err(|_| RejectReason::InvalidSecret)?, title, username)),
            "size" => digits = value.parse::<u8>().ok().and_then(|d| Digits::new(d).ok()).ok_or(RejectReason::UnsupportedDigits)?,
            "step" => period = value.parse::<u32>().ok().and_then(|p| Period::new(p).ok()).ok_or(RejectReason::InvalidPeriod)?,
            "otphashmode" => algorithm = Algorithm::from_uri_name(value).ok_or(RejectReason::UnknownAlgorithm)?,
            "type" if !value.eq_ignore_ascii_case("totp") => return Err(RejectReason::UnknownType),
            _ => {}
        }
    }
    let mut auth = auth.ok_or(RejectReason::NotOtpauth)?;
    auth.kind = OtpKind::Totp { period };
    auth.digits = digits;
    auth.algorithm = algorithm;
    Ok(auth)
}
