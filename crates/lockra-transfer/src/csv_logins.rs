//! Browsers' and password managers' CSV exports (docs/formats.md §11), recognised by their header
//! row: Chrome and Edge, Firefox, Safari and Apple Passwords, Bitwarden, 1Password, LastPass; and
//! logins written as Bitwarden's CSV, which every one of them imports.

use zeroize::Zeroizing;

use crate::item::RejectReason;
use crate::login::{App, Found, Login, non_blank, tags, totp};

/// Which app wrote a CSV export, judged from its header row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsvFormat {
    /// `name,url,username,password[,note]`: Chrome and Edge.
    Chrome,
    /// `url,username,password,httpRealm,formActionOrigin,…`.
    Firefox,
    /// `Title,URL,Username,Password,Notes,OTPAuth` (older Safari: the first four, as `Url`).
    Safari,
    /// `folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,…`.
    Bitwarden,
    /// `Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes`.
    OnePassword,
    /// `url,username,password,totp,extra,name,grouping,fav`.
    LastPass,
}

impl CsvFormat {
    fn app(self) -> App {
        match self {
            Self::Chrome => App::Chrome,
            Self::Firefox => App::Firefox,
            Self::Safari => App::Safari,
            Self::Bitwarden => App::Bitwarden,
            Self::OnePassword => App::OnePassword,
            Self::LastPass => App::LastPass,
        }
    }
}

/// A CSV export that does not read (it never happens with what the apps write: every row is
/// taken, however many cells it has).
#[derive(Debug, thiserror::Error)]
#[error("the CSV does not read")]
pub struct CsvError;

/// The format of a CSV export, from its first row; `None` for text that is not one (an
/// `otpauth://` list goes on to be read as one).
pub fn recognize(bytes: &[u8]) -> Option<CsvFormat> {
    let mut rows = reader(bytes);
    let mut header = csv::ByteRecord::new();
    if !rows.read_byte_record(&mut header).ok()? {
        return None;
    }
    let raw: Vec<String> = header.iter().map(|cell| String::from_utf8_lossy(cell).trim().to_owned()).collect();
    let names: Vec<String> = raw.iter().map(|cell| cell.to_ascii_lowercase()).collect();
    let has = |name: &str| names.iter().any(|n| n == name);
    if has("name") && has("login_uri") && has("login_password") {
        return Some(CsvFormat::Bitwarden);
    }
    if has("url") && has("username") && has("password") && has("extra") && has("name") && has("grouping") {
        return Some(CsvFormat::LastPass);
    }
    if has("url") && has("username") && has("password") && (has("httprealm") || has("formactionorigin")) {
        return Some(CsvFormat::Firefox);
    }
    if has("title") && has("url") && has("username") && has("password") {
        // Both write `Title,…,OTPAuth`: 1Password spells its URL column `Url` and adds favourites,
        // the archive and tags; Safari spells it `URL` (older versions wrote four columns only).
        let onepassword = has("favorite") || has("archived") || has("tags") || (has("otpauth") && raw.iter().any(|cell| cell == "Url"));
        return Some(if onepassword { CsvFormat::OnePassword } else { CsvFormat::Safari });
    }
    if has("name") && has("url") && has("username") && has("password") {
        return Some(CsvFormat::Chrome);
    }
    None
}

/// The logins of a CSV export in `format`, and the items it leaves out (Bitwarden's and
/// LastPass's secure notes), row by row.
pub fn read(bytes: &[u8], format: CsvFormat) -> Result<Vec<Found>, CsvError> {
    let mut rows = reader(bytes);
    let mut record = csv::ByteRecord::new();
    if !rows.read_byte_record(&mut record).map_err(|_| CsvError)? {
        return Ok(Vec::new());
    }
    let columns: Vec<String> = record.iter().map(|cell| String::from_utf8_lossy(cell).trim().to_ascii_lowercase()).collect();
    let mut found = Vec::new();
    while rows.read_byte_record(&mut record).map_err(|_| CsvError)? {
        let row = Row { columns: &columns, record: &record };
        if row.is_blank() {
            continue;
        }
        if let Some(item) = read_row(&row, format) {
            found.push(item);
        }
    }
    Ok(found)
}

fn reader(bytes: &[u8]) -> csv::Reader<&[u8]> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    csv::ReaderBuilder::new().has_headers(false).flexible(true).from_reader(bytes)
}

/// One row, its cells found by the header's names (in lower case).
struct Row<'a> {
    columns: &'a [String],
    record: &'a csv::ByteRecord,
}

impl Row<'_> {
    /// The cell under `name`, as written (empty when the row has none).
    fn get(&self, name: &str) -> String {
        self.columns
            .iter()
            .position(|column| column == name)
            .and_then(|index| self.record.get(index))
            .map(|cell| String::from_utf8_lossy(cell).into_owned())
            .unwrap_or_default()
    }

    /// The first of `names` this row has a cell for.
    fn first(&self, names: &[&str]) -> String {
        names.iter().map(|name| self.get(name)).find(|cell| !cell.is_empty()).unwrap_or_default()
    }

    fn is_blank(&self) -> bool {
        self.record.iter().all(|cell| cell.iter().all(u8::is_ascii_whitespace))
    }
}

fn read_row(row: &Row<'_>, format: CsvFormat) -> Option<Found> {
    let mut login = Login::new(format.app());
    match format {
        CsvFormat::Chrome => {
            let url = row.get("url");
            login.title = non_blank(&row.get("name")).unwrap_or_else(|| host_of(&url));
            login.add_url(&url, None);
            login.username = row.get("username");
            login.password = row.get("password").into();
            login.notes = row.get("note").into();
        }
        CsvFormat::Firefox => {
            let url = row.get("url");
            // The Firefox account's own sign-in, not a site's.
            if url.trim().eq_ignore_ascii_case("chrome://FirefoxAccounts") {
                return None;
            }
            login.title = host_of(&url);
            login.add_url(&url, None);
            login.username = row.get("username");
            login.password = row.get("password").into();
        }
        CsvFormat::Safari | CsvFormat::OnePassword => {
            login.title = row.get("title").trim().to_owned();
            login.add_url(&row.get("url"), None);
            login.username = row.get("username");
            login.password = row.get("password").into();
            login.notes = row.get("notes").into();
            login.favorite = truthy(&row.get("favorite"));
            login.tags = tags(&row.get("tags"));
            login.totp = totp(&row.get("otpauth"), &login.title, &login.username);
        }
        CsvFormat::Bitwarden => {
            let name = row.get("name").trim().to_owned();
            if row.get("type").trim().eq_ignore_ascii_case("note") {
                return Some(Found::left(&name, RejectReason::NoteItem));
            }
            login.title = name;
            // An organization's export names its collections (the first of them, commas apart)
            // instead of a folder; a folder's name is taken whole.
            let folder = row.get("folder");
            login.folder = non_blank(&folder).or_else(|| row.get("collections").split(',').next().and_then(non_blank));
            login.favorite = truthy(&row.get("favorite"));
            login.reprompt = truthy(&row.get("reprompt"));
            login.notes = row.get("notes").into();
            // `login_uri` holds the URIs as a CSV row of its own.
            for uri in cells_of(&row.first(&["login_uri", "uri"])) {
                login.add_url(&uri, None);
            }
            login.username = row.first(&["login_username", "username"]);
            login.password = row.first(&["login_password", "password"]).into();
            // Custom fields, one `name: value` per line.
            for line in row.get("fields").lines() {
                if let Some((label, value)) = line.rsplit_once(": ") {
                    login.add_field(label, value, false);
                }
            }
            login.totp = totp(&row.first(&["login_totp", "totp"]), &login.title, &login.username);
        }
        CsvFormat::LastPass => {
            let name = row.get("name").trim().to_owned();
            let extra = row.get("extra");
            // A secure note is a row whose URL is `http://sn`; a card or an address is a note of
            // that `NoteType`.
            if row.get("url").trim().eq_ignore_ascii_case("http://sn") {
                return Some(Found::left(&name, note_reason(&extra)));
            }
            login.title = name;
            login.add_url(&row.get("url"), None);
            login.username = row.get("username");
            login.password = row.get("password").into();
            login.notes = extra.into();
            let grouping = row.get("grouping").replace('\\', "/");
            login.folder = non_blank(&grouping).filter(|g| g != "(none)");
            login.favorite = truthy(&row.get("fav"));
            login.totp = totp(&row.get("totp"), &login.title, &login.username);
        }
    }
    Some(Found::Login(Box::new(login)))
}

/// The header of Bitwarden's CSV export of a vault.
const BITWARDEN_HEADER: [&str; 11] =
    ["folder", "favorite", "type", "name", "notes", "fields", "reprompt", "login_uri", "login_username", "login_password", "login_totp"];

/// `logins` as Bitwarden's CSV export of a vault, as Bitwarden writes it (rows end with CRLF):
/// plain text, every secret readable. The custom fields go as `name: value` lines, the URLs
/// joined by commas, the code as its `otpauth://` URI; tags and which fields were hidden have no
/// column.
pub fn write_bitwarden(logins: &[Login]) -> Zeroizing<String> {
    // Room for every cell quoted and doubled, so that the text is not moved (and copied) as it
    // grows.
    let room: usize = logins
        .iter()
        .map(|l| {
            let fields: usize = l.fields.iter().map(|f| f.label.len() + f.value.len() + 3).sum();
            let urls: usize = l.urls.iter().map(|u| u.url.len() + 1).sum();
            l.title.len() + l.username.len() + l.password.len() + l.notes.len() + l.folder.as_ref().map_or(0, String::len) + fields + urls + 160
        })
        .sum();
    let mut out = Zeroizing::new(String::with_capacity(2 * room + 128));
    row(&mut out, BITWARDEN_HEADER.iter().copied());
    for login in logins {
        let mut fields = Zeroizing::new(String::new());
        for field in &login.fields {
            if !fields.is_empty() {
                fields.push('\n');
            }
            fields.push_str(&field.label);
            fields.push_str(": ");
            fields.push_str(&field.value);
        }
        let urls = login.urls.iter().map(|u| u.url.as_str()).collect::<Vec<_>>().join(",");
        let code = match &login.totp {
            Some(crate::item::Item::Account(auth)) => auth.to_uri(),
            _ => Zeroizing::new(String::new()),
        };
        let cells = [
            login.folder.as_deref().unwrap_or_default(),
            if login.favorite { "1" } else { "" },
            "login",
            &login.title,
            &login.notes,
            &fields,
            if login.reprompt { "1" } else { "0" },
            &urls,
            &login.username,
            &login.password,
            &code,
        ];
        row(&mut out, cells.into_iter());
    }
    out
}

/// One CSV row of `cells`, each quoted where it has to be, and its CRLF.
fn row<'a>(out: &mut String, cells: impl Iterator<Item = &'a str>) {
    for (index, cell) in cells.enumerate() {
        if index > 0 {
            out.push(',');
        }
        let quoted = cell.contains([',', '"', '\r', '\n']) || cell.starts_with(char::is_whitespace) || cell.ends_with(char::is_whitespace);
        if quoted {
            out.push('"');
            for c in cell.chars() {
                if c == '"' {
                    out.push('"');
                }
                out.push(c);
            }
            out.push('"');
        } else {
            out.push_str(cell);
        }
    }
    out.push_str("\r\n");
}

/// `1`, `true` or `yes` (any case).
fn truthy(cell: &str) -> bool {
    matches!(cell.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes")
}

/// The cells of `text` read as one CSV row.
fn cells_of(text: &str) -> Vec<String> {
    let mut rows = csv::ReaderBuilder::new().has_headers(false).flexible(true).from_reader(text.as_bytes());
    let mut record = csv::StringRecord::new();
    match rows.read_record(&mut record) {
        Ok(true) => record.iter().map(str::to_owned).collect(),
        _ => Vec::new(),
    }
}

/// What a LastPass note is, by the `NoteType:` its text starts with.
fn note_reason(extra: &str) -> RejectReason {
    let kind = extra.lines().next().and_then(|line| line.strip_prefix("NoteType:")).map(str::trim).unwrap_or_default().to_ascii_lowercase();
    match kind.as_str() {
        "credit card" | "bank account" => RejectReason::CardItem,
        "address" | "driver's license" | "passport" | "social security" | "insurance" | "health insurance" | "membership" => RejectReason::IdentityItem,
        "" => RejectReason::NoteItem,
        _ => RejectReason::OtherItem,
    }
}

/// A login's name when its source gives none: the URL's host (an Android app's package for an
/// `android://…@com.example.app/` address).
fn host_of(url: &str) -> String {
    let rest = url.trim().split_once("://").map_or(url.trim(), |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let host = match host.find(']') {
        Some(end) if host.starts_with('[') => &host[..=end],
        _ => host.split(':').next().unwrap_or_default(),
    };
    host.to_owned()
}

#[cfg(test)]
mod tests;
