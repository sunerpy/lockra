use super::*;

const MANIFEST_1: &[u8] = br#"{"manifest_version": 3, "version": "0.10.1"}"#;
const MANIFEST_2: &[u8] = br#"{"manifest_version": 3, "version": "0.10.2"}"#;

const FIRST: &[(&str, &[u8])] = &[("manifest.json", MANIFEST_1), ("background.js", b"one"), ("assets/old.js", b"old")];
const SECOND: &[(&str, &[u8])] = &[("manifest.json", MANIFEST_2), ("background.js", b"two"), ("assets/new.js", b"new")];

fn bundle(files: &'static [(&'static str, &'static [u8])]) -> Bundle<'static> {
    Bundle::new(files).unwrap()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// The folder's parent holds the folder alone: nothing of a staging or an old copy is left.
fn alone(folder: &Path) {
    let names: Vec<String> = std::fs::read_dir(folder.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["extension"]);
}

#[test]
fn a_bundle_is_a_manifest_with_a_version_and_its_files() {
    assert!(Bundle::new(&[("background.js", b"one")]).is_none());
    assert!(Bundle::new(&[("manifest.json", b"{}")]).is_none());
    assert!(Bundle::new(&[("manifest.json", b"not json")]).is_none());
    let first = bundle(FIRST);
    assert_eq!(first.version(), "0.10.1");
    assert!(first.digest().starts_with("sha256:") && first.digest().len() == 7 + 64);
}

#[test]
fn the_digest_covers_every_name_and_every_byte_in_any_order() {
    let digest = |files: &'static [(&'static str, &'static [u8])]| bundle(files).digest().to_owned();
    let base = digest(FIRST);
    assert_eq!(digest(&[("assets/old.js", b"old"), ("background.js", b"one"), ("manifest.json", MANIFEST_1)]), base);
    assert_ne!(digest(&[("manifest.json", MANIFEST_1), ("background.js", b"onE"), ("assets/old.js", b"old")]), base);
    assert_ne!(digest(&[("manifest.json", MANIFEST_1), ("background.js", b"one"), ("assets/olD.js", b"old")]), base);
    // A name and a content that only add up to the same bytes are still two files.
    assert_ne!(digest(&[("manifest.json", MANIFEST_1), ("ab", b"c")]), digest(&[("manifest.json", MANIFEST_1), ("a", b"bc")]));
}

#[test]
fn the_folder_is_in_lockras_local_data_folder() {
    let places = Places {
        home: PathBuf::from("/home/me"),
        local_data: PathBuf::from("/home/me/.local/share"),
        roaming_data: PathBuf::from("/home/me/.config"),
        dev_dir: None,
    };
    assert_eq!(folder(&places), Path::new("/home/me/.local/share").join("Lockra").join("browser").join("extension"));
}

#[test]
fn a_new_folder_gets_every_file_and_the_marker() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Lockra/browser/extension");
    let first = bundle(FIRST);
    assert_eq!(sync(&folder, &first).unwrap(), Synced::Written);
    assert_eq!(read(&folder.join("background.js")), "one");
    assert_eq!(read(&folder.join("assets/old.js")), "old");
    assert_eq!(marker(&folder), Some(Marker { version: "0.10.1".into(), digest: first.digest().into() }));
    let status = status(&folder, &first);
    assert!(status.ready && status.version == "0.10.1" && status.path == folder.display().to_string());
    alone(&folder);
}

#[test]
fn a_folder_that_holds_the_bundle_is_left_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("extension");
    let first = bundle(FIRST);
    sync(&folder, &first).unwrap();
    std::fs::write(folder.join("background.js"), "changed by hand").unwrap();
    assert_eq!(sync(&folder, &first).unwrap(), Synced::Unchanged);
    assert_eq!(read(&folder.join("background.js")), "changed by hand");
}

#[test]
fn another_bundle_takes_the_whole_folders_place() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("extension");
    sync(&folder, &bundle(FIRST)).unwrap();
    let second = bundle(SECOND);
    assert!(!status(&folder, &second).ready);
    assert_eq!(sync(&folder, &second).unwrap(), Synced::Written);
    assert_eq!(read(&folder.join("background.js")), "two");
    assert!(folder.join("assets/new.js").is_file());
    // Nothing of the old copy is left in the folder.
    assert!(!folder.join("assets/old.js").exists());
    assert_eq!(marker(&folder).unwrap().version, "0.10.2");
    assert!(status(&folder, &second).ready);
    alone(&folder);
}

#[test]
fn a_folder_without_a_readable_marker_is_written_again() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("extension");
    let first = bundle(FIRST);
    sync(&folder, &first).unwrap();
    std::fs::write(folder.join(MARKER), "{").unwrap();
    assert!(!status(&folder, &first).ready);
    assert_eq!(sync(&folder, &first).unwrap(), Synced::Written);
    assert!(status(&folder, &first).ready);
}

#[test]
fn what_an_interrupted_run_left_beside_the_folder_goes_first() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("extension");
    std::fs::create_dir_all(dir.path().join("extension.staging/assets")).unwrap();
    std::fs::write(dir.path().join("extension.staging/assets/half.js"), "half").unwrap();
    std::fs::create_dir_all(dir.path().join("extension.old")).unwrap();
    std::fs::write(dir.path().join("extension.old/background.js"), "older").unwrap();
    assert_eq!(sync(&folder, &bundle(SECOND)).unwrap(), Synced::Written);
    assert!(!folder.join("assets/half.js").exists());
    alone(&folder);
}

#[test]
fn a_name_that_is_not_plain_is_refused_and_the_folder_kept() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("extension");
    sync(&folder, &bundle(FIRST)).unwrap();
    for name in ["../escape.js", "/etc/x", "a//b", MARKER, ""] {
        let files = [("manifest.json", MANIFEST_2), (name, b"x".as_slice())];
        let error = sync(&folder, &Bundle::new(&files).unwrap()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{name:?}");
        assert_eq!(marker(&folder).unwrap().version, "0.10.1", "{name:?}");
    }
    assert!(!dir.path().join("escape.js").exists());
}

#[test]
fn the_build_carries_the_extension_it_was_made_with_or_none() {
    // build.rs embeds apps/extension/dist/chromium-unpacked when it has been built: then it names a
    // version (the root package.json's in a release build).
    if let Some(carried) = carried() {
        assert!(!carried.version().is_empty());
        assert!(carried.files.iter().any(|(name, _)| *name == "background.js"));
    }
}
