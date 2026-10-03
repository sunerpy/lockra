#![allow(clippy::unwrap_used, clippy::expect_used)]
//! IPC contract fixtures shared with the TypeScript side.
//!
//! The Rust types are the source of truth for the wire format. This test serializes representative
//! values and compares them byte for byte with the JSON under `packages/shared/src/fixtures/ipc/`;
//! `ipc-contract.test.ts` parses the same files with the zod schemas and replays the commands
//! through `TauriBackend`. After an intentional contract change, regenerate and commit them:
//! `UPDATE_IPC_FIXTURES=1 cargo test -p lockra-bridge --test contract`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lockra_bridge::{COMMANDS, PHONE_COMMANDS, SHELL_COMMANDS, UiCommand, dispatch};
use lockra_core::fakes::{FakeBiometrics, FakeClipboard, FakeClock, FakeKeychain, FakeTransport, FakeUpdater, RecordingSink};
use lockra_core::settings::{AccentId, AutoBackup, Density, LocaleSetting, Settings, SortOrder, ThemeId};
use lockra_core::ui::{
    BackupFailure, BackupView, BiometricKind, BiometricView, CandidateAction, CandidateStatus, CandidateView, CodeView, CodesFrame, DeviceUnlockView, Excluded,
    ExportPage, ExportStarted, ExportTarget, GoogleBatchView, ImportSource, ImportView, InstallMethod, LockView, Notice, Phase, Platform, RestoreView,
    Revealed, StorageView, SyncCreated, SyncDeviceView, SyncInvite, SyncSpaceView, SyncStatus, SyncView, UiEvent, UiState, UpdateStatus, UpdateView,
};
use lockra_core::{AccountColor, Core, CoreConfig, CoreError, EntryView, ErrorCode, ExportCompat, KdfCost, Outcome, Ports};
use lockra_otp::{Algorithm, Digits, OtpKind, Period};
use lockra_transfer::{Incompatible, Origin, RejectReason};
use lockra_vault::FileKind;
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

const UPDATE_ENV: &str = "UPDATE_IPC_FIXTURES";
const T0: u64 = 1_790_000_000_000;
const SECRET: &str = "JBSWY3DPEHPK3PXP";

fn id(n: u128) -> Uuid {
    Uuid::from_u128(0x0f3f_1a1e_8d4b_4c8e_9f7a_0000_0000_0000 + n)
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/shared/src/fixtures/ipc")
}

/// Compare `value` with the checked-in fixture, or rewrite it when `UPDATE_IPC_FIXTURES` is set.
fn check(name: &str, value: &impl Serialize) {
    let path = fixtures_dir().join(name);
    let text = format!("{}\n", serde_json::to_string_pretty(value).unwrap());
    if std::env::var_os(UPDATE_ENV).is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{} is missing: run with {UPDATE_ENV}=1", path.display()));
    assert_eq!(committed, text, "{name} is stale: run `{UPDATE_ENV}=1 cargo test -p lockra-bridge --test contract` and commit the result");
}

fn entry(n: u128, issuer: &str, account: &str, kind: OtpKind, digits: Digits, export: ExportCompat) -> EntryView {
    EntryView {
        id: id(n),
        issuer: issuer.into(),
        account: account.into(),
        kind,
        algorithm: Algorithm::Sha1,
        digits,
        group: None,
        favorite: false,
        color: AccountColor::Auto,
        mark: None,
        origin: Origin::Uri,
        created_at_ms: T0 - 86_400_000,
        updated_at_ms: T0 - 3_600_000,
        last_used_at_ms: None,
        export,
    }
}

fn entries() -> Vec<EntryView> {
    let ok = ExportCompat { google: None, microsoft: None };
    let mut github = entry(1, "GitHub", "octocat", OtpKind::Totp { period: Period::THIRTY }, Digits::SIX, ok);
    github.favorite = true;
    github.group = Some("Work".into());
    github.color = AccountColor::Purple;
    github.mark = Some("GH".into());
    github.last_used_at_ms = Some(T0 - 60_000);
    let mut microsoft = entry(
        2,
        "Microsoft",
        "me@outlook.com",
        OtpKind::Totp { period: Period::THIRTY },
        Digits::EIGHT,
        ExportCompat { google: None, microsoft: Some(Incompatible::DigitsNot6) },
    );
    microsoft.origin = Origin::Microsoft;
    let mut bank =
        entry(3, "Bank", "1234", OtpKind::Hotp { counter: 7 }, Digits::SIX, ExportCompat { google: None, microsoft: Some(Incompatible::HotpNotSupported) });
    bank.algorithm = Algorithm::Sha256;
    bank.origin = Origin::Manual;
    let mut steamish = entry(
        4,
        "Game",
        "player",
        OtpKind::Totp { period: Period::new(60).unwrap() },
        Digits::new(7).unwrap(),
        ExportCompat { google: Some(Incompatible::PeriodNot30), microsoft: Some(Incompatible::PeriodNot30) },
    );
    steamish.algorithm = Algorithm::Sha512;
    steamish.origin = Origin::Google;
    vec![github, microsoft, bank, steamish]
}

fn settings() -> Settings {
    Settings {
        theme: ThemeId::Dark,
        follow_system_theme: false,
        accent: AccentId::Green,
        density: Density::Compact,
        font_size_px: 13,
        reduce_motion: true,
        locale: LocaleSetting::ZhCn,
        auto_lock_minutes: 10,
        clipboard_clear_seconds: 20,
        hide_codes: true,
        sort: SortOrder::Recent,
        group_codes: false,
        auto_backup: AutoBackup { enabled: true, dir: Some("/home/user/Backups/Lockra".into()), keep: 7 },
        auto_update: true,
    }
}

/// Every update status, in the order a run goes through them.
fn update_statuses() -> Vec<UpdateStatus> {
    vec![
        UpdateStatus::Idle,
        UpdateStatus::Checking,
        UpdateStatus::UpToDate { checked_at_ms: T0 - 30_000 },
        UpdateStatus::Available {
            version: "0.2.0".into(),
            notes: Some("## [0.2.0](https://github.com/sunerpy/lockra/compare/v0.1.1...v0.2.0) (2026-10-02)\n\n### Features\n\n* **update:** check for updates and install them\n".into()),
            date: Some("2026-10-02T08:00:00Z".into()),
            checked_at_ms: T0 - 30_000,
        },
        UpdateStatus::Available { version: "0.2.1".into(), notes: None, date: None, checked_at_ms: T0 },
        UpdateStatus::Downloading { version: "0.2.0".into(), received: 0, total: None },
        UpdateStatus::Downloading { version: "0.2.0".into(), received: 4_194_304, total: Some(11_508_084) },
        UpdateStatus::Ready { version: "0.2.0".into() },
        UpdateStatus::Installing { version: "0.2.0".into() },
        UpdateStatus::Failed { code: ErrorCode::UpdateSignature, at_ms: T0 - 5_000 },
    ]
}

const DESKTOP_TAG: &str = "6c1f0b5e2d9a4f7380e1c2b3a4d5e6f7";
const PHONE_TAG: &str = "0a9b8c7d6e5f40312233445566778899";
const ALTERED_TAG: &str = "ffeeddccbbaa99887766554433221100";
const SYNC_KEY: &str = "LKS1-AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ-KKKK-LLLL-MMMM-NNNN";

/// Every sync status, in the order a run goes through them.
fn sync_statuses() -> Vec<SyncStatus> {
    vec![
        SyncStatus::Idle,
        SyncStatus::Syncing,
        SyncStatus::Synced { at_ms: T0 - 60_000 },
        SyncStatus::Failed { code: ErrorCode::SyncNetwork, at_ms: T0 - 5_000 },
    ]
}

fn sync_space(status: SyncStatus) -> SyncSpaceView {
    SyncSpaceView {
        storage: StorageView::S3 {
            endpoint: "https://s3.eu-central-1.amazonaws.com".into(),
            region: "eu-central-1".into(),
            bucket: "my-lockra".into(),
            prefix: "lockra/".into(),
            access_key_id: "AKIAIOSFODNN7EXAMPLE".into(),
            path_style: false,
        },
        device_name: "Desktop".into(),
        devices: vec![
            SyncDeviceView { tag: DESKTOP_TAG.into(), name: "Desktop".into(), written_at_ms: Some(T0 - 60_000), this_device: true },
            SyncDeviceView { tag: PHONE_TAG.into(), name: "Pixel 8".into(), written_at_ms: Some(T0 - 3_600_000), this_device: false },
        ],
        status,
        last_sync_ms: Some(T0 - 60_000),
        rolled_back: Vec::new(),
        unreadable: vec![ALTERED_TAG.into()],
        keyring_pending: false,
    }
}

fn import_view() -> ImportView {
    let candidate =
        |n: u32, source: ImportSource, origin: Origin, issuer: &str, account: &str, status: CandidateStatus, action: CandidateAction| CandidateView {
            id: n,
            source,
            origin,
            issuer: issuer.into(),
            account: account.into(),
            kind: Some(OtpKind::Totp { period: Period::THIRTY }),
            algorithm: Some(Algorithm::Sha1),
            digits: Some(Digits::SIX),
            line: None,
            status,
            default_action: action,
        };
    let file = |name: &str| ImportSource::File { name: name.into() };
    let mut unsupported =
        candidate(4, ImportSource::Text, Origin::Uri, "", "", CandidateStatus::Unsupported { reason: RejectReason::NotOtpauth }, CandidateAction::Skip);
    unsupported.kind = None;
    unsupported.algorithm = None;
    unsupported.digits = None;
    unsupported.line = Some(3);
    let mut hotp = candidate(5, ImportSource::Clipboard, Origin::Uri, "Bank", "card", CandidateStatus::New, CandidateAction::Add);
    hotp.kind = Some(OtpKind::Hotp { counter: 0 });
    ImportView {
        candidates: vec![
            candidate(0, file("google-1.png"), Origin::Google, "GitHub", "octocat", CandidateStatus::Exists { entry_id: id(1) }, CandidateAction::Skip),
            candidate(1, file("google-1.png"), Origin::Google, "Mail", "me@example.com", CandidateStatus::New, CandidateAction::Add),
            candidate(
                2,
                file("PhoneFactor"),
                Origin::Microsoft,
                "Microsoft",
                "me@outlook.com",
                CandidateStatus::Conflict { entry_id: id(2) },
                CandidateAction::Add,
            ),
            candidate(3, file("PhoneFactor"), Origin::Microsoft, "Mail", "me@example.com", CandidateStatus::Duplicate, CandidateAction::Skip),
            unsupported,
            hotp,
            candidate(6, file("mine.lockrabackup"), Origin::Backup, "Old", "account", CandidateStatus::New, CandidateAction::Add),
            candidate(7, ImportSource::Camera, Origin::Google, "Cloud", "me@example.com", CandidateStatus::New, CandidateAction::Add),
        ],
        google_batches: vec![GoogleBatchView { id: 412_337, size: 3, received: vec![0, 2], missing: vec![1] }],
        awaiting_password: Some("other.lockrabackup".into()),
    }
}

fn state(phase: Phase) -> UiState {
    let unlocked = phase == Phase::Unlocked;
    UiState {
        app_version: "0.1.0".into(),
        platform: Platform::Linux,
        phase,
        data_dir: "/home/user/.local/share/dev.lockra.desktop".into(),
        lock: LockView {
            device_unlock: DeviceUnlockView {
                available: true,
                enabled: phase != Phase::NoVault,
                biometric: BiometricView { kind: Some(BiometricKind::TouchId), enabled: phase == Phase::Locked },
            },
            failed_attempts: if phase == Phase::Locked { 4 } else { 0 },
            retry_at_ms: (phase == Phase::Locked).then_some(T0 + 2000),
        },
        entries: if unlocked { entries() } else { Vec::new() },
        collapsed_groups: if unlocked { vec![String::new(), "Work".into()] } else { Vec::new() },
        settings: if unlocked { settings() } else { Settings::default() },
        import: unlocked.then(import_view),
        backup: BackupView {
            last_backup_ms: unlocked.then_some(T0 - 7_200_000),
            last_auto_file: unlocked.then(|| "lockra-auto-20260921-114640.lockrabackup".to_owned()),
            last_auto_error: unlocked.then_some(BackupFailure { code: ErrorCode::BackupDirUnavailable, at_ms: T0 - 60_000 }),
        },
        restore: (phase == Phase::NoVault).then(|| RestoreView {
            file_name: "restore-me.lockrabackup".into(),
            kind: FileKind::Backup,
            created_at_ms: T0 - 864_000_000,
        }),
        auto_lock_at_ms: unlocked.then_some(T0 + 600_000),
        update: match phase {
            Phase::Unlocked => UpdateView { method: Some(InstallMethod::Deb), status: update_statuses()[3].clone() },
            Phase::Locked => UpdateView { method: Some(InstallMethod::Nsis), status: update_statuses()[6].clone() },
            Phase::NoVault => UpdateView { method: None, status: UpdateStatus::Idle },
        },
        sync: SyncView { space: unlocked.then(|| sync_space(sync_statuses()[2].clone())) },
    }
}

fn notices() -> Vec<Notice> {
    vec![
        Notice::Copied { entry_id: id(1), clear_after_s: Some(30) },
        Notice::Copied { entry_id: id(1), clear_after_s: None },
        Notice::ClipboardCleared,
        Notice::Imported { added: 3, replaced: 1, skipped: 2 },
        Notice::FileUnrecognized { name: "notes.bin".into() },
        Notice::FileUnreadable { name: "huge.png".into() },
        Notice::BackupWritten { file_name: "lockra-auto-20260921-134640.lockrabackup".into(), automatic: true },
        Notice::BackupFailed { code: ErrorCode::BackupDirUnavailable },
        Notice::Restored { entries: 12 },
        Notice::AutoLocked,
        Notice::ExportExpired { session: id(100) },
        Notice::DeviceUnlockTurnedOff,
    ]
}

/// One example of every command, as the webview sends them.
fn commands() -> Vec<Value> {
    let entry_id = id(1).to_string();
    vec![
        json!({"command": "app_state"}),
        json!({"command": "vault_create", "password": "correct horse battery"}),
        json!({"command": "vault_unlock", "password": "correct horse battery"}),
        json!({"command": "vault_unlock_device", "reason": "unlock Lockra"}),
        json!({"command": "vault_lock"}),
        json!({"command": "vault_change_password", "current": "correct horse battery", "new": "a new password"}),
        json!({"command": "vault_reset"}),
        json!({"command": "device_unlock_enable"}),
        json!({"command": "device_unlock_disable", "password": "a new password"}),
        json!({"command": "device_biometric_enable", "reason": "turn on Touch ID"}),
        json!({"command": "device_biometric_disable", "password": "a new password"}),
        json!({"command": "entry_add_uri", "uri": format!("otpauth://totp/GitHub:octocat?secret={SECRET}&issuer=GitHub")}),
        json!({"command": "entry_add_manual", "draft": {
            "issuer": "Mail", "account": "me@example.com", "secret": "GEZD GNBV GY3T QOJQ",
            "kind": {"type": "totp", "period": 30}, "algorithm": "sha256", "digits": 8, "group": "Work"
        }}),
        json!({"command": "entry_update", "id": entry_id, "patch": {"issuer": "GitHub Enterprise", "favorite": true, "group": ""}}),
        json!({"command": "entry_delete", "id": entry_id}),
        json!({"command": "entries_set_group", "ids": [entry_id], "group": "Work"}),
        json!({"command": "entry_hotp_next", "id": entry_id}),
        json!({"command": "entry_copy", "id": entry_id}),
        json!({"command": "entry_reveal", "id": entry_id, "password": "a new password"}),
        json!({"command": "view_collapse_groups", "groups": ["Work", ""]}),
        json!({"command": "import_text", "text": "otpauth://totp/A:b?secret=GEZDGNBV"}),
        json!({"command": "import_clipboard"}),
        json!({"command": "import_backup_password", "password": "backup password"}),
        json!({"command": "import_commit", "choices": [{"id": 2, "action": "replace"}, {"id": 1, "action": "skip"}]}),
        json!({"command": "import_cancel"}),
        json!({"command": "export_start", "target": "google", "entry_ids": [entry_id], "password": "a new password"}),
        json!({"command": "export_page", "session": id(100).to_string(), "index": 0}),
        json!({"command": "export_close", "session": id(100).to_string()}),
        json!({"command": "secret_view_closed"}),
        json!({"command": "backup_auto_now"}),
        json!({"command": "restore_commit", "password": "a new password", "mode": "replace"}),
        json!({"command": "restore_cancel"}),
        json!({"command": "settings_set", "settings": serde_json::to_value(settings()).unwrap()}),
        json!({"command": "activity"}),
        json!({"command": "update_check"}),
        json!({"command": "update_install"}),
        json!({"command": "sync_create", "storage": s3_storage(), "password": "a new password", "device_name": "Desktop"}),
        json!({
            "command": "sync_join", "source": {"type": "invite", "text": "lockra-invite:1:eyJzdG9yYWdlIjp7fX0"},
            "password": "a new password", "device_name": "Pixel 8", "space_password": "another device's password"
        }),
        json!({"command": "sync_invite", "password": "a new password"}),
        json!({"command": "sync_set_storage", "storage": webdav_storage(), "password": "a new password"}),
        json!({"command": "sync_rename_device", "name": "Work desktop"}),
        json!({"command": "sync_remove_device", "tag": PHONE_TAG}),
        json!({"command": "sync_now"}),
        json!({"command": "sync_disable"}),
    ]
}

/// A storage as the webview sends it, credentials included.
fn s3_storage() -> Value {
    json!({
        "kind": "s3", "endpoint": "https://s3.eu-central-1.amazonaws.com", "region": "eu-central-1", "bucket": "my-lockra", "prefix": "lockra/",
        "access_key_id": "AKIAIOSFODNN7EXAMPLE", "secret_access_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY", "path_style": false
    })
}

fn webdav_storage() -> Value {
    json!({"kind": "webdav", "url": "https://dav.jianguoyun.com/dav/", "prefix": "lockra", "username": "me@example.com", "password": "app password"})
}

#[test]
fn state_fixtures() {
    // The phone: locked, with the fingerprint in front of its remembered key.
    let mut phone = state(Phase::Locked);
    phone.lock.device_unlock.biometric.kind = Some(BiometricKind::Fingerprint);
    check(
        "state.json",
        &json!({ "unlocked": state(Phase::Unlocked), "locked": state(Phase::Locked), "no_vault": state(Phase::NoVault), "phone_locked": phone }),
    );
}

#[test]
fn update_fixtures() {
    let methods = [InstallMethod::Deb, InstallMethod::Rpm, InstallMethod::Appimage, InstallMethod::Nsis, InstallMethod::Msi, InstallMethod::App];
    let views: Vec<UpdateView> = update_statuses()
        .into_iter()
        .zip(methods.iter().copied().map(Some).chain([None]).cycle())
        .map(|(status, method)| UpdateView { method, status })
        .collect();
    check("update.json", &views);
}

#[test]
fn sync_fixtures() {
    let mut views: Vec<SyncView> = sync_statuses().into_iter().map(|status| SyncView { space: Some(sync_space(status)) }).collect();
    let mut webdav = sync_space(SyncStatus::Idle);
    webdav.storage = StorageView::Webdav { url: "https://dav.jianguoyun.com/dav/".into(), prefix: "lockra".into(), username: "me@example.com".into() };
    webdav.devices.truncate(1);
    webdav.devices[0].written_at_ms = None;
    webdav.last_sync_ms = None;
    webdav.rolled_back = vec![PHONE_TAG.into()];
    webdav.unreadable = Vec::new();
    webdav.keyring_pending = true;
    views.push(SyncView { space: Some(webdav) });
    views.push(SyncView { space: None });
    check("sync.json", &views);
}

#[test]
fn event_fixtures() {
    let mut events: Vec<UiEvent> = vec![UiEvent::State { state: Box::new(state(Phase::Locked)) }];
    events.extend(notices().into_iter().map(|notice| UiEvent::Notice { notice }));
    check("events.json", &events);
}

#[test]
fn response_fixtures() {
    let codes = CodesFrame {
        at_ms: T0,
        codes: vec![
            CodeView {
                entry_id: id(1),
                code: "492039".into(),
                next_code: Some("114415".into()),
                valid_from_ms: Some(T0 - 20_000),
                valid_until_ms: Some(T0 + 10_000),
            },
            CodeView { entry_id: id(3), code: "287082".into(), next_code: None, valid_from_ms: None, valid_until_ms: None },
        ],
    };
    let errors: Vec<CoreError> = vec![
        ErrorCode::WrongPassword.into(),
        CoreError { code: ErrorCode::RateLimited, retry_at_ms: Some(T0 + 4000) },
        ErrorCode::ExportExpired.into(),
        ErrorCode::SyncWrongCredentials.into(),
        ErrorCode::Internal.into(),
    ];
    check(
        "responses.json",
        &json!({
            "entry_added": { "id": id(5) },
            "export_started": ExportStarted {
                session: id(100),
                target: ExportTarget::Google,
                pages: 2,
                excluded: vec![Excluded { entry_id: id(4), reason: Incompatible::PeriodNot30 }],
            },
            "export_page": ExportPage { session: id(100), index: 1, total: 2, svg: "<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into(), entry_ids: vec![id(1), id(2)] },
            "revealed": Revealed {
                entry_id: id(1),
                secret: "JBSW Y3DP EHPK 3PXP".into(),
                uri: format!("otpauth://totp/GitHub:octocat?secret={SECRET}&issuer=GitHub&algorithm=SHA1&digits=6&period=30"),
                svg: "<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into(),
            },
            "import_outcome": Outcome { added: 3, replaced: 1, skipped: 2 },
            "sync_created": SyncCreated { sync_key: SYNC_KEY.into() },
            "sync_invite": SyncInvite {
                invite: "lockra-invite:1:eyJzdG9yYWdlIjp7fX0".into(),
                svg: "<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into(),
                sync_key: SYNC_KEY.into(),
            },
            "codes_frame": codes,
            "codes_frame_locked": CodesFrame { at_ms: T0, codes: Vec::new() },
            "errors": errors,
        }),
    );
}

#[test]
fn command_fixtures_cover_every_command_and_parse() {
    let commands = commands();
    check("commands.json", &json!({ "commands": commands, "shell_commands": SHELL_COMMANDS, "phone_commands": PHONE_COMMANDS }));
    let names: Vec<&str> = commands.iter().map(|c| c["command"].as_str().unwrap()).collect();
    assert_eq!(names, COMMANDS, "commands.json must list every UiCommand once, in order");
    for command in &commands {
        serde_json::from_value::<UiCommand>(command.clone()).unwrap_or_else(|e| panic!("{command}: {e}"));
    }
}

#[test]
fn unknown_commands_and_fields_are_refused() {
    assert!(serde_json::from_value::<UiCommand>(json!({"command": "import_files", "paths": ["/etc/passwd"]})).is_err());
    assert!(serde_json::from_value::<UiCommand>(json!({"command": "backup_to", "path": "/tmp/x"})).is_err());
    // A command with fields refuses extra ones (serde cannot do so for field-less commands, which
    // ignore them: nothing they do depends on the payload).
    assert!(serde_json::from_value::<UiCommand>(json!({"command": "entry_copy", "id": id(1), "path": "/tmp/x"})).is_err());
    assert!(serde_json::from_value::<UiCommand>(json!({"command": "entry_copy"})).is_err());
}

#[test]
fn secret_views_are_flagged_for_the_shell() {
    let parse = |v: Value| serde_json::from_value::<UiCommand>(v).unwrap();
    assert!(parse(json!({"command": "entry_reveal", "id": id(1), "password": "x"})).shows_secret());
    assert!(parse(json!({"command": "export_start", "target": "microsoft", "entry_ids": [], "password": "x"})).shows_secret());
    assert!(parse(json!({"command": "sync_create", "storage": s3_storage(), "password": "x", "device_name": "d"})).shows_secret());
    assert!(parse(json!({"command": "sync_invite", "password": "x"})).shows_secret());
    assert!(!parse(json!({"command": "sync_now"})).shows_secret());
    assert!(!parse(json!({"command": "app_state"})).shows_secret());
    for command in [json!({"command": "secret_view_closed"}), json!({"command": "export_close", "session": id(1)}), json!({"command": "vault_lock"})] {
        assert!(parse(command).hides_secret());
    }
    assert!(!parse(json!({"command": "activity"})).hides_secret());
}

fn ok(value: Result<Value, CoreError>, answers: &mut Vec<String>) -> Value {
    let value = value.unwrap();
    answers.push(value.to_string());
    value
}

/// Every command dispatched on a real core: answers and events carry no secret, except the two
/// answers whose purpose is to show one.
#[tokio::test(start_paused = true)]
async fn dispatch_answers_and_leaks_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let keychain = Arc::new(FakeKeychain::default());
    let clipboard = Arc::new(FakeClipboard::default());
    let config = CoreConfig {
        data_dir: dir.path().join("data"),
        config_dir: dir.path().join("config"),
        app_version: "0.1.0".into(),
        kdf: KdfCost::FAST_INSECURE,
        platform: Platform::Linux,
    };
    let updater = Arc::new(FakeUpdater::installed(InstallMethod::Deb));
    *updater.check.lock() = Ok(Some(FakeUpdater::release("0.2.0")));
    let transport = Arc::new(FakeTransport::default());
    let ports = Ports {
        secrets: keychain,
        clipboard: clipboard.clone(),
        clock: Arc::new(FakeClock::new(T0)),
        updater: updater.clone(),
        sync: transport,
        biometrics: Arc::new(FakeBiometrics::default()),
    };
    let core = Core::start(config, ports);
    let mut events = core.subscribe();
    let sink = Arc::new(RecordingSink::default());
    core.subscribe_codes(sink.clone());
    let run = |command: Value| {
        let core = core.clone();
        async move { dispatch(&core, serde_json::from_value(command).unwrap()).await }
    };
    let mut answers: Vec<String> = Vec::new();
    ok(run(json!({"command": "vault_create", "password": "correct horse battery"})).await, &mut answers);
    let added = ok(run(json!({"command": "entry_add_uri", "uri": format!("otpauth://totp/GitHub:octocat?secret={SECRET}")})).await, &mut answers);
    let entry_id = added["id"].as_str().unwrap().to_owned();
    ok(
        run(json!({"command": "entry_add_manual", "draft": {"secret": "GEZDGNBVGY3TQOJQ", "kind": {"type": "hotp", "counter": 0}, "issuer": "Bank"}})).await,
        &mut answers,
    );
    for command in [
        json!({"command": "app_state"}),
        json!({"command": "entry_update", "id": entry_id, "patch": {"favorite": true}}),
        json!({"command": "view_collapse_groups", "groups": [""]}),
        json!({"command": "entries_set_group", "ids": [entry_id], "group": "Work"}),
        json!({"command": "entry_copy", "id": entry_id}),
        json!({"command": "import_text", "text": "otpauth://totp/Mail:me?secret=MZXW6YTBOI"}),
        json!({"command": "import_commit"}),
        json!({"command": "device_unlock_enable"}),
        json!({"command": "device_biometric_enable", "reason": "turn on Touch ID"}),
        json!({"command": "vault_lock"}),
        json!({"command": "vault_unlock_device"}),
        json!({"command": "device_biometric_disable", "password": "correct horse battery"}),
        json!({"command": "activity"}),
        json!({"command": "secret_view_closed"}),
        json!({"command": "import_cancel"}),
        json!({"command": "restore_cancel"}),
    ] {
        ok(run(command).await, &mut answers);
    }
    let error = run(json!({"command": "vault_unlock", "password": "x"})).await;
    assert!(error.is_ok(), "unlock while unlocked is a no-op");
    let wrong = run(json!({"command": "entry_reveal", "id": entry_id, "password": "wrong password"})).await.unwrap_err();
    assert_eq!(wrong.code, ErrorCode::WrongPassword);
    let revealed = run(json!({"command": "entry_reveal", "id": entry_id, "password": "correct horse battery"})).await.unwrap();
    assert_eq!(revealed["secret"], "JBSW Y3DP EHPK 3PXP");
    let started = run(json!({"command": "export_start", "target": "google", "entry_ids": [entry_id], "password": "correct horse battery"})).await.unwrap();
    answers.push(started.to_string());
    let page = run(json!({"command": "export_page", "session": started["session"], "index": 0})).await.unwrap();
    assert!(page["svg"].as_str().unwrap().contains("<svg"));
    ok(run(json!({"command": "export_close", "session": started["session"]})).await, &mut answers);
    let settings = run(json!({"command": "app_state"})).await.unwrap()["settings"].clone();
    ok(run(json!({"command": "settings_set", "settings": settings})).await, &mut answers);
    // The update runs in the background: its states join the events scanned below.
    for command in [json!({"command": "update_check"}), json!({"command": "update_install"})] {
        ok(run(command).await, &mut answers);
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
    }
    assert_eq!(updater.calls(), ["check", "download", "install"], "the install goes on from what the check found");
    // Sync on the fake storage: the two answers that show a secret are kept out of the scan.
    let storage = json!({
        "kind": "s3", "endpoint": "https://s3.example.com", "region": "us-east-1", "bucket": "lockra", "prefix": "",
        "access_key_id": "AKIDLOCKRA", "secret_access_key": FakeTransport::SECRET, "path_style": false
    });
    let created = run(json!({"command": "sync_create", "storage": storage, "password": "correct horse battery", "device_name": "Desktop"})).await.unwrap();
    let sync_key = created["sync_key"].as_str().unwrap().to_owned();
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    let invite = run(json!({"command": "sync_invite", "password": "correct horse battery"})).await.unwrap();
    assert!(invite["invite"].as_str().unwrap().starts_with("lockra-invite:1:") && invite["sync_key"] == sync_key.as_str());
    ok(run(json!({"command": "sync_now"})).await, &mut answers);
    ok(run(json!({"command": "sync_rename_device", "name": "Work desktop"})).await, &mut answers);
    ok(run(json!({"command": "sync_set_storage", "storage": storage, "password": "correct horse battery"})).await, &mut answers);
    let own_tag = run(json!({"command": "app_state"})).await.unwrap()["sync"]["space"]["devices"][0]["tag"].clone();
    assert_eq!(run(json!({"command": "sync_remove_device", "tag": own_tag})).await.unwrap_err().code, ErrorCode::Internal);
    ok(run(json!({"command": "sync_disable"})).await, &mut answers);
    let source = json!({"type": "manual", "storage": storage, "sync_key": sync_key});
    let join = json!({"command": "sync_join", "source": source, "password": "correct horse battery", "device_name": "Desktop", "space_password": "correct horse battery"});
    ok(run(join).await, &mut answers);
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    assert_eq!(run(json!({"command": "app_state"})).await.unwrap()["sync"]["space"]["status"]["state"], "synced");
    assert_eq!(run(json!({"command": "app_state"})).await.unwrap()["update"]["status"]["state"], "installing");
    assert_eq!(run(json!({"command": "restore_commit", "password": "x", "mode": "merge"})).await.unwrap_err().code, ErrorCode::NoRestore);
    assert_eq!(run(json!({"command": "backup_auto_now"})).await.unwrap_err().code, ErrorCode::BackupDirMissing);
    let mut texts = answers;
    while let Ok(event) = events.try_recv() {
        texts.push(serde_json::to_string(&event).unwrap());
    }
    texts.extend(sink.frames().iter().map(|f| serde_json::to_string(f).unwrap()));
    for text in &texts {
        for secret in [SECRET, "GEZDGNBVGY3TQOJQ", "MZXW6YTBOI", "secret=", FakeTransport::SECRET, &sync_key[5..19], "lockra-invite"] {
            assert!(!text.contains(secret), "{secret} leaked: {text}");
        }
    }
    assert!(clipboard.current().is_some_and(|c| c.len() == 6));
    // The remaining commands, each through `dispatch` once.
    let bank = run(json!({"command": "app_state"})).await.unwrap()["entries"].as_array().unwrap().iter().find(|e| e["issuer"] == "Bank").unwrap()["id"].clone();
    ok(run(json!({"command": "entry_hotp_next", "id": bank})).await, &mut Vec::new());
    ok(run(json!({"command": "entry_delete", "id": bank})).await, &mut Vec::new());
    assert_eq!(run(json!({"command": "import_clipboard"})).await.unwrap_err().code, ErrorCode::ClipboardEmpty);
    assert_eq!(run(json!({"command": "import_backup_password", "password": "x"})).await.unwrap_err().code, ErrorCode::NoImport);
    ok(run(json!({"command": "vault_change_password", "current": "correct horse battery", "new": "a new password"})).await, &mut Vec::new());
    ok(run(json!({"command": "device_unlock_disable", "password": "a new password"})).await, &mut Vec::new());
    ok(run(json!({"command": "vault_lock"})).await, &mut Vec::new());
    ok(run(json!({"command": "vault_unlock", "password": "a new password"})).await, &mut Vec::new());
    ok(run(json!({"command": "vault_lock"})).await, &mut Vec::new());
    ok(run(json!({"command": "vault_reset"})).await, &mut Vec::new());
    assert_eq!(run(json!({"command": "app_state"})).await.unwrap()["phase"], "no_vault");
}
