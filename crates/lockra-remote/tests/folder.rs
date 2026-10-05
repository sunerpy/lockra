//! Devices meeting in a folder that a cloud drive keeps in sync: the sync step over
//! [`FolderStore`], with the drive played here by copying files from one computer's folder to
//! another's, late, in pieces, or beside conflicted copies of its own.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use lockra_remote::FolderStore;
use lockra_sync::{Outcome, RemoteStore, Replica, Space, SpaceKeys, SyncError, SyncState, devices_dir, step};
use uuid::Uuid;
use zeroize::Zeroizing;

/// A replica of a set of names: what a device has, merged by union.
#[derive(Default)]
struct Names(BTreeSet<String>);

impl Replica for Names {
    fn payload(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(serde_json::to_vec(&self.0).unwrap())
    }

    fn absorb(&mut self, payload: &[u8]) -> Result<bool, SyncError> {
        let theirs: BTreeSet<String> = serde_json::from_slice(payload).map_err(|_| SyncError::Corrupted)?;
        let before = self.0.len();
        self.0.extend(theirs);
        Ok(self.0.len() > before)
    }
}

/// A device of the space: its number, its state, its replica.
struct Device {
    number: u64,
    name: &'static str,
    state: SyncState,
    names: Names,
}

impl Device {
    fn new(number: u64, name: &'static str, names: &[&str]) -> Self {
        Self { number, name, state: SyncState::default(), names: Names(names.iter().map(|n| (*n).to_owned()).collect()) }
    }

    async fn sync(&mut self, store: &dyn RemoteStore, keys: &SpaceKeys) -> Result<Outcome, SyncError> {
        let space = Space { prefix: "", keys, device: self.number, device_name: self.name, keyring: b"keyring" };
        step(store, &space, &mut self.state, &mut self.names, 1_000).await
    }

    fn has(&self, names: &[&str]) -> bool {
        names.iter().all(|n| self.names.0.contains(*n))
    }
}

/// The space's devices directory under `root`.
fn devices(root: &Path, keys: &SpaceKeys) -> std::path::PathBuf {
    root.join(devices_dir("", keys.space_id()).trim_end_matches('/'))
}

/// What a cloud drive does between two computers: every snapshot of `from` copied to `to`.
fn carry(from: &Path, to: &Path, keys: &SpaceKeys) {
    let (from, to) = (devices(from, keys), devices(to, keys));
    fs::create_dir_all(&to).unwrap();
    for entry in fs::read_dir(&from).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    names
}

#[tokio::test]
async fn two_devices_meet_in_one_folder() {
    let folder = tempfile::tempdir().unwrap();
    let store = FolderStore::new(folder.path());
    let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
    let mut laptop = Device::new(1, "Laptop", &["GitHub"]);
    let mut desktop = Device::new(2, "Desktop", &["Mail"]);
    laptop.sync(&store, &keys).await.unwrap();
    desktop.sync(&store, &keys).await.unwrap();
    laptop.sync(&store, &keys).await.unwrap();
    assert!(laptop.has(&["GitHub", "Mail"]) && desktop.has(&["GitHub", "Mail"]));
    // One file per device, and nothing else for the drive to carry.
    let tags = [keys.device_tag(1), keys.device_tag(2)];
    let mut expected: Vec<String> = tags.iter().map(|t| format!("{t}.lks")).collect();
    expected.sort();
    assert_eq!(files(&devices(folder.path(), &keys)), expected);
    // Nothing new: nothing written again.
    assert!(!laptop.sync(&store, &keys).await.unwrap().wrote);
}

#[tokio::test]
async fn a_drive_that_carries_files_late_and_in_pieces_loses_nothing() {
    let (pc, mac) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (pc_store, mac_store) = (FolderStore::new(pc.path()), FolderStore::new(mac.path()));
    let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
    let mut windows = Device::new(1, "Windows", &["GitHub"]);
    let mut macbook = Device::new(2, "MacBook", &["Mail"]);
    windows.sync(&pc_store, &keys).await.unwrap();
    macbook.sync(&mac_store, &keys).await.unwrap();
    // The drive has carried nothing yet: each sees itself alone.
    assert!(!macbook.has(&["GitHub"]));

    // Windows' snapshot arrives cut short (the drive writes it as it downloads): it does not open,
    // is not taken as a deletion, and is read again once whole.
    let windows_file = format!("{}.lks", keys.device_tag(1));
    let whole = fs::read(devices(pc.path(), &keys).join(&windows_file)).unwrap();
    fs::write(devices(mac.path(), &keys).join(&windows_file), &whole[..whole.len() / 2]).unwrap();
    let outcome = macbook.sync(&mac_store, &keys).await.unwrap();
    assert_eq!(outcome.unreadable, [keys.device_tag(1)]);
    assert!(!macbook.has(&["GitHub"]));
    carry(pc.path(), mac.path(), &keys);
    let outcome = macbook.sync(&mac_store, &keys).await.unwrap();
    assert!(outcome.unreadable.is_empty() && macbook.has(&["GitHub", "Mail"]));

    // The other way, the drive's conflicted copies beside the snapshots change nothing.
    carry(mac.path(), pc.path(), &keys);
    let pc_devices = devices(pc.path(), &keys);
    let mac_file = format!("{}.lks", keys.device_tag(2));
    fs::copy(pc_devices.join(&mac_file), pc_devices.join(format!("{} (MacBook's conflicted copy).lks", keys.device_tag(2)))).unwrap();
    fs::write(pc_devices.join(format!("{}.lks.tmp", keys.device_tag(2))), b"half").unwrap();
    let outcome = windows.sync(&pc_store, &keys).await.unwrap();
    assert!(outcome.unreadable.is_empty() && outcome.rolled_back.is_empty(), "{outcome:?}");
    assert!(windows.has(&["GitHub", "Mail"]));
}

#[tokio::test]
async fn a_drive_that_brings_an_older_snapshot_back_is_found_out() {
    let folder = tempfile::tempdir().unwrap();
    let store = FolderStore::new(folder.path());
    let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
    let mut laptop = Device::new(1, "Laptop", &["GitHub"]);
    let mut desktop = Device::new(2, "Desktop", &[]);
    laptop.sync(&store, &keys).await.unwrap();
    let laptop_file = devices(folder.path(), &keys).join(format!("{}.lks", keys.device_tag(1)));
    let first = fs::read(&laptop_file).unwrap();
    desktop.sync(&store, &keys).await.unwrap();
    laptop.names.0.insert("Bank".into());
    laptop.sync(&store, &keys).await.unwrap();
    desktop.sync(&store, &keys).await.unwrap();
    assert!(desktop.has(&["GitHub", "Bank"]));
    // The drive's version history puts the first one back.
    fs::write(&laptop_file, &first).unwrap();
    let outcome = desktop.sync(&store, &keys).await.unwrap();
    assert_eq!(outcome.rolled_back, [keys.device_tag(1)]);
    // The laptop sees its own file went back and writes it again.
    assert!(laptop.sync(&store, &keys).await.unwrap().wrote);
}

#[tokio::test]
async fn a_copied_vault_writing_under_the_same_name_is_found() {
    let folder = tempfile::tempdir().unwrap();
    let store = FolderStore::new(folder.path());
    let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
    let mut original = Device::new(1, "Laptop", &["GitHub"]);
    original.sync(&store, &keys).await.unwrap();
    let mut copy = Device { number: 1, name: "Laptop", state: original.state.clone(), names: Names(original.names.0.clone()) };
    copy.names.0.insert("Mail".into());
    copy.sync(&store, &keys).await.unwrap();
    original.names.0.insert("Bank".into());
    assert_eq!(original.sync(&store, &keys).await.err(), Some(SyncError::DeviceClash));
}

#[tokio::test]
async fn a_folder_that_went_away_stops_the_run_and_is_not_made_again() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("Dropbox");
    fs::create_dir(&root).unwrap();
    let store = FolderStore::new(&root);
    let keys = SpaceKeys::generate(Uuid::new_v4()).unwrap();
    let mut laptop = Device::new(1, "Laptop", &["GitHub"]);
    laptop.sync(&store, &keys).await.unwrap();
    fs::remove_dir_all(&root).unwrap();
    laptop.names.0.insert("Bank".into());
    assert_eq!(laptop.sync(&store, &keys).await.err(), Some(SyncError::FolderMissing));
    assert!(!root.exists());
}
