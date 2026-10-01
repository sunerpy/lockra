# In-app updates and the install scripts

What is proven automatically, what was run by hand here, and what has to be checked on each
system before a release is announced. The design is in `docs/security.md` ("Updates") and
`docs/release.md` ("In-app updates", "The install scripts").

## Automated

| Layer      | What                                                                                                                                                                                                                                                                                                                                                                                                 |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| core       | `crates/lockra-core/src/tests/update.rs` on the fake update source and the paused clock: unavailable copies refuse; check, install with throttled progress, the pending backup written first; one run at a time; failures and their codes; automatic checks (off by default, 10 s after start, daily, retry within the hour, one notice per version, skipped while the user runs one)                |
| shell      | `apps/desktop/src-tauri/tests/update.rs`: the real tauri-plugin-updater against a manifest server on 127.0.0.1: nothing newer (204), a package whose signature verifies, a valid signature made for another version (refused: `requireSignedVersion`), a package changed after signing (refused), no manifest, no server                                                                             |
| release    | `.github/scripts/test-tauri-release.py`: Lockra's real updater settings, every leg collected with its signatures, `latest.json` with a key per target and per installer (17), each pointing at its own asset and signature                                                                                                                                                                           |
| installers | `scripts/test-install.sh` (a fake GitHub, fake package managers and macOS tools; apt, dnf, zypper, the AppImage, Apple silicon through Rosetta, Intel, every refusal) and `scripts/test-install.ps1` (PowerShell 7 with the network, installer and registry cmdlets replaced); `.github/workflows/install-scripts.yml` runs both scripts against the real latest release on Linux, macOS and Windows |

## Run here

`make smoke-update` (`scripts/smoke-update-linux.sh`), 2026-10-01, Ubuntu 24.04 x64 under Xvfb:
two release AppImages, 9.0.0 and 9.0.1, signed with a throwaway key and pointed at a manifest on
127.0.0.1; 9.0.0 started and driven over WebDriver.

```text
smoke-update: Lockra 9.0.0, installed as an AppImage
smoke-update: the check found 9.0.1 (2026-10-01T12:00:00Z)
smoke-update: install started; the app replaces its AppImage and restarts
smoke-update: the AppImage on disk is now 9.0.1
smoke-update: Lockra restarted from the updated AppImage
smoke-update: OK, 9.0.0 updated itself to 9.0.1
```

So on Linux the whole path is the real one: the installer key the bundler wrote into the AppImage,
the manifest lookup, the download, the signature and its version, the replacement of the file and
the restart from it. The `.deb` and `.rpm` paths differ only in the installer the plugin runs
(`pkexec dpkg -i`, `pkexec rpm -U`), which needs an administrator prompt this run cannot answer.

## By hand, on each system

Install the release before the one being tested with the install script, open **Settings ›
About**, choose **Check for updates**, then **Download and install**, and check:

| System                         | Expect                                                                                                                                                                       |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Ubuntu (`.deb`)                | the polkit password prompt; afterwards `dpkg -s lockra` shows the new version and Lockra is running again                                                                    |
| Fedora (`.rpm`)                | the same with `rpm -q lockra`                                                                                                                                                |
| Windows x64 and ARM64          | the installer's progress window, no administrator prompt, Lockra opening again at the new version (**Settings › About**)                                                     |
| macOS, Apple silicon and Intel | Lockra restarting at the new version without a Gatekeeper prompt; **Remember on this device** still unlocking, or unlocking with the master password and turning it on again |
| A copy run from the build tree | **Settings › About** saying this copy cannot update itself, and no check button                                                                                              |

Then turn on **Check for updates automatically**, restart Lockra and check that the notice of a
newer version appears about 10 seconds after the start, and only once.

Lockra 0.1.x has no updater: the first release this can be checked from is 0.2.0, updating to the
next one.
