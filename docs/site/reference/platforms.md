# Platform notes

This page lists what differs between Windows, macOS, Linux and the Android app.

## Windows

- **Versions**: Windows 10 and 11, x64 and ARM64. Lockra uses Microsoft Edge WebView2, which both
  include; the installer downloads it where it is missing.
- **Installation**: per user, without administrator rights. The installer is not code-signed yet,
  so SmartScreen asks before the first start.
- **Remember on this device** uses Credential Manager.
- **Clipboard**: copied codes are kept out of the clipboard history (`Win V`), the cloud clipboard
  and clipboard monitors.
- **Screenshots** of the window are blocked while a secret or an export code is shown.
- **Window**: Lockra draws its own title bar, so snap layouts do not appear when you hover over the
  maximize button; `Win ←` and `Win →` still snap the window.

## macOS

- **Versions**: macOS 11 or later, a dmg for Apple silicon and one for Intel.
- **First start**: the app is not signed with an Apple Developer ID yet; allow it in **System
  Settings › Privacy & Security › Open Anyway**.
- **Remember on this device** uses the Keychain; an in-app update keeps the key without asking
  (below).
- **Clipboard**: copied codes are marked to stay out of clipboard history.
- **Screenshots** of the window are blocked while a secret or an export code is shown.

### No keychain prompt after an update

<StatusTag status="available" /> Available from version 0.7.1.

The Mac app is signed with Lockra's own fixed certificate, and an in-app update hands the
remembered key to the new version before installing it. After the update and the restart, macOS no
longer asks whether Lockra may use the Keychain, and Touch ID unlocks as before. The update from
0.7.0 or earlier to 0.7.1, and a dmg installed by hand, still ask once.

## Linux

- **Packages**: .deb, .rpm and AppImage, for x64 and ARM64. Lockra needs WebKitGTK 4.1; X11 and
  Wayland both work.
- **Remember on this device** needs a Secret Service keychain, such as GNOME Keyring or KWallet. On a
  desktop without one, the setting is disabled and says why.
- **Clipboard**: copied codes are marked to stay out of clipboard history; KDE's clipboard respects
  the mark, other clipboard managers may not.
- **Screenshots** cannot be blocked: Linux offers no way for an app to exclude its window from
  capture. The reveal and export views say so; mind screen sharing while they are open.

## Android

<StatusTag status="available" /> Available from version 0.7.0.

- **Versions**: Android 8 or later, on ARM64 phones, from the APK on the releases page.
- **Unlocking**: with the master password, or with **Settings › Security › Unlock with
  fingerprint**, which keeps the vault's key in the phone's secure hardware. There is no
  **Remember on this device** without the fingerprint. A fingerprint enrolled later makes the key
  unusable: the master password unlocks until the switch is turned on again.
- **Leaving the app** (another app, the home screen, the screen turned off) locks the vault at
  once.
- **Screenshots** and screen recordings of Lockra are always blocked, and the recent apps show a
  blank card.
- **Clipboard**: copied codes are marked sensitive, so the keyboard's clipboard history and the
  paste preview leave them out.
- **Backups** are saved and restored where the system's file picker says; there are no automatic
  backups. Android's own backup and device transfer leave Lockra out, so keep a Lockra backup.
- **Updates**: **Settings › About › Check for updates**; a newer release opens its page.
