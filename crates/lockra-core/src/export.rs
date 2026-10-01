//! Export sessions: the QR codes of an export, built once after the master password was entered
//! again, held in memory while the user pages through them, dropped when closed, when idle for
//! [`EXPORT_IDLE`], or when the vault locks.

use std::time::Duration;

use lockra_otp::OtpAuth;
use lockra_transfer::{Incompatible, google, microsoft, qr};
use tokio::time::Instant;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::entry::Entry;
use crate::error::{CoreResult, ErrorCode};
use crate::ui::{Excluded, ExportTarget};

/// How long an export session lives without a page being shown.
pub const EXPORT_IDLE: Duration = Duration::from_secs(120);

pub(crate) struct Page {
    pub(crate) svg: Zeroizing<String>,
    pub(crate) entry_ids: Vec<Uuid>,
}

pub(crate) struct ExportSession {
    pub(crate) id: Uuid,
    pub(crate) pages: Vec<Page>,
    pub(crate) expires_at: Instant,
}

/// The pages for `entries` and the entries left out. CPU work (QR encoding): run off the runtime.
pub(crate) fn build(target: ExportTarget, entries: &[Entry]) -> CoreResult<(Vec<Page>, Vec<Excluded>)> {
    let mut excluded = Vec::new();
    let check = match target {
        ExportTarget::Google => google::exportable,
        ExportTarget::Microsoft => microsoft::exportable,
    };
    let mut fit: Vec<(&Entry, OtpAuth)> = Vec::new();
    for entry in entries {
        let auth = entry.to_auth();
        match check(&auth) {
            Ok(()) => fit.push((entry, auth)),
            Err(reason) => excluded.push(Excluded { entry_id: entry.id, reason }),
        }
    }
    let pages = match target {
        ExportTarget::Google => loop {
            let refs: Vec<&OtpAuth> = fit.iter().map(|(_, a)| a).collect();
            match google::encode(&refs) {
                Ok(codes) => {
                    break codes
                        .into_iter()
                        .map(|code| {
                            let svg = qr::svg(&code.uri).map_err(|_| ErrorCode::Internal)?;
                            Ok(Page { svg, entry_ids: code.accounts.iter().map(|&i| fit[i].0.id).collect() })
                        })
                        .collect::<CoreResult<Vec<_>>>()?;
                }
                Err((index, reason)) => {
                    let (entry, _) = fit.remove(index);
                    excluded.push(Excluded { entry_id: entry.id, reason });
                }
            }
        },
        ExportTarget::Microsoft => {
            let mut pages = Vec::new();
            for (entry, auth) in &fit {
                match qr::svg(&auth.to_uri()) {
                    Ok(svg) => pages.push(Page { svg, entry_ids: vec![entry.id] }),
                    Err(_) => excluded.push(Excluded { entry_id: entry.id, reason: Incompatible::TooLarge }),
                }
            }
            pages
        }
    };
    if pages.is_empty() {
        return Err(ErrorCode::ExportNothing.into());
    }
    Ok((pages, excluded))
}

#[cfg(test)]
mod tests {
    use lockra_otp::uri;
    use lockra_transfer::Origin;

    use super::*;

    fn entry(text: &str) -> Entry {
        Entry::from_auth(uri::parse(text).unwrap(), Origin::Uri, 1)
    }

    #[test]
    fn google_pages_carry_their_entries_and_exclusions_are_named() {
        let entries = vec![
            entry("otpauth://totp/A:a?secret=GEZDGNBV"),
            entry("otpauth://totp/B:b?secret=GEZDGNBV&period=60"),
            entry(&format!("otpauth://totp/{}:c?secret=GEZDGNBV", "x".repeat(150))),
        ];
        let (pages, excluded) = build(ExportTarget::Google, &entries).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].entry_ids, [entries[0].id, entries[2].id]);
        assert!(pages[0].svg.contains("<svg"));
        assert_eq!(excluded, [Excluded { entry_id: entries[1].id, reason: Incompatible::PeriodNot30 }]);
    }

    #[test]
    fn microsoft_gets_one_page_per_compatible_entry() {
        let entries = vec![
            entry("otpauth://totp/A:a?secret=GEZDGNBV"),
            entry("otpauth://totp/B:b?secret=GEZDGNBV&digits=8"),
            entry("otpauth://totp/C:c?secret=MZXW6YTBOI"),
        ];
        let (pages, excluded) = build(ExportTarget::Microsoft, &entries).unwrap();
        assert_eq!(pages.iter().map(|p| p.entry_ids[0]).collect::<Vec<_>>(), [entries[0].id, entries[2].id]);
        assert_eq!(excluded, [Excluded { entry_id: entries[1].id, reason: Incompatible::DigitsNot6 }]);
    }

    #[test]
    fn nothing_exportable_is_an_error() {
        let entries = vec![entry("otpauth://hotp/A:a?secret=GEZDGNBV")];
        assert_eq!(build(ExportTarget::Microsoft, &entries).err().map(|e| e.code), Some(ErrorCode::ExportNothing));
        assert_eq!(build(ExportTarget::Google, &[]).err().map(|e| e.code), Some(ErrorCode::ExportNothing));
    }
}
