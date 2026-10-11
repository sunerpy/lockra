//! Text sources: a pasted string, or a `.txt` list with one `otpauth://` URI per line.

use lockra_otp::{OtpAuth, uri};
use zeroize::Zeroizing;

use crate::google::{self, Batch};
use crate::item::{Item, RejectReason};

/// What one line of text held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// An `otpauth://` URI, or a line that is no URI at all.
    Uri(Item),
    /// A Google Authenticator export code: its place in the export and its accounts.
    Google {
        /// Its place in the export.
        batch: Batch,
        /// The accounts.
        items: Vec<Item>,
    },
}

/// Every non-empty line of `text`; lines starting with `#` are comments.
pub fn read(text: &str) -> Vec<Line> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .map(|(index, line)| read_line(line.trim(), u32::try_from(index + 1).unwrap_or(u32::MAX)))
        .collect()
}

fn read_line(line: &str, number: u32) -> Line {
    if line.get(..20).is_some_and(|head| head.eq_ignore_ascii_case("otpauth-migration://")) {
        return match google::decode(line) {
            Ok(migration) => Line::Google { batch: migration.batch, items: migration.items },
            Err(_) => Line::Uri(Item::Rejected { label: String::new(), line: Some(number), reason: RejectReason::InvalidSecret }),
        };
    }
    match uri::parse(line) {
        Ok(auth) => Line::Uri(Item::Account(auth)),
        Err(error) => Line::Uri(Item::Rejected { label: String::new(), line: Some(number), reason: RejectReason::from(&error) }),
    }
}

/// The accounts as a list file: one URI per line, after a comment saying what the file is.
pub fn write(accounts: &[&OtpAuth]) -> Zeroizing<String> {
    let header = "# Lockra export: one otpauth:// URI per line. Anyone who reads this file can generate your codes.\n";
    let mut out = Zeroizing::new(String::with_capacity(header.len() + accounts.len() * 200));
    out.push_str(header);
    for auth in accounts {
        out.push_str(&auth.to_uri());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use lockra_otp::{Algorithm, Digits, OtpKind, Period};

    use super::*;

    #[test]
    fn lines_are_classified_and_numbered() {
        let migration = {
            let auth = uri::parse("otpauth://totp/G:a?secret=JBSWY3DPEHPK3PXP").unwrap();
            google::encode(&[&auth]).unwrap().remove(0).uri
        };
        let text = format!(
            "# comment\n\notpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP\nnot a uri\n  {}  \notpauth://totp/x?secret=GEZDGNBV&digits=12\r\n",
            migration.as_str()
        );
        let lines = read(&text);
        assert_eq!(lines.len(), 4);
        assert!(matches!(&lines[0], Line::Uri(Item::Account(a)) if a.issuer == "A"));
        assert_eq!(lines[1], Line::Uri(Item::Rejected { label: String::new(), line: Some(4), reason: RejectReason::NotOtpauth }));
        assert!(matches!(&lines[2], Line::Google { items, .. } if matches!(&items[0], Item::Account(a) if a.issuer == "G")));
        assert_eq!(lines[3], Line::Uri(Item::Rejected { label: String::new(), line: Some(6), reason: RejectReason::UnsupportedDigits }));
    }

    #[test]
    fn a_damaged_migration_line_is_rejected_without_echoing_it() {
        let lines = read("otpauth-migration://offline?data=%%%");
        assert_eq!(lines, [Line::Uri(Item::Rejected { label: String::new(), line: Some(1), reason: RejectReason::InvalidSecret })]);
    }

    #[test]
    fn written_lists_read_back() {
        let a = uri::parse("otpauth://totp/A:b?secret=JBSWY3DPEHPK3PXP").unwrap();
        let b = OtpAuth {
            kind: OtpKind::Hotp { counter: 3 },
            algorithm: Algorithm::Sha256,
            digits: Digits::EIGHT,
            secret: Zeroizing::new(vec![7; 20]),
            issuer: String::new(),
            account: "solo".into(),
        };
        let text = write(&[&a, &b]);
        assert!(text.starts_with("# Lockra export"));
        let back: Vec<OtpAuth> = read(&text).into_iter().map(|l| if let Line::Uri(Item::Account(x)) = l { x } else { panic!() }).collect();
        assert_eq!(back, [a, b]);
        let _ = Period::THIRTY;
    }
}
