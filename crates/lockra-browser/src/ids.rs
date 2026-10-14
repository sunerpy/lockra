//! The browser extension's ids, defined once (plan §九): the native-messaging manifests the shell
//! writes allow exactly these, and the tests check them against the extension's own manifests
//! (`apps/extension/manifest/`). The store ids come when the first drafts are uploaded.

use data_encoding::{BASE64, HEXLOWER};
use sha2::{Digest, Sha256};

/// The Firefox add-on's id (`browser_specific_settings.gecko.id`), the same in every build: a Firefox
/// manifest's `allowed_extensions`.
pub const FIREFOX: &str = "lockra@firlab.app";

/// The unpacked development build's id in Chromium browsers (Chrome, Edge, Brave), fixed by the
/// public key in `apps/extension/manifest/dev.json`. Its private key was not kept: nothing is ever
/// signed with it, and anyone may load an unpacked build under this id, so only debug builds of
/// Lockra allow it.
pub const CHROMIUM_DEV: &str = "egbpficliljjnpkjbigonldakfllebco";

/// The Chrome Web Store's id, assigned when the first draft is uploaded: a placeholder until then
/// (PR 14 fills it in).
pub const CHROME_WEB_STORE: Option<&str> = None;

/// Microsoft Edge Add-ons' id, the same way: a placeholder until the first draft.
pub const EDGE_ADDONS: Option<&str> = None;

/// The Chromium extension ids a native-messaging manifest allows: the stores' (Edge installs from
/// the Chrome Web Store too, so both go in every Chromium manifest) and, in a debug build, the
/// development one.
pub fn chromium(debug: bool) -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = [CHROME_WEB_STORE, EDGE_ADDONS].into_iter().flatten().collect();
    if debug {
        ids.push(CHROMIUM_DEV);
    }
    ids
}

/// A Chromium manifest's `allowed_origins`: `chrome-extension://<id>/` for each of [`chromium`].
pub fn chromium_origins(debug: bool) -> Vec<String> {
    chromium(debug).into_iter().map(|id| format!("chrome-extension://{id}/")).collect()
}

/// A Chromium extension id: 32 letters from `a` to `p`.
pub fn is_chromium_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

/// The id Chromium gives an extension whose manifest `key` is `key_base64` (its public key, DER, in
/// base64): the first 128 bits of the key's SHA-256, each hex digit written as a letter `a`–`p`.
pub fn chromium_id_of_key(key_base64: &str) -> Option<String> {
    let der = BASE64.decode(key_base64.as_bytes()).ok()?;
    let hex = HEXLOWER.encode(&Sha256::digest(&der)[..16]);
    hex.chars().map(|digit| digit.to_digit(16).and_then(|n| char::from_digit(n + 10, 26))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn manifest(name: &str) -> serde_json::Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/extension/manifest").join(name);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn the_firefox_id_is_the_manifests() {
        assert_eq!(manifest("firefox.json")["browser_specific_settings"]["gecko"]["id"], FIREFOX);
    }

    #[test]
    fn the_development_id_is_the_one_its_key_gives() {
        let key = manifest("dev.json")["key"].as_str().unwrap().to_owned();
        assert_eq!(chromium_id_of_key(&key).as_deref(), Some(CHROMIUM_DEV));
        assert!(is_chromium_id(CHROMIUM_DEV));
        // The store builds carry no key: the stores give them their ids.
        assert!(manifest("base.json").get("key").is_none());
        assert!(manifest("chromium.json").get("key").is_none());
        assert_eq!(chromium_id_of_key("not base64!"), None);
    }

    #[test]
    fn the_store_ids_are_ids_once_known_and_the_release_allows_only_them() {
        for id in [CHROME_WEB_STORE, EDGE_ADDONS].into_iter().flatten() {
            assert!(is_chromium_id(id), "{id}");
        }
        assert!(!chromium(false).contains(&CHROMIUM_DEV));
        assert_eq!(chromium(true).last(), Some(&CHROMIUM_DEV));
        assert_eq!(chromium_origins(true).last().map(String::as_str), Some("chrome-extension://egbpficliljjnpkjbigonldakfllebco/"));
        assert!(!is_chromium_id("egbpficliljjnpkjbigonldakfllebc"));
        assert!(!is_chromium_id("egbpficliljjnpkjbigonldakfllebcz"));
    }
}
