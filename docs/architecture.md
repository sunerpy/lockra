# Lockra architecture

Lockra is a desktop TOTP/HOTP authenticator: a Rust core in a Tauri 2 shell, with a React
webview for the interface; an Android app on the same core ships from 0.7.0 (below, "The phone
shell"). Password management (logins, the generator, the browser extension) is in every build
from 0.10.0 (below, "The core"). Read this first; `docs/formats.md` has the file formats,
`docs/security.md` the security model and `DESIGN.md` the design system.

## Layout

```
crates/
  lockra-otp       HOTP / TOTP (RFC 4226 / 6238), lenient Base32, otpauth:// URIs. Pure logic.
  lockra-vault     The encrypted container of the vault and the backups: Argon2id, XChaCha20-
                   Poly1305, key slots, atomic writes.
  lockra-transfer  Google's migration codes, Microsoft's PhoneFactor database, otpauth lists, QR
                   codes (SVG out with qrcode, images in with rxing), file type detection.
  lockra-core      The application: session state machine, entries, import preview, export
                   sessions, backups, settings, the scheduler; ports for everything native.
  lockra-bridge    The wire contract: the UiCommand enum, dispatch, and the contract fixtures.
  lockra-sync      End-to-end encrypted sync, without I/O: the device snapshots and the keyrings
                   they carry, hybrid logical clocks, the last-writer-wins merge, one sync step.
  lockra-remote    The sync's storage: over HTTP (S3-compatible or WebDAV, through OpenDAL; a
                   Lockra relay, RelayStore, waited on by RelayWatch), or a folder a cloud drive
                   keeps in sync (FolderStore, watched by FolderWatch).
  lockra-relay     The relay server, a binary of its own (docs/relay.md): keeps the snapshots of
                   spaces whose devices have no storage, opening none; never linked into the apps.
  lockra-autofill  Autofill's pure logic: URL matching (Public Suffix List, equivalent domains),
                   password rules, the generator, strength (zxcvbn); bundled data under data/.
  lockra-browser   The browser extension's channel (docs/formats.md §10): frames, envelopes, Noise
                   (XX to pair, KK for a session), the app's service loop on a listener the shell
                   gives it, the host's relay. Every decision on a request is the core's.
apps/desktop/
  src-tauri/       lockra-desktop: the Tauri shell (commands, keychain, clipboard, dialogs, drops,
                   screen-capture protection, single instance, the tray, the quick search's global
                   shortcut; the browser channel's socket and manifests, and the host mode the
                   browsers start it in).
  src/             The React app: shell, pages, dialogs.
apps/mobile/       The Android app (from 0.7.0).
  src-tauri/       lockra-mobile: the phone's Tauri shell; gen/android is its Gradle project, with
                   a Kotlin class per native capability.
  src/             The React phone app: create or unlock the vault, the codes, adding accounts
                   (the camera, photos, files, links, the clipboard, by hand) through the import
                   preview, an account's actions (edit, pin, show the secret, delete), settings,
                   sync.
apps/extension/    The browser extension (Chrome, Edge, Firefox; Manifest V3): the background
                   worker with Noise in TypeScript, the content scripts, the inline menu, the popup
                   and the options page.
packages/shared    zod contract, the Backend interface (TauriBackend, MockBackend), i18n, labels.
packages/ui        The design system (Voltip's tokens and components, plus Lockra's own).
```

## The core

`lockra-core` holds all behaviour and no platform code. `Core::start(config, ports)` returns a
cheap handle; the shell injects the ports:

| Port                        | Desktop adapter                                                                   | Test fake                           |
| --------------------------- | --------------------------------------------------------------------------------- | ----------------------------------- |
| `SecretStore` (device keys) | `keyring` (Credential Manager, Keychain, Secret Service), probed at start         | `FakeKeychain`, `MemorySecretStore` |
| `Clipboard`                 | `arboard` on its own thread (on Linux the owner process serves the clipboard)     | `FakeClipboard`                     |
| `Clock`                     | `SystemClock`                                                                     | `FakeClock`                         |
| `CodeSink` (code frames)    | a Tauri `Channel`                                                                 | `RecordingSink`                     |
| `Updater` (in-app update)   | tauri-plugin-updater (`src-tauri/src/updater.rs`), only in a packaged copy        | `FakeUpdater`, `NoUpdater`          |
| `SyncTransport` (sync)      | lockra-remote: S3/WebDAV/relay over HTTPS, or a folder (`src-tauri/src/sync.rs`)  | `FakeTransport`, `NoSync`           |
| `Biometrics` (unlock check) | Touch ID, Windows Hello via robius-authentication (`src-tauri/src/biometrics.rs`) | `FakeBiometrics`, `NoBiometrics`    |

State machine: **NoVault → Locked → Unlocked**. Create or restore leads from NoVault to Unlocked;
unlock (password or device key) from Locked; lock, auto-lock and closing return to Locked; reset
returns from Locked to NoVault. Only Unlocked holds decrypted entries, the import preview and
export sessions; leaving it drops them.

Beside the entries the vault keeps logins (`item.rs`, docs/formats.md §2). A login merges part by
part, its metadata one register and each field another, so two devices changing different parts
at once both keep their change; a hidden value a write replaces stays in a short history, from
which a conflict is derived. The webview's view of a login holds no secret (`logins.rs`); one comes
back only in `item_reveal` and the generator's answers. Their commands, the generator's and the
site rules' (`session/items.rs`) answer `feature_unavailable` while the build does not offer
password management (`Features.passwords`, on in every build since 0.10.0), and the vault still
keeps and syncs what other devices wrote. The browser extension's part is
`browser.rs` and `session/browser.rs`: what the vault keeps of it (the app's static key, the paired
extensions, the policy) and every decision on a paired extension's request (the match for the
frame, frames of other sites, the HTTP downgrade, the protection level, the rate limits).
`lockra-browser` calls `Core::browser_*`; the core holds no port of it, and a browser request never
restarts the idle timer.

Argon2 and file I/O run on `spawn_blocking`. One **scheduler task** owns every timer — the next
code window, auto-lock, clipboard clearing, the automatic backup debounce, export expiry, the
automatic update, the next sync run — sleeps until the earliest deadline, and is woken through a
`Notify` whenever the state changes, so nothing polls. The desktop shell tells the core when its
window gains or loses the focus (`Core::set_foreground`): in front, sync runs every minute
rather than every five. While the vault is unlocked, a space in a folder or on a relay is also
watched (`SyncTransport::watch`: the folder's file events, the relay's held-back listing), and a
change wakes the scheduler for a run a second later. The transport opens a space with its
`SpaceAccess`, derived from the sync key when needed (a relay's access token) and never stored.
Core tests run on tokio's paused clock with the fakes.

The in-app update follows Voltip's design. It is a run in the background, one at a time:
`update_check` asks the `Updater` afresh (`UiState.update` goes `checking` → `up_to_date` /
`available` / `failed`); `update_install` goes on from what the last run left (a found release is
not asked for again, a downloaded package not downloaded again), downloads with progress (the port
verifies the signature) to `ready`, writes a pending automatic backup, then installs and lets the
shell restart. With `Settings.auto_update` (off by default) the scheduler runs the automatic
update once, 10 s after start: it downloads a newer release to `ready` and remembers the version
in `update-ready.json`; the next start installs that same version at once, and turning the switch
on checks and downloads without installing. The webview shows the status in the title bar (a note
that opens the update dialog) and in Settings › General and › About. The port reports how this
copy installs (`deb`, `rpm`, `appimage`, `nsis`, `msi`, `app`), read from the bundle type the
bundler patched into the executable; a build from the tree has none and cannot update itself
(`docs/security.md`, "Updates").

## The bridge and the shell

The webview talks to the shell through Tauri commands, all `async` (a sync command would run on
the main thread and freeze the window while Argon2 works):

- `lockra_dispatch { command }` — every core command. `UiCommand` is one tagged enum
  (`{"command": "vault_unlock", "password": …}`); its struct variants reject unknown fields, and
  no variant carries a path.
- `import_pick_files`, `backup_save`, `backup_pick_dir`, `restore_pick`, `export_otpauth_file`,
  `sync_key_save`, `sync_pick_folder` — the native dialogs, run by Rust; the webview gets a file
  name, a folder to show or `false`, and never sends a path. `sync_pick_folder` hands the folder to
  the core (`Core::sync_choose_folder`), and the storage `{kind: "folder"}` of the next setup, join
  or move stands for it.
- `codes_subscribe { onFrame }` / `codes_unsubscribe` — code frames through a `Channel` created by
  the webview. The core keeps one subscription; a new one replaces it. The first frame arrives at
  once (empty while locked); then one at each code window's end.

Events: `lockra://event` carries `{type: "state", state}` (the whole `UiState` after every change)
and `{type: "notice", notice}` (copied, imported, backup written or failed, auto-locked, what a
sync run brought…);
`lockra://drag` carries `enter` / `leave` while files are dragged over the window (dropped files
go to the import in Rust).

The contract is kept honest from both sides: `lockra-bridge/tests/contract.rs` serialises
representative states, events, commands and answers into `packages/shared/src/fixtures/ipc/`
(byte for byte; `UPDATE_IPC_FIXTURES=1` rewrites them after an intended change), and
`ipc-contract.test.ts` parses the same files with the zod schemas and replays them through
`TauriBackend`. Errors cross as `{code, retry_at_ms?}`; the webview translates the code.

## The browser channel

The extension keeps no vault and caches no secret: it asks the running app. The browser starts
`lockra-desktop` as its native-messaging host (`dev.lockra.browser`), which passes frames between its
stdio and the app's local socket. Noise is end to end between the extension and the app, so the
host and the socket are pipes that read nothing (docs/formats.md §10, docs/security.md, "Browser
extension").

- The host mode (`src-tauri/src/browser/host.rs`): `run()` knows the browser's arguments (the
  extension's origin first, or Firefox's manifest path and add-on id) and, before Tauri or the
  single-instance check, runs `lockra_browser::host::relay` on a current-thread runtime. stdout is
  the protocol's; the log goes to stderr. With no app on the socket the host answers `not_running`
  itself; the extension's `start_app` starts Lockra with `--from-browser` (it stays in the tray
  when it runs in the background, and a running Lockra keeps its window as it is) or, with `show`,
  brings its window up.
- The socket (`browser/socket.rs`): a Unix socket in a folder of the user's alone, connections
  from the same user only (`peer_cred`); on Windows a named pipe named after the config folder,
  created as the first instance, refusing remote clients.
- The service (`browser/service.rs`): while `settings.json`'s `browser_integration` is on,
  `lockra_browser::serve` runs on the socket from the start, answering even while the vault is
  locked. The shell commands `browser_enable { browsers }` (the chosen browsers' manifests written,
  `browser/manifest.rs`, with winreg's keys on Windows; the socket listened on; the flag set),
  `browser_disable` (no longer listening: every session ended, the socket removed; every manifest
  removed) and `browser_status` (per browser: installed, Lockra's manifest in place) answer
  `browser_unavailable` without password management. At each start with the integration on, the
  manifests are written again (the program may have moved with an update).
- The extension's folder (`browser/extension.rs`): Chrome, Edge and Brave never update an extension
  loaded unpacked, so the desktop app carries the Chromium build of its own release (`build.rs`
  embeds `apps/extension/dist/chromium-unpacked/`, which the `beforeBuildCommand` builds first;
  `LOCKRA_EXTENSION_REQUIRED=1` in the release legs makes a missing build, or one of another
  version, fail) and keeps it in `<local data>/Lockra/browser/extension`. At each start with the
  integration on (before the socket is listened on) and at `browser_enable`, the folder is brought
  to this build's copy: written beside it, `lockra-folder.json` last (the version and the digest of
  the files), then two renames; a folder whose marker names this build's digest is left as it is,
  and a failure is only logged. `browser_extension` (where it is, the version, whether it holds it),
  `browser_extension_copy` (its path on the clipboard) and `browser_extension_reveal` (the file
  manager) serve Settings › 浏览器. The extension reads the marker from its own package
  (`apps/extension/src/background/update.ts`) as its background starts, at each new version of
  Lockra and as one of its pages opens; when the folder holds another version than the one that
  runs, it calls `runtime.reload()` once nothing of it is open and no request or pairing is under
  way, at most once per version. The browser reads the folder again and keeps the extension's
  storage. Without the marker (a folder the user unzipped, Firefox) the popup tells an extension
  older than Lockra; `PairedView.version` lets Settings › 浏览器 tell it too.
- The tray (`src-tauri/src/background.rs`): an icon whenever Lockra runs on a desktop, the app
  icon's padlock without its tile (`icons/tray/`, drawn at the size the system shows it; a template
  image on macOS). A left press brings the window up (Windows, macOS: the press alone, never its
  release or a double click's second press); the right button opens the menu, and on Linux, whose
  AppIndicator sends no clicks, any click does: open, the quick search where it is offered, lock,
  settings, check for updates where this copy updates itself, quit, in the app's language, rebuilt
  as that, the lock or the offer changes (lock, settings and check for updates are off while
  locked). Settings and check for updates bring the window up and ask the core
  (`Core::open_settings`, `Core::open_updates`), which sends `Notice::OpenSettings` /
  `Notice::OpenUpdates` while unlocked; `open_updates` asks for a newer release first only when no
  run is in flight and no release waits, so a download never starts over from the tray.
- Running in the background (`background.rs`, `watch_close` in `lib.rs`): with `run_in_background`
  on, closing the window hides it and Lockra stays in the tray, so the extension and the quick
  search can reach it; off, closing the window quits as it always has. Another start brings the window up, except one for the browser (and one with
  `--palette` brings the palette too, below); on macOS the Dock's icon does too
  (`RunEvent::Reopen`). The Linux packages depend on libayatana-appindicator for the tray.
- The interface: Settings › 浏览器 (`apps/desktop/src/pages/settings/Browser.tsx`) turns the
  integration and running in the background on and off, and shows the browsers, the pairings, the
  policy, the site lists and the activity; the requests that wait for the user, a pairing to
  approve or a fill to confirm, come up over anything open (`features/browser/BrowserRequests.tsx`),
  and the shell brings the window up with them (`background::brings_window_up`).
- The extension (`apps/extension`): the background worker holds the connection
  (`background/connection.ts`: reconnecting with a backoff; after `start_app`, a hello every second
  for thirty seconds), Noise in TypeScript (`noise/`), the content scripts that find the fields and
  fill them (`content/`), the inline menu in an extension-origin frame (`inline/`), the popup and
  the options page; `protocol/` holds the zod schemas of the messages.

## The quick search

The command palette comes up from any app (`src-tauri/src/quick_access.rs`), to copy a login's user
name, password or code: by a global shortcut (Settings › 通用 › 快捷搜索, `settings.json`'s
`quick_access_shortcut`, none until one is chosen, Ctrl/⌘+Shift+Space suggested), by the tray's
「快捷搜索」, and by `lockra-desktop --palette`, which a running Lockra hears through the
single-instance check. Each brings the window up and asks the core (`Core::quick_access_open`),
which sends `Notice::OpenPalette` only while unlocked: locked, the window shows the unlock screen
and nothing of the vault comes up over another app. The webview opens the palette on the notice,
unless a dialog is open (the window then shows it as it is).

- The shortcut is registered from Rust only: tauri-plugin-global-shortcut is added the first time
  one is to be registered, on the main thread, and the webview has no permission for its commands.
  The shell follows `UiState.quick_access.shortcut` as the state changes (at start too: it works
  while locked) and reports whether the system took it (`Core::quick_access_registered`); a
  failure shows as `quick_access.error` = `shortcut_unavailable`, in memory only, cleared as
  another shortcut is chosen. On Linux only an X11 session lets an app take a global shortcut: a
  Wayland compositor keeps the keys from apps (XWayland's included), so there the shell registers
  nothing, reports the shortcut unavailable, and Settings gives `lockra-desktop --palette` to bind
  in the desktop's own keyboard settings.
- The shell remembers how the window was (hidden in the tray, minimized). When the palette the quick
  search opened closes, the webview says whether something was copied from it (the shell command
  `quick_access_done { copied }`); after a copy the window goes back: hidden again (on macOS the app
  hides, so the app the user came from has the keys back) or minimized again. Closed without a copy,
  brought up some other way (the tray's 打开, the Dock, another start, an extension's request), or
  locked meanwhile, the window stays where it is.
- Like the rest of password management it needs `Features.passwords`: a build without it registers
  nothing, the tray has no 「快捷搜索」 and Settings shows no row; `--palette` then only brings the
  window up.

## The phone shell

`apps/mobile` is the Android app, released from 0.7.0. `lockra-mobile` (a `staticlib`,
`cdylib` and `rlib`, Tauri without its desktop features) starts the same core with the phone's ports
and registers lockra-bridge's `PHONE_COMMANDS`: `lockra_dispatch`, `codes_subscribe`,
`codes_unsubscribe`, `import_pick_files` (images with the photo picker, any other file with the
system's file picker), `import_scan` and `sync_scan_join` (the camera), `backup_save`,
`restore_pick` and `export_otpauth_file` (the system's file picker), and `update_open_release` (the
browser, for a newer release's page). Each native capability is a
small Tauri plugin whose Kotlin class lives in `gen/android` (generated by `tauri android init`,
then kept; the files Lockra wrote or changed say so), called from Rust with `run_mobile_plugin` on
a blocking thread; its answers stay in Rust:

- `Clipboard`: `ClipboardPlugin.kt` through `src/clipboard.rs`. A code goes on marked
  `EXTRA_IS_SENSITIVE`; clearing compares, and in the background (where Android lets no app read
  the clipboard) clears only when nothing changed since Lockra's own write.
- The camera: `ScannerPlugin.kt` opens `ScannerActivity.kt` (CameraX with ZXing's QR reader, no
  Play services, `FLAG_SECURE`), which answers the call itself, once, in this process: the first
  code read goes to `Core::import_scanned` (the preview's source `camera`) or, for
  `sync_scan_join`, as the invitation to `Core::sync_join` (`src/sync.rs`); `left`, `denied` and
  `noCamera` end the scan, and `away` (the app left while the camera was open) locks the vault
  in Rust at once (`src/scanner.rs`).
- The photo picker and the file picker: `FilesPlugin.kt` hands over what was picked as names and
  bytes, never a path, each file read to one byte past the import's limit; `Core::import_picked`
  reads them as it reads files, a Lockra backup waiting for its password in the preview, and
  `Core::restore_open_bytes` opens a backup to restore. `saveFile` writes the bytes it is handed
  where the user picks: a backup (`Core::backup_sealed` makes it, `Core::backup_recorded` notes it
  once saved) or a plain otpauth list (`Core::export_otpauth_text`, after the master password and
  before the picker opens) (`src/files.rs`).

- The fingerprint: `BiometricPlugin.kt` through `src/biometrics.rs` is both the `Biometrics` port
  (`BiometricPrompt`, strong biometrics only) and the `SecretStore`: the device key sealed by an
  Android Keystore key that works for ten seconds after a passed check and is invalidated by a new
  enrollment, so the core's check-then-read (and check-then-write when it is turned on) needs one
  prompt. The store's status is learnt with the fingerprint's availability, which the core asks
  for at start and at every lock, never from a call of its own (the state reads it all the time).

The sync storage is lockra-remote's, as on the desktop (the phone chooses no folder: a space in a
computer's cloud drive folder is reached over the same drive's WebDAV, `sync_scan_join` carrying
that storage when the invitation holds no storage; a relay is waited on as on the desktop),
except for the certificate authorities:
on Android they are read from the files the system keeps them in (the platform verifier would need
JNI glue in unsafe code; docs/security.md, "Sync"). The updater is `src/updater.rs`: it reads the
release manifest with the same client when the user checks, and is `InstallMethod::Android`, for
which the core only checks, or `InstallMethod::Play` for a copy Google Play installed, for which it
does not even check. Who installed the app is asked of `UpdatePlugin.kt` once after start, off the
setup thread (a plugin call waits for the main thread setup holds); until it answers the copy counts
as a GitHub one, and when it names Google Play the core republishes its state
(`Core::refresh_update`). The environment cannot carry it from `MainActivity.kt`: an arm64 app
translated on an x86 device has a libc of its own (PR #102's emulator runs). `update_open_release` opens
the page of the release found, or the Play listing (in the Play app first), through
`BrowserPlugin.kt` (`src/browser.rs`), and answers with the address where nothing opens it.
`MainActivity.kt` keeps the window `FLAG_SECURE` and draws it edge to edge (the webview pads with
`env(safe-area-inset-*)`); edge to edge the window no longer shrinks for the keyboard, so the
content takes the keyboard's height as bottom padding and the field being typed in stays above it
(the device smoke test checks that the webview gives way). The webview locks the vault when the page
is hidden (`visibilitychange`: another app in front, the screen off), except behind a screen of the
phone's own that Lockra opened, the camera's or the photo picker (`app/phone-screen.ts`): wry
pauses the webview for any activity in front, even the permission prompt, and the call comes back
as that screen closes, often before the webview has resumed, so the exception lasts until the page
is visible again. Leaving from the camera's page locks the vault in Rust (`away`).

The phone app (`apps/mobile/src`) uses `@lockra/ui` and `@lockra/shared` as the desktop does; what
both need lives there: the codes list's logic (sorting, filtering, group sections) and a form's kind
in `packages/shared/src/entries.ts`, the import preview's choices in `import-preview.ts`, the colour
and avatar text editor in `@lockra/ui`, the sync storage's form in `storage-form.ts` and
`StorageFields`, the dragged order (`sortEntries` and `groupSections` with this device's
`entry_order` and `group_order`, `moveItem`) and its list (`ReorderList` on `Sortable`, @dnd-kit,
with ArrowUp and ArrowDown on a handle), and the pinned group (`pinned_group`: `groupSections` puts
its section first, `entryFilters` its chip, `pinnedFilter` the chip the list opens on; set with
`view_pin_group` from `PinnedGroupSelect` in Settings, or the desktop's ⋯ menu). The desktop picks a group with `GroupCombobox`; the phone
with a sheet from the bottom (`components/BottomSheet.tsx`), which holds a history entry of its own
above its page's, so the back gesture closes the sheet first. The phone's pages that prove presence
take the fingerprint that unlocks the vault (`app/presence.ts`): asked by itself where it is the
default unlock and the page asks nothing else, else with the password left empty. The unlocked vault's pages lie over the codes
(`app/nav.tsx`): each page opened adds a history entry that records its depth, so the phone's back
gesture (wry goes back in the webview's history) and a page's own back button close the top page
alike; leaving the import preview discards the import, leaving the restore ends it, and locking
closes every page. A page closes once what it shows is gone (the account, the import, the restore;
a merged restore goes on to the preview), but only after it has been there: the command's answer
and the state event come by different routes, in no fixed order.
`make android-apk` builds a debug APK; CI's `android` job builds the release APK and AAB unsigned,
signs them with a key made for the run and checks them (`.github/scripts/check-android-package.sh`),
and `android-device` runs that APK on an emulator.

## The webview

- `packages/shared`: `schema.ts` (zod, the mirror of the Rust types), `Backend` with
  `TauriBackend` (IPC) and `MockBackend` (an in-memory core for the browser preview and the page
  tests; loaded only under `import.meta.env.DEV`, absent from the release bundle —
  `scripts/check-web-bundle.sh`), the dictionaries (`zh-CN.ts` defines the keys, `en.ts` must match)
  and label helpers.
- `packages/ui`: tokens, theme helpers, components, `BackendProvider` (`useUiState`, `useCodes`),
  and `useClock` — one shared, second-aligned clock for every countdown.
- `apps/desktop`: `shell/` (window frame, sidebar, title bar, footer, command palette), `pages/`
  (welcome, unlock, codes, import, export, backup, settings), `features/` (account dialogs, the
  export viewer), `app/` (shell state, dispatch helpers, shortcuts, appearance).

The shell state holds the page and one overlay at a time (an account dialog, the export viewer or
settings); global shortcuts pause while an overlay is open. Effects never close sessions in their
cleanup: React's StrictMode runs cleanups on mount, which closed export sessions and stopped the
code stream in development (both fixed, both covered by tests).

## Tests

| Layer       | How                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| crates      | unit tests and RFC vectors; core scenarios on fakes and a paused tokio clock; line coverage ≥ 90 % (`make coverage`)                                                                                                                                                                                                                                                                                                                        |
| contract    | Rust fixtures ⇄ zod schemas, secret-leak assertions                                                                                                                                                                                                                                                                                                                                                                                         |
| shell       | `tests/ipc.rs` on `tauri::test::MockRuntime`: the registered commands, dispatch, typed errors, no path, the code channel; `tests/update.rs`: the real updater plugin against a local manifest (signature, signed version, tampering)                                                                                                                                                                                                        |
| web         | vitest + Testing Library on `MockBackend`, line coverage ≥ 85 % per package                                                                                                                                                                                                                                                                                                                                                                 |
| app         | `make smoke-desktop`: the real app under Xvfb driven over WebDriver and X input (`docs/acceptance/visual-qa.md`); it turns the browser integration on and reaches the running app through the real binary in host mode                                                                                                                                                                                                                      |
| browser     | `crates/lockra-browser/tests/vectors/`: every message and the Noise handshakes as the Rust types and snow write them (`UPDATE_BROWSER_FIXTURES=1`), read by the extension's tests too; the host's relay on fakes; the shell's socket and every system's manifest paths on any machine, the named pipe and the registry on CI's Windows; `tests/host_mode.rs`: the real binary started as a browser starts it (the release build on Windows) |
| browser e2e | `make smoke-browser`: the development build of the extension in Chrome for Testing and Firefox (pinned, checked by SHA-256) paired with the real app under Xvfb: the menu, the shortcut (a real X key), the next page's code, the policy's limits, pages that try to hijack a click, a sign-up, the save and update prompts, locking (`docs/acceptance/browser.md`)                                                                         |
| phone       | `apps/mobile/src-tauri/tests/ipc.rs` on `MockRuntime`; CI's `android-device`: the release APK on an emulator creates a vault, locks on leaving, unlocks again, adds an account by hand and copies its code, and reaches AWS S3 over HTTPS with the system's certificate authorities (`.github/scripts/android-device-smoke.sh`)                                                                                                             |

`make verify` runs every gate; `make linux-x64` and `make windows-x64` build the packages.
