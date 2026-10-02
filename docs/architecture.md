# Lockra architecture

Lockra is a desktop TOTP/HOTP authenticator: a Rust core in a Tauri 2 shell, with a React
webview for the interface. Read this first; `docs/formats.md` has the file formats,
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
  lockra-sync      End-to-end encrypted sync, without I/O: the keyring, the device snapshots,
                   hybrid logical clocks, the last-writer-wins merge and one sync step.
  lockra-remote    The sync's storage over HTTP (S3-compatible or WebDAV, through OpenDAL).
apps/desktop/
  src-tauri/       lockra-desktop: the Tauri shell (commands, keychain, clipboard, dialogs, drops,
                   screen-capture protection, single instance).
  src/             The React app: shell, pages, dialogs.
packages/shared    zod contract, the Backend interface (TauriBackend, MockBackend), i18n, labels.
packages/ui        The design system (Voltip's tokens and components, plus Lockra's own).
```

## The core

`lockra-core` holds all behaviour and no platform code. `Core::start(config, ports)` returns a
cheap handle; the shell injects the ports:

| Port                        | Desktop adapter                                                               | Test fake                           |
| --------------------------- | ----------------------------------------------------------------------------- | ----------------------------------- |
| `SecretStore` (device keys) | `keyring` (Credential Manager, Keychain, Secret Service), probed at start     | `FakeKeychain`, `MemorySecretStore` |
| `Clipboard`                 | `arboard` on its own thread (on Linux the owner process serves the clipboard) | `FakeClipboard`                     |
| `Clock`                     | `SystemClock`                                                                 | `FakeClock`                         |
| `CodeSink` (code frames)    | a Tauri `Channel`                                                             | `RecordingSink`                     |
| `Updater` (in-app update)   | tauri-plugin-updater (`src-tauri/src/updater.rs`), only in a packaged copy    | `FakeUpdater`, `NoUpdater`          |
| `SyncTransport` (sync)      | lockra-remote, S3 or WebDAV over HTTPS (`src-tauri/src/sync.rs`)              | `FakeTransport`, `NoSync`           |

State machine: **NoVault → Locked → Unlocked**. Create or restore leads from NoVault to Unlocked;
unlock (password or device key) from Locked; lock, auto-lock and closing return to Locked; reset
returns from Locked to NoVault. Only Unlocked holds decrypted entries, the import preview and
export sessions; leaving it drops them.

Argon2 and file I/O run on `spawn_blocking`. One **scheduler task** owns every timer — the next
code window, auto-lock, clipboard clearing, the automatic backup debounce, export expiry, the
automatic update — sleeps until the earliest deadline, and is woken through a `Notify`
whenever the state changes, so nothing polls. Core tests run on tokio's paused clock with the
fakes.

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
- `import_pick_files`, `backup_save`, `backup_pick_dir`, `restore_pick`, `export_otpauth_file` —
  the native dialogs, run by Rust; the webview gets a file name or `false`, never a path.
- `codes_subscribe { onFrame }` / `codes_unsubscribe` — code frames through a `Channel` created by
  the webview. The core keeps one subscription; a new one replaces it. The first frame arrives at
  once (empty while locked); then one at each code window's end.

Events: `lockra://event` carries `{type: "state", state}` (the whole `UiState` after every change)
and `{type: "notice", notice}` (copied, imported, backup written or failed, auto-locked…);
`lockra://drag` carries `enter` / `leave` while files are dragged over the window (dropped files
go to the import in Rust).

The contract is kept honest from both sides: `lockra-bridge/tests/contract.rs` serialises
representative states, events, commands and answers into `packages/shared/src/fixtures/ipc/`
(byte for byte; `UPDATE_IPC_FIXTURES=1` rewrites them after an intended change), and
`ipc-contract.test.ts` parses the same files with the zod schemas and replays them through
`TauriBackend`. Errors cross as `{code, retry_at_ms?}`; the webview translates the code.

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

| Layer    | How                                                                                                                                                                                                                                  |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| crates   | unit tests and RFC vectors; core scenarios on fakes and a paused tokio clock; line coverage ≥ 90 % (`make coverage`)                                                                                                                 |
| contract | Rust fixtures ⇄ zod schemas, secret-leak assertions                                                                                                                                                                                  |
| shell    | `tests/ipc.rs` on `tauri::test::MockRuntime`: the registered commands, dispatch, typed errors, no path, the code channel; `tests/update.rs`: the real updater plugin against a local manifest (signature, signed version, tampering) |
| web      | vitest + Testing Library on `MockBackend`, line coverage ≥ 85 % per package                                                                                                                                                          |
| app      | `make smoke-desktop`: the real app under Xvfb driven over WebDriver and X input (`docs/acceptance/visual-qa.md`)                                                                                                                     |

`make verify` runs every gate; `make linux-x64` and `make windows-x64` build the packages.
