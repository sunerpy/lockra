//! The mobile shell's command layer on Tauri's mock runtime: its commands are the bridge's phone
//! commands, a core command runs through `lockra_dispatch` with the core's typed errors, a copied
//! code goes to the clipboard port, the code stream subscribes through a channel, and what the
//! camera or the photo picker hands over reaches the import.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use lockra_bridge::PHONE_COMMANDS;
use lockra_core::fakes::FakeClipboard;
use lockra_core::ports::{PortError, SyncTransport as _};
use lockra_core::{Core, ErrorCode, KdfCost, PickedFile, StorageConfig};
use lockra_mobile_lib::scanner::Scan;
use lockra_mobile_lib::{COMMANDS, NoSync, ShellOptions, build_app, files, scanner};
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{App, Manager as _, WebviewWindow, WebviewWindowBuilder};
use tempfile::TempDir;
use zeroize::Zeroizing;

const PASSWORD: &str = "correct horse battery";

struct Shell {
    _dir: TempDir,
    app: App<MockRuntime>,
    webview: WebviewWindow<MockRuntime>,
    clipboard: Arc<FakeClipboard>,
}

fn shell() -> Shell {
    let dir = tempfile::tempdir().unwrap();
    let clipboard = Arc::new(FakeClipboard::default());
    let options = ShellOptions {
        clipboard: Some(clipboard.clone()),
        data_dir: Some(dir.path().join("data")),
        config_dir: Some(dir.path().join("config")),
        kdf: KdfCost::FAST_INSECURE,
        ..ShellOptions::default()
    };
    let mut app = build_app(mock_builder(), options).build(mock_context(noop_assets())).unwrap();
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
    // The setup hook (which starts the core) runs on the first turn of the event loop.
    #[allow(deprecated)]
    app.run_iteration(|_, _| {});
    Shell { _dir: dir, app, webview, clipboard }
}

impl Shell {
    fn invoke(&self, cmd: &str, body: Value) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        get_ipc_response(&self.webview, request).map(|body| body.deserialize::<Value>().unwrap())
    }

    fn dispatch(&self, command: Value) -> Result<Value, Value> {
        self.invoke("lockra_dispatch", json!({ "command": command }))
    }

    fn core(&self) -> Core {
        self.app.state::<Core>().inner().clone()
    }

    fn import(&self) -> Value {
        self.dispatch(json!({ "command": "app_state" })).unwrap()["import"].clone()
    }
}

#[test]
fn the_shell_registers_the_bridge_s_phone_commands() {
    assert_eq!(COMMANDS, PHONE_COMMANDS);
}

#[test]
fn what_the_camera_ends_with_reaches_the_import() {
    let shell = shell();
    shell.dispatch(json!({ "command": "vault_create", "password": PASSWORD })).unwrap();
    let core = shell.core();
    let uri = "otpauth://totp/Scanned:me?secret=MZXW6YTBOI&issuer=Scanned";
    assert!(scanner::import(&core, Ok(Scan::Code(Zeroizing::new(uri.into())))).unwrap());
    assert_eq!(shell.import()["candidates"][0]["source"], json!({ "type": "camera" }));
    assert!(!scanner::import(&core, Ok(Scan::Left)).unwrap());
    assert_eq!(scanner::import(&core, Ok(Scan::Denied)).unwrap_err().code, ErrorCode::CameraDenied);
    assert_eq!(scanner::import(&core, Ok(Scan::NoCamera)).unwrap_err().code, ErrorCode::CameraUnavailable);
    assert_eq!(scanner::import(&core, Err(PortError("gone".into()))).unwrap_err().code, ErrorCode::CameraUnavailable);
    // The app left while the camera was open: the vault locks then and there (on the runtime, as
    // the command runs).
    assert!(!tauri::async_runtime::block_on(async { scanner::import(&core, Ok(Scan::Away)) }).unwrap());
    assert_eq!(shell.dispatch(json!({ "command": "app_state" })).unwrap()["phase"], "locked");
    // This build has no camera, and its command says so.
    shell.dispatch(json!({ "command": "vault_unlock", "password": PASSWORD })).unwrap();
    let texts = json!({ "prompt": "Point the camera at a QR code", "cancel": "Cancel" });
    assert_eq!(shell.invoke("import_scan", texts).unwrap_err(), json!({ "code": "camera_unavailable" }));
}

#[test]
fn what_the_photo_picker_hands_over_reaches_the_import() {
    let shell = shell();
    shell.dispatch(json!({ "command": "vault_create", "password": PASSWORD })).unwrap();
    let core = shell.core();
    let list = b"otpauth://totp/Picked:me?secret=MZXW6YTBOI&issuer=Picked\n".to_vec();
    let picked = vec![PickedFile { name: "codes.txt".into(), bytes: Zeroizing::new(list) }];
    assert!(tauri::async_runtime::block_on(files::import(&core, Ok(picked))).unwrap());
    assert_eq!(shell.import()["candidates"][0]["source"], json!({ "type": "file", "name": "codes.txt" }));
    assert!(!tauri::async_runtime::block_on(files::import(&core, Ok(Vec::new()))).unwrap());
    assert_eq!(tauri::async_runtime::block_on(files::import(&core, Err(PortError("gone".into())))).unwrap_err().code, ErrorCode::IoFailed);
    // This build has no picker of either kind, and its command says so.
    assert_eq!(shell.invoke("import_pick_files", json!({ "kind": "images" })).unwrap_err(), json!({ "code": "io_failed" }));
    assert_eq!(shell.invoke("import_pick_files", json!({ "kind": "any" })).unwrap_err(), json!({ "code": "io_failed" }));
}

#[test]
fn a_vault_is_made_and_a_code_copied_through_the_phone_shell() {
    let shell = shell();
    assert_eq!(shell.dispatch(json!({ "command": "app_state" })).unwrap()["phase"], "no_vault");
    shell.dispatch(json!({ "command": "vault_create", "password": PASSWORD })).unwrap();
    let uri = "otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP&issuer=GitHub";
    let id = shell.dispatch(json!({ "command": "entry_add_uri", "uri": uri })).unwrap()["id"].clone();
    shell.dispatch(json!({ "command": "entry_copy", "id": id })).unwrap();
    let copied = shell.clipboard.current().unwrap();
    assert!(copied.len() == 6 && copied.chars().all(|c| c.is_ascii_digit()), "{copied}");
    // A locked vault answers with the core's code.
    shell.dispatch(json!({ "command": "vault_lock" })).unwrap();
    assert_eq!(shell.dispatch(json!({ "command": "vault_unlock", "password": "wrong" })).unwrap_err(), json!({ "code": "wrong_password" }));
    // Nothing to remember the vault with yet on the phone: no keychain, no fingerprint.
    let state = shell.dispatch(json!({ "command": "app_state" })).unwrap();
    assert_eq!(state["lock"]["device_unlock"]["available"], false);
    assert_eq!(state["lock"]["device_unlock"]["biometric"]["kind"], Value::Null);
}

#[test]
fn a_backup_goes_out_and_comes_back_through_the_file_picker() {
    let shell = shell();
    shell.dispatch(json!({ "command": "vault_create", "password": PASSWORD })).unwrap();
    shell.dispatch(json!({ "command": "entry_add_uri", "uri": "otpauth://totp/Kept:me?secret=MZXW6YTBOI&issuer=Kept" })).unwrap();
    let core = shell.core();
    let bytes = tauri::async_runtime::block_on(core.backup_sealed(None)).unwrap();
    // Saved: recorded under the name the picker gave; left: nothing recorded.
    assert_eq!(files::saved(&core, Ok(None)).unwrap(), None);
    assert_eq!(shell.dispatch(json!({ "command": "app_state" })).unwrap()["backup"]["last_backup_ms"], Value::Null);
    assert_eq!(files::saved(&core, Ok(Some("lockra-backup.lockrabackup".into()))).unwrap().as_deref(), Some("lockra-backup.lockrabackup"));
    assert!(shell.dispatch(json!({ "command": "app_state" })).unwrap()["backup"]["last_backup_ms"].is_number());
    assert_eq!(files::saved(&core, Err(PortError("gone".into()))).unwrap_err().code, ErrorCode::IoFailed);
    // Picked again, the backup opens for restoring.
    let picked = PickedFile { name: "lockra-backup.lockrabackup".into(), bytes: Zeroizing::new(bytes) };
    assert!(tauri::async_runtime::block_on(files::restore(&core, Ok(Some(picked)))).unwrap());
    let state = shell.dispatch(json!({ "command": "app_state" })).unwrap();
    assert_eq!(state["restore"]["file_name"], "lockra-backup.lockrabackup");
    assert!(!tauri::async_runtime::block_on(files::restore(&core, Ok(None))).unwrap());
    // This build has no file picker, and the commands say so; a list's password is checked first.
    assert_eq!(shell.invoke("backup_save", json!({ "separatePassword": null })).unwrap_err(), json!({ "code": "io_failed" }));
    assert_eq!(shell.invoke("restore_pick", json!({})).unwrap_err(), json!({ "code": "io_failed" }));
    let state = shell.dispatch(json!({ "command": "app_state" })).unwrap();
    let id = state["entries"][0]["id"].clone();
    let list = |password: &str| shell.invoke("export_otpauth_file", json!({ "entryIds": [id.clone()], "password": password }));
    assert_eq!(list("wrong password").unwrap_err(), json!({ "code": "wrong_password" }));
    assert_eq!(list(PASSWORD).unwrap_err(), json!({ "code": "io_failed" }));
    assert_eq!(files::listed(Ok(Some(files::LIST_NAME.into()))).unwrap().as_deref(), Some(files::LIST_NAME));
    assert_eq!(files::listed(Ok(None)).unwrap(), None);
}

#[test]
fn the_code_stream_subscribes_through_a_channel_and_stops() {
    let shell = shell();
    // Spelled in two halves: the scaffold check reads a literal double-underscore token as a
    // template placeholder.
    let channel = concat!("__", "CHANNEL__:7");
    assert_eq!(shell.invoke("codes_subscribe", json!({ "onFrame": channel })).unwrap(), Value::Null);
    assert!(shell.invoke("codes_subscribe", json!({ "onFrame": "not a channel" })).is_err());
    assert_eq!(shell.invoke("codes_unsubscribe", json!({})).unwrap(), Value::Null);
}

#[test]
fn the_phone_offers_no_sync_storage_yet() {
    let dav =
        StorageConfig::Webdav { url: "https://dav.example.com/".into(), prefix: String::new(), username: "me".into(), password: Zeroizing::new("pw".into()) };
    assert!(NoSync.open(&dav).is_err());
}
