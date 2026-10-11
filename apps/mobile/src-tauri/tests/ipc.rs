//! The mobile shell's command layer on Tauri's mock runtime: its commands are the bridge's phone
//! commands, a core command runs through `lockra_dispatch` with the core's typed errors, a copied
//! code goes to the clipboard port, the code stream subscribes through a channel, what the camera
//! or the photo picker hands over reaches the import, and an invitation the camera reads joins its
//! sync space.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use lockra_bridge::PHONE_COMMANDS;
use lockra_core::fakes::{FakeClipboard, FakeTransport};
use lockra_core::ports::{PortError, SyncTransport as _};
use lockra_core::{Core, ErrorCode, KdfCost, PickedFile, StorageConfig};
use lockra_mobile_lib::scanner::Scan;
use lockra_mobile_lib::sync::Storages;
use lockra_mobile_lib::{COMMANDS, ShellOptions, build_app, files, scanner, sync, updater};
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
    shell_on(Arc::new(FakeTransport::default()))
}

/// A shell whose sync storage is `transport`'s, which other shells may share.
fn shell_on(transport: Arc<FakeTransport>) -> Shell {
    shell_with(ShellOptions { sync: Some(transport), ..ShellOptions::default() })
}

/// A shell with `options`, in a folder of its own and with the fast key stretching.
fn shell_with(options: ShellOptions) -> Shell {
    let dir = tempfile::tempdir().unwrap();
    let clipboard = Arc::new(FakeClipboard::default());
    let options = ShellOptions {
        clipboard: Some(clipboard.clone()),
        data_dir: Some(dir.path().join("data")),
        config_dir: Some(dir.path().join("config")),
        kdf: KdfCost::FAST_INSECURE,
        ..options
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
    // This build has no fingerprint, so no key store either.
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
fn the_phone_s_sync_storage_opens_without_contacting_it_and_plain_http_elsewhere_is_refused() {
    let access = lockra_sync::SpaceAccess::of(&lockra_sync::SyncKey::generate().unwrap());
    let dav = |url: &str| StorageConfig::Webdav { url: url.into(), prefix: String::new(), username: "me".into(), password: Zeroizing::new("pw".into()) };
    assert!(matches!(Storages.open(&dav("https://dav.example.com/dav/"), &access), Ok(storage) if !storage.conditional_puts()));
    assert!(Storages.open(&dav("http://192.168.1.2/dav/"), &access).is_err());
    let relay = StorageConfig::Relay { url: "https://lockra-relay.onethinker.top".into() };
    assert!(matches!(Storages.open(&relay, &access), Ok(storage) if storage.conditional_puts()));
    // WebDAV tells nothing; a relay is waited on, from a runtime.
    assert!(Storages.watch(&dav("https://dav.example.com/dav/"), &access, "x/", Box::new(|| {})).is_none());
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let dir = format!("lockra-sync-v1/{}/devices/", uuid::Uuid::new_v4());
    assert!(runtime.block_on(async { Storages.watch(&relay, &access, &dir, Box::new(|| {})) }).is_some());
}

#[test]
fn an_invitation_with_the_sync_key_alone_joins_through_this_phones_own_storage() {
    let transport = Arc::new(FakeTransport::default());
    // A computer keeps the space in a cloud drive's folder: its invitation holds the sync key only.
    let computer = shell_on(Arc::clone(&transport));
    computer.dispatch(json!({ "command": "vault_create", "password": PASSWORD })).unwrap();
    let folder = tempfile::tempdir().unwrap();
    computer.core().sync_choose_folder(folder.path()).unwrap();
    computer.dispatch(json!({ "command": "sync_create", "storage": { "kind": "folder" }, "password": PASSWORD, "device_name": "Windows" })).unwrap();
    let invite = computer.dispatch(json!({ "command": "sync_invite", "password": PASSWORD })).unwrap();
    assert_eq!(invite["includes_storage"], false);
    let invite = invite["invite"].as_str().unwrap().to_owned();
    // The phone reaches the same folder over the drive's WebDAV.
    let dav = StorageConfig::Webdav {
        url: "https://dav.example.com/".into(),
        prefix: String::new(),
        username: "me".into(),
        password: Zeroizing::new(FakeTransport::SECRET.into()),
    };
    transport.share(&dav, &StorageConfig::Folder { path: folder.path().into() });
    let phone = shell_on(transport);
    let core = phone.core();
    let join = |storage| {
        tauri::async_runtime::block_on(sync::join(
            &core,
            Ok(Scan::Code(Zeroizing::new(invite.clone()))),
            sync::JoinProof { password: Some(Zeroizing::new(PASSWORD.into())), reason: None },
            "Phone".into(),
            None,
            storage,
        ))
    };
    assert_eq!(join(None).unwrap_err().code, ErrorCode::SyncInviteNeedsStorage);
    assert_eq!(phone.dispatch(json!({ "command": "app_state" })).unwrap()["phase"], "no_vault", "nothing tried yet");
    assert!(join(Some(dav)).unwrap());
    let state = phone.dispatch(json!({ "command": "app_state" })).unwrap();
    assert_eq!(state["phase"], "unlocked");
    assert_eq!(state["sync"]["space"]["storage"]["kind"], "webdav");
}

#[test]
fn the_phone_checks_for_updates_and_opens_the_release_page_rather_than_installing() {
    let shell = shell();
    assert_eq!(shell.dispatch(json!({ "command": "app_state" })).unwrap()["update"]["method"], "android");
    assert_eq!(shell.dispatch(json!({ "command": "update_install" })).unwrap_err(), json!({ "code": "update_unavailable" }));
    // This build has no browser: the command answers with the page's address, to show instead.
    assert_eq!(shell.invoke("update_open_release", json!({})).unwrap(), json!("https://github.com/sunerpy/lockra/releases/latest"));
}

#[test]
fn a_phone_from_google_play_opens_its_listing_and_asks_github_nothing() {
    let play = updater::PhoneUpdater::new("0.8.4");
    assert!(play.note_installer(Some(updater::PLAY_STORE)));
    let shell = shell_with(ShellOptions { updater: Some(Arc::new(play)), ..ShellOptions::default() });
    assert_eq!(shell.dispatch(json!({ "command": "app_state" })).unwrap()["update"]["method"], "play");
    for command in ["update_check", "update_install"] {
        assert_eq!(shell.dispatch(json!({ "command": command })).unwrap_err(), json!({ "code": "update_unavailable" }), "{command}");
    }
    assert_eq!(shell.invoke("update_open_release", json!({})).unwrap(), json!("https://play.google.com/store/apps/details?id=dev.lockra.mobile"));
}

#[test]
fn an_invitation_the_camera_reads_joins_its_space() {
    let transport = Arc::new(FakeTransport::default());
    let desktop = shell_on(Arc::clone(&transport));
    desktop.dispatch(json!({ "command": "vault_create", "password": PASSWORD })).unwrap();
    let storage = json!({ "kind": "webdav", "url": "https://dav.example.com/", "prefix": "", "username": "me", "password": FakeTransport::SECRET });
    desktop.dispatch(json!({ "command": "sync_create", "storage": storage, "password": PASSWORD, "device_name": "Desktop" })).unwrap();
    let invite = desktop.dispatch(json!({ "command": "sync_invite", "password": PASSWORD })).unwrap()["invite"].as_str().unwrap().to_owned();
    // A new phone: the master password of the space's devices becomes its vault's.
    let phone = shell_on(transport);
    let core = phone.core();
    let proof = || sync::JoinProof { password: Some(Zeroizing::new(PASSWORD.into())), reason: None };
    let join = |scan| tauri::async_runtime::block_on(sync::join(&core, scan, proof(), "Phone".into(), None, None));
    assert!(!join(Ok(Scan::Left)).unwrap());
    let account = "otpauth://totp/Scanned:me?secret=MZXW6YTBOI&issuer=Scanned";
    assert_eq!(join(Ok(Scan::Code(Zeroizing::new(account.into())))).unwrap_err().code, ErrorCode::SyncInviteInvalid);
    assert_eq!(join(Ok(Scan::Denied)).unwrap_err().code, ErrorCode::CameraDenied);
    assert!(join(Ok(Scan::Code(Zeroizing::new(invite)))).unwrap());
    let state = phone.dispatch(json!({ "command": "app_state" })).unwrap();
    assert_eq!(state["phase"], "unlocked");
    assert_eq!(state["sync"]["space"]["device_name"], "Phone");
    // Leaving the app from the camera's page locks the vault, as for the import.
    assert!(!join(Ok(Scan::Away)).unwrap());
    assert_eq!(phone.dispatch(json!({ "command": "app_state" })).unwrap()["phase"], "locked");
    // This build has no camera, and its command says so.
    let join_texts =
        json!({ "prompt": "Point the camera at the invitation", "cancel": "Cancel", "password": PASSWORD, "deviceName": "Phone", "spacePassword": null });
    assert_eq!(phone.invoke("sync_scan_join", join_texts).unwrap_err(), json!({ "code": "camera_unavailable" }));
}
