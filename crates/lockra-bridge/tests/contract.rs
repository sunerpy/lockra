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
use lockra_core::settings::{AccentId, AutoBackup, CodesView, DefaultUnlock, Density, LocaleSetting, Settings, SortOrder, ThemeId};
use lockra_core::ui::{
    AwaitingKind, AwaitingView, BackupFailure, BackupView, BiometricKind, BiometricView, CandidateAction, CandidateStatus, CandidateView, CodeView, CodesFrame,
    ConflictReason, CopiedPart, DeviceUnlockView, Excluded, ExportPage, ExportStarted, ExportTarget, Features, GeneratorStatus, GoogleBatchView, ImportSource,
    ImportView, InstallMethod, LockView, LoginCandidateView, Notice, Phase, Platform, RestoreView, Revealed, SharedInvite, StorageView, SyncDeviceView,
    SyncInvite, SyncKeyView, SyncSpaceView, SyncStatus, SyncView, UiEvent, UiState, UpdateStatus, UpdateView,
};
use lockra_core::{
    AccountColor, ActivityEvent, ActivityView, AutofillChoice, BrowserPolicy, BrowserView, ConfirmAction, ConfirmReason, ConfirmView, ConflictView, Core,
    CoreConfig, CoreError, EntryView, ErrorCode, ExportCompat, FieldView, FrameRule, GeneratedPastView, GeneratedView, GeneratorOptions, ItemRevealed,
    ItemView, KdfCost, Lenient, LoginOutcome, Matching, Outcome, PairRequestView, PairedView, PassphraseOptions, PastView, PinnedGroup, Ports, Protection,
    Purpose, Refusal, RevealedField, ScoreView, SiteRuleView, Tri, UrlView,
};
use lockra_otp::{Algorithm, Digits, OtpKind, Period};
use lockra_transfer::{Incompatible, Origin, RejectReason};
use lockra_vault::FileKind;
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

const UPDATE_ENV: &str = "UPDATE_IPC_FIXTURES";
const T0: u64 = 1_790_000_000_000;
const SECRET: &str = "JBSWY3DPEHPK3PXP";
const LOGIN_PASSWORD: &str = "hunter2-correct-horse";
const LOGIN_NOTES: &str = "the recovery codes are 4815162342";
const LOGIN_FIELD: &str = "pin-90210";
/// What a password manager's export brings: its password, notes and hidden field.
const IMPORTED_PASSWORD: &str = "imported-hunter2-correct";
const IMPORTED_NOTES: &str = "imported notes 314159";
const IMPORTED_FIELD: &str = "imported-pin-2718";

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
        theme: ThemeId::GlassNight,
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
        codes_view: CodesView::Grid,
        auto_backup: AutoBackup { enabled: true, dir: Some("/home/user/Backups/Lockra".into()), keep: 7 },
        auto_update: true,
        default_unlock: DefaultUnlock::Password,
        biometric_offer: false,
        browser_integration: true,
        item_sort: lockra_core::settings::ItemSort::Recent,
        generator: GeneratorOptions::Passphrase(PassphraseOptions { words: 5, separator: " ".into(), capitalize: true, number: true }),
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
        UpdateStatus::Failed { code: ErrorCode::UpdateKeychain, at_ms: T0 - 4_000 },
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
        keyring_unsealed: true,
        key_saved: false,
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
            login: None,
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
    // A password manager's export: logins (a new one with its code right after it, one the vault
    // holds, one of another password), and an item it does not take.
    let export = || file("bitwarden_export.json");
    let login = |n: u32, title: &str, username: &str, host: &str, totp: Option<u32>, status: CandidateStatus, action: CandidateAction| {
        let mut view = candidate(n, export(), Origin::Uri, "", "", status, action);
        (view.kind, view.algorithm, view.digits) = (None, None, None);
        view.login = Some(LoginCandidateView { title: title.into(), username: username.into(), host: host.into(), totp });
        view
    };
    let mut card = candidate(12, export(), Origin::Uri, "", "Visa", CandidateStatus::Unsupported { reason: RejectReason::CardItem }, CandidateAction::Skip);
    (card.kind, card.algorithm, card.digits) = (None, None, None);
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
            login(8, "Dropbox", "alex@example.com", "www.dropbox.com", Some(9), CandidateStatus::New, CandidateAction::Add),
            candidate(9, export(), Origin::Uri, "Dropbox", "alex@example.com", CandidateStatus::New, CandidateAction::Add),
            login(10, "GitHub", "octocat", "github.com", None, CandidateStatus::LoginExists { item_id: id(20) }, CandidateAction::Skip),
            login(
                11,
                "Router",
                "admin",
                "",
                None,
                CandidateStatus::LoginConflict { item_id: id(24), reason: ConflictReason::Password },
                CandidateAction::Replace,
            ),
            card,
        ],
        google_batches: vec![GoogleBatchView { id: 412_337, size: 3, received: vec![0, 2], missing: vec![1] }],
        awaiting: Some(AwaitingView { name: "Passwords.kdbx".into(), kind: AwaitingKind::Kdbx, key_file: Some("Passwords.keyx".into()) }),
    }
}

fn state(phase: Phase) -> UiState {
    let unlocked = phase == Phase::Unlocked;
    UiState {
        app_version: "0.1.0".into(),
        platform: Platform::Linux,
        // A release build before password management ships, while locked; a debug build otherwise.
        features: Features { passwords: phase != Phase::Locked },
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
        items_rev: if unlocked { 7 } else { 8 },
        items_count: if unlocked { 2 } else { 0 },
        site_rules: if unlocked {
            vec![
                SiteRuleView { id: id(30), kind: "never_save".into(), domains: vec!["bank.example".into()] },
                SiteRuleView { id: id(31), kind: "equivalent".into(), domains: vec!["example.com".into(), "example.co.uk".into()] },
            ]
        } else {
            Vec::new()
        },
        generator: GeneratorStatus { history_count: if unlocked { 3 } else { 0 } },
        item_collapsed_groups: if unlocked { vec![String::new(), "Work".into()] } else { Vec::new() },
        browser: if unlocked { browser() } else { BrowserView { available: true, enabled: true, ..BrowserView::default() } },
        collapsed_groups: if unlocked { vec![String::new(), "Work".into()] } else { Vec::new() },
        entry_order: if unlocked { vec![id(2), id(1)] } else { Vec::new() },
        group_order: if unlocked { vec!["Work".into(), String::new()] } else { Vec::new() },
        pinned_group: unlocked.then_some(PinnedGroup::Favorites),
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
        Notice::ItemCopied { item_id: Some(id(20)), part: CopiedPart::Password, clear_after_s: Some(30) },
        Notice::ItemCopied { item_id: None, part: CopiedPart::Generated, clear_after_s: None },
        Notice::ItemSaved { item_id: id(20), title: "GitHub".into(), updated: true },
        Notice::BrowserPairRequested { id: id(40), browser: "chrome".into(), code: "123456".into() },
        Notice::BrowserConfirmRequested { id: id(41), browser: "firefox".into(), title: "Bank".into() },
        Notice::BrowserPaused { pairing: id(42), browser: "edge".into(), label: "Work".into() },
        Notice::BrowserUnlockRequested { browser: "chrome".into() },
        Notice::ClipboardCleared,
        Notice::Imported { added: 3, replaced: 1, skipped: 2, logins: LoginOutcome::default() },
        Notice::Imported { added: 1, replaced: 0, skipped: 0, logins: LoginOutcome { added: 12, replaced: 2, skipped: 3 } },
        Notice::FileUnrecognized { name: "notes.bin".into() },
        Notice::FileUnreadable { name: "huge.png".into() },
        Notice::BackupWritten { file_name: "lockra-auto-20260921-134640.lockrabackup".into(), automatic: true },
        Notice::BackupFailed { code: ErrorCode::BackupDirUnavailable },
        Notice::Restored { entries: 12 },
        Notice::AutoLocked,
        Notice::ExportExpired { session: id(100) },
        Notice::DeviceUnlockTurnedOff,
        Notice::SyncBrought { added: 2, updated: 1, removed: 0, logins: 3, devices: vec!["MacBook".into(), "Pixel 8".into()] },
        Notice::SyncBrought { added: 0, updated: 0, removed: 1, logins: 0, devices: vec![] },
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
        json!({"command": "view_order_entries", "ids": [id(2).to_string(), entry_id]}),
        json!({"command": "view_order_groups", "groups": ["Work", "", "Money"]}),
        json!({"command": "view_pin_group", "pinned": {"type": "group", "name": "Work"}}),
        json!({"command": "import_text", "text": "otpauth://totp/A:b?secret=GEZDGNBV"}),
        json!({"command": "import_clipboard"}),
        json!({"command": "import_backup_password", "password": "backup password"}),
        json!({"command": "import_key_file_clear"}),
        json!({"command": "import_commit", "choices": [{"id": 2, "action": "replace"}, {"id": 1, "action": "skip"}]}),
        json!({"command": "import_cancel"}),
        // Without a password: the biometric check that unlocks the vault, with the prompt's words.
        json!({"command": "export_start", "target": "google", "entry_ids": [entry_id], "reason": "export the accounts"}),
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
        // On a Lockra relay: its address alone (the space's access comes from the sync key).
        json!({"command": "sync_create", "storage": {"kind": "relay", "url": "https://lockra-relay.onethinker.top"}, "password": "a new password", "device_name": "Desktop"}),
        // With this device's own way to the space (needed when the invitation holds the sync key
        // alone: a space in another computer's folder).
        json!({
            "command": "sync_join", "source": {"type": "invite", "text": "lockra-invite:2:TEtTSU5WVDI", "code": "7K2QM-XW4FD", "storage": webdav_storage()},
            "password": "a new password", "device_name": "Pixel 8", "space_password": "another device's password"
        }),
        json!({"command": "sync_invite", "password": "a new password"}),
        json!({"command": "sync_invite_copy", "text": "lockra-invite:2:TEtTSU5WVDI"}),
        json!({"command": "sync_key_acknowledge"}),
        json!({"command": "sync_key_reveal", "password": "a new password"}),
        // A cloud drive's folder, as the interface names it: "the folder chosen", no path.
        json!({"command": "sync_set_storage", "storage": {"kind": "folder"}, "reason": "change the storage settings"}),
        json!({"command": "sync_rename_device", "name": "Work desktop"}),
        json!({"command": "sync_remove_device", "tag": PHONE_TAG}),
        json!({"command": "sync_now"}),
        json!({"command": "sync_disable"}),
        json!({"command": "items_list"}),
        // The password from the generator: the webview does not send it back.
        json!({"command": "item_add", "draft": {
            "title": "GitHub", "username": "octocat", "email": "octocat@example.com", "password": {"type": "generated"},
            "urls": [{"url": "https://github.com/login", "matching": "domain"}, {"url": "^https://github\\.com/session", "matching": "regex"}],
            "notes": "recovery codes in the safe", "tags": ["work"], "group": "Dev", "favorite": true, "color": "purple", "mark": "GH",
            "protection": "confirm", "autofill": {"on_page_load": "off", "auto_submit": "on", "fill_otp": "default"}, "totp_entry": entry_id,
            "fields": [{"label": "PIN", "value": {"type": "set", "value": "4242"}, "hidden": true, "page_field": "pin"}]
        }}),
        json!({"command": "item_update", "id": id(20).to_string(), "patch": {
            "title": "GitHub (work)", "password": {"type": "set", "value": "new password"}, "notes": {"type": "clear"}, "totp_entry": null,
            "fields": [{"id": id(21).to_string(), "remove": true}], "settle": [{"field": id(22).to_string(), "take": "1234567890"}]
        }}),
        json!({"command": "item_delete", "ids": [id(20).to_string()]}),
        json!({"command": "item_reveal", "id": id(20).to_string(), "parts": {"password": true, "notes": true, "history": true, "fields": [id(21).to_string()]}, "password": "a new password"}),
        json!({"command": "item_copy", "id": id(20).to_string(), "part": {"type": "field", "id": id(21).to_string()}, "reason": "copy the PIN"}),
        json!({"command": "generator_generate", "options": {"kind": "password", "length": 24, "lower": true, "upper": true, "digits": true, "symbols": false, "avoid_ambiguous": true, "exclude": "0O"}, "domain": "apple.com"}),
        json!({"command": "generator_copy"}),
        json!({"command": "generator_history_reveal", "reason": "show generated passwords"}),
        json!({"command": "generator_history_clear"}),
        json!({"command": "password_score", "password": "correct horse battery staple"}),
        json!({"command": "site_rule_add", "kind": "equivalent", "domains": ["example.com", "example.co.uk"]}),
        json!({"command": "site_rule_remove", "id": id(30).to_string()}),
        json!({"command": "view_collapse_items", "groups": ["", "Work"]}),
        json!({"command": "browser_pair_approve", "id": id(40).to_string(), "password": "a new password"}),
        json!({"command": "browser_pair_deny", "id": id(40).to_string()}),
        json!({"command": "browser_fill_confirm", "id": id(41).to_string(), "reason": "fill Bank in Firefox"}),
        json!({"command": "browser_fill_deny", "id": id(41).to_string()}),
        json!({"command": "browser_unpair", "id": id(42).to_string()}),
        json!({"command": "browser_resume"}),
        json!({"command": "browser_activity_clear"}),
        json!({"command": "browser_set_policy", "policy": {
            "fill_otp": true, "copy_otp": false, "auto_submit": false, "confirm_fill": true, "cross_origin_frames": "ask", "on_page_load": false
        }, "password": "a new password"}),
    ]
}

/// Settings › Browser with one of everything: two pairings (one paused), a pairing request, a fill
/// waiting for proof, and activity.
fn browser() -> BrowserView {
    BrowserView {
        available: true,
        enabled: true,
        paired: vec![
            PairedView {
                id: id(42),
                browser: "chrome".into(),
                label: "Work".into(),
                extension: "abcdefghijklmnopabcdefghijklmnop".into(),
                paired_at_ms: T0 - 86_400_000,
                last_seen_ms: Some(T0 - 60_000),
                connected: true,
                page_access: Some(true),
                paused: false,
            },
            PairedView {
                id: id(43),
                browser: "firefox".into(),
                label: String::new(),
                extension: "lockra@firlab.app".into(),
                paired_at_ms: T0 - 3_600_000,
                last_seen_ms: None,
                connected: false,
                page_access: None,
                paused: true,
            },
        ],
        pairing: vec![PairRequestView {
            id: id(40),
            code: "123456".into(),
            browser: "edge".into(),
            label: String::new(),
            extension: "ponmlkjihgfedcbaponmlkjihgfedcba".into(),
            expires_at_ms: T0 + 112_000,
        }],
        confirms: vec![ConfirmView {
            id: id(41),
            item_id: id(20),
            title: "Bank".into(),
            username: "me".into(),
            frame_url: "https://bank.example/login".into(),
            top_url: Some("https://portal.example/".into()),
            browser: "chrome".into(),
            label: "Work".into(),
            reasons: vec![ConfirmReason::Protected, ConfirmReason::CrossOrigin],
            action: ConfirmAction::Fill,
            needs_proof: true,
            expires_at_ms: T0 + 58_000,
        }],
        policy: BrowserPolicy { confirm_fill: true, cross_origin_frames: Lenient::Known(FrameRule::Ask), ..BrowserPolicy::default() },
        activity: vec![
            ActivityView {
                at_ms: T0 - 1_000,
                browser: "chrome".into(),
                label: "Work".into(),
                site: "github.com".into(),
                event: ActivityEvent::Filled,
                refusal: None,
                item_id: Some(id(20)),
                title: Some("GitHub".into()),
            },
            ActivityView {
                at_ms: T0 - 2_000,
                browser: "chrome".into(),
                label: "Work".into(),
                site: "evil.example".into(),
                event: ActivityEvent::Refused,
                refusal: Some(Refusal::CrossOrigin),
                item_id: None,
                title: None,
            },
        ],
    }
}

/// Logins as `items_list` shows them: one with every part, one with none but its name, one a
/// later Lockra wrote.
fn items() -> Vec<ItemView> {
    let full = ItemView {
        id: id(20),
        kind: "login".into(),
        title: "GitHub".into(),
        username: "octocat".into(),
        email: "octocat@example.com".into(),
        has_password: true,
        urls: vec![
            UrlView { url: "https://github.com/login".into(), matching: Matching::Domain },
            UrlView { url: "https://gist.github.com/".into(), matching: Matching::Host },
        ],
        has_notes: true,
        tags: vec!["work".into()],
        group: Some("Dev".into()),
        favorite: true,
        color: AccountColor::Purple,
        mark: Some("GH".into()),
        protection: Protection::Confirm,
        autofill: AutofillChoice { on_page_load: Tri::Off, auto_submit: Tri::On, fill_otp: Tri::Default },
        totp_entry: Some(id(1)),
        fields: vec![
            FieldView { id: id(21), label: "PIN".into(), hidden: true, value: None, has_value: true, page_field: Some("pin".into()) },
            FieldView { id: id(23), label: "Customer no.".into(), hidden: false, value: Some("C-77".into()), has_value: true, page_field: None },
        ],
        history_count: 2,
        conflicts: vec![ConflictView { field: id(22), purpose: Purpose::Password, label: String::new() }],
        source: Some("bitwarden".into()),
        created_at_ms: T0 - 86_400_000,
        updated_at_ms: T0 - 3_600_000,
        last_used_at_ms: Some(T0 - 60_000),
        read_only: false,
    };
    let bare = ItemView {
        id: id(24),
        kind: "login".into(),
        title: "Router".into(),
        username: String::new(),
        email: String::new(),
        has_password: false,
        urls: Vec::new(),
        has_notes: false,
        tags: Vec::new(),
        group: None,
        favorite: false,
        color: AccountColor::Auto,
        mark: None,
        protection: Protection::Fill,
        autofill: AutofillChoice::default(),
        totp_entry: None,
        fields: Vec::new(),
        history_count: 0,
        conflicts: Vec::new(),
        source: None,
        created_at_ms: T0,
        updated_at_ms: T0,
        last_used_at_ms: None,
        read_only: false,
    };
    let later = ItemView { id: id(25), kind: "bank_card".into(), title: "Card".into(), read_only: true, ..bare.clone() };
    vec![full, bare, later]
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
    phone.platform = Platform::Android;
    check(
        "state.json",
        &json!({ "unlocked": state(Phase::Unlocked), "locked": state(Phase::Locked), "no_vault": state(Phase::NoVault), "phone_locked": phone }),
    );
}

#[test]
fn update_fixtures() {
    let methods = [
        InstallMethod::Deb,
        InstallMethod::Rpm,
        InstallMethod::Appimage,
        InstallMethod::Nsis,
        InstallMethod::Msi,
        InstallMethod::App,
        InstallMethod::Android,
        InstallMethod::Play,
    ];
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
    let mut folder = sync_space(SyncStatus::Failed { code: ErrorCode::SyncFolderMissing, at_ms: T0 });
    folder.storage = StorageView::Folder { path: "C:\\Users\\me\\OneDrive\\Lockra".into() };
    views.push(SyncView { space: Some(folder) });
    let mut relay = sync_space(SyncStatus::Synced { at_ms: T0 });
    relay.storage = StorageView::Relay { url: "https://lockra-relay.onethinker.top".into() };
    views.push(SyncView { space: Some(relay) });
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
        ErrorCode::SyncInviteNeedsStorage.into(),
        ErrorCode::ImportKeyFileInvalid.into(),
        ErrorCode::FeatureUnavailable.into(),
        ErrorCode::ItemNotFound.into(),
        ErrorCode::ItemInvalid.into(),
        ErrorCode::ItemReadOnly.into(),
        ErrorCode::GeneratorRulesUnsatisfiable.into(),
        ErrorCode::BrowserUnavailable.into(),
        ErrorCode::BrowserNotPaired.into(),
        ErrorCode::BrowserPairExpired.into(),
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
            "import_outcome": Outcome { added: 3, replaced: 1, skipped: 2, logins: LoginOutcome { added: 12, replaced: 2, skipped: 3 } },
            "sync_key_reveal": SyncKeyView { sync_key: SYNC_KEY.into() },
            "sync_invite": SyncInvite {
                invite: "lockra-invite:1:eyJzdG9yYWdlIjp7fX0".into(),
                svg: "<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into(),
                shared: Some(SharedInvite { text: "lockra-invite:2:TEtTSU5WVDI".into(), code: "7K2QM-XW4FD".into() }),
                includes_storage: true,
            },
            // A space on a relay: its invitation is the pairing link, sent as it is.
            "sync_invite_relay": SyncInvite {
                invite: "lockra-invite:1:eyJzdG9yYWdlIjp7fX0".into(),
                svg: "<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into(),
                shared: None,
                includes_storage: true,
            },
            "items": items(),
            "item_added": { "id": id(20) },
            "item_revealed": ItemRevealed {
                item_id: id(20),
                password: Some("correct horse battery staple".into()),
                notes: Some("recovery codes in the safe".into()),
                fields: vec![RevealedField { id: id(21), value: "4242".into() }],
                history: Some(vec![
                    PastView { id: "1234567890".into(), field: id(22), value: "from the phone".into(), set_at_ms: T0 - 7_200_000, conflict: true },
                    PastView { id: "987654321".into(), field: id(22), value: "the first one".into(), set_at_ms: T0 - 86_400_000, conflict: false },
                ]),
            },
            "item_revealed_part": ItemRevealed { item_id: id(20), password: Some("correct horse battery staple".into()), notes: None, fields: Vec::new(), history: None },
            "generated": GeneratedView { password: "q7#Lm2vX9!pRt4zW8@kD".into(), bits: 128, rules_met: true },
            "generator_history": vec![
                GeneratedPastView { password: "q7#Lm2vX9!pRt4zW8@kD".into(), at_ms: T0 - 60_000, site: Some("apple.com".into()) },
                GeneratedPastView { password: "plaza-ribbon-onward-lucky-cosmic-fable".into(), at_ms: T0 - 3_600_000, site: None },
            ],
            "password_score": ScoreView { score: 1, warning: Some("common".into()) },
            "password_score_strong": ScoreView { score: 4, warning: None },
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
    // Setting up shows nothing: the key is shown on its own command.
    assert!(!parse(json!({"command": "sync_create", "storage": s3_storage(), "password": "x", "device_name": "d"})).shows_secret());
    assert!(parse(json!({"command": "sync_invite", "password": "x"})).shows_secret());
    assert!(parse(json!({"command": "sync_key_reveal", "password": "x"})).shows_secret());
    // Without a password the biometric check proves presence; the answer is the same secret.
    assert!(parse(json!({"command": "sync_invite", "reason": "show the invitation"})).shows_secret());
    assert!(parse(json!({"command": "sync_key_reveal", "reason": "show the recovery key"})).shows_secret());
    // The logins' secrets and the generator's passwords.
    assert!(parse(json!({"command": "item_reveal", "id": id(20), "parts": {"password": true}})).shows_secret());
    assert!(parse(json!({"command": "generator_generate", "options": {"kind": "passphrase"}})).shows_secret());
    assert!(parse(json!({"command": "generator_history_reveal", "reason": "x"})).shows_secret());
    assert!(!parse(json!({"command": "items_list"})).shows_secret());
    assert!(!parse(json!({"command": "item_copy", "id": id(20), "part": {"type": "password"}})).shows_secret(), "the clipboard, not the webview");
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

/// Every command dispatched on a real core: answers and events carry no secret, except the
/// answers whose purpose is to show one (the seven `shows_secret` names, docs/security.md).
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
        features: Features { passwords: true },
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
        json!({"command": "view_pin_group", "pinned": {"type": "group", "name": "Work"}}),
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
    // Logins: their password, notes and hidden field go in, and come back only when revealed.
    let draft = json!({
        "title": "GitHub", "username": "octocat", "password": {"type": "set", "value": LOGIN_PASSWORD}, "notes": LOGIN_NOTES,
        "fields": [{"label": "PIN", "value": {"type": "set", "value": LOGIN_FIELD}, "hidden": true}]
    });
    let added = ok(run(json!({"command": "item_add", "draft": draft})).await, &mut answers);
    let item_id = added["id"].as_str().unwrap().to_owned();
    let listed = ok(run(json!({"command": "items_list"})).await, &mut answers);
    let pin = listed[0]["fields"][0]["id"].clone();
    for command in [
        json!({"command": "item_update", "id": item_id, "patch": {"favorite": true, "urls": [{"url": "https://github.com/", "matching": "host"}]}}),
        json!({"command": "item_copy", "id": item_id, "part": {"type": "username"}}),
        json!({"command": "site_rule_add", "kind": "never_save", "domains": ["bank.example"]}),
        json!({"command": "password_score", "password": LOGIN_PASSWORD}),
    ] {
        ok(run(command).await, &mut answers);
    }
    let revealed =
        run(json!({"command": "item_reveal", "id": item_id, "parts": {"password": true, "notes": true, "history": true, "fields": [pin]}})).await.unwrap();
    assert_eq!(
        (revealed["password"].as_str(), revealed["notes"].as_str(), revealed["fields"][0]["value"].as_str()),
        (Some(LOGIN_PASSWORD), Some(LOGIN_NOTES), Some(LOGIN_FIELD))
    );
    let generated = run(json!({"command": "generator_generate", "options": {"kind": "password", "length": 32}})).await.unwrap();
    let generated_password = generated["password"].as_str().unwrap().to_owned();
    ok(run(json!({"command": "generator_copy"})).await, &mut answers);
    let history = run(json!({"command": "generator_history_reveal", "password": "correct horse battery"})).await.unwrap();
    assert_eq!(history[0]["password"].as_str(), Some(generated_password.as_str()));
    ok(run(json!({"command": "item_add", "draft": {"title": "Generated", "password": {"type": "generated"}}})).await, &mut answers);
    // A password manager's export: its password, notes and hidden field go into the core with the
    // file and come back only through item_reveal, neither in the preview nor in the list.
    let export = dir.path().join("bitwarden_export.json");
    let item = json!({"type": 1, "name": "Imported", "notes": IMPORTED_NOTES, "fields": [{"name": "PIN", "value": IMPORTED_FIELD, "type": 1}],
        "login": {"uris": [{"uri": "https://imported.example/"}], "username": "importer", "password": IMPORTED_PASSWORD}});
    std::fs::write(&export, json!({"encrypted": false, "folders": [], "items": [item]}).to_string()).unwrap();
    core.import_files(vec![export]).await.unwrap();
    ok(run(json!({"command": "app_state"})).await, &mut answers);
    let committed = ok(run(json!({"command": "import_commit"})).await, &mut answers);
    assert_eq!(committed["logins"]["added"], 1);
    ok(run(json!({"command": "items_list"})).await, &mut answers);
    ok(run(json!({"command": "generator_history_clear"})).await, &mut answers);
    let rule = run(json!({"command": "app_state"})).await.unwrap()["site_rules"][0]["id"].clone();
    ok(run(json!({"command": "site_rule_remove", "id": rule})).await, &mut answers);
    ok(run(json!({"command": "view_collapse_items", "groups": ["", "Gone"]})).await, &mut answers);
    // The browser extension's commands, without an extension: what can run does, the rest says why.
    for command in [
        json!({"command": "browser_resume"}),
        json!({"command": "browser_activity_clear"}),
        json!({"command": "browser_set_policy", "policy": {"fill_otp": false, "copy_otp": true, "auto_submit": false, "confirm_fill": true, "cross_origin_frames": "never", "on_page_load": false}}),
    ] {
        ok(run(command).await, &mut answers);
    }
    assert_eq!(run(json!({"command": "browser_pair_deny", "id": id(40)})).await.unwrap_err().code, ErrorCode::BrowserPairExpired);
    assert_eq!(run(json!({"command": "browser_unpair", "id": id(42)})).await.unwrap_err().code, ErrorCode::BrowserNotPaired);
    let riskier = json!({"command": "browser_set_policy", "policy": {"fill_otp": true, "copy_otp": true, "auto_submit": true, "confirm_fill": false, "cross_origin_frames": "ask", "on_page_load": true}, "password": "correct horse battery"});
    ok(run(riskier).await, &mut answers);
    // The clipboard holds a code again, for the check below.
    ok(run(json!({"command": "entry_copy", "id": entry_id})).await, &mut answers);
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
    ok(run(json!({"command": "sync_create", "storage": storage, "password": "correct horse battery", "device_name": "Desktop"})).await, &mut answers);
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    let revealed = run(json!({"command": "sync_key_reveal", "password": "correct horse battery"})).await.unwrap();
    let sync_key = revealed["sync_key"].as_str().unwrap().to_owned();
    assert!(sync_key.starts_with("LKS1-"));
    let invite = run(json!({"command": "sync_invite", "password": "correct horse battery"})).await.unwrap();
    assert!(invite["invite"].as_str().unwrap().starts_with("lockra-invite:1:") && invite.get("sync_key").is_none());
    ok(run(json!({"command": "sync_now"})).await, &mut answers);
    ok(run(json!({"command": "sync_rename_device", "name": "Work desktop"})).await, &mut answers);
    ok(run(json!({"command": "sync_set_storage", "storage": storage, "password": "correct horse battery"})).await, &mut answers);
    // A folder only as the dialog chose it: none chosen yet, and never a path the webview names.
    let folder = dir.path().join("Dropbox");
    std::fs::create_dir(&folder).unwrap();
    let named = json!({"kind": "folder", "path": folder});
    assert_eq!(
        run(json!({"command": "sync_set_storage", "storage": {"kind": "folder"}, "password": "correct horse battery"})).await.unwrap_err().code,
        ErrorCode::SyncFolderNotChosen
    );
    core.sync_choose_folder(&folder).unwrap();
    assert_eq!(run(json!({"command": "sync_set_storage", "storage": named, "password": "correct horse battery"})).await.unwrap_err().code, ErrorCode::Internal);
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
        for secret in [
            SECRET,
            "GEZDGNBVGY3TQOJQ",
            "MZXW6YTBOI",
            "secret=",
            FakeTransport::SECRET,
            &sync_key[5..19],
            "lockra-invite",
            LOGIN_PASSWORD,
            LOGIN_NOTES,
            LOGIN_FIELD,
            IMPORTED_PASSWORD,
            IMPORTED_NOTES,
            IMPORTED_FIELD,
            &generated_password,
        ] {
            assert!(!text.contains(secret), "{secret} leaked: {text}");
        }
    }
    assert!(clipboard.current().is_some_and(|c| c.len() == 6));
    // The remaining commands, each through `dispatch` once. The sealed invitation goes to the
    // clipboard and nothing comes back.
    let sealed = invite["shared"]["text"].as_str().unwrap();
    assert_eq!(run(json!({"command": "sync_invite_copy", "text": sealed})).await.unwrap(), Value::Null);
    assert_eq!(clipboard.current().as_deref(), Some(sealed));
    assert_eq!(run(json!({"command": "sync_invite_copy", "text": invite["invite"]})).await.unwrap_err().code, ErrorCode::SyncInviteInvalid);
    let bank = run(json!({"command": "app_state"})).await.unwrap()["entries"].as_array().unwrap().iter().find(|e| e["issuer"] == "Bank").unwrap()["id"].clone();
    ok(run(json!({"command": "entry_hotp_next", "id": bank})).await, &mut Vec::new());
    ok(run(json!({"command": "entry_delete", "id": bank})).await, &mut Vec::new());
    assert_eq!(run(json!({"command": "import_clipboard"})).await.unwrap_err().code, ErrorCode::ClipboardEmpty);
    assert_eq!(run(json!({"command": "import_backup_password", "password": "x"})).await.unwrap_err().code, ErrorCode::NoImport);
    assert_eq!(run(json!({"command": "import_key_file_clear"})).await.unwrap_err().code, ErrorCode::NoImport);
    ok(run(json!({"command": "item_delete", "ids": [item_id]})).await, &mut Vec::new());
    ok(run(json!({"command": "vault_change_password", "current": "correct horse battery", "new": "a new password"})).await, &mut Vec::new());
    ok(run(json!({"command": "device_unlock_disable", "password": "a new password"})).await, &mut Vec::new());
    ok(run(json!({"command": "vault_lock"})).await, &mut Vec::new());
    ok(run(json!({"command": "vault_unlock", "password": "a new password"})).await, &mut Vec::new());
    ok(run(json!({"command": "vault_lock"})).await, &mut Vec::new());
    ok(run(json!({"command": "vault_reset"})).await, &mut Vec::new());
    assert_eq!(run(json!({"command": "app_state"})).await.unwrap()["phase"], "no_vault");
}
