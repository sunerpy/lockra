//! The shell's command layer on Tauri's mock runtime: the commands it registers are the ones the
//! bridge names, a core command runs through `lockra_dispatch` with the core's typed errors, the
//! webview cannot name a file path, and the code stream subscribes through a channel.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use lockra_bridge::SHELL_COMMANDS;
use lockra_core::KdfCost;
use lockra_core::fakes::{FakeClipboard, FakeTransport, FakeUpdater};
use lockra_core::ports::{MemorySecretStore, SyncTransport, Updater};
use lockra_core::ui::{BiometricKind, InstallMethod};
use lockra_desktop_lib::{COMMANDS, ShellOptions, build_app, dev_biometric_requested, dev_memory_store_requested};
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{App, Manager as _, WebviewWindow, WebviewWindowBuilder};
use tempfile::TempDir;

const PASSWORD: &str = "correct horse battery";

struct Shell {
    _dir: TempDir,
    app: App<MockRuntime>,
    webview: WebviewWindow<MockRuntime>,
}

fn shell() -> Shell {
    shell_with(None)
}

/// The shell with `updater` as its update source (none: this copy cannot update itself).
fn shell_with(updater: Option<Arc<dyn Updater>>) -> Shell {
    shell_on(updater, Some(Arc::new(FakeTransport::default())))
}

/// The shell with `updater`, and `sync` as its sync storage (none: the shell's own, lockra-remote).
fn shell_on(updater: Option<Arc<dyn Updater>>, sync: Option<Arc<dyn SyncTransport>>) -> Shell {
    let dir = tempfile::tempdir().unwrap();
    let options = ShellOptions {
        secrets: Some(Arc::new(MemorySecretStore::default())),
        clipboard: Some(Arc::new(FakeClipboard::default())),
        data_dir: Some(dir.path().join("data")),
        config_dir: Some(dir.path().join("config")),
        kdf: KdfCost::FAST_INSECURE,
        single_instance: false,
        updater,
        plugin_updates: false,
        sync,
    };
    let mut app = build_app(mock_builder(), options).build(mock_context(noop_assets())).unwrap();
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
    // Tauri runs the setup hook (which starts the core and wires the window) on the first turn of
    // the event loop; the mock runtime's turn returns at once.
    #[allow(deprecated)]
    app.run_iteration(|_, _| {});
    Shell { _dir: dir, app, webview }
}

impl Shell {
    fn invoke(&self, cmd: &str, body: Value) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: if cfg!(windows) { "http://tauri.localhost" } else { "tauri://localhost" }.parse().unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        get_ipc_response(&self.webview, request).map(|body| body.deserialize::<Value>().unwrap())
    }

    fn dispatch(&self, command: Value) -> Result<Value, Value> {
        self.invoke("lockra_dispatch", json!({ "command": command }))
    }

    fn core(&self) -> lockra_core::Core {
        self.app.state::<lockra_core::Core>().inner().clone()
    }

    fn state(&self) -> Value {
        self.dispatch(json!({ "command": "app_state" })).unwrap()
    }
}

/// Wait until `check` holds, for the core's background runs; fails after ten seconds.
fn until(what: &str, check: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !check() {
        assert!(std::time::Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// The `.lks` files under `root`, wherever they are.
fn snapshots(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

/// Two computers whose cloud drive keeps one folder in sync (here, the same folder), on the
/// shell's own storage: the folder chosen as the dialog does, an invitation with the key alone.
#[test]
fn two_computers_sync_through_a_cloud_drive_folder() {
    let drive = tempfile::tempdir().unwrap();
    let create = json!({ "command": "sync_create", "storage": { "kind": "folder" }, "password": PASSWORD, "device_name": "Windows" });
    let windows = shell_on(None, None);
    windows.dispatch(json!({ "command": "vault_create", "password": PASSWORD })).unwrap();
    windows.dispatch(json!({ "command": "entry_add_uri", "uri": "otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP&issuer=GitHub" })).unwrap();
    assert_eq!(windows.dispatch(create.clone()).unwrap_err(), json!({ "code": "sync_folder_not_chosen" }));
    windows.core().sync_choose_folder(drive.path()).unwrap();
    windows.dispatch(create).unwrap();
    assert_eq!(windows.state()["sync"]["space"]["storage"], json!({ "kind": "folder", "path": drive.path().display().to_string() }));
    let invite = windows.dispatch(json!({ "command": "sync_invite", "password": PASSWORD })).unwrap();
    assert_eq!(invite["includes_storage"], false);

    let mac = shell_on(None, None);
    mac.core().sync_choose_folder(drive.path()).unwrap();
    let source = json!({ "type": "invite", "text": invite["invite"], "storage": { "kind": "folder" } });
    mac.dispatch(json!({ "command": "sync_join", "source": source, "password": PASSWORD, "device_name": "MacBook" })).unwrap();
    until("the account on the Mac", || mac.state()["entries"].as_array().is_some_and(|e| e.iter().any(|x| x["issuer"] == "GitHub")));
    // One snapshot per device in the drive's folder, and nothing else of Lockra's.
    until("both snapshots", || snapshots(drive.path()).len() == 2);
    for path in snapshots(drive.path()) {
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("lks"), "{}", path.display());
        let bytes = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("GitHub"), "ciphertext only");
    }
    // Back the other way, by itself: the Mac writes its new account three seconds later, and
    // Windows, watching the folder, runs a second after it changed (its next look would come
    // only fifteen seconds after its last).
    mac.dispatch(json!({ "command": "entry_add_uri", "uri": "otpauth://totp/Mail:me?secret=GEZDGNBVGY3TQOJQ&issuer=Mail" })).unwrap();
    until("the account on Windows", || windows.state()["entries"].as_array().is_some_and(|e| e.iter().any(|x| x["issuer"] == "Mail")));
}

#[test]
fn the_shell_registers_exactly_the_commands_the_bridge_names() {
    assert_eq!(COMMANDS, SHELL_COMMANDS);
}

#[test]
fn a_core_command_runs_through_lockra_dispatch() {
    let shell = shell();
    let state = shell.dispatch(json!({ "command": "app_state" })).unwrap();
    assert_eq!(state["phase"], "no_vault");
    assert_eq!(shell.dispatch(json!({ "command": "vault_create", "password": PASSWORD })).unwrap(), Value::Null);
    let added = shell.dispatch(json!({ "command": "entry_add_uri", "uri": "otpauth://totp/Example:me?secret=JBSWY3DPEHPK3PXP" })).unwrap();
    assert!(added["id"].is_string());
    let state = shell.dispatch(json!({ "command": "app_state" })).unwrap();
    assert_eq!(state["phase"], "unlocked");
    assert_eq!(state["entries"].as_array().unwrap().len(), 1);
    // Nothing the webview receives in the state carries the secret.
    assert!(!state.to_string().contains("JBSWY3DPEHPK3PXP"));
    shell.dispatch(json!({ "command": "vault_lock" })).unwrap();
    let error = shell.dispatch(json!({ "command": "vault_unlock", "password": "wrong password" })).unwrap_err();
    assert_eq!(error["code"], "wrong_password");
    shell.dispatch(json!({ "command": "vault_unlock", "password": PASSWORD })).unwrap();
    assert_eq!(shell.dispatch(json!({ "command": "app_state" })).unwrap()["phase"], "unlocked");
}

#[test]
fn security_the_webview_cannot_name_a_file_path() {
    let shell = shell();
    shell.dispatch(json!({ "command": "vault_create", "password": PASSWORD })).unwrap();
    // The core can read files, but no command the webview may send carries a path: the shell
    // rejects such a message before the core sees it (an argument error, not a core error).
    let rejected = |result: Result<Value, Value>| {
        let error = result.unwrap_err();
        assert!(error.as_str().is_some_and(|e| e.contains("invalid args")), "{error}");
    };
    rejected(shell.dispatch(json!({ "command": "import_files", "paths": ["/etc/passwd"] })));
    rejected(shell.dispatch(json!({ "command": "restore_commit", "password": PASSWORD, "mode": "merge", "path": "/tmp/x" })));
    // Neither a shell command of that name nor the dialog plugin's own commands are reachable.
    assert!(shell.invoke("import_files", json!({ "paths": ["/etc/passwd"] })).is_err());
    assert!(shell.invoke("plugin:dialog|open", json!({})).is_err());
}

#[test]
fn the_code_stream_subscribes_through_a_channel() {
    let shell = shell();
    shell.dispatch(json!({ "command": "vault_create", "password": PASSWORD })).unwrap();
    // A webview channel id, as `new Channel()` serialises it. Spelled with concat! because the
    // scaffold check reads any literal double-underscore NAME token as a template placeholder.
    let channel = concat!("__", "CHANNEL__:7");
    assert_eq!(shell.invoke("codes_subscribe", json!({ "onFrame": channel })).unwrap(), Value::Null);
    assert!(shell.invoke("codes_subscribe", json!({ "onFrame": "not a channel" })).is_err());
    assert_eq!(shell.invoke("codes_unsubscribe", json!({})).unwrap(), Value::Null);
}

#[test]
fn the_password_is_checked_before_a_plain_export_opens_its_dialog() {
    let shell = shell();
    shell.dispatch(json!({ "command": "vault_create", "password": PASSWORD })).unwrap();
    let error = shell.invoke("export_otpauth_file", json!({ "entryIds": [], "password": "wrong password" })).unwrap_err();
    assert_eq!(error["code"], "wrong_password");
}

#[test]
fn the_update_commands_run_through_lockra_dispatch() {
    // A build without an update source refuses, with the core's code.
    let shell = shell();
    let state = shell.dispatch(json!({ "command": "app_state" })).unwrap();
    assert_eq!(state["update"], json!({ "method": null, "status": { "state": "idle" } }));
    assert_eq!(shell.dispatch(json!({ "command": "update_check" })).unwrap_err(), json!({ "code": "update_unavailable" }));

    let updater = Arc::new(FakeUpdater::installed(InstallMethod::Msi));
    *updater.check.lock() = Ok(Some(FakeUpdater::release("0.2.0")));
    let shell = shell_with(Some(updater.clone()));
    assert_eq!(shell.dispatch(json!({ "command": "update_check" })).unwrap(), Value::Null);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let update = loop {
        let update = shell.dispatch(json!({ "command": "app_state" })).unwrap()["update"].clone();
        if update["status"]["state"] == "available" || std::time::Instant::now() > deadline {
            break update;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert_eq!(update["method"], "msi");
    assert_eq!(update["status"]["version"], "0.2.0");
    assert_eq!(updater.calls(), ["check"]);
}

#[test]
fn only_a_debug_build_may_keep_the_keychain_in_memory() {
    assert!(dev_memory_store_requested(Some("memory"), true));
    assert!(!dev_memory_store_requested(Some("memory"), false));
    assert!(!dev_memory_store_requested(Some("file"), true));
    assert!(!dev_memory_store_requested(None, true));
}

#[test]
fn only_a_debug_build_may_stand_in_for_touch_id_or_windows_hello() {
    assert_eq!(dev_biometric_requested(Some("touch_id"), true), Some(BiometricKind::TouchId));
    assert_eq!(dev_biometric_requested(Some("windows_hello"), true), Some(BiometricKind::WindowsHello));
    assert_eq!(dev_biometric_requested(Some("touch_id"), false), None);
    assert_eq!(dev_biometric_requested(Some("windows_hello"), false), None);
    assert_eq!(dev_biometric_requested(Some("face"), true), None);
    assert_eq!(dev_biometric_requested(None, true), None);
}
