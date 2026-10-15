# Platform notes

This page lists what differs between Windows, macOS, Linux and the Android app. An app for iPhone
and iPad is planned ([below](#iphone-and-ipad)).

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

### When the new version cannot take the key over

<StatusTag status="available" /> Available from version 0.7.5.

If the new version cannot take over the remembered key, the update is not installed. Lockra keeps
running the version you have, shows "The new version could not take over the device key in the
Keychain; nothing was installed", and writes the reason to
`~/Library/Logs/dev.lockra.desktop/lockra.log`. Up to 0.7.4 the update installed anyway, and macOS
asked once after the restart.

## Linux

- **Packages**: .deb, .rpm and AppImage, for x64 and ARM64. Lockra needs WebKitGTK 4.1; X11 and
  Wayland both work.
- **Remember on this device** needs a Secret Service keychain, such as GNOME Keyring or KWallet. On a
  desktop without one, the setting is disabled and says why.
- **Clipboard**: copied codes are marked to stay out of clipboard history; KDE's clipboard respects
  the mark, other clipboard managers may not.
- **Screenshots** cannot be blocked: Linux offers no way for an app to exclude its window from
  capture. The reveal and export views say so; mind screen sharing while they are open.
- **The tray icon** goes through AppIndicator, and a click on it opens its menu. GNOME shows tray
  icons only with an AppIndicator extension installed; without one, nothing shows that Lockra still
  runs in the background, and starting Lockra again brings its window back.

## Browser extension

<StatusTag status="available" /> Available from version 0.10.0.

- **Computers only**: the [browser extension](/passwords/browser) works with Lockra on Windows,
  macOS and Linux. The Android app has none.
- **Browsers**: Chrome and Microsoft Edge 133 or later, and Firefox 140 or later. Lockra also writes
  its connection file for Chromium and Brave. Safari is not supported.
- **Snap and Flatpak**: a browser installed as a Snap or a Flatpak cannot reach Lockra; Ubuntu
  installs Firefox as a Snap by default. Install the browser from a deb or rpm package, or from its
  maker's download, instead.
- **The connection files**: each browser's folder for native-messaging hosts, such as
  `~/.config/google-chrome/NativeMessagingHosts/` on Linux and
  `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/` on macOS; on Windows,
  `%LOCALAPPDATA%\Lockra\browser\`, with registry entries under `HKEY_CURRENT_USER`. Lockra writes
  them again each time it starts with browser integration on, so after moving an AppImage, start it
  once from its new place.
- **The tray**: on Linux, Lockra's icon appears where the desktop shows AppIndicator icons. KDE
  Plasma shows them; GNOME shows none unless an AppIndicator extension is installed. The `.deb` and
  `.rpm` packages bring the library the icon needs.

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
- **Clipboard**: copied codes and passwords are marked sensitive, so the keyboard's clipboard
  history and the paste preview leave them out.
- **Backups** are saved and restored where the system's file picker says; there are no automatic
  backups. Android's own backup and device transfer leave Lockra out, so keep a Lockra backup.
- **Updates**: installed from the APK on the release page, **Settings › About › Check for
  updates**, and a newer release opens its page; installed from Google Play, Google Play updates it,
  and **Open in Google Play** opens Lockra's page there.

## iPhone and iPad

<StatusTag status="planned" />

An app for iPhone and iPad is planned ([Roadmap](/roadmap#planned)). Until it exists, Lockra on a
computer can move accounts to an authenticator on the iPhone with export codes
([Google Authenticator](/transfer/google#moving-accounts-to-google-authenticator),
[Microsoft Authenticator](/transfer/microsoft)).
