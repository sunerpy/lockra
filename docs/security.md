# Lockra security model

What Lockra protects, how, and what it does not. Formats are in `docs/formats.md`.

## What is protected

The TOTP/HOTP secrets, at rest and in the running app, against: someone who copies the vault file
or a backup; other processes and web content reaching the app's IPC; the webview (the least
trusted part of the app) reading files or secrets it was not explicitly given; secrets lingering
on the clipboard or in screenshots. Lockra never goes online: there is no account, sync, telemetry
or update check, and no network stack in the desktop build (`deny.toml` bans HTTP clients and TLS).

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

## Residual risks

- With "remember on this device" on, the vault is as safe as the OS account: anyone who can sign
  in to the computer can open it.
- An ad-hoc signed macOS build may ask for keychain access again after every update.
- Linux has no screen-capture exclusion (the reveal and export views say so); clipboard hints only
  work where the clipboard manager honours them (KDE does).
- The master password and secrets typed by hand pass through the webview; Lockra drops them from
  its state as soon as the core has them, but a compromised webview process could read them.
- Memory is not locked (`mlock`); decrypted entries could reach swap or a crash dump.
- A forgotten master password cannot be recovered; _reset_ keeps the old file but cannot open it.
- Importing from Microsoft Authenticator needs a rooted Android phone, and newer versions of that
  app may encrypt the field Lockra reads.
- The packages are not code-signed (Windows SmartScreen and macOS Gatekeeper warn).
