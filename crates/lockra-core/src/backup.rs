//! Automatic backups: their file names (UTC, sortable) and pruning to the configured count.

use std::fs;
use std::io;
use std::path::Path;

/// Prefix of every automatic backup's file name.
pub const AUTO_PREFIX: &str = "lockra-auto-";
/// Extension of every backup.
pub const BACKUP_EXTENSION: &str = "lockrabackup";

/// `lockra-auto-YYYYMMDD-HHMMSS.lockrabackup` for `now_ms` (UTC), with `-2`, `-3`, … appended
/// while `taken` says the name exists.
pub fn auto_file_name(now_ms: u64, taken: impl Fn(&str) -> bool) -> String {
    let stamp = utc_stamp(now_ms);
    let base = format!("{AUTO_PREFIX}{stamp}.{BACKUP_EXTENSION}");
    if !taken(&base) {
        return base;
    }
    (2..).map(|n| format!("{AUTO_PREFIX}{stamp}-{n}.{BACKUP_EXTENSION}")).find(|name| !taken(name)).unwrap_or(base)
}

/// `pre-restore-YYYYMMDD-HHMMSS.lockrabackup`: the vault as it was before a replacing restore.
pub fn pre_restore_file_name(now_ms: u64) -> String {
    format!("pre-restore-{}.{BACKUP_EXTENSION}", utc_stamp(now_ms))
}

/// Whether `name` is one of Lockra's automatic backups (only those are ever pruned).
pub fn is_auto_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(AUTO_PREFIX).and_then(|r| r.strip_suffix(&format!(".{BACKUP_EXTENSION}"))) else { return false };
    let (stamp, counter) = match rest.get(15..) {
        Some("") => (rest, None),
        Some(tail) => (&rest[..15], tail.strip_prefix('-')),
        None => return false,
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    stamp.len() == 15 && digits(&stamp[..8]) && stamp.as_bytes()[8] == b'-' && digits(&stamp[9..]) && counter.is_none_or(digits)
}

/// Delete the oldest automatic backups in `dir` beyond `keep`; returns the names removed. Files
/// that are not automatic backups are never touched.
pub fn prune(dir: &Path, keep: usize) -> io::Result<Vec<String>> {
    let mut names: Vec<String> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_auto_name(n))
        .collect();
    names.sort_by_key(|name| sort_key(name));
    let excess = names.len().saturating_sub(keep);
    let removed: Vec<String> = names.into_iter().take(excess).collect();
    for name in &removed {
        fs::remove_file(dir.join(name))?;
    }
    Ok(removed)
}

/// Timestamp, then the numeric counter (`-10` after `-9`).
fn sort_key(name: &str) -> (String, u64) {
    let rest = &name[AUTO_PREFIX.len()..name.len() - BACKUP_EXTENSION.len() - 1];
    let counter = rest.get(16..).and_then(|c| c.parse().ok()).unwrap_or(1);
    (rest[..15].to_owned(), counter)
}

/// `YYYYMMDD-HHMMSS` in UTC.
pub fn utc_stamp(unix_ms: u64) -> String {
    let secs = unix_ms / 1000;
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (year, month, day) = civil_from_days(i64::try_from(days).unwrap_or(0));
    format!("{year:04}{month:02}{day:02}-{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_are_utc() {
        assert_eq!(utc_stamp(0), "19700101-000000");
        assert_eq!(utc_stamp(1_790_812_530_000), "20260930-235530");
        assert_eq!(utc_stamp(951_782_400_000), "20000229-000000");
        assert_eq!(utc_stamp(4_102_444_799_000), "20991231-235959");
    }

    #[test]
    fn names_are_unique_and_recognized() {
        let first = auto_file_name(0, |_| false);
        assert_eq!(first, "lockra-auto-19700101-000000.lockrabackup");
        assert_eq!(auto_file_name(0, |n| n == first), "lockra-auto-19700101-000000-2.lockrabackup");
        for good in ["lockra-auto-20261001-081530.lockrabackup", "lockra-auto-20261001-081530-12.lockrabackup"] {
            assert!(is_auto_name(good), "{good}");
        }
        for bad in [
            "lockra-auto-2026101-081530.lockrabackup",
            "lockra-auto-20261001-081530.lockrabackup.tmp",
            "my-lockra-auto-20261001-081530.lockrabackup",
            "lockra-auto-20261001-081530-.lockrabackup",
            "lockra-auto-20261001x081530.lockrabackup",
            "notes.txt",
        ] {
            assert!(!is_auto_name(bad), "{bad}");
        }
        assert_eq!(pre_restore_file_name(0), "pre-restore-19700101-000000.lockrabackup");
    }

    #[test]
    fn prune_keeps_the_newest_and_touches_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let names = [
            "lockra-auto-20260101-000000.lockrabackup",
            "lockra-auto-20260102-000000.lockrabackup",
            "lockra-auto-20260102-000000-2.lockrabackup",
            "lockra-auto-20260102-000000-10.lockrabackup",
            "lockra-auto-20260103-000000.lockrabackup",
            "manual.lockrabackup",
            "lockra-auto-notes.txt",
        ];
        for name in names {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        fs::create_dir(dir.path().join("lockra-auto-20250101-000000.lockrabackup")).unwrap();
        let removed = prune(dir.path(), 2).unwrap();
        assert_eq!(
            removed,
            ["lockra-auto-20260101-000000.lockrabackup", "lockra-auto-20260102-000000.lockrabackup", "lockra-auto-20260102-000000-2.lockrabackup"]
        );
        let mut left: Vec<String> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(
            left,
            [
                "lockra-auto-20250101-000000.lockrabackup",
                "lockra-auto-20260102-000000-10.lockrabackup",
                "lockra-auto-20260103-000000.lockrabackup",
                "lockra-auto-notes.txt",
                "manual.lockrabackup"
            ]
        );
        assert!(prune(&dir.path().join("missing"), 1).is_err());
    }
}
