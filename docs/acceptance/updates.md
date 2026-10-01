# In-app updates and the install scripts

What is proven automatically, what was run by hand here, and what has to be checked on each
system before a release is announced. The design is in `docs/security.md` ("Updates") and
`docs/release.md` ("In-app updates", "The install scripts").

## Automated

| Layer      | What                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| core       | `crates/lockra-core/src/tests/update.rs` on the fake update source and the paused clock, Voltip's design: unavailable copies refuse; check, install with throttled progress to `ready`, the pending backup written first; an install after a check neither asks nor downloads again; one run at a time; failures and their codes; the automatic update (off by default, 10 s after start, downloads to `ready` and remembers the version, the next start installs that version, a newer one waits again, the switch downloads without installing, skipped while the user runs one) |
| shell      | `apps/desktop/src-tauri/tests/update.rs`: the real tauri-plugin-updater against a manifest server on 127.0.0.1: nothing newer (204), a package whose signature verifies, a valid signature made for another version (refused: `requireSignedVersion`), a package changed after signing (refused), no manifest, no server                                                                                                                                                                                                                                                           |
| release    | `.github/scripts/test-tauri-release.py`: Lockra's real updater settings, every leg collected with its signatures, `latest.json` with a key per target and per installer (17), each pointing at its own asset and signature                                                                                                                                                                                                                                                                                                                                                         |
| installers | `scripts/test-install.sh` (a fake GitHub, fake package managers and macOS tools; apt, dnf, zypper, the AppImage, Apple silicon through Rosetta, Intel, every refusal) and `scripts/test-install.ps1` (PowerShell 7 with the network, installer and registry cmdlets replaced); `.github/workflows/install-scripts.yml` runs both scripts against the real latest release on Linux, macOS and Windows                                                                                                                                                                               |

## Run here

`make smoke-update` (`scripts/smoke-update-linux.sh`), 2026-10-01, Ubuntu 24.04 x64 under Xvfb:
two release AppImages, 9.0.0 and 9.0.1, signed with a throwaway key and pointed at a manifest on
127.0.0.1; 9.0.0 started and driven over WebDriver: a check, then in the unlocked app the title
bar's note, Settings › General and the update dialog over it (screenshots `update-badge-light`,
`update-settings-light`, `update-dialog-light` in `screens/desktop/`), and the install started
with the dialog's 「立即更新」.

```text
smoke-update: Lockra 9.0.0, installed as an AppImage
smoke-update: the check found 9.0.1 (2026-10-01T12:00:00Z)
smoke-update: the title bar and Settings › General named 9.0.1; the update dialog opened
smoke-update: install started from the dialog; the app replaces its AppImage and restarts
smoke-update: the AppImage on disk is now 9.0.1
smoke-update: Lockra restarted from the updated AppImage
smoke-update: OK, 9.0.0 updated itself to 9.0.1
```

So on Linux the whole path is the real one: the installer key the bundler wrote into the AppImage,
the manifest lookup, the download, the signature and its version, the replacement of the file and
the restart from it. The `.deb` and `.rpm` paths differ only in the installer the plugin runs
(`pkexec dpkg -i`, `pkexec rpm -U`), which needs an administrator prompt this run cannot answer.

## After the 0.2.0 release

Checked on 2026-10-01 against the published release (release.yml run 36863249429 on 92d4438):

- `https://github.com/sunerpy/lockra/releases/latest/download/latest.json` is the release's
  `latest.json` byte for byte: version 0.2.0 and 17 keys, each pointing at a v0.2.0 asset listed
  in `SHA256SUMS` and carrying that asset's `.sig`. The packages downloaded here (both `.deb`,
  both `.rpm`, both Windows setup programs) match `SHA256SUMS` and verify with `minisign` against
  the key in `tauri.conf.json`, with the trusted comment `version:0.2.0`; one changed byte is
  refused. `gh attestation verify` traces those six packages, `latest.json` and `SHA256SUMS` to
  `release.yml` on `main` at 92d4438.
- The install scripts on real runners (install-scripts.yml run 36865836191): apt and the AppImage
  on Linux x64 and ARM64, dnf on Fedora, the dmg on macOS, and the setup program on Windows x64 and
  ARM64 each installed Lockra 0.2.0.
- The update from GitHub itself, under Xvfb: an AppImage built from `main` as 0.1.99 with the
  shipped updater settings (the release key, the GitHub endpoint), started the way
  `scripts/smoke-update-linux.sh` starts it and driven with `scripts/smoke/update.py`:

```sh
(cd apps/desktop && pnpm exec tauri build --ci --bundles appimage \
  --config '{"version":"0.1.99","bundle":{"createUpdaterArtifacts":false}}')
```

```text
smoke-update: Lockra 0.1.99, installed as an AppImage
smoke-update: the check found 0.2.0 (2026-10-01T12:49:44Z)
smoke-update: install started; the app replaces its AppImage and restarts
live-update: the AppImage on disk is now the published 0.2.0 (sha256 3d130234263e9856b72f75af6c7ea11585c504d7a28306bfd61c1971af3c890d)
live-update: OK, 0.1.99 updated itself to the published 0.2.0 from GitHub and restarted
```

## By hand, on each system

Install the release before the one being tested with the install script (from 0.3.0: the title
bar's note, or **Settings › General › Check for updates** and **View the new version**; then
**Update now** in the update dialog; 0.2.0 has **Settings › About › Check for updates** and
**Download and install**), and check:

| System                         | Expect                                                                                                                                                                       |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Ubuntu (`.deb`)                | the polkit password prompt; afterwards `dpkg -s lockra` shows the new version and Lockra is running again                                                                    |
| Fedora (`.rpm`)                | the same with `rpm -q lockra`                                                                                                                                                |
| Windows x64 and ARM64          | the installer's progress window, no administrator prompt, Lockra opening again at the new version (**Settings › About**)                                                     |
| macOS, Apple silicon and Intel | Lockra restarting at the new version without a Gatekeeper prompt; **Remember on this device** still unlocking, or unlocking with the master password and turning it on again |
| A copy run from the build tree | **Settings › General** saying this copy cannot update itself, and the check button disabled                                                                                  |

Then, on a copy one release behind, turn on **Settings › General › Automatic updates**: the
title bar shows **Restart to update** once the download is verified, and nothing installs. Quit
Lockra instead of restarting and start it again: about 10 seconds later it installs that version
and restarts at it.

Lockra 0.1.x has no updater: the first release this can be checked from is 0.2.0, updating to the
next one.
