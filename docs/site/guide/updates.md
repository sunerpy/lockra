# Updates, uninstalling and your data

This page explains how to move to a new version, where Lockra keeps its files, and how to remove
it completely.

## Updating

From version 0.2.0, Lockra updates itself. When a newer version is out, the title bar shows a
note such as **Version 0.3.0**; choose it to open the update dialog with the notes of that
version. **Update now** downloads the package, checks its signature and installs it, and Lockra
restarts. During the download the dialog shows the progress, the speed and the time left;
**Download in the background** closes the dialog and the download continues. Your vault and
settings are kept. Before updating, make sure a recent backup exists: **Backup › Save backup**.

To look for a new version yourself, choose **Check for updates** in **Settings › General**, or in
**Settings › About**. Lockra 0.2.0 has these controls in **Settings › About** only: **Check for
updates**, then **Download and install**.

How the update installs depends on how Lockra was installed:

| Installed from    | The update                                                                |
| ----------------- | ------------------------------------------------------------------------- |
| `.deb` or `.rpm`  | installs the new package; the system asks for an administrator's password |
| AppImage          | replaces the AppImage file, then Lockra restarts                          |
| Windows installer | the installer closes Lockra and opens it again when it is done            |
| macOS app         | replaces the app, then Lockra restarts                                    |

Lockra installs a package only when it carries Lockra's signature and was signed for the version
the release announces, so a changed or older package is refused. Lockra 0.1.x has no in-app
update: install 0.2.0 or later over it once, with the [install script](/guide/install#install-with-one-command)
or a package. A copy that was not installed from a package, such as one built from the source,
cannot update itself and says so in **Settings › General**.

You can always update with the install script instead, or by installing the new package over the
old one.

### Automatic updates

**Settings › General › Automatic updates** is off until you turn it on. Then, 10 seconds after it
starts, Lockra looks for a newer version and downloads it in the background; the title bar shows
**Restart to update**. Nothing is installed until you choose it, or until the next start: if you
quit Lockra instead, it installs the downloaded version when it starts again, then restarts. A
version newer than the downloaded one waits for you again. Turning the switch on looks for a new
version and downloads it at once.

### What a check sends

A check asks GitHub, where Lockra is released, for a short file that names the newest version; a
download fetches the package from the same release. Nothing from your accounts, your vault or your
settings goes with either request. GitHub sees the request and the address it came from, as for
any download. Apart from checking for and downloading updates, Lockra makes no network
connections.

## Where the files are

|          | Windows                                      | macOS                                                            | Linux                                            |
| -------- | -------------------------------------------- | ---------------------------------------------------------------- | ------------------------------------------------ |
| Vault    | `%APPDATA%\dev.lockra.desktop\vault.lockra`  | `~/Library/Application Support/dev.lockra.desktop/vault.lockra`  | `~/.local/share/dev.lockra.desktop/vault.lockra` |
| Settings | `%APPDATA%\dev.lockra.desktop\settings.json` | `~/Library/Application Support/dev.lockra.desktop/settings.json` | `~/.config/dev.lockra.desktop/settings.json`     |

**Settings › About** shows the data folder of the computer you are on. Next to the vault Lockra
keeps `vault.lockra.prev`, the previous version, and the copies it makes before a replacing
restore (`pre-restore-<date>.lockrabackup`); with automatic updates on, `update-ready.json` names
a version downloaded but not installed yet. The settings file holds no secrets.

## Moving to another computer

Save a backup, copy the file to the other computer, install Lockra there and choose **Restore a
backup** on the welcome screen. The accounts arrive with the backup's password as the master
password.

## Uninstalling

Uninstall Lockra the usual way for your system:

| Installed from | To uninstall                                                                           |
| -------------- | -------------------------------------------------------------------------------------- |
| `.deb`         | `sudo apt remove lockra`                                                               |
| `.rpm`         | `sudo dnf remove lockra` (or `sudo zypper remove lockra`)                              |
| AppImage       | delete `~/.local/bin/Lockra.AppImage` and `~/.local/share/applications/lockra.desktop` |
| macOS app      | move Lockra from Applications to the Trash                                             |
| Windows        | **Settings › Apps › Installed apps › Lockra › Uninstall**                              |

The vault and the settings stay in the folders above, so a later install finds them again; delete
those folders to remove everything. If you used **Remember on this device**, turn it off before
uninstalling so the key in the system keychain is deleted too.
