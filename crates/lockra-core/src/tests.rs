//! Behaviour tests of the core on the fakes. Time is tokio's paused clock: `FakeClock` follows it,
//! so `advance` moves the timers and the codes together.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use lockra_otp::{Algorithm, Digits, OtpKind, Period, hotp, totp, uri};
use lockra_vault::{FileKind, KdfCost, Sealed, read_header};
use tokio::sync::broadcast;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::fakes::{FakeClipboard, FakeClock, FakeKeychain, FakeTransport, FakeUpdater, RecordingSink};
use crate::ports::{ClipboardImage, SecretStore, SyncTransport};
use crate::settings::{AutoBackup, Settings, ThemeId};
use crate::ui::{CandidateAction, CandidateStatus, ExportTarget, Notice, Phase, Platform, UiEvent};
use crate::{Choice, Core, CoreConfig, EntryDraft, EntryPatch, ErrorCode, Outcome, Ports, RestoreMode, VAULT_FILE};

/// 2026-09-21T13:46:40Z: an arbitrary moment well inside a 30-second window.
const T0: u64 = 1_790_000_000_000;
const MASTER: &str = "correct horse battery";
const SECRET: &str = "JBSWY3DPEHPK3PXP";

mod sync;
mod update;

struct Harness {
    core: Core,
    keychain: Arc<FakeKeychain>,
    clipboard: Arc<FakeClipboard>,
    updater: Arc<FakeUpdater>,
    sync: Arc<dyn SyncTransport>,
    events: broadcast::Receiver<UiEvent>,
    dir: tempfile::TempDir,
}

impl Harness {
    fn data_dir(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    fn vault_path(&self) -> PathBuf {
        self.data_dir().join(VAULT_FILE)
    }

    /// A second core on the same directories, as after a restart.
    fn restart(&self) -> Core {
        start(self.dir.path(), Arc::clone(&self.keychain), Arc::clone(&self.clipboard), Arc::clone(&self.updater), Arc::clone(&self.sync))
    }

    /// Every notice received since the last call.
    fn notices(&mut self) -> Vec<Notice> {
        let mut out = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            if let UiEvent::Notice { notice } = event {
                out.push(notice);
            }
        }
        out
    }

    async fn unlocked() -> Self {
        let h = harness();
        h.core.create_vault(pw(MASTER)).await.unwrap();
        h
    }
}

fn start(root: &Path, keychain: Arc<FakeKeychain>, clipboard: Arc<FakeClipboard>, updater: Arc<FakeUpdater>, sync: Arc<dyn SyncTransport>) -> Core {
    let config = CoreConfig {
        data_dir: root.join("data"),
        config_dir: root.join("config"),
        app_version: "0.1.0-test".into(),
        kdf: KdfCost::FAST_INSECURE,
        platform: Platform::Linux,
    };
    Core::start(config, Ports { secrets: keychain, clipboard, clock: Arc::new(FakeClock::new(T0)), updater, sync })
}

fn harness() -> Harness {
    harness_with(FakeUpdater::default())
}

/// A core whose update source is `updater`.
fn harness_with(updater: FakeUpdater) -> Harness {
    harness_on(updater, Arc::new(FakeTransport::default()))
}

/// A core whose update source is `updater` and whose sync storage is `sync`.
fn harness_on(updater: FakeUpdater, sync: Arc<dyn SyncTransport>) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let keychain = Arc::new(FakeKeychain::default());
    let clipboard = Arc::new(FakeClipboard::default());
    let updater = Arc::new(updater);
    let core = start(dir.path(), Arc::clone(&keychain), Arc::clone(&clipboard), Arc::clone(&updater), Arc::clone(&sync));
    let events = core.subscribe();
    Harness { core, keychain, clipboard, updater, sync, events, dir }
}

fn pw(text: &str) -> Zeroizing<String> {
    Zeroizing::new(text.to_owned())
}

fn otpauth(issuer: &str, account: &str, secret: &str) -> String {
    format!("otpauth://totp/{issuer}:{account}?secret={secret}&issuer={issuer}")
}

/// Let the scheduler run what the last `advance` made due.
async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

async fn advance(duration: Duration) {
    tokio::time::advance(duration).await;
    settle().await;
}

fn code_err<T: std::fmt::Debug>(result: Result<T, crate::CoreError>) -> ErrorCode {
    result.unwrap_err().code
}

fn png_of_qr(data: &str) -> Vec<u8> {
    let code = qrcode::QrCode::new(data.as_bytes()).unwrap();
    let image = code.render::<image::Luma<u8>>().module_dimensions(6, 6).build();
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(image).write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

// ---- lifecycle ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn create_unlock_and_lock() {
    let h = harness();
    assert_eq!(h.core.state().phase, Phase::NoVault);
    assert_eq!(code_err(h.core.unlock(pw(MASTER)).await), ErrorCode::NoVault);
    assert_eq!(code_err(h.core.create_vault(pw("short")).await), ErrorCode::PasswordTooShort);
    h.core.create_vault(pw(MASTER)).await.unwrap();
    assert_eq!(h.core.state().phase, Phase::Unlocked);
    assert_eq!(read_header(&fs::read(h.vault_path()).unwrap()).unwrap().kind, FileKind::Vault);
    assert_eq!(code_err(h.core.create_vault(pw(MASTER)).await), ErrorCode::VaultExists);
    h.core.lock_vault();
    assert_eq!(h.core.state().phase, Phase::Locked);
    let wrong = h.core.unlock(pw("not the password")).await.unwrap_err();
    assert_eq!((wrong.code, wrong.retry_at_ms), (ErrorCode::WrongPassword, None));
    assert_eq!(h.core.state().lock.failed_attempts, 1);
    h.core.unlock(pw(MASTER)).await.unwrap();
    let state = h.core.state();
    assert_eq!((state.phase, state.lock.failed_attempts), (Phase::Unlocked, 0));
    h.core.unlock(pw("ignored while unlocked")).await.unwrap();
    assert_eq!(h.restart().state().phase, Phase::Locked);
}

#[tokio::test(start_paused = true)]
async fn wrong_passwords_wait_after_three() {
    let h = Harness::unlocked().await;
    h.core.lock_vault();
    for attempt in 1..=3 {
        let error = h.core.unlock(pw("guess")).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::WrongPassword, "attempt {attempt}");
        assert_eq!(error.retry_at_ms.is_some(), attempt == 3, "attempt {attempt}");
    }
    let limited = h.core.unlock(pw(MASTER)).await.unwrap_err();
    assert_eq!(limited.code, ErrorCode::RateLimited);
    assert_eq!(limited.retry_at_ms, h.core.state().lock.retry_at_ms);
    advance(Duration::from_millis(1000)).await;
    let fourth = h.core.unlock(pw("guess")).await.unwrap_err();
    assert_eq!(fourth.code, ErrorCode::WrongPassword);
    advance(Duration::from_millis(1999)).await;
    assert_eq!(code_err(h.core.unlock(pw(MASTER)).await), ErrorCode::RateLimited);
    advance(Duration::from_millis(1)).await;
    h.core.unlock(pw(MASTER)).await.unwrap();
    assert_eq!(h.core.state().lock.retry_at_ms, None);
}

#[tokio::test(start_paused = true)]
async fn a_damaged_vault_is_reported_not_lost() {
    let h = Harness::unlocked().await;
    h.core.lock_vault();
    let mut bytes = fs::read(h.vault_path()).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(h.vault_path(), &bytes).unwrap();
    assert_eq!(code_err(h.core.unlock(pw(MASTER)).await), ErrorCode::VaultCorrupted);
    fs::write(h.vault_path(), b"LKRAVLT1garbage").unwrap();
    let restarted = h.restart();
    assert_eq!(restarted.state().phase, Phase::Locked);
    assert_eq!(code_err(restarted.unlock(pw(MASTER)).await), ErrorCode::VaultCorrupted);
    assert_eq!(code_err(restarted.unlock_with_device().await), ErrorCode::DeviceUnlockOff);
}

#[tokio::test(start_paused = true)]
async fn reset_moves_the_vault_aside() {
    let h = Harness::unlocked().await;
    h.core.enable_device_unlock().unwrap();
    let vault_id = read_header(&fs::read(h.vault_path()).unwrap()).unwrap().vault_id;
    h.core.add_uri(&otpauth("A", "a", SECRET)).unwrap();
    assert_eq!(code_err(h.core.reset_vault()), ErrorCode::NoVault, "only a locked vault is reset");
    h.core.lock_vault();
    h.core.reset_vault().unwrap();
    assert_eq!(h.core.state().phase, Phase::NoVault);
    assert!(h.keychain.get(&vault_id.to_string()).unwrap().is_none());
    let names: Vec<String> = fs::read_dir(h.data_dir()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    assert!(names.iter().any(|n| n.starts_with("vault.lockra.reset-") && !n.ends_with(".prev")), "{names:?}");
    assert!(names.iter().any(|n| n.starts_with("vault.lockra.reset-") && n.ends_with(".prev")), "{names:?}");
    h.core.create_vault(pw("another password")).await.unwrap();
    assert!(h.core.state().entries.is_empty());
}

// ---- remember on this device ------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn device_unlock_round_trip() {
    let h = Harness::unlocked().await;
    h.core.enable_device_unlock().unwrap();
    assert!(h.core.state().lock.device_unlock.enabled);
    assert!(read_header(&fs::read(h.vault_path()).unwrap()).unwrap().has_device_slot);
    h.core.lock_vault();
    h.core.unlock_with_device().await.unwrap();
    assert_eq!(h.core.state().phase, Phase::Unlocked);
    assert!(h.restart().state().lock.device_unlock.enabled, "known from the header while locked");
    assert_eq!(code_err(h.core.disable_device_unlock(pw("wrong")).await), ErrorCode::WrongPassword);
    h.core.disable_device_unlock(pw(MASTER)).await.unwrap();
    let vault_id = read_header(&fs::read(h.vault_path()).unwrap()).unwrap().vault_id;
    assert!(h.keychain.get(&vault_id.to_string()).unwrap().is_none());
    assert_eq!(code_err(h.core.disable_device_unlock(pw(MASTER)).await), ErrorCode::DeviceUnlockOff);
    h.core.lock_vault();
    assert_eq!(code_err(h.core.unlock_with_device().await), ErrorCode::DeviceUnlockOff);
}

#[tokio::test(start_paused = true)]
async fn device_unlock_reports_every_keychain_failure() {
    let h = Harness::unlocked().await;
    h.core.enable_device_unlock().unwrap();
    let account = read_header(&fs::read(h.vault_path()).unwrap()).unwrap().vault_id.to_string();
    h.core.lock_vault();
    h.keychain.fail_get.store(true, Ordering::SeqCst);
    assert_eq!(code_err(h.core.unlock_with_device().await), ErrorCode::KeychainFailed);
    h.keychain.fail_get.store(false, Ordering::SeqCst);
    let key = h.keychain.get(&account).unwrap().unwrap();
    h.keychain.delete(&account).unwrap();
    assert_eq!(code_err(h.core.unlock_with_device().await), ErrorCode::DeviceKeyMissing);
    h.keychain.set(&account, "AAAA").unwrap();
    assert_eq!(code_err(h.core.unlock_with_device().await), ErrorCode::DeviceKeyStale);
    let other = Sealed::create(b"x", KdfCost::FAST_INSECURE, 0).unwrap().enable_device().unwrap();
    h.keychain.set(&account, &other.to_text()).unwrap();
    assert_eq!(code_err(h.core.unlock_with_device().await), ErrorCode::DeviceKeyStale);
    h.keychain.set(&account, &key).unwrap();
    h.keychain.unavailable.store(true, Ordering::SeqCst);
    assert_eq!(code_err(h.core.unlock_with_device().await), ErrorCode::KeychainUnavailable);
    assert!(!h.core.state().lock.device_unlock.available);
    h.keychain.unavailable.store(false, Ordering::SeqCst);
    h.core.unlock_with_device().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn enabling_device_unlock_changes_nothing_when_the_keychain_fails() {
    let h = Harness::unlocked().await;
    h.keychain.fail_set.store(true, Ordering::SeqCst);
    assert_eq!(code_err(h.core.enable_device_unlock()), ErrorCode::KeychainFailed);
    assert!(!h.core.state().lock.device_unlock.enabled);
    assert!(!read_header(&fs::read(h.vault_path()).unwrap()).unwrap().has_device_slot);
    h.keychain.fail_set.store(false, Ordering::SeqCst);
    h.keychain.unavailable.store(true, Ordering::SeqCst);
    assert_eq!(code_err(h.core.enable_device_unlock()), ErrorCode::KeychainUnavailable);
}

#[tokio::test(start_paused = true)]
async fn changing_the_password_keeps_or_drops_device_unlock() {
    let mut h = Harness::unlocked().await;
    h.core.enable_device_unlock().unwrap();
    assert_eq!(code_err(h.core.change_password(pw("wrong"), pw("new password")).await), ErrorCode::WrongPassword);
    assert_eq!(code_err(h.core.change_password(pw(MASTER), pw("short")).await), ErrorCode::PasswordTooShort);
    h.core.change_password(pw(MASTER), pw("new password")).await.unwrap();
    h.core.lock_vault();
    assert_eq!(code_err(h.core.unlock(pw(MASTER)).await), ErrorCode::WrongPassword);
    h.core.unlock(pw("new password")).await.unwrap();
    h.core.lock_vault();
    h.core.unlock_with_device().await.unwrap();
    h.notices();
    h.keychain.fail_get.store(true, Ordering::SeqCst);
    h.core.change_password(pw("new password"), pw("newer password")).await.unwrap();
    h.keychain.fail_get.store(false, Ordering::SeqCst);
    assert!(h.notices().contains(&Notice::DeviceUnlockTurnedOff));
    assert!(!h.core.state().lock.device_unlock.enabled);
    assert!(!read_header(&fs::read(h.vault_path()).unwrap()).unwrap().has_device_slot);
}

// ---- entries ----------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn entries_are_added_edited_deleted_and_kept() {
    let h = Harness::unlocked().await;
    let github = h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    assert_eq!(code_err(h.core.add_uri(&otpauth("Renamed", "x", SECRET))), ErrorCode::DuplicateEntry);
    assert_eq!(code_err(h.core.add_uri("https://example.com")), ErrorCode::InvalidUri);
    assert_eq!(code_err(h.core.add_uri("otpauth://totp/x?secret=GEZDGNBV&digits=12")), ErrorCode::InvalidParameters);
    assert_eq!(code_err(h.core.add_uri("otpauth://totp/x?secret=1")), ErrorCode::InvalidSecret);
    let draft = EntryDraft {
        issuer: " Mail ".into(),
        account: "me@example.com".into(),
        secret: pw("gezd gnbv gy3t qojq"),
        kind: OtpKind::Totp { period: Period::new(60).unwrap() },
        algorithm: Algorithm::Sha256,
        digits: Digits::EIGHT,
        group: Some(" Work ".into()),
    };
    let mail = h.core.add_manual(draft.clone()).unwrap();
    assert_eq!(code_err(h.core.add_manual(EntryDraft { secret: pw("not base32!"), ..draft })), ErrorCode::InvalidSecret);
    h.core
        .update_entry(
            github,
            EntryPatch { issuer: Some("GitHub Enterprise".into()), favorite: Some(true), group: Some(String::new()), ..EntryPatch::default() },
        )
        .unwrap();
    assert_eq!(code_err(h.core.update_entry(Uuid::new_v4(), EntryPatch::default())), ErrorCode::EntryNotFound);
    let third = h.core.add_uri(&otpauth("Bank", "me", "MZXW6YTBOI")).unwrap();
    h.core.delete_entry(third).unwrap();
    assert_eq!(code_err(h.core.delete_entry(third)), ErrorCode::EntryNotFound);
    h.core.lock_vault();
    let restarted = h.restart();
    restarted.unlock(pw(MASTER)).await.unwrap();
    let entries = restarted.state().entries;
    assert_eq!(entries.len(), 2);
    let gh = entries.iter().find(|e| e.id == github).unwrap();
    assert_eq!((gh.issuer.as_str(), gh.favorite, gh.group.as_deref()), ("GitHub Enterprise", true, None));
    let m = entries.iter().find(|e| e.id == mail).unwrap();
    assert_eq!((m.issuer.as_str(), m.group.as_deref(), m.digits, m.algorithm), ("Mail", Some("Work"), Digits::EIGHT, Algorithm::Sha256));
    assert_eq!(m.origin, lockra_transfer::Origin::Manual);
}

#[tokio::test(start_paused = true)]
async fn commands_need_an_unlocked_vault() {
    let h = harness();
    assert_eq!(code_err(h.core.add_uri(&otpauth("A", "a", SECRET))), ErrorCode::NoVault);
    h.core.create_vault(pw(MASTER)).await.unwrap();
    h.core.lock_vault();
    assert_eq!(code_err(h.core.add_uri(&otpauth("A", "a", SECRET))), ErrorCode::Locked);
    assert_eq!(code_err(h.core.copy_code(Uuid::new_v4())), ErrorCode::Locked);
    assert_eq!(code_err(h.core.import_text("x")), ErrorCode::Locked);
    assert_eq!(code_err(h.core.enable_device_unlock()), ErrorCode::Locked);
    assert_eq!(code_err(h.core.backup_auto_now()), ErrorCode::Locked);
    assert!(h.core.state().entries.is_empty());
}

// ---- codes ------------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn codes_follow_the_windows_and_the_lock() {
    let h = Harness::unlocked().await;
    let id = h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    let secret = lockra_otp::base32::decode(SECRET).unwrap();
    let sink = Arc::new(RecordingSink::default());
    h.core.subscribe_codes(sink.clone());
    let first = sink.frames().last().cloned().unwrap();
    assert_eq!(first.codes.len(), 1);
    let code = &first.codes[0];
    assert_eq!(code.entry_id, id);
    assert_eq!(code.code, totp(&secret, first.at_ms, Period::THIRTY, Algorithm::Sha1, Digits::SIX));
    let until = code.valid_until_ms.unwrap();
    assert_eq!(code.next_code.as_deref(), Some(totp(&secret, until, Period::THIRTY, Algorithm::Sha1, Digits::SIX).as_str()));
    let frames_before = sink.frames().len();
    advance(Duration::from_millis(until - first.at_ms + 1)).await;
    let later = sink.frames();
    assert!(later.len() > frames_before, "a frame at the window boundary");
    let at_boundary = later.last().unwrap();
    assert_eq!(at_boundary.codes[0].valid_from_ms, Some(until));
    assert_eq!(Some(at_boundary.codes[0].code.clone()), code.next_code);
    h.core.lock_vault();
    assert!(sink.frames().last().unwrap().codes.is_empty(), "locked: no codes");
    sink.closed.store(true, Ordering::SeqCst);
    h.core.unlock(pw(MASTER)).await.unwrap();
    let count = sink.frames().len();
    sink.closed.store(false, Ordering::SeqCst);
    advance(Duration::from_secs(90)).await;
    assert_eq!(sink.frames().len(), count, "a failed send ends the subscription");
    let replacement = Arc::new(RecordingSink::default());
    h.core.subscribe_codes(replacement.clone());
    assert_eq!(replacement.frames().len(), 1);
    h.core.unsubscribe_codes();
    advance(Duration::from_secs(60)).await;
    assert_eq!(replacement.frames().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn the_hotp_counter_is_saved_before_the_new_code_shows() {
    let h = Harness::unlocked().await;
    let id = h.core.add_uri("otpauth://hotp/Bank:me?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&counter=0").unwrap();
    let totp_id = h.core.add_uri(&otpauth("A", "a", SECRET)).unwrap();
    let sink = Arc::new(RecordingSink::default());
    h.core.subscribe_codes(sink.clone());
    let code_of = |frames: &[crate::ui::CodesFrame]| frames.last().unwrap().codes.iter().find(|c| c.entry_id == id).unwrap().clone();
    let first = code_of(&sink.frames());
    assert_eq!((first.code.as_str(), first.next_code.as_deref(), first.valid_until_ms), ("755224", None, None));
    h.core.hotp_next(id).unwrap();
    assert_eq!(code_of(&sink.frames()).code, "287082");
    let restarted = h.restart();
    restarted.unlock(pw(MASTER)).await.unwrap();
    assert_eq!(restarted.state().entries.iter().find(|e| e.id == id).unwrap().kind, OtpKind::Hotp { counter: 1 });
    assert_eq!(code_err(h.core.hotp_next(totp_id)), ErrorCode::InvalidParameters);
    assert_eq!(hotp(b"12345678901234567890", 1, Algorithm::Sha1, Digits::SIX), "287082");
}

#[tokio::test(start_paused = true)]
async fn copied_codes_are_cleared_only_while_unchanged() {
    let mut h = Harness::unlocked().await;
    let id = h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    h.notices();
    h.core.copy_code(id).unwrap();
    let code = h.clipboard.current().unwrap();
    assert_eq!(code.len(), 6);
    assert_eq!(h.clipboard.secret_writes.load(Ordering::SeqCst), 1);
    assert!(h.notices().contains(&Notice::Copied { entry_id: id, clear_after_s: Some(30) }));
    assert!(h.core.state().entries[0].last_used_at_ms.is_some());
    advance(Duration::from_secs(29)).await;
    assert_eq!(h.clipboard.current().as_deref(), Some(code.as_str()));
    advance(Duration::from_secs(1)).await;
    assert_eq!(h.clipboard.current(), None);
    assert!(h.notices().contains(&Notice::ClipboardCleared));
    h.core.copy_code(id).unwrap();
    h.clipboard.put_text("something the user copied");
    advance(Duration::from_secs(31)).await;
    assert_eq!(h.clipboard.current().as_deref(), Some("something the user copied"));
    h.core.set_settings(Settings { clipboard_clear_seconds: 0, ..h.core.state().settings }).unwrap();
    h.core.copy_code(id).unwrap();
    advance(Duration::from_secs(120)).await;
    assert!(h.clipboard.current().is_some(), "0 never clears");
    h.clipboard.fail.store(true, Ordering::SeqCst);
    assert_eq!(code_err(h.core.copy_code(id)), ErrorCode::ClipboardFailed);
    assert_eq!(code_err(h.core.copy_code(Uuid::new_v4())), ErrorCode::EntryNotFound);
}

#[tokio::test(start_paused = true)]
async fn an_idle_vault_locks_itself() {
    let mut h = Harness::unlocked().await;
    let deadline = h.core.state().auto_lock_at_ms.unwrap();
    assert_eq!(deadline, T0 + 5 * 60 * 1000);
    advance(Duration::from_secs(4 * 60)).await;
    h.core.activity();
    advance(Duration::from_secs(4 * 60)).await;
    assert_eq!(h.core.state().phase, Phase::Unlocked);
    advance(Duration::from_secs(61)).await;
    assert_eq!(h.core.state().phase, Phase::Locked);
    assert!(h.notices().contains(&Notice::AutoLocked));
    h.core.unlock(pw(MASTER)).await.unwrap();
    h.core.set_settings(Settings { auto_lock_minutes: 0, ..h.core.state().settings }).unwrap();
    assert_eq!(h.core.state().auto_lock_at_ms, None);
    advance(Duration::from_secs(3 * 3600)).await;
    assert_eq!(h.core.state().phase, Phase::Unlocked);
}

#[tokio::test(start_paused = true)]
async fn reveal_needs_the_password_again() {
    let h = Harness::unlocked().await;
    let id = h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    assert_eq!(code_err(h.core.reveal(id, pw("wrong")).await), ErrorCode::WrongPassword);
    assert_eq!(code_err(h.core.reveal(Uuid::new_v4(), pw(MASTER)).await), ErrorCode::EntryNotFound);
    assert_eq!(code_err(h.core.verify_password(pw("wrong")).await), ErrorCode::WrongPassword);
    h.core.verify_password(pw(MASTER)).await.unwrap();
    let revealed = h.core.reveal(id, pw(MASTER)).await.unwrap();
    assert_eq!(revealed.secret, "JBSW Y3DP EHPK 3PXP");
    let back = uri::parse(&revealed.uri).unwrap();
    assert_eq!((back.issuer.as_str(), back.account.as_str()), ("GitHub", "octocat"));
    assert!(revealed.svg.contains("<svg"));
}

// ---- import -----------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn pasted_text_is_previewed_then_committed() {
    let mut h = Harness::unlocked().await;
    h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    let text = format!("{}\n{}\n{}\nnot a uri\n", otpauth("Renamed", "x", SECRET), otpauth("Mail", "me", "GEZDGNBV"), otpauth("Mail", "again", "GEZDGNBV"));
    h.core.import_text(&text).unwrap();
    let preview = h.core.state().import.unwrap();
    let statuses: Vec<_> = preview.candidates.iter().map(|c| c.status).collect();
    assert!(matches!(statuses[0], CandidateStatus::Exists { .. }));
    assert_eq!(statuses[1..3], [CandidateStatus::New, CandidateStatus::Duplicate]);
    assert!(matches!(statuses[3], CandidateStatus::Unsupported { .. }));
    assert_eq!(preview.candidates[3].line, Some(4));
    h.notices();
    let outcome = h.core.import_commit(&[]).unwrap();
    assert_eq!(outcome, Outcome { added: 1, replaced: 0, skipped: 3 });
    assert!(h.notices().contains(&Notice::Imported { added: 1, replaced: 0, skipped: 3 }));
    assert_eq!(h.core.state().entries.len(), 2);
    assert!(h.core.state().import.is_none());
    assert_eq!(code_err(h.core.import_commit(&[])), ErrorCode::NoImport);
    assert_eq!(code_err(h.core.import_text("   \n# only a comment")), ErrorCode::ImportEmpty);
    h.core.import_text(&otpauth("Mail", "me", "MZXW6YTBOI")).unwrap();
    let conflict = h.core.state().import.unwrap().candidates[0].clone();
    assert!(matches!(conflict.status, CandidateStatus::Conflict { .. }));
    h.core.import_commit(&[Choice { id: conflict.id, action: CandidateAction::Replace }]).unwrap();
    assert_eq!(h.core.state().entries.len(), 2);
    h.core.import_text(&otpauth("Z", "z", "MFRGGZDF")).unwrap();
    h.core.import_cancel();
    assert!(h.core.state().import.is_none());
    assert_eq!(h.core.state().entries.len(), 2);
}

#[tokio::test(start_paused = true)]
async fn a_google_export_reports_its_missing_codes() {
    let h = Harness::unlocked().await;
    let accounts: Vec<lockra_otp::OtpAuth> =
        (0..12).map(|i| uri::parse(&otpauth("S", &format!("u{i}"), &format!("GEZDGNBV{}", ["MZXW6YTB", "MFRGGZDF"][i % 2]))).unwrap()).collect();
    let mut accounts = accounts;
    for (i, a) in accounts.iter_mut().enumerate() {
        a.secret = Zeroizing::new(vec![u8::try_from(i).unwrap(); 10]);
    }
    let refs: Vec<&lockra_otp::OtpAuth> = accounts.iter().collect();
    let codes = lockra_transfer::google::encode(&refs).unwrap();
    assert_eq!(codes.len(), 2);
    h.core.import_text(&codes[0].uri).unwrap();
    let batch = &h.core.state().import.unwrap().google_batches[0];
    assert_eq!((batch.size, batch.received.clone(), batch.missing.clone()), (2, vec![0], vec![1]));
    h.core.import_text(&codes[1].uri).unwrap();
    assert!(h.core.state().import.unwrap().google_batches[0].missing.is_empty());
    assert_eq!(h.core.import_commit(&[]).unwrap().added, 12);
    assert!(h.core.state().entries.iter().all(|e| e.origin == lockra_transfer::Origin::Google));
}

#[tokio::test(start_paused = true)]
async fn files_of_every_kind_are_imported() {
    let mut h = Harness::unlocked().await;
    let files = tempfile::tempdir().unwrap();
    // Microsoft Authenticator's database (checkpointed, so it reads without its log).
    let db = files.path().join("PhoneFactor");
    {
        let connection = rusqlite::Connection::open(&db).unwrap();
        connection.execute_batch("CREATE TABLE accounts (name TEXT, username TEXT, oath_secret_key TEXT, account_type INTEGER)").unwrap();
        connection.execute("INSERT INTO accounts VALUES ('Contoso', 'me@contoso.com', 'GEZDGNBVGY3TQOJQ', 0)", []).unwrap();
    }
    let image = files.path().join("screenshot.png");
    fs::write(&image, png_of_qr(&otpauth("FromImage", "x", "MZXW6YTBOI"))).unwrap();
    let list = files.path().join("list.txt");
    fs::write(&list, format!("\u{feff}{}\n", otpauth("FromList", "y", "MFRGGZDF"))).unwrap();
    let unknown = files.path().join("notes.bin");
    fs::write(&unknown, b"\x00\x01binary").unwrap();
    let blank = files.path().join("blank.png");
    fs::write(&blank, {
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(64, 64, image::Luma([255]))).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    })
    .unwrap();
    let missing = files.path().join("missing.txt");
    h.notices();
    h.core.import_files(vec![db, image, list, unknown, blank, missing]).await.unwrap();
    let preview = h.core.state().import.unwrap();
    let names: Vec<(&str, lockra_transfer::Origin)> = preview.candidates.iter().map(|c| (c.issuer.as_str(), c.origin)).collect();
    assert_eq!(
        names,
        [("Contoso", lockra_transfer::Origin::Microsoft), ("FromImage", lockra_transfer::Origin::Uri), ("FromList", lockra_transfer::Origin::Uri)]
    );
    let notices = h.notices();
    assert!(notices.contains(&Notice::FileUnrecognized { name: "notes.bin".into() }), "{notices:?}");
    assert!(notices.contains(&Notice::FileUnrecognized { name: "blank.png".into() }), "{notices:?}");
    assert!(notices.contains(&Notice::FileUnreadable { name: "missing.txt".into() }), "{notices:?}");
    assert_eq!(h.core.import_commit(&[]).unwrap().added, 3);
    let only_unknown = files.path().join("notes.bin");
    assert_eq!(code_err(h.core.import_files(vec![only_unknown]).await), ErrorCode::ImportEmpty);
}

#[tokio::test(start_paused = true)]
async fn a_lockra_backup_in_the_import_asks_for_its_password() {
    let source = Harness::unlocked().await;
    let mut entry_ids = Vec::new();
    for (issuer, secret) in [("One", "GEZDGNBV"), ("Two", "MZXW6YTBOI")] {
        entry_ids.push(source.core.add_uri(&otpauth(issuer, "me", secret)).unwrap());
    }
    source.core.update_entry(entry_ids[1], EntryPatch { group: Some("Work".into()), favorite: Some(true), ..EntryPatch::default() }).unwrap();
    let backup = source.dir.path().join("mine.lockrabackup");
    source.core.backup_to(backup.clone(), Some(pw("backup password"))).await.unwrap();
    let h = Harness::unlocked().await;
    h.core.add_uri(&otpauth("One", "me", "GEZDGNBV")).unwrap();
    h.core.import_files(vec![backup]).await.unwrap();
    assert_eq!(h.core.state().import.unwrap().awaiting_password.as_deref(), Some("mine.lockrabackup"));
    assert_eq!(code_err(h.core.import_backup_password(pw(MASTER)).await), ErrorCode::WrongPassword);
    h.core.import_backup_password(pw("backup password")).await.unwrap();
    let preview = h.core.state().import.unwrap();
    assert_eq!(preview.awaiting_password, None);
    assert!(matches!(preview.candidates[0].status, CandidateStatus::Exists { .. }));
    assert_eq!(preview.candidates[1].status, CandidateStatus::New);
    assert_eq!(h.core.import_commit(&[]).unwrap(), Outcome { added: 1, replaced: 0, skipped: 1 });
    let two = h.core.state().entries.into_iter().find(|e| e.issuer == "Two").unwrap();
    assert_eq!((two.group.as_deref(), two.favorite, two.origin), (Some("Work"), true, lockra_transfer::Origin::Backup));
}

#[tokio::test(start_paused = true)]
async fn the_clipboard_imports_images_then_uri_text() {
    let h = Harness::unlocked().await;
    assert_eq!(code_err(h.core.import_clipboard().await), ErrorCode::ClipboardEmpty);
    let png = png_of_qr(&otpauth("Shot", "x", "GEZDGNBV"));
    let rgba = image::load_from_memory(&png).unwrap().to_rgba8();
    h.clipboard.put_image(ClipboardImage { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() });
    h.core.import_clipboard().await.unwrap();
    h.clipboard.put_text("just a sentence");
    assert_eq!(code_err(h.core.import_clipboard().await), ErrorCode::ClipboardEmpty);
    h.clipboard.put_text(&otpauth("Text", "y", "MZXW6YTBOI"));
    h.core.import_clipboard().await.unwrap();
    let issuers: Vec<String> = h.core.state().import.unwrap().candidates.into_iter().map(|c| c.issuer).collect();
    assert_eq!(issuers, ["Shot", "Text"]);
    h.clipboard.fail.store(true, Ordering::SeqCst);
    assert_eq!(code_err(h.core.import_clipboard().await), ErrorCode::ClipboardFailed);
}

// ---- export -----------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn export_sessions_need_the_password_and_expire() {
    let mut h = Harness::unlocked().await;
    let a = h.core.add_uri(&otpauth("A", "a", "GEZDGNBV")).unwrap();
    let b = h.core.add_uri(&otpauth("B", "b", "MZXW6YTBOI")).unwrap();
    let sixty = h.core.add_uri("otpauth://totp/C:c?secret=MFRGGZDF&period=60").unwrap();
    let ids = [a, b, sixty];
    assert_eq!(code_err(h.core.export_start(ExportTarget::Google, &ids, pw("wrong")).await), ErrorCode::WrongPassword);
    assert_eq!(code_err(h.core.export_start(ExportTarget::Google, &[Uuid::new_v4()], pw(MASTER)).await), ErrorCode::ExportNothing);
    let started = h.core.export_start(ExportTarget::Google, &ids, pw(MASTER)).await.unwrap();
    assert_eq!((started.pages, started.excluded.len()), (1, 1));
    assert_eq!(started.excluded[0].entry_id, sixty);
    let page = h.core.export_page(started.session, 0).unwrap();
    assert_eq!((page.total, page.entry_ids.clone()), (1, vec![a, b]));
    assert!(page.svg.contains("<svg"));
    assert_eq!(code_err(h.core.export_page(started.session, 1)), ErrorCode::ExportExpired);
    advance(Duration::from_secs(119)).await;
    h.core.export_page(started.session, 0).unwrap();
    advance(Duration::from_secs(119)).await;
    h.core.export_page(started.session, 0).unwrap();
    h.notices();
    advance(Duration::from_secs(121)).await;
    assert_eq!(code_err(h.core.export_page(started.session, 0)), ErrorCode::ExportExpired);
    assert!(h.notices().contains(&Notice::ExportExpired { session: started.session }));
    let microsoft = h.core.export_start(ExportTarget::Microsoft, &ids, pw(MASTER)).await.unwrap();
    assert_eq!(microsoft.pages, 2);
    h.core.export_close(microsoft.session);
    assert_eq!(code_err(h.core.export_page(microsoft.session, 0)), ErrorCode::ExportExpired);
    let again = h.core.export_start(ExportTarget::Microsoft, &ids, pw(MASTER)).await.unwrap();
    h.core.lock_vault();
    h.core.unlock(pw(MASTER)).await.unwrap();
    assert_eq!(code_err(h.core.export_page(again.session, 0)), ErrorCode::ExportExpired, "locking drops every session");
}

#[tokio::test(start_paused = true)]
async fn a_plain_list_export_writes_a_private_file() {
    let h = Harness::unlocked().await;
    let a = h.core.add_uri(&otpauth("A", "a", "GEZDGNBV")).unwrap();
    let path = h.dir.path().join("export.txt");
    assert_eq!(code_err(h.core.export_otpauth_file(&[a], pw("wrong"), path.clone()).await), ErrorCode::WrongPassword);
    assert_eq!(code_err(h.core.export_otpauth_file(&[], pw(MASTER), path.clone()).await), ErrorCode::ExportNothing);
    assert_eq!(h.core.export_otpauth_file(&[a], pw(MASTER), path.clone()).await.unwrap(), "export.txt");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("otpauth://totp/A:a?secret=GEZDGNBV"), "{text}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

// ---- backup and restore -----------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn manual_backups_open_with_the_right_password() {
    let mut h = Harness::unlocked().await;
    h.core.add_uri(&otpauth("A", "a", "GEZDGNBV")).unwrap();
    let plain = h.dir.path().join("plain.lockrabackup");
    assert_eq!(h.core.backup_to(plain.clone(), None).await.unwrap(), "plain.lockrabackup");
    let opened = Sealed::open_with_password(&fs::read(&plain).unwrap(), MASTER.as_bytes()).unwrap();
    assert_eq!(opened.kind, FileKind::Backup);
    assert_eq!(crate::VaultData::from_bytes(&opened.payload).unwrap().entries.len(), 1);
    assert!(h.notices().contains(&Notice::BackupWritten { file_name: "plain.lockrabackup".into(), automatic: false }));
    assert!(h.core.state().backup.last_backup_ms.is_some());
    let own = h.dir.path().join("own.lockrabackup");
    assert_eq!(code_err(h.core.backup_to(own.clone(), Some(pw("short"))).await), ErrorCode::PasswordTooShort);
    h.core.backup_to(own.clone(), Some(pw("backup password"))).await.unwrap();
    let bytes = fs::read(&own).unwrap();
    assert!(Sealed::open_with_password(&bytes, MASTER.as_bytes()).is_err());
    assert!(Sealed::open_with_password(&bytes, b"backup password").is_ok());
    // A backup written right after a device unlock still opens with the master password.
    h.core.enable_device_unlock().unwrap();
    h.core.lock_vault();
    h.core.unlock_with_device().await.unwrap();
    let after_device = h.dir.path().join("after-device.lockrabackup");
    h.core.backup_to(after_device.clone(), None).await.unwrap();
    assert!(Sealed::open_with_password(&fs::read(&after_device).unwrap(), MASTER.as_bytes()).is_ok());
}

#[tokio::test(start_paused = true)]
async fn automatic_backups_debounce_prune_and_report_failures() {
    let mut h = Harness::unlocked().await;
    let folder = h.dir.path().join("backups");
    let settings = || h.core.state().settings;
    assert_eq!(
        code_err(h.core.set_settings(Settings { auto_backup: AutoBackup { enabled: true, dir: None, keep: 2 }, ..settings() })),
        ErrorCode::BackupDirMissing
    );
    assert_eq!(code_err(h.core.set_auto_backup_dir(&folder)), ErrorCode::BackupDirUnavailable);
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("notes.txt"), b"keep me").unwrap();
    h.core.set_auto_backup_dir(&folder).unwrap();
    let mut s = settings();
    s.auto_backup.enabled = true;
    s.auto_backup.keep = 2;
    h.core.set_settings(s).unwrap();
    let autos = |dir: &Path| {
        let mut names: Vec<String> =
            fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).filter(|n| crate::is_auto_name(n)).collect();
        names.sort();
        names
    };
    advance(Duration::from_secs(3)).await;
    assert_eq!(autos(&folder).len(), 1, "turning it on backs up once");
    for i in 0..3 {
        advance(Duration::from_secs(2)).await;
        h.core.add_uri(&otpauth("S", &format!("u{i}"), ["GEZDGNBV", "MZXW6YTBOI", "MFRGGZDF"][i])).unwrap();
        advance(Duration::from_millis(1500)).await;
        h.core.update_entry(h.core.state().entries[i].id, EntryPatch { favorite: Some(true), ..EntryPatch::default() }).unwrap();
        advance(Duration::from_millis(2999)).await;
        assert_eq!(autos(&folder).len(), (i + 1).min(2), "debounced: nothing yet at {i}");
        advance(Duration::from_millis(1)).await;
    }
    let names = autos(&folder);
    assert_eq!(names.len(), 2);
    let newest = Sealed::open_with_password(&fs::read(folder.join(names.last().unwrap())).unwrap(), MASTER.as_bytes()).unwrap();
    assert_eq!(crate::VaultData::from_bytes(&newest.payload).unwrap().entries.len(), 3);
    assert_eq!(fs::read(folder.join("notes.txt")).unwrap(), b"keep me");
    assert_eq!(h.core.state().backup.last_auto_file.as_deref(), names.last().map(String::as_str));
    // A change locked away inside the debounce is still backed up.
    h.core.add_uri(&otpauth("Last", "z", "MFRGG")).unwrap();
    h.core.lock_vault();
    // Written within the same second as the one before: `-2`, which a plain string sort puts first.
    let last_name = h.core.state().backup.last_auto_file.unwrap();
    assert!(last_name.ends_with("-2.lockrabackup"), "{last_name}");
    assert_eq!(autos(&folder).len(), 2);
    let last = Sealed::open_with_password(&fs::read(folder.join(&last_name)).unwrap(), MASTER.as_bytes()).unwrap();
    assert_eq!(crate::VaultData::from_bytes(&last.payload).unwrap().entries.len(), 4);
    h.core.unlock(pw(MASTER)).await.unwrap();
    fs::remove_dir_all(&folder).unwrap();
    h.notices();
    h.core.add_uri(&otpauth("Fails", "f", "MFRGGZA")).unwrap();
    advance(Duration::from_secs(3)).await;
    assert_eq!(h.core.state().backup.last_auto_error.map(|f| f.code), Some(ErrorCode::BackupDirUnavailable));
    assert!(h.notices().contains(&Notice::BackupFailed { code: ErrorCode::BackupDirUnavailable }));
    assert_eq!(code_err(h.core.backup_auto_now()), ErrorCode::BackupDirUnavailable);
}

#[tokio::test(start_paused = true)]
async fn restoring_into_a_new_vault_and_into_an_unlocked_one() {
    let source = Harness::unlocked().await;
    source.core.add_uri(&otpauth("A", "a", "GEZDGNBV")).unwrap();
    source.core.add_uri(&otpauth("B", "b", "MZXW6YTBOI")).unwrap();
    let file = source.dir.path().join("restore-me.lockrabackup");
    source.core.backup_to(file.clone(), None).await.unwrap();
    let not_lockra = source.dir.path().join("notes.txt");
    fs::write(&not_lockra, b"hello").unwrap();

    // A new installation: the backup becomes the vault, its password the master password.
    let fresh = harness();
    assert_eq!(code_err(fresh.core.restore_commit(pw(MASTER), RestoreMode::Merge).await), ErrorCode::NoRestore);
    assert_eq!(code_err(fresh.core.restore_open(not_lockra.clone()).await), ErrorCode::NotLockra);
    fresh.core.restore_open(file.clone()).await.unwrap();
    let pending = fresh.core.state().restore.unwrap();
    assert_eq!((pending.file_name.as_str(), pending.kind), ("restore-me.lockrabackup", FileKind::Backup));
    assert_eq!(code_err(fresh.core.restore_commit(pw("wrong"), RestoreMode::Merge).await), ErrorCode::WrongPassword);
    assert!(fresh.core.state().restore.is_some(), "a wrong password keeps the restore open");
    fresh.core.restore_commit(pw(MASTER), RestoreMode::Merge).await.unwrap();
    let state = fresh.core.state();
    assert_eq!((state.phase, state.entries.len(), state.restore.is_none()), (Phase::Unlocked, 2, true));
    fresh.core.lock_vault();
    fresh.core.unlock(pw(MASTER)).await.unwrap();

    // An unlocked vault: replace keeps a pre-restore backup under the current password.
    let mut h = Harness::unlocked().await;
    h.core.add_uri(&otpauth("Mine", "m", "MFRGGZDF")).unwrap();
    h.core.restore_open(file.clone()).await.unwrap();
    h.core.restore_commit(pw(MASTER), RestoreMode::Replace).await.unwrap();
    let issuers: Vec<String> = h.core.state().entries.into_iter().map(|e| e.issuer).collect();
    assert_eq!(issuers, ["A", "B"]);
    assert!(h.notices().contains(&Notice::Restored { entries: 2 }));
    let pre =
        fs::read_dir(h.data_dir()).unwrap().map(|e| e.unwrap().path()).find(|p| p.file_name().unwrap().to_string_lossy().starts_with("pre-restore-")).unwrap();
    let kept = Sealed::open_with_password(&fs::read(pre).unwrap(), MASTER.as_bytes()).unwrap();
    assert_eq!(crate::VaultData::from_bytes(&kept.payload).unwrap().entries[0].issuer, "Mine");

    // Merge goes through the import preview.
    h.core.restore_open(file.clone()).await.unwrap();
    h.core.restore_commit(pw(MASTER), RestoreMode::Merge).await.unwrap();
    let preview = h.core.state().import.unwrap();
    assert!(preview.candidates.iter().all(|c| matches!(c.status, CandidateStatus::Exists { .. })));
    h.core.restore_open(file.clone()).await.unwrap();
    h.core.restore_cancel();
    assert!(h.core.state().restore.is_none());
    h.core.lock_vault();
    assert_eq!(code_err(h.core.restore_open(file).await), ErrorCode::Locked);
}

// ---- settings and secrecy ---------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn settings_are_saved_normalized_and_reloaded() {
    let h = harness();
    let mut s = h.core.state().settings;
    s.theme = ThemeId::Graphite;
    s.font_size_px = 40;
    s.auto_lock_minutes = 7;
    h.core.set_settings(s).unwrap();
    let state = h.core.state();
    assert_eq!((state.settings.theme, state.settings.font_size_px, state.settings.auto_lock_minutes), (ThemeId::Graphite, 18, 5));
    assert_eq!(h.restart().state().settings.theme, ThemeId::Graphite);
}

#[tokio::test(start_paused = true)]
async fn state_and_events_never_carry_a_secret() {
    let mut h = Harness::unlocked().await;
    let id = h.core.add_uri(&otpauth("GitHub", "octocat", SECRET)).unwrap();
    h.core.add_uri("otpauth://hotp/Bank:me?secret=GEZDGNBVGY3TQOJQ&counter=3").unwrap();
    h.core.import_text(&otpauth("Pending", "p", "MZXW6YTBOI")).unwrap();
    h.core.copy_code(id).unwrap();
    h.core.enable_device_unlock().unwrap();
    let mut texts = vec![serde_json::to_string(&h.core.state()).unwrap()];
    while let Ok(event) = h.events.try_recv() {
        texts.push(serde_json::to_string(&event).unwrap());
    }
    let sink = Arc::new(RecordingSink::default());
    h.core.subscribe_codes(sink.clone());
    texts.extend(sink.frames().iter().map(|f| serde_json::to_string(f).unwrap()));
    for text in &texts {
        for secret in [SECRET, "GEZDGNBVGY3TQOJQ", "MZXW6YTBOI", "secret="] {
            assert!(!text.contains(secret), "{secret} leaked in {text}");
        }
    }
    assert!(texts.len() > 5);
}
