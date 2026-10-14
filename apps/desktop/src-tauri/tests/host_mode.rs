#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Host mode as a browser starts it (plan §四): the real `lockra-desktop` with the extension's
//! origin as its first argument, frames on its stdin and stdout in the machine's byte order. With
//! no Lockra running (its folders are a fresh temporary home), a `hello` is answered by the host
//! itself, and the host ends with the browser's end of stdin. CI runs it in the release build on
//! Windows, where Lockra is a GUI-subsystem program reading and writing the pipes it was given.

use std::io::{Read as _, Write as _};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn frame(json: &str) -> Vec<u8> {
    let mut out = u32::try_from(json.len()).unwrap().to_ne_bytes().to_vec();
    out.extend_from_slice(json.as_bytes());
    out
}

fn envelope(plain: &str) -> String {
    use data_encoding::BASE64;
    format!(r#"{{"v":1,"t":"hello","d":"{}"}}"#, BASE64.encode(plain.as_bytes()))
}

#[test]
fn a_browsers_hello_is_told_lockra_is_not_running() {
    let home = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_lockra-desktop"));
    command
        .arg("chrome-extension://egbpficliljjnpkjbigonldakfllebco/")
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_RUNTIME_DIR", home.path().join("run"))
        .env("APPDATA", home.path().join("AppData/Roaming"))
        .env("LOCALAPPDATA", home.path().join("AppData/Local"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut host = command.spawn().unwrap();
    let mut stdin = host.stdin.take().unwrap();
    let mut stdout = host.stdout.take().unwrap();
    stdin.write_all(&frame(&envelope(r#"{"op":"hello","proto_min":1,"proto_max":1,"browser":"chrome","extension":"e","version":"0.10.0"}"#))).unwrap();
    stdin.flush().unwrap();
    let mut length = [0u8; 4];
    stdout.read_exact(&mut length).unwrap();
    let mut answer = vec![0u8; usize::try_from(u32::from_ne_bytes(length)).unwrap()];
    stdout.read_exact(&mut answer).unwrap();
    let answer: serde_json::Value = serde_json::from_slice(&answer).unwrap();
    assert_eq!(answer["t"], "hello");
    let plain = data_encoding::BASE64.decode(answer["d"].as_str().unwrap().as_bytes()).unwrap();
    assert_eq!(String::from_utf8(plain).unwrap(), r#"{"op":"state","state":"not_running"}"#);
    // The browser closes its end: the host ends, with success.
    drop(stdin);
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = host.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "the host did not end with its stdin");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "{status}");
}
