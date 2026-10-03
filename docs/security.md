# Lockra security model

What Lockra protects, how, and what it does not. Formats are in `docs/formats.md`.

## What is protected

The TOTP/HOTP secrets, at rest and in the running app, against: someone who copies the vault file
or a backup; other processes and web content reaching the app's IPC; the webview (the least
trusted part of the app) reading files or secrets it was not explicitly given; secrets lingering
on the clipboard or in screenshots; an update that is not Lockra's; and, with sync on, whoever runs
or reaches the user's sync storage. Lockra goes online for two things only: its update (below), a
check and a download when the user asks or at start once automatic updates are on; and the sync of
a space the user set up on storage of their own (below). There is no account, server or telemetry.
The HTTP client and TLS stack reach the desktop build only through tauri-plugin-updater and
`lockra-remote` (`deny.toml` bans them from every other crate).

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
- **Touch ID or Windows Hello first** (macOS, Windows): with "remember on this device" on, the
  device slot can ask the platform to check the user before its key is used. Turning it on passes
  one check; with "remember on this device" off, it is turned on as well, the slot and its check in
  the same write, so the remembered key never exists without the check (no fingerprint is asked for
  when the keychain cannot take the key). Turning it off takes the master password and leaves
  "remember on this device" on, as its own switch shows. The check is Lockra's, made before the
  keychain is read: it stops someone at the unlocked computer, not a program running as the user,
  which could read the keychain item itself. It is recorded in the vault's header (formats §1), so
  a file edited to skip it no longer opens, and a settings file cannot turn it off. The prompt's
  words come from the interface; failures and cancellations are the platform's to count (macOS and
  Windows lock the sensor out after repeated failures), and the master password always unlocks.
  Windows Hello's own fallback is its PIN. Lockra asks Windows whether Hello is ready before each
  check and offers none without it; robius-authentication looks again just before its prompt and,
  if Hello became unavailable in that instant (busy with another app's prompt, a reader unplugged),
  asks for the password of the signed-in Windows account instead, which Windows verifies for that
  same account: a password in place of the fingerprint, not a way around the check. The platform
  calls go through robius-authentication, so Lockra's crates keep forbidding unsafe code. A release
  build only ever asks the platform (only a debug build honours `LOCKRA_DEV_BIOMETRIC=touch_id` or
  `windows_hello`, a stand-in that always passes, for headless test runs).
- **The default unlock** (Settings › Security, `default_unlock` in `settings.json`) only decides
  whether the lock screen asks for the check by itself; the check, the keychain and the master
  password stay as above, so the setting opens nothing. The lock screen asks only while Lockra is
  in front (its window has the focus; on the phone, the app is on the screen), never right after
  the user locked it, and after a cancelled check only once Lockra has been left and come back:
  a system prompt never comes up over another app.
- Unlock attempts slow down after three failures (1 s, doubling, at most 30 s).

## In the running app

- **The webview holds no paths and no secrets.** Every file is opened by Rust after a native dialog
  or a drop on the window (the webview only hears that a drag is over it); no command carries a
  path, and the capability file grants the webview no `fs`, `dialog`, `shell` or `http`
  permission — only Lockra's own commands and the title bar's window buttons
  (`apps/desktop/src-tauri/tests/ipc.rs`, `apps/desktop/src/window-config.test.ts`).
- What the webview receives is entry metadata and current codes. Four answers carry a secret, all
  behind the master password entered again: _reveal_ (the secret, its URI and QR code), an
  _export_ (QR codes; a plain otpauth file asks for the password before the save dialog opens),
  the new sync key when a sync space is created (`sync_create`), and a sync invitation
  (`sync_invite`). The IPC contract test asserts that the known secrets of its fixtures, the sync
  storage's credentials and the sync key included, appear in no other message
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
- The Android app (`apps/mobile`, in progress and not released) keeps these rules with the phone's
  means: its window is always `FLAG_SECURE` (no screenshots or screen recording, a blank card among
  the recent apps); a copied code goes on marked `EXTRA_IS_SENSITIVE` (the paste preview and the
  keyboards' clipboard history leave it out) and comes off after the configured time only while the
  clipboard still holds it; the vault locks as soon as the app leaves the screen. **Unlock with
  fingerprint** keeps the device key sealed with AES-GCM by an Android Keystore key that the
  secure hardware holds: it works only for ten seconds after a passed strong-biometric check, and
  never again once a new fingerprint is enrolled (the master password unlocks then, and the
  fingerprint is turned on anew). There is no "Remember on this device" without that check. The
  app's data is kept out of Android's backups and device transfers (`allowBackup="false"` and the
  data extraction rules): the vault leaves the phone only in Lockra's own encrypted backups. The
  camera's page is `FLAG_SECURE` too, and what it reads, like the photos and files picked, goes to
  the import preview in Rust and never to the webview; an invitation it reads (the storage's
  credentials and the sync key) goes the same way into joining its space. A backup leaves
  encrypted, written where the user picks, and a plain otpauth list only after the master password
  and the user's acknowledgement that it is plaintext.
  Leaving the app from the camera's page locks the vault at once; the system photo picker runs in
  another app that Lockra cannot watch, so the vault stays unlocked behind it until the auto-lock
  time.

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

## Sync

- **Off unless set up, on storage of the user's own.** Sync stays off until the user sets up a
  space on an S3-compatible bucket or a WebDAV folder of their own; Lockra runs no server. With a
  space, Lockra contacts that storage only while the vault is unlocked: at unlock, 3 s after a
  change, every 5 minutes, and on **Sync now**. Setting up, joining, showing an invitation and
  moving the storage settings ask for the master password again.
- **The storage sees ciphertext.** A space is one snapshot per device under
  `lockra-sync-v1/<space id>/devices/` (`docs/formats.md` §9), and nothing else. A snapshot is the
  device's whole replica, secrets included, encrypted under a key derived from the space's random
  data key, its header (format, space, device tag, nonce, the device's keyring) bound as
  associated data, padded to 4 KiB so that its size says little about the number of accounts. The
  names say nothing but the number of devices: device names, times and sequence numbers are inside
  the ciphertext, and a keyring's header names no device and no time.
- **Two secrets open a space.** Every snapshot carries its device's keyring: the data key wrapped
  under HKDF(Argon2id(that device's master password) ‖ sync key), with a fresh salt, so that two
  devices with the same password carry keyrings that do not read alike. The sync key is 256
  random bits shown once, when the space is created, to be kept with the master password; it also
  names the space. The storage's contents and a master password open nothing without it, and with
  it every guess of a password still costs an Argon2id run. A device keeps the data key in its
  vault's encrypted local part and needs neither secret again.
- **No object has two writers.** A device writes only its own snapshot, its keyring inside, so
  runs on different devices never write the same object: no lock and no conditional write is
  needed, and S3 and WebDAV (which has no conditional writes) behave alike. What the space holds is
  the merge of the snapshots, whatever order the runs take, so changes made at the same moment on
  two devices are both kept. Another device only ever deletes a snapshot (removing a device); a
  device still in use writes it again on its next run.
- **Joining.** Another device shows an invitation (text and QR code): the storage settings with
  their credentials and the sync key, everything but a master password, which the joining device
  asks for: that of any device in the space. It is to be scanned on the user's own devices only.
  Without another device, the storage settings and the sync key typed in do the same. A device
  with no vault yet becomes one, under that password. A device with a vault checks its own master
  password first, and opens the space with it, or with another device's typed in apart. Either way
  the joining device's keyring goes in under its own master password, so the passwords that open
  a space are those of its devices, no other.
- **Altered, moved and older objects are refused; deletion is not prevented.** Every object
  authenticates and is bound to its space and its device's name: an altered or moved snapshot is
  reported as unreadable. An older snapshot of a device than one already seen is refused, and the
  merge (last writer wins on hybrid logical clock stamps) never lets an older change win over a
  newer one. Whoever can write to the storage can delete the space's objects: that stops the sync,
  not the vaults, which keep every account. A copied vault writing under the same device name is
  found (on S3 by conditional writes, elsewhere by a snapshot this device did not write) and the
  device takes a new number; a write whose answer was lost (a dropped connection, the vault
  locked meanwhile) is recorded before it goes out, and recognised as this device's own. An
  object larger than any snapshot (16 MiB) is not read at all. An HOTP counter never goes back on
  any device, whichever version of the account wins.
- **The credentials stay in the vault.** The storage settings and credentials, the data key and
  the sync key are in the vault's encrypted local part: never in `settings.json`, never in a
  backup (a restored backup joins its space again), and never sent back to the webview, which is
  shown the storage without its secret. An address carrying a user name or password is refused:
  the credentials go in their own fields.
- **Transport.** HTTPS only, rustls with the operating system's verifier and the system proxy;
  on Android, with the certificate authorities the system keeps for apps, read from its files (the
  platform verifier would ask Android through JNI glue that needs unsafe code), so an authority
  the user installed is not trusted, as for any app that does not opt in to them. Plain HTTP is
  refused except to this computer (the tests' servers), and a redirect may not lead to it either.
  The requests carry the storage's credentials (S3 signatures, WebDAV basic authentication inside
  TLS) and ciphertext.
- **A new master password** re-wraps this device's keyring, which its next run writes with the
  snapshot (the data key stays); until then the old password still joins new devices through this
  device. Devices change their passwords apart, each its own keyring: neither change can be lost
  to the other. A password no device uses any more opens nothing the storage holds; a copy of the
  storage taken earlier still opens with it, since the data key never changes (see Residual
  risks). New storage settings are taken only where a snapshot of this space opens under its data
  key. A run that started under the old settings writes nothing more there once they changed,
  except a write already on its way, which may still land at the old place: the space at the new
  place stays whole, and what lands at the old place is ciphertext like everything it held.

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
- Sync: whoever holds the storage's contents and the sync key (an invitation photographed, for
  instance) can try master passwords offline at Argon2id's cost, against the keyring of every
  device: the weakest master password among the space's devices is the last line. The storage's
  operator sees when devices write and how many there are, and can delete the space. Removing a
  device deletes its snapshot and its keyring but revokes nothing: the device keeps the data key,
  and a copy of the storage taken earlier keeps its keyring. To shut out a lost device, or someone
  who has the sync key and an old master password, set up a new space and join the other devices
  to it.
- On Android, the sync trusts the certificate authorities the system ships (its Conscrypt
  module's since Android 14, the system image's before): one the user turned off in the system's
  settings is still trusted by the sync, and Android's blocklist of distrusted certificates is not
  consulted.
- The Android app trusts its release key the same way: an APK signed with it can update an
  installed Lockra, and one signed with any other key cannot. Losing the key means users reinstall
  (losing the vault unless they made a Lockra backup, since Android's backups leave the app out);
  whoever holds it can sign an update (`docs/release.md`, "Android").
- Importing from Microsoft Authenticator needs a rooted Android phone, and newer versions of that
  app may encrypt the field Lockra reads.
- The packages are not code-signed (Windows SmartScreen and macOS Gatekeeper warn); the update
  signature above proves an update comes from Lockra's release key, not who the publisher is to
  the operating system. The install scripts check each package against the release's
  `SHA256SUMS`, which proves no more than the HTTPS download it came with; `gh attestation verify`
  proves the build provenance.
