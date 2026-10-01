# Platform notes

This page lists what differs between Windows, macOS and Linux.

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
- **Remember on this device** uses the Keychain. After an update, macOS may ask again whether
  Lockra may use the key.
- **Clipboard**: copied codes are marked to stay out of clipboard history.
- **Screenshots** of the window are blocked while a secret or an export code is shown.

## Linux

- **Packages**: .deb, .rpm and AppImage, for x64 and ARM64. Lockra needs WebKitGTK 4.1; X11 and
  Wayland both work.
- **Remember on this device** needs a Secret Service keychain, such as GNOME Keyring or KWallet. On a
  desktop without one, the setting is disabled and says why.
- **Clipboard**: copied codes are marked to stay out of clipboard history; KDE's clipboard respects
  the mark, other clipboard managers may not.
- **Screenshots** cannot be blocked: Linux offers no way for an app to exclude its window from
  capture. The reveal and export views say so; mind screen sharing while they are open.
