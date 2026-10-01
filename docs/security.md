# Lockra security model

What Lockra protects, how, and what it does not. Formats are in `docs/formats.md`.

## What is protected

The TOTP/HOTP secrets, at rest and in the running app, against: someone who copies the vault file
or a backup; other processes and web content reaching the app's IPC; the webview (the least
trusted part of the app) reading files or secrets it was not explicitly given; secrets lingering
on the clipboard or in screenshots; and an update that is not Lockra's. Lockra goes online for one
thing only, its update (below): a check and a download when the user asks, or at start once
automatic updates are on. There is no account, sync or telemetry. The HTTP client and TLS stack reach the desktop build
only through tauri-plugin-updater (`deny.toml` bans them from every other crate).

## At rest

- One file, `vault.lockra`: Argon2id (64 MiB, 3 passes) derives the key that unwraps a random data
  key; XChaCha20-Poly1305 encrypts the entries with the whole header as associated data, so a
  tampered slot, parameter or byte is detected. Every write is a whole new file, written atomically
  with the previous version kept as `.prev`. Backups use the same container.
- "Remember on this device" adds a second slot whose key lives in the OS keychain (Windows
  Credential Manager, macOS Keychain, the Secret Service on Linux). Turning it off asks for the
  master password, deletes the keychain entry and rotates the data key, so the old device key opens
  nothing written afterwards. When no keychain is reachable the switch is disabled and says why;
  a **release build never falls back to anything weaker** (only a debug build honours
  `LOCKRA_DEV_SECRET_STORE=memory`, for headless test runs).
- Unlock attempts slow down after three failures (1 s, doubling, at most 30 s).

## In the running app

- **The webview holds no paths and no secrets.** Every file is opened by Rust after a native dialog
  or a drop on the window (the webview only hears that a drag is over it); no command carries a
  path, and the capability file grants the webview no `fs`, `dialog`, `shell` or `http`
  permission — only Lockra's own commands and the title bar's window buttons
  (`apps/desktop/src-tauri/tests/ipc.rs`, `apps/desktop/src/window-config.test.ts`).
- What the webview receives is entry metadata and current codes. Two views carry a secret, both
  behind the master password entered again: _reveal_ (the secret, its URI and QR code) and an
  _export_ (QR codes; a plain otpauth file asks for the password before the save dialog opens).
  The IPC contract test asserts that the known secrets of its fixtures appear in no other message
  (`crates/lockra-bridge/tests/contract.rs`).
- While a secret view is open the window is excluded from screen capture
  (`set_content_protected`, Windows and macOS); it is lifted when the view closes or the vault
  locks. Revealed secrets and export codes hide themselves after two minutes; the core drops an
  idle export session after the same time.
- QR codes are SVG rendered by the core and shown through an `<img>` data URL: nothing in them can
  run. The CSP allows only the app's own assets, inline styles, `data:` fonts and images, and IPC;
  fonts ship with the app.
- Copied codes are written with the platforms' "do not keep" hints (Windows: no clipboard history,
  monitoring or cloud sync; Linux and macOS: no history) and cleared after the configured time,
  **only if the clipboard still holds that code**.
- The vault locks after the configured idle time and on demand (Ctrl+L); locking drops the
  decrypted entries, the import preview and every export session.
- One process at a time writes the vault (single instance: a second start focuses the first).

## Updates

- **Off the network by default.** A check runs when the user chooses **Check for updates**, and
  the package downloads when the user chooses **Update now**. With **Automatic updates** on (off
  by default, Settings › General), Lockra checks 10 s after start and downloads a newer release
  in the background, then shows **Restart to update**: nothing is installed until the user
  restarts for it, or until the next start finds the same version again (remembered in
  `update-ready.json` in the data directory); a newer version than the remembered one waits for
  the user again, so a release the user has not been shown is never installed unattended. 0.2.0's
  check-only switch (`auto_check_updates`) is not carried over: agreeing to checks was not agreeing
  to downloads. 0.3.0 did carry it over and saved it as `auto_update`, so from 0.3.2 the settings
  file has a schema and an `auto_update` saved before it reads as off until turned on again. This is Voltip's design (`crates/lockra-core/src/update.rs`, `crates/lockra-core/src/tests/update.rs`).
- **From Rust, not the webview.** The webview sends `update_check` / `update_install` through the
  same dispatcher as every command and has no permission for the updater plugin; the CSP still
  grants no remote host. The requests go to `https://github.com/sunerpy/lockra/releases/latest/download/latest.json`
  (a GitHub redirect to its download host) and to the package's release asset; they carry no
  account, vault or setting.
- **Signed packages only.** Every package is signed in the release workflow with a minisign key
  whose public half is in `tauri.conf.json` (`plugins.updater.pubkey`). tauri-plugin-updater
  checks the signature of the downloaded bytes before anything is installed, and with
  `requireSignedVersion` it also requires the version in the signature's trusted comment to be
  the one the manifest announces, so a genuine older package offered as a newer version (a forced
  downgrade) is refused, as is a package changed after signing
  (`apps/desktop/src-tauri/tests/update.rs`). The release workflow verifies every signature
  against the same public key before it publishes (`docs/release.md`).
- **The right package for the copy.** The bundler writes the install method into each package's
  executable; the manifest has a key per method (`linux-x86_64-deb`, …), so a copy installed from
  the `.deb` updates through a `.deb` (installed with `pkexec dpkg -i`, which asks for an
  administrator), an AppImage replaces itself, the Windows installer runs in its passive mode and
  the macOS app is replaced in place. A copy that was not installed from a package (a build from
  the tree) reports that it cannot update itself and never replaces its own executable.
- **Before installing**, an automatic backup still inside its debounce is written, because the
  process ends with the install.

## Residual risks

- With "remember on this device" on, the vault is as safe as the OS account: anyone who can sign
  in to the computer can open it.
- An ad-hoc signed macOS build may ask for keychain access again after every update, or lose
  access to the "remember on this device" key; unlocking with the master password and turning the
  option on again restores it.
- The updater trusts the release key (a GitHub Actions secret, with an offline copy kept by the
  maintainer) and GitHub's TLS and release storage. Whoever holds the key and can publish a release
  of `sunerpy/lockra` can ship an update; a stolen key alone cannot, because the manifest's address
  is fixed in the app. Losing the key ends updates for installed copies (each would have to be
  reinstalled with a build that carries a new public key).
- Linux has no screen-capture exclusion (the reveal and export views say so); clipboard hints only
  work where the clipboard manager honours them (KDE does).
- The master password and secrets typed by hand pass through the webview; Lockra drops them from
  its state as soon as the core has them, but a compromised webview process could read them.
- Memory is not locked (`mlock`); decrypted entries could reach swap or a crash dump.
- A forgotten master password cannot be recovered; _reset_ keeps the old file but cannot open it.
- Importing from Microsoft Authenticator needs a rooted Android phone, and newer versions of that
  app may encrypt the field Lockra reads.
- The packages are not code-signed (Windows SmartScreen and macOS Gatekeeper warn); the update
  signature above proves an update comes from Lockra's release key, not who the publisher is to
  the operating system. The install scripts check each package against the release's
  `SHA256SUMS`, which proves no more than the HTTPS download it came with; `gh attestation verify`
  proves the build provenance.
