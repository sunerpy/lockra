use super::*;

fn places(home: &Path) -> Places {
    Places { home: home.to_owned(), local_data: home.join("AppData/Local"), roaming_data: home.join("AppData/Roaming"), dev_dir: None }
}

#[test]
fn every_browser_has_its_place_on_every_system() {
    let home = Path::new("/home/me");
    let at = |system, browser| manifest_path(system, browser, &places(home));
    assert_eq!(at(System::Linux, Browser::Chrome), home.join(".config/google-chrome/NativeMessagingHosts/dev.lockra.browser.json"));
    assert_eq!(at(System::Linux, Browser::Chromium), home.join(".config/chromium/NativeMessagingHosts/dev.lockra.browser.json"));
    assert_eq!(at(System::Linux, Browser::Edge), home.join(".config/microsoft-edge/NativeMessagingHosts/dev.lockra.browser.json"));
    assert_eq!(at(System::Linux, Browser::Brave), home.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts/dev.lockra.browser.json"));
    assert_eq!(at(System::Linux, Browser::Firefox), home.join(".mozilla/native-messaging-hosts/dev.lockra.browser.json"));
    let support = home.join("Library/Application Support");
    assert_eq!(at(System::MacOs, Browser::Chrome), support.join("Google/Chrome/NativeMessagingHosts/dev.lockra.browser.json"));
    assert_eq!(at(System::MacOs, Browser::Edge), support.join("Microsoft Edge/NativeMessagingHosts/dev.lockra.browser.json"));
    assert_eq!(at(System::MacOs, Browser::Firefox), support.join("Mozilla/NativeMessagingHosts/dev.lockra.browser.json"));
    // Windows: Lockra's own folder, one file for the Chromium browsers and one for Firefox.
    assert_eq!(at(System::Windows, Browser::Chrome), home.join("AppData/Local/Lockra/browser/dev.lockra.browser.chromium.json"));
    assert_eq!(at(System::Windows, Browser::Edge), at(System::Windows, Browser::Chrome));
    assert_eq!(at(System::Windows, Browser::Firefox), home.join("AppData/Local/Lockra/browser/dev.lockra.browser.firefox.json"));
    assert_eq!(registry_key(Browser::Chrome), r"Software\Google\Chrome\NativeMessagingHosts\dev.lockra.browser");
    assert_eq!(registry_key(Browser::Brave), registry_key(Browser::Chrome));
    assert_eq!(registry_key(Browser::Edge), r"Software\Microsoft\Edge\NativeMessagingHosts\dev.lockra.browser");
    assert_eq!(registry_key(Browser::Chromium), r"Software\Chromium\NativeMessagingHosts\dev.lockra.browser");
    assert_eq!(registry_key(Browser::Firefox), r"Software\Mozilla\NativeMessagingHosts\dev.lockra.browser");
    assert_eq!(profile_dir(System::Linux, Browser::Firefox, &places(home)), home.join(".mozilla"));
    assert_eq!(profile_dir(System::Windows, Browser::Edge, &places(home)), home.join("AppData/Local").join(r"Microsoft\Edge\User Data"));
}

#[test]
fn a_debug_build_may_put_the_chromium_manifests_where_a_test_browser_reads_them() {
    let home = Path::new("/home/me");
    let dev = Places { dev_dir: Some(PathBuf::from("/tmp/chrome-profile")), ..places(home) };
    assert_eq!(manifest_path(System::Linux, Browser::Chrome, &dev), PathBuf::from("/tmp/chrome-profile/NativeMessagingHosts/dev.lockra.browser.json"));
    // Firefox reads its own place (the test's HOME).
    assert_eq!(manifest_path(System::Linux, Browser::Firefox, &dev), home.join(".mozilla/native-messaging-hosts/dev.lockra.browser.json"));
}

#[test]
fn the_manifests_allow_the_extensions_and_point_at_the_program() {
    let program = Path::new("/opt/Lockra/lockra-desktop");
    let chrome = serde_json::to_value(manifest(Browser::Chrome, program, true)).unwrap();
    assert_eq!(chrome["name"], "dev.lockra.browser");
    assert_eq!(chrome["type"], "stdio");
    assert_eq!(chrome["path"], "/opt/Lockra/lockra-desktop");
    assert_eq!(chrome["allowed_origins"], serde_json::json!(ids::chromium_origins(true)));
    assert!(chrome.get("allowed_extensions").is_none());
    // A release allows the stores' ids alone, never the development one: Edge Add-ons', which the
    // self-install package has too.
    let release = serde_json::to_value(manifest(Browser::Edge, program, false)).unwrap();
    assert_eq!(release["allowed_origins"], serde_json::json!(["chrome-extension://flgfdomeabdfbebfjonaocddjcamgblk/"]));
    assert!(!release["allowed_origins"].as_array().unwrap().iter().any(|o| o.as_str().unwrap().contains(ids::CHROMIUM_DEV)));
    let firefox = serde_json::to_value(manifest(Browser::Firefox, program, false)).unwrap();
    assert_eq!(firefox["allowed_extensions"], serde_json::json!(["lockra@firlab.app"]));
    assert!(firefox.get("allowed_origins").is_none());
    assert_eq!(Browser::from_id("firefox"), Some(Browser::Firefox));
    assert_eq!(Browser::from_id("opera"), None);
}

#[cfg(not(windows))]
#[test]
fn manifests_are_written_refreshed_and_removed_under_a_home() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".config/google-chrome")).unwrap();
    let manifests = Manifests { system: System::Linux, places: places(home.path()), program: PathBuf::from("/opt/lockra"), debug: true };
    let status = |manifests: &Manifests| manifests.statuses().into_iter().map(|s| (s.browser.id(), s.installed, s.manifest)).collect::<Vec<_>>();
    assert_eq!(status(&manifests)[0], ("chrome", true, false));
    assert_eq!(status(&manifests)[2], ("firefox", false, false));
    manifests.write(Browser::Chrome).unwrap();
    manifests.write(Browser::Firefox).unwrap();
    assert_eq!(status(&manifests)[0], ("chrome", true, true));
    assert!(status(&manifests)[2].2);
    // The program moved (an update): a refresh points the written ones at it and writes no other.
    let moved = Manifests { program: PathBuf::from("/opt/lockra-new"), ..manifests };
    moved.refresh().unwrap();
    let written: serde_json::Value = serde_json::from_slice(&std::fs::read(manifest_path(System::Linux, Browser::Chrome, &moved.places)).unwrap()).unwrap();
    assert_eq!(written["path"], "/opt/lockra-new");
    assert!(!moved.written(Browser::Edge));
    for browser in Browser::ALL {
        moved.remove(browser).unwrap();
    }
    assert!(moved.statuses().iter().all(|s| !s.manifest));
}

#[cfg(windows)]
#[test]
fn manifests_and_their_keys_are_written_and_removed() {
    let home = tempfile::tempdir().unwrap();
    let root = format!("Software\\Lockra-Test\\{}", uuid::Uuid::new_v4());
    let manifests = Manifests {
        system: System::Windows,
        places: places(home.path()),
        program: PathBuf::from(r"C:\Program Files\Lockra\lockra-desktop.exe"),
        debug: true,
        registry_root: Some(root.clone()),
    };
    manifests.write(Browser::Chrome).unwrap();
    manifests.write(Browser::Edge).unwrap();
    assert!(manifests.written(Browser::Chrome) && manifests.written(Browser::Edge));
    assert!(!manifests.written(Browser::Firefox));
    // The Chromium file is shared: it stays while Edge's key points at it.
    manifests.remove(Browser::Chrome).unwrap();
    assert!(!manifests.written(Browser::Chrome));
    assert!(manifests.written(Browser::Edge));
    manifests.remove(Browser::Edge).unwrap();
    assert!(!manifests.path(Browser::Edge).exists());
    let _ = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).delete_subkey_all(root);
}
