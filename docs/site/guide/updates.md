# Updates, uninstalling and your data

This page explains how to move to a new version, where Lockra keeps its files, and how to remove
it completely.

## Updating

Lockra does not check for updates itself, because it never goes online. Watch the
[releases page](https://github.com/sunerpy/lockra/releases) (on GitHub, **Watch › Custom ›
Releases**) and install the new package over the old one. Your vault and settings are kept.

Before updating, make sure a recent backup exists: **Backup › Save backup**.

## Where the files are

|          | Windows                                      | macOS                                                            | Linux                                            |
| -------- | -------------------------------------------- | ---------------------------------------------------------------- | ------------------------------------------------ |
| Vault    | `%APPDATA%\dev.lockra.desktop\vault.lockra`  | `~/Library/Application Support/dev.lockra.desktop/vault.lockra`  | `~/.local/share/dev.lockra.desktop/vault.lockra` |
| Settings | `%APPDATA%\dev.lockra.desktop\settings.json` | `~/Library/Application Support/dev.lockra.desktop/settings.json` | `~/.config/dev.lockra.desktop/settings.json`     |

**Settings › About** shows the data folder of the computer you are on. Next to the vault Lockra
keeps `vault.lockra.prev`, the previous version, and the copies it makes before a replacing
restore (`pre-restore-<date>.lockrabackup`). The settings file holds no secrets.

## Moving to another computer

Save a backup, copy the file to the other computer, install Lockra there and choose **Restore a
backup** on the welcome screen. The accounts arrive with the backup's password as the master
password.

## Uninstalling

Uninstall Lockra the usual way for your system. The vault and the settings stay in the folders
above, so a later install finds them again; delete those folders to remove everything. If you used
**Remember on this device**, turn it off before uninstalling so the key in the system keychain is
deleted too.
