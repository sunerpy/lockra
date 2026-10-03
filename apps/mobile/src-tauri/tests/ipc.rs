//! The mobile shell's command layer on Tauri's mock runtime: its commands are names the bridge
//! knows, a core command runs through `lockra_dispatch` with the core's typed errors, a copied code
//! goes to the clipboard port, and the code stream subscribes through a channel.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use lockra_bridge::SHELL_COMMANDS;
use lockra_core::fakes::FakeClipboard;
use lockra_core::ports::SyncTransport as _;
use lockra_core::{KdfCost, StorageConfig};
use lockra_mobile_lib::{COMMANDS, NoSync, ShellOptions, build_app};
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewWindow, WebviewWindowBuilder};
use tempfile::TempDir;
use zeroize::Zeroizing;

const PASSWORD: &str = "correct horse battery";

struct Shell {
    _dir: TempDir,
    _app: App<MockRuntime>,
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
    Shell { _dir: dir, _app: app, webview, clipboard }
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
}

#[test]
fn the_shell_registers_only_commands_the_bridge_names() {
    assert!(COMMANDS.iter().all(|name| SHELL_COMMANDS.contains(name)), "{COMMANDS:?}");
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
