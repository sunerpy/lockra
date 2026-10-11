//! The relay reports Lockra's release version (package.json, which release-please bumps), not the
//! workspace crates' placeholder.

use std::fs;

fn main() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../package.json");
    println!("cargo:rerun-if-changed={manifest}");
    let version = fs::read_to_string(manifest).ok().and_then(|text| version_of(&text)).unwrap_or_else(|| "0.0.0".to_owned());
    println!("cargo:rustc-env=LOCKRA_VERSION={version}");
}

/// The value of the first `"version": "…"` in `json`.
fn version_of(json: &str) -> Option<String> {
    let rest = &json[json.find("\"version\"")? + "\"version\"".len()..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let value = &rest[..rest.find('"')?];
    (!value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))).then(|| value.to_owned())
}
