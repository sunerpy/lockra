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

### Keychain items across updates (macOS)

The login keychain gives every item a partition list, and a program signed with a certificate that
has no Apple Team ID gets the partition `cdhash:<that build>`: reading an item another build
created shows the system's dialog, even when the item's access list trusts the signing
requirement, and 「始终允许」 (_Always Allow_) lets in only the build it was pressed for. Up to 0.7
the releases were signed ad hoc and kept the device key in one item, so every update asked once.
The method is Voltip's, measured on GitHub's Macs and users' keychains in 2026-09:

- **One certificate.** Releases are signed with the project's self-signed "Lockra Code Signing"
  certificate (docs/release.md, "macOS signing"); its designated requirement,
  `identifier "dev.lockra.desktop" and certificate leaf = H"d660…6855"`, is compiled into the app
  (`code_identity::RELEASE_REQUIREMENT`) and checked against `.github/macos-signing.json`.
- **An item per build.** A process that satisfies that requirement keeps each secret in an item it
  created: service `dev.lockra.desktop/<vault id>`, account `lockra.signed.<build>`, the build
  being a hash of its own code signature (`per_build.rs`, `code_identity.rs`). Reads go quietly
  first, with the dialog off; an item of its own never needs it.
- **The hand-over before installation.** Once the updater has verified a package's minisign
  signature, the running build expands it into a temporary directory, starts the staged executable
  with `--lockra-keychain-handoff` and one end of a socket pair as its stdin, and sends the entries
  its store holds (every item of its own, read quietly, and what this process wrote or deleted; an
  item of its own that cannot be listed or read quietly, as in a locked keychain, stops the
  hand-over instead of being left out). Each side first checks the other process against the
  requirement (`SecCodeCopyGuestWithAttributes` by pid), so the values only ever go from one
  release to the next; nothing goes through the disk, the command line or the environment. The
  staged build writes items of its own, reads them back and only then acknowledges; on any failure
  it rolls its items back and does not acknowledge.
- **Installed only after the hand-over** (the releases after 0.7.4; `updater::prepare_then_install`): when it
  fails, nothing is installed, the running version keeps its items, and the update ends with
  `update_keychain` ("the new version could not take over the device key"); a version that would
  ask for the keychain on its first start is never installed. Up to 0.7.4 the update installed
  anyway and the new version asked once. The installed build reads its own item and removes the
  older copies, which needs no permission (removing goes by reference and never reads the value).
- **The log**: an app started from the Finder has no stderr anyone reads, so the Mac app also logs
  to `~/Library/Logs/dev.lockra.desktop/lockra.log` (owner-only, restarted past 1 MiB with the
  previous one kept as `lockra.log.1`; `logging.rs`). The staged build logs there too: a hand-over
  that failed says why. Nothing secret is logged.
- **Without a hand-over** (a dmg installed by hand, an update from 0.7 or earlier, an update 0.7.4
  or earlier installed after its hand-over failed) the build reads the newest older copy once, with
  the dialog, and moves it into its own item; the item of 0.7 (service `dev.lockra.desktop`,
  account the vault id) is the oldest. A build that has not done so yet (the vault was unlocked
  with the master password since) has nothing of its own to hand over, so the next version asks
  that once instead.
- **Not signed so** (local and CI builds, ad hoc): the keyring store as before, and no hand-over. A
  debug build compiled with `LOCKRA_DEV_RELEASE_REQUIREMENT` trusts that requirement instead of the
  release's, which is how CI checks the real app end to end; a release build never reads it.
- **Checked in CI** on the login keychains of macOS 15 and 26, Apple silicon and Intel
  (`macos-keychain`): `check-keychain-handoff.sh` between harness builds, and
  `check-keychain-preinstall.sh` with the real app, signed with the hardened runtime as the release
  is, staged from an updater-shaped `Lockra.app.tar.gz` and installed at another path; a package
  signed otherwise is handed nothing.

All of it is security-framework's safe calls: the workspace keeps forbidding unsafe code.

## In the running app

- **The webview holds no paths and no secrets.** Every file is opened by Rust after a native dialog
  or a drop on the window (the webview only hears that a drag is over it); no command carries a
  path (a sync folder neither: the webview asks for "the folder chosen" through the folder dialog,
  and a path in its place is refused), and the capability file grants the webview no `fs`, `dialog`, `shell`, `http`
  or `global-shortcut` permission — only Lockra's own commands and the title bar's window buttons
  (`apps/desktop/src-tauri/tests/ipc.rs`, `apps/desktop/src/window-config.test.ts`).
- What the webview receives is entry metadata and current codes, and the logins' metadata
  (`items_list`: titles, user names, email addresses, sites, the values of custom fields that are
  not hidden; no password, notes or hidden value). Seven answers carry a secret. Four are behind
  the master password entered again or the biometric check that unlocks the vault (Touch ID,
  Windows Hello, the fingerprint; the phone offers it for all four, the desktop for the last two):
  _reveal_ (the secret, its URI and QR code), an _export_ (QR codes; a plain otpauth file asks for
  the proof before the save dialog opens), the space's sync key, which the interface calls the
  recovery key (`sync_key_reveal`), and a sync invitation (`sync_invite`). The generator's history
  (`generator_history_reveal`) is behind the same proof. A login's secrets (`item_reveal`: its
  password, notes, hidden fields and earlier values) follow the login's own protection level,
  which the user picks per login: one of the `confirm` level asks for the proof first, one of the
  `fill` or `no_extension` levels shows them as it would fill them, without asking (so a login is
  as easy to see as its password is to fill). A password just made (`generator_generate`) comes
  back without proof: it protects nothing yet. Copying a login's part (`item_copy`) answers
  nothing: Rust writes the clipboard, with the hints and the clearing a code gets, and a secret
  of a `confirm` login asks for the proof first; the user name, the email and the linked code do
  not.
  Setting up a space (`sync_create`) answers nothing. Saving the sync
  key to a file (`sync_key_save`, the same proof) answers no secret: the core puts the key in the
  interface's words in Rust and writes the file where the user chose, readable by its owner only
  on the desktop. Exporting logins to a file (`export_items_file`, the same proof, asked before the
  save dialog opens) answers the file's name only: the core makes the KeePass database or the
  plain CSV in Rust and writes it where the user chose, readable by its owner only; a CSV is
  refused (`export_acknowledge_needed`) until the user confirmed that it is plain text. The IPC
  contract test asserts that the known secrets of its fixtures, the sync
  storage's credentials, the sync key, a login's password, notes and hidden field and a generated
  password included, appear in no other message (`crates/lockra-bridge/tests/contract.rs`). A
  password made by the generator stays in the core for the session, so that a login saved with it
  takes it without the webview sending it back.
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
- The quick search (docs/architecture.md, "The quick search") brings the window up over other
  apps, and its command palette opens only while the vault is unlocked: locked, its shortcut, the
  tray's 「快捷搜索」 and `lockra-desktop --palette` show the unlock screen and nothing of the vault.
  The palette lists what the window already shows (names, user names) and copies as the window
  does (`item_copy`, with a `confirm` login's proof). Rust registers the shortcut; the webview has
  no permission for the plugin (`apps/desktop/src-tauri/tests/ipc.rs`). Any program of the user's
  can run `lockra-desktop --palette`, as it can start Lockra: that brings the window up and copies
  nothing by itself.
- The Android app (`apps/mobile`, from 0.7.0) keeps these rules with the phone's
  means: its window is always `FLAG_SECURE` (no screenshots or screen recording, a blank card among
  the recent apps); a copied code goes on marked `EXTRA_IS_SENSITIVE` (the paste preview and the
  keyboards' clipboard history leave it out) and comes off after the configured time only while the
  clipboard still holds it; the vault locks as soon as the app leaves the screen. **Unlock with
  fingerprint** keeps the device key sealed with AES-GCM by an Android Keystore key that the
  secure hardware holds: it works only for ten seconds after a passed strong-biometric check, and
  never again once a new fingerprint is enrolled (the master password unlocks then, and the
  fingerprint is turned on anew). There is no "Remember on this device" without that check. Where
  a fingerprint is enrolled, a vault made, restored or joined on the phone turns it on by default
  (the welcome page's switch, one check once the vault is open), and an existing vault unlocked
  with the master password offers it once (settings `biometric_offer`, off after **Not now**); the
  key is made only by that passed check, never without it. The
  app's data is kept out of Android's backups and device transfers (`allowBackup="false"` and the
  data extraction rules): the vault leaves the phone only in Lockra's own encrypted backups. The
  camera's page is `FLAG_SECURE` too, and what it reads, like the photos and files picked, goes to
  the import preview in Rust and never to the webview; an invitation it reads (the storage's
  credentials and the space's keys) goes the same way into joining its space. A backup leaves
  encrypted, written where the user picks, and a plain otpauth list only after the master password
  and the user's acknowledgement that it is plaintext.
  Leaving the app from the camera's page locks the vault at once; the system photo picker runs in
  another app that Lockra cannot watch, so the vault stays unlocked behind it until the auto-lock
  time.

## Browser extension

Password management's browser extension (since 0.10.0) is a thin client: it keeps no vault and caches no secret, and every decision is the core's.

- **The channel.** The extension reaches the running app through native messaging, a host process
  that only passes frames between its stdio and the app's local socket. Both are treated as
  untrusted pipes: Noise encrypts end to end between the extension and the app (docs/formats.md
  §10), so whoever takes over the socket or the named pipe can make a connection fail, not read or
  forge one. Pairing is `Noise_XX` with a six-digit code drawn from the handshake hash on both
  screens, which the user compares before approving in Lockra with the master password or the
  biometric check; a party in the middle would show different codes. Each session is `Noise_KK`
  between the paired keys with fresh ephemeral keys. At most three pairing requests wait, for two
  minutes each. The app's static key is in the vault's local part (never synced, never in a
  backup) and stays in the app's memory, never in the webview.
- **The host and the socket.** The native-messaging manifests are written only for the browsers
  the user picks and removed when the integration is turned off; they allow Lockra's extensions
  alone: Edge Add-ons' id, which the release's self-install package has too (it carries that
  item's public key), the Chrome Web Store's once the extension is listed there, and Firefox's
  add-on id; only a debug build adds the development id. An id proves nothing about an extension
  loaded unpacked, which may carry any public key, a store item's too: whatever loads under an
  allowed id still has to pair, approved in Lockra. The host reads nothing it passes on: it says
  by itself only that Lockra is not running, and starts Lockra when the extension asks (started
  for the browser, a Lockra running in the background stays in the tray). On Linux and macOS the
  socket's folder is the user's alone (theirs, not a link, 0700): the app makes it so before it
  listens and takes a connection only from a process of the same user (`peer_cred`), and the host
  connects only into such a folder. On Windows the named pipe is created as its first instance, so
  a process holding the name first makes Lockra not listen rather than share the pipe, and it
  refuses remote clients; its default access lets other users of the machine open it, which
  gets them no further than a Noise handshake they cannot complete.
- **The extension's folder.** The desktop app writes the extension it was built with (from its own
  binary, so the same signed update that brought the app brought it) into a folder of the user's
  local data, for the user to load unpacked; it writes nothing else there and reads only its own
  marker back. The folder is as writable as any folder the user unzips the release package into,
  and like any unpacked extension's its id proves nothing: whatever runs from it still has to pair.
  The extension reloads itself only from its own package (no remote code, nothing downloaded), and
  the app sends it no path: Settings › 浏览器 shows the folder's path, the extension never learns it.
- **Locked means nothing.** While locked the app answers only `hello` and the request to unlock
  (at most every ten seconds); locking ends every session, refuses every waiting request and
  empties the activity list. A browser's requests never restart the idle timer.
- **What a request gets** is decided in `crates/lockra-core/src/session/browser.rs`, from what the
  extension's background script took from the browser (the frame's address, the tab's, the frame's
  ancestors), never from the page: only the logins at the best match level for the frame's own
  address (the never-fill list is checked first); nothing in a frame of another site unless the
  user set that rule to ask, and then Lockra asks; an `https` login on an `http` page only once the
  user agreed in the page; a `no_extension` login never; a `confirm` login only after the master
  password or the biometric check in Lockra (and copying its password from the popup the same); a
  linked code for a second step only on the site just filled, for five minutes, or, where the user
  asks for a login's code in the popup, the code of a login offered for the frame, in the page or
  a frame of its own site. The sign-up menu's
  copy (`copy_generated`) puts on the clipboard only a password of the generator's history made
  for the frame's site, with the hints and the clearing a login's password gets, so the extension
  cannot have the app write anything else there.
- **Deleting from the browser** (from 0.12): the menu and the popup ask first, in the extension;
  the menu's click on **Delete** goes through the same checks as a fill's (the user's, in a
  visible, still frame), and the popup is the browser's own window, which a page cannot cover. The
  app deletes only a login it offers for the frame the extension names, in the
  page or a frame of its own site, never on a never-fill site; a `confirm` login only after the
  master password or the biometric check in Lockra. Deletions count with the fills against the
  session's limits (ten a minute, twenty sites in ten minutes), past which the extension is paused
  until the user resumes it, so even an extension that misbehaves cannot empty the vault in a
  burst. A deletion cannot be undone, and it syncs to the other devices like one made in Lockra.
- **Filling as a page loads** is off unless the user turns it on (a warning, then the master
  password or the biometric check), and then only in the page itself (no frame), on `https` or
  where the login was saved for `http`, with exactly one match, for a `fill` login whose own
  setting is not off; never a code, never a submit.
- **Rate limits.** A session gets at most 10 answers carrying a secret a minute, 120 queries a
  minute and secrets of 20 sites in ten minutes; past that its pairing is paused until the user
  resumes it in Lockra (the window says so).
- **The policy** is in the vault, so editing `settings.json` cannot turn a risky option on; any
  change that lets the extensions do more asks for the proof. `browser_integration` in
  `settings.json` only says whether Lockra listens.
- **The extension's side** (`apps/extension`). Its static key is a WebCrypto X25519 key made with
  a private half that cannot be exported, kept in IndexedDB; the pairing (its id and the app's public
  key) and how its menu opens (as a field gets the focus, above the field) are all `storage.local`
  holds besides (no login, no site); session keys live in the background's memory only, and
  the popup's last list of the site's logins (no secret) in `storage.session`, emptied as Lockra
  locks. A fill the user did not ask for at that moment happens only as the next step of one they
  did, decided by the app: the code of the login filled on the same site within five minutes
  (`otp`), and, after a fill that put the user name in alone, that login's password once on a page
  of the same site within two minutes (`fill` with `"trigger": "next_step"`, named, never for a
  login of the `confirm` level, never in a frame of another site or over a downgrade to `http`,
  and never after a fill as the page loads). Its one optional API permission, `privacy`, is asked
  for only when the user turns on "Let Lockra take over the browser's passwords" on its options
  page; it switches the browser's password saving (and, if chosen, its address and card autofill)
  off, and turning the switch off clears those settings and removes the permission. While it is
  granted, the background also reads the password saving in Firefox, whose list of saved passwords
  goes with it, to open the menu below the field rather than above. The content
  script runs in its isolated world, listens to no `window` message and reports
  no address; the menu and the save prompt are extension frames in a closed shadow root in the top
  layer, shown nothing without the token the background handed out (where the browser lists the frame, it must also sit under the page's; Chromium's webNavigation leaves the extension's own frames out, so there the token alone binds it), and a click in them counts only
  when the frame has shown unmoved for 300 ms, is on top at the click, has no see-through, filtered,
  clipped or blended ancestor, was not touched by the page, and (Chromium) its IntersectionObserver
  v2 saw it visible.

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
  administrator), an AppImage replaces itself, the Windows NSIS installer runs silently and starts
  Lockra again (`/S /UPDATE /R`; it installs for the current user, without elevation), the MSI runs
  in its passive mode, which can show the administrator prompt it needs (`updater::windows_install_mode`,
  set before the app runs), and the macOS app is replaced in place. A copy that was not installed from a package (a build from
  the tree) reports that it cannot update itself and never replaces its own executable.
- **Before installing**, an automatic backup still inside its debounce is written, because the
  process ends with the install.
- **The phone checks; it installs nothing.** On Android, Settings › About › Check for updates reads
  the same manifest, only when tapped, over HTTPS with the system's certificate authorities (as
  the sync does), and compares its version with the app's. The core never checks by itself there
  and refuses to download or install. A newer release opens its page in the phone's browser: the
  address is made in Rust from the version found, and the browser plugin refuses any address
  outside Lockra's releases. The APK there is signed with the same Android key, so it installs
  over the app and keeps the vault (`docs/release.md`, "Android"). A copy Google Play installed
  (the system names `com.android.vending` as its installer) asks GitHub nothing: the core refuses
  its check, and the button opens Lockra's Play listing, the one other address the browser plugin
  accepts. The installer is not a secret and cannot widen anything: a wrong one only picks which of
  the two pages opens.

## Sync

- **Off unless set up.** Sync stays off until the user sets up a space: on a Lockra relay (the
  built-in one, which Lockra runs, or one of the user's own; "The relay" below), on an
  S3-compatible bucket or a WebDAV folder of their own, or, on a computer, in a folder their cloud
  drive's client keeps in sync. With a space, Lockra contacts that storage only while the vault is
  unlocked: at unlock, 3 s after a change, every minute while
  the app is in front (every 5 minutes behind other windows and after a failed run), when the
  desktop window comes back to the front once the last run is 30 s old, and on **Sync now**. A
  folder costs no request: it is looked at every 15 s in front and every minute otherwise, and a
  second after the operating system says a snapshot in it changed (the space's own folder only,
  while the vault is unlocked). A relay is asked, one request at a time while the vault is
  unlocked, to hold the space's listing back until a snapshot in it changes; a run follows a second
  after it answers with a change. Setting up asks for the master password again: the space's
  first keyring is sealed under it. Moving the storage settings, showing an invitation and saving
  the sync key to a file accept instead the biometric check that unlocks this vault (Touch ID,
  Windows Hello, the fingerprint): it proves the user is at the device, and nothing is sealed
  under it. Joining from an invitation accepts it too where a vault is unlocked (the phone offers
  it): this device's keyring is then sealed, at the usual Argon2id cost, under a random password
  of 244 bits kept nowhere, so that the storage cannot tell it apart and it opens nothing, until
  the next unlock with the master password seals it under that password (`keyring_unsealed`; the
  next run writes it, as after a new password). Until then this device's password recovers
  nothing from the storage, the other devices' passwords do. A recovery, and a device with no
  vault yet, need a password.
- **The storage sees ciphertext.** A space is one snapshot per device under
  `lockra-sync-v1/<space id>/devices/` (`docs/formats.md` §9), and nothing else; in a cloud drive's
  folder, the drive carries those same files. There Lockra reaches nothing outside the folder: its
  path is kept with every link resolved when it is chosen, and every read and write goes through
  folders opened one at a time without following a link, so a link a drive or a sync peer brings,
  before or during a run, sends nothing elsewhere. A snapshot is the
  device's whole replica, secrets included, encrypted under a key derived from the space's random
  data key, its header (format, space, device tag, nonce, the device's keyring) bound as
  associated data, padded to 4 KiB so that its size says little about the number of accounts. The
  names say nothing but the number of devices: device names, times and sequence numbers are inside
  the ciphertext, and a keyring's header names no device and no time.
- **From the storage, two secrets open a space.** Every snapshot carries its device's keyring: the
  data key wrapped under HKDF(Argon2id(that device's master password) ‖ sync key), with a fresh
  salt, so that two devices with the same password carry keyrings that do not read alike. The sync
  key (the recovery key, to the user) is 256 random bits, kept apart from the master password and
  shown only on request; it also names the space. The storage's contents and a master password
  open nothing without it, and with it every guess of a password still costs an Argon2id run. A
  device keeps the data key in its vault's encrypted local part and needs neither secret again.
- **No object has two writers.** A device writes only its own snapshot, its keyring inside, so
  runs on different devices never write the same object: no lock and no conditional write is
  needed, and S3 and WebDAV (which has no conditional writes) behave alike. What the space holds is
  the merge of the snapshots, whatever order the runs take, so changes made at the same moment on
  two devices are both kept. Another device only ever deletes a snapshot (removing a device); a
  device still in use writes it again on its next run.
- **Joining.** Another device shows an invitation: the storage settings with their credentials
  (for a relay, its address alone), the sync key and the space's data key. It hands the space
  over: the joining device asks no other device's master password, checks that a snapshot of the
  space opens under the data key, and seals its own keyring under its own master password (a new
  one, on a device with no vault yet; joined by the biometric check, under a random one until the
  next unlock with the master password). An invitation is therefore worth a device of the space: it
  is shown only after the master password or the biometric check that unlocks the vault, hides
  after two minutes, and is excluded from screen capture where the system allows. The QR code
  carries it as it is, to be scanned on the user's own devices only. For a space on a relay the
  same text is also the pairing link the user copies and sends: it carries the relay's address and
  the keys, no credentials, and it opens the space alone and for good. That is a choice of
  convenience over a code (the user's, for one link and nothing else to type): a link left in a
  chat's history, a synced clipboard or a screenshot is worth a device of the space until a new
  space is started. On other storage the text to send is sealed under a one-time code shown only
  beside it (50 bits, stretched with Argon2id; `docs/formats.md`): sent through a chat or a mail,
  it is of no use without the code, which is to travel another way. For a computer, which scans
  nothing, the other device copies that text (`sync_invite_copy`): the core puts it on the
  clipboard marked as excluded from history and cloud sync, and clears it after the clipboard
  time, as a code. It copies a sealed text, or on a relay this space's own invitation (rebuilt
  from the vault and compared); an invitation that carries storage credentials is never copied.
  A space in a cloud drive's folder invites with the keys alone: the folder is of no use on
  another device, and no path is taken from an invitation. The joining device reaches the space its
  own way (the same drive's folder on a computer, its WebDAV on a phone) and checks it is that
  space, as when the storage settings change. An invitation of Lockra up to 0.8.2, which carries
  no data key, is refused with a word to update the device that shows it.
  Without another device, a recovery: the storage settings and the sync key typed in, with the
  master password of a device in the space, which opens that device's keyring. A device with no
  vault yet becomes one, under that password. A device with a vault checks its own master
  password first, and opens the space with it; only when that opens nothing does it ask for
  another device's. Either way
  the joining device's keyring goes in under its own master password, so the passwords that
  recover a space are those of its devices, no other.
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
  TLS; a relay's access token, below) and ciphertext. A cloud drive's folder is no transport of Lockra's: Lockra reads and writes
  local files, and the drive's own client carries them.
- **A new master password** re-wraps this device's keyring, which its next run writes with the
  snapshot (the data key stays); until then the old password still joins new devices through this
  device. Devices change their passwords apart, each its own keyring: neither change can be lost
  to the other. A password no device uses any more opens nothing the storage holds; a copy of the
  storage taken earlier still opens with it, since the data key never changes (see Residual
  risks). New storage settings are taken only where a snapshot of this space opens under its data
  key. A run that started under the old settings writes nothing more there once they changed,
  except a write already on its way, which may still land at the old place: the space at the new
  place stays whole, and what lands at the old place is ciphertext like everything it held.

### The relay

A relay (`docs/relay.md`) is storage that Lockra or the user runs: the built-in one at
`https://lockra-relay.onethinker.top`, or anyone's own `lockra-relay`. It holds what any storage
holds, the space's snapshots as the devices sealed them, and is trusted with no more.

- **It cannot read the space.** The snapshots are encrypted under the space's data key, which never
  leaves the devices; the keyrings in them open only with a device's master password together with
  the sync key, and neither reaches the relay. A device shows it an access token instead: HKDF of
  the sync key with a label of its own (`docs/formats.md` §9, "The relay"), from which neither the
  sync key nor anything else of the space can be computed. The device derives it from the sync key
  whenever it opens the relay and stores it nowhere; the relay keeps only the token's SHA-256.
  Whoever takes the relay's disk, or the relay itself, holds ciphertext and hashes, as an S3 bucket's
  operator would, and so does anyone its operator is made to hand them to: nothing on it opens
  without the sync key, which the relay never sees, and the built-in relay's disk holds no client
  address.
- **What it sees.** The space's id (derived one way from the sync key), its devices' tags (keyed
  hashes, which link no device across spaces), the snapshots' sizes in 4 KiB steps, when they
  change, and the addresses that ask. The built-in relay keeps the addresses in memory only, for
  its rates; it writes no log of requests, and its load balancer keeps no access or connection
  logs. A relay of one's own logs what its operator makes it log.
- **Only the space's devices change it.** A space is bound to the token of its first write; a
  request with another token is refused, so someone who learns a space's id (from a URL in a log,
  say) can neither read nor change it. The relay itself can delete, withhold or roll back
  snapshots, like any storage: the devices refuse an older snapshot than one seen, keep every
  account when the space goes, and write their snapshots again.
- **Limits.** The relay refuses more than it keeps (snapshot size, devices per space, bytes per
  space and in all, spaces) and more than a client may ask (requests per client address and per
  space, new spaces per address): one client cannot exhaust it for the others. A space no device
  reached for 400 days is removed; its devices still hold it whole and write it again.
- **Transport.** HTTPS, as for any storage; the token goes in the `Authorization` header, never in
  the URL. Behind its load balancer or proxy the relay speaks plain HTTP on a network the
  operator controls, and believes `X-Forwarded-For` only from the proxies it was told to.

## Residual risks

- With "remember on this device" on, the vault is as safe as the OS account: anyone who can sign
  in to the computer can open it.
- macOS asks once for the keychain item on the first update that brings the hand-over (from an
  ad hoc 0.7 or earlier), after a dmg installed by hand, and after a change of the release
  certificate. A downgrade to an ad hoc build finds no item of its own: unlocking with the master password and
  turning the option on again restores it. The hand-over trusts any process that satisfies the
  release requirement; whoever holds the release certificate's private key could have a program
  accepted as the next build.
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
- On the phone, the fingerprint that unlocks the vault also shows secrets, exports the accounts
  and the logins and joins the vault to a sync space: whoever passes the check (any finger enrolled
  on the phone) can take every account and login off it, as whoever knows the master password can. The check proves that
  an enrolled finger is there, nothing more.
- The browser extension: malware running as the same user can copy the extension's key out of
  the browser's profile and act as the paired extension, within the rate limits and the protection
  levels; a script on a site a login matches (by its registrable domain, unless the user chose a
  closer rule) can read what is filled there, and with filling as pages load it needs no click; the
  extension's strings cannot be wiped from memory.
- Running in the background (off unless the user turns it on) keeps Lockra running after its
  window closes: the vault stays unlocked until it locks by itself or from the tray. Some Linux
  desktops show no tray icon (GNOME without an AppIndicator extension), and there nothing shows
  that Lockra still runs; starting it again brings its window back.
- On Windows, a Lockra the host starts inherits the host's handles to the browser's pipes (Rust
  starts a process with every inheritable handle, and the workspace forbids the unsafe call that
  would narrow them). It never reads or writes them, and they close as it quits; until then the
  browser may not notice that host ending. Chrome starts its hosts outside a job object that would end
  them with it, so the started Lockra outlives the browser, as it should.
- A forgotten master password cannot be recovered; _reset_ keeps the old file but cannot open it.
- Sync: whoever holds an invitation (its QR code photographed, a relay space's pairing link, or
  the sealed text together with its code) holds the space, as a device does, with no password to
  guess; a pairing link does not expire. Whoever holds the
  storage's contents and the sync key can try master passwords offline at Argon2id's cost,
  against the keyring of every device: the weakest master password among the space's devices is
  the last line. The storage's
  operator sees when devices write and how many there are, and can delete the space. Removing a
  device deletes its snapshot and its keyring but revokes nothing: the device keeps the data key,
  and a copy of the storage taken earlier keeps its keyring (a cloud drive's version history is
  such a copy, and so is every computer its folder reaches). To shut out a lost device, or someone
  who has the sync key and an old master password, set up a new space and join the other devices
  to it. A new space stops only what comes next: whoever held the space may have read every
  account already, so the logins' passwords and the codes' secrets (each site's two-step setup)
  are to be changed too. The app says so where it shows an invitation or a pairing link.
- A relay, the built-in one included, sees when and from which addresses a space's devices sync,
  and its operator can make the space unavailable; the built-in relay is a single server the
  project runs for free, with no promise of service, and can be down or close. For a space that
  must not depend on Lockra's server, run a relay of your own or use storage of your own: the
  app recommends them (the storage form starts on WebDAV, a relay on one of one's own, and the
  built-in relay is chosen by name, for trying sync out).
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
