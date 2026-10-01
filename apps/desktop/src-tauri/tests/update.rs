//! The update adapter on the real tauri-plugin-updater, against a manifest server on 127.0.0.1:
//! nothing newer (204), a newer release whose package verifies, a signature made for another
//! version (`requireSignedVersion`), a package changed after signing, no manifest, no server.
//! Nothing is installed here: an executable `cargo test` built has no package to replace, and the
//! packaged app's update is checked by hand (docs/acceptance/updates.md).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use lockra_core::ports::{Release, UpdateFailure, Updater};
use lockra_core::ui::InstallMethod;
use lockra_desktop_lib::updater::PluginUpdater;
use serde_json::{Value, json};
use tauri::App;
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

/// A minisign key made for this test only (`minisign -G -W`), as tauri.conf.json carries one.
const PUBKEY: &str =
    "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXkgRjA2QzlDQTg0MjFCQzY0ClJXUmt2Q0dFeXNrR0R3ZzF3NFRFSTdvQ1diMHVJeWpzWDRiUk1rL3BKUVlHeXpXdTk1cEl2QXA3Cg==";
/// [`PACKAGE`] signed with that key, the trusted comment naming version 9.9.9 as the Tauri CLI
/// writes it.
const SIGNED_999: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIG1pbmlzaWduIHNlY3JldCBrZXkKUlVSa3ZDR0V5c2tHRHpraTVEbmJNdS9jMmdiZjZSUlduNW4veWFoY1NaK0U3a0VQbTZPSG9hSytjQnBqT0VWUDQvaExjSHFwdXBkK21ZN3gyZjRqN3FEaklhTGw2U2FJSXd3PQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkwMDAwMDAwCWZpbGU6TG9ja3JhXzkuOS45X2FtZDY0LkFwcEltYWdlCXZlcnNpb246OS45LjkKS0tYRGVDK21VaDB5NWhSdSt1YTAvUFZsUHVTZU5nMEN5WGRINllIcWFxTDhqakcya01vTkx4bDN3ZU91aEpjOGhvRG55S3ZZbytqUlFJcU9nSlJpQWc9PQo=";
/// The same package signed as version 9.9.8: valid, but not for the version announced.
const SIGNED_998: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIG1pbmlzaWduIHNlY3JldCBrZXkKUlVSa3ZDR0V5c2tHRHpraTVEbmJNdS9jMmdiZjZSUlduNW4veWFoY1NaK0U3a0VQbTZPSG9hSytjQnBqT0VWUDQvaExjSHFwdXBkK21ZN3gyZjRqN3FEaklhTGw2U2FJSXd3PQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkwMDAwMDAwCWZpbGU6TG9ja3JhXzkuOS45X2FtZDY0LkFwcEltYWdlCXZlcnNpb246OS45LjgKcUUyZ09wb2Z6YU8za1FMZXpSWVR1UXIzc05WcXdsQ3NBdTVadDJtcjFiRVM4K28zemkyR29mUHh0VlFyQjFYa1Zna21rS2VXcjBnZGRBeXFndGhFQlE9PQo=";
const PACKAGE: &[u8] = b"lockra update fixture\n";

/// Answers each path with a fixed status and body until the test process ends.
fn serve(listener: TcpListener, routes: Vec<(&'static str, u16, Vec<u8>)>) {
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            if reader.read_line(&mut request).is_err() {
                continue;
            }
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).map_or(true, |n| n == 0) || header == "\r\n" {
                    break;
                }
            }
            let path = request.split_whitespace().nth(1).unwrap_or_default();
            let (status, body) = routes.iter().find(|(p, ..)| *p == path).map_or((404, Vec::new()), |(_, s, b)| (*s, b.clone()));
            let head = match status {
                204 => "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_owned(),
                _ => format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n", body.len()),
            };
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
}

/// A manifest announcing 9.9.9 with `signature` for every desktop target, in the layout
/// tauri-release.py writes.
fn manifest(base: &str, signature: &str) -> Vec<u8> {
    let platform = json!({ "url": format!("{base}/package"), "signature": signature });
    let platforms: serde_json::Map<String, Value> = ["linux-x86_64", "linux-aarch64", "windows-x86_64", "windows-aarch64", "darwin-x86_64", "darwin-aarch64"]
        .into_iter()
        .map(|key| (key.to_owned(), platform.clone()))
        .collect();
    serde_json::to_vec(&json!({ "version": "9.9.9", "notes": "## 9.9.9\n\n- What changed", "pub_date": "2026-10-02T08:00:00Z", "platforms": platforms }))
        .unwrap()
}

/// The app with the plugin, configured as tauri.conf.json does but for this server and key.
fn updater(base: &str) -> (App<MockRuntime>, PluginUpdater<MockRuntime>) {
    let mut context = mock_context(noop_assets());
    context.config_mut().plugins.0.insert(
        "updater".to_owned(),
        json!({
            "pubkey": PUBKEY,
            "endpoints": [format!("{base}/latest.json")],
            "requireSignedVersion": true,
            "dangerousInsecureTransportProtocol": true,
        }),
    );
    let app = mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).build(context).unwrap();
    let port = PluginUpdater::new(app.handle().clone(), Some(InstallMethod::Appimage));
    (app, port)
}

fn server(routes: impl FnOnce(&str) -> Vec<(&'static str, u16, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    serve(listener, routes(&base));
    base
}

/// Bytes so far and the size, as the progress callback sees them.
type Progress = Vec<(u64, Option<u64>)>;

fn download(port: &PluginUpdater<MockRuntime>) -> (Result<(), UpdateFailure>, Progress) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    let result = tauri::async_runtime::block_on(port.download(Box::new(move |received, total| record.lock().unwrap().push((received, total)))));
    let progress = seen.lock().unwrap().clone();
    (result, progress)
}

#[test]
fn nothing_newer_answers_none() {
    let base = server(|_| vec![("/latest.json", 204, Vec::new())]);
    let (_app, port) = updater(&base);
    assert_eq!(port.method(), Some(InstallMethod::Appimage));
    assert_eq!(tauri::async_runtime::block_on(port.check()), Ok(None));
}

#[test]
fn a_newer_release_downloads_and_its_signature_verifies() {
    let base = server(|base| vec![("/latest.json", 200, manifest(base, SIGNED_999)), ("/package", 200, PACKAGE.to_vec())]);
    let (_app, port) = updater(&base);
    let release = tauri::async_runtime::block_on(port.check()).unwrap();
    assert_eq!(release, Some(Release { version: "9.9.9".into(), notes: Some("## 9.9.9\n\n- What changed".into()), date: Some("2026-10-02T08:00:00Z".into()) }));
    let (result, progress) = download(&port);
    assert_eq!(result, Ok(()));
    let size = u64::try_from(PACKAGE.len()).unwrap();
    assert_eq!(progress.last().map(|(received, _)| *received), Some(size), "progress reaches the package's size");
}

#[test]
fn security_a_signature_for_another_version_is_refused() {
    // A genuine, older package offered as 9.9.9: the forced downgrade requireSignedVersion stops.
    let base = server(|base| vec![("/latest.json", 200, manifest(base, SIGNED_998)), ("/package", 200, PACKAGE.to_vec())]);
    let (_app, port) = updater(&base);
    assert!(tauri::async_runtime::block_on(port.check()).unwrap().is_some());
    assert_eq!(download(&port).0, Err(UpdateFailure::Signature));
    assert_eq!(tauri::async_runtime::block_on(port.install()), Err(UpdateFailure::Install), "nothing verified, nothing to install");
}

#[test]
fn security_a_package_changed_after_signing_is_refused() {
    let base = server(|base| vec![("/latest.json", 200, manifest(base, SIGNED_999)), ("/package", 200, b"lockra update fixture!\n".to_vec())]);
    let (_app, port) = updater(&base);
    assert!(tauri::async_runtime::block_on(port.check()).unwrap().is_some());
    assert_eq!(download(&port).0, Err(UpdateFailure::Signature));
}

#[test]
fn a_missing_manifest_is_invalid_and_no_server_is_a_network_failure() {
    let base = server(|_| Vec::new());
    let (_app, port) = updater(&base);
    assert_eq!(tauri::async_runtime::block_on(port.check()), Err(UpdateFailure::Invalid));

    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    };
    let (_app, port) = updater(&closed);
    assert_eq!(tauri::async_runtime::block_on(port.check()), Err(UpdateFailure::Network));
}

#[test]
fn nothing_is_downloaded_or_installed_before_a_check_finds_a_release() {
    let base = server(|_| vec![("/latest.json", 204, Vec::new())]);
    let (_app, port) = updater(&base);
    assert_eq!(download(&port).0, Err(UpdateFailure::Invalid));
    assert_eq!(tauri::async_runtime::block_on(port.install()), Err(UpdateFailure::Install));
}
