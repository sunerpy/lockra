<div align="center">

<img src="./apps/desktop/src-tauri/icons/128x128.png" alt="Lockra" width="96" />

# Lockra

### Two-factor codes on your own computer, encrypted and offline.

[![CI](https://github.com/sunerpy/lockra/actions/workflows/ci.yml/badge.svg)](https://github.com/sunerpy/lockra/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/sunerpy/lockra)](https://github.com/sunerpy/lockra/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](./LICENSE)

[Features](#features) · [Install](#install) · [Quick start](#quick-start) · [Documentation](https://firlab.app/lockra/) · [Development](#development)

[**English**](./README.md) · [简体中文](./docs/readme/README.zh-CN.md)

</div>

---

Lockra is a TOTP/HOTP authenticator for Windows, macOS and Linux. Your accounts live in one
encrypted file on your computer, and Lockra goes online only to check for updates. It imports from
Google Authenticator and Microsoft Authenticator, exports back to them, and keeps encrypted backups
in a folder you choose.

![Lockra's codes page](./docs/acceptance/screens/desktop/codes-1280-light.png)

## Features

- **Codes**: TOTP (SHA1, SHA256, SHA512, 6–8 digits, any period) and HOTP. Click a row to copy;
  the clipboard is cleared after 30 seconds if it still holds the code, and the next code shows in
  a code's last five seconds. Search, groups, pinned accounts and hidden codes.
- **Import**: Google Authenticator's _Transfer accounts_ QR codes (photos or screenshots, several
  at once), Microsoft Authenticator's database from a rooted Android phone, `otpauth://` links and
  lists, and Lockra backups. A preview marks every account as new, existing, conflicting or
  unsupported, with the reason, before anything is saved.
- **Export**: migration QR codes for Google Authenticator, one standard code per account for
  Microsoft Authenticator, or a plain list. Each asks for the master password again and shows the
  current codes beside the QR code so you can check the phone.
- **Backups**: encrypted `.lockrabackup` files on demand, and automatic ones into a folder (a
  OneDrive, Google Drive or iCloud folder works) a few seconds after each change, keeping the
  newest few. Restore by merging or by replacing.
- **Security**: Argon2id and XChaCha20-Poly1305, optional unlock with the system keychain,
  auto-lock, and screen-capture protection while a secret is shown (Windows and macOS).
- **Updates**: Settings › About checks for a new version when you ask (or once a day, if you turn
  that on) and installs it only when the package carries Lockra's signature, through the package
  Lockra was installed from (deb, rpm, AppImage, the Windows installer or the macOS app).
- **Interface**: four themes, eight accents, Chinese and English, a command palette (`Ctrl K`) and
  keyboard shortcuts throughout.

## Install

1. **One command**: the script picks the package for your computer, checks it against the
   release's `SHA256SUMS` and installs it
   ([install guide](https://firlab.app/lockra/guide/install#install-with-one-command)).

   ```bash
   # Linux and macOS
   curl -fsSL https://raw.githubusercontent.com/sunerpy/lockra/main/scripts/install.sh | sh
   ```

   ```powershell
   # Windows (PowerShell)
   irm https://raw.githubusercontent.com/sunerpy/lockra/main/scripts/install.ps1 | iex
   ```

2. **Download a package** from the [releases page](https://github.com/sunerpy/lockra/releases):
   the Windows installer (x64 and ARM64), a dmg for Apple silicon or Intel Macs, or a deb, rpm or
   AppImage for Linux (x64 and ARM64). Every release carries `SHA256SUMS` and build attestations:

   ```bash
   sha256sum -c SHA256SUMS --ignore-missing
   gh attestation verify Lockra_*_amd64.deb --repo sunerpy/lockra
   ```

   The packages are not code-signed yet: Windows SmartScreen and macOS Gatekeeper ask before the
   first start ([platform notes](https://firlab.app/lockra/reference/platforms)).

3. **Build from source** ([Development](#development)).

From 0.2.0, Lockra updates itself: **Settings › About › Check for updates**
([updates](https://firlab.app/lockra/guide/updates)).

## Quick start

1. Start Lockra and choose a master password. It cannot be recovered, so keep it somewhere safe.
2. Bring your accounts in on the Import page: photos of Google Authenticator's export codes, links
   pasted or read from the clipboard, or a backup.
3. Click an account to copy its code. Turn on automatic backups on the Backup page.

The [documentation](https://firlab.app/lockra/) covers each step, moving accounts from and to
phones, backups, security and questions.

## Development

Rust 1.98 (`rust-toolchain.toml`), Node ≥ 20.19 with pnpm 9, and on Linux webkit2gtk 4.1.
`make check` also needs Python ≥ 3.9, cargo-llvm-cov, cargo-deny, actionlint and shellcheck, and
names any that is missing before it starts.

```bash
pnpm install --frozen-lockfile
pnpm --filter @lockra/desktop tauri dev   # the app with hot reload
pnpm dev                                  # the interface alone, on an in-memory core
make check                                # every gate
make help                                 # everything else
```

[CONTRIBUTING.md](CONTRIBUTING.md) is the workflow, [AGENTS.md](AGENTS.md) the rules,
[docs/architecture.md](docs/architecture.md) the code, [docs/formats.md](docs/formats.md) the file
formats, [docs/security.md](docs/security.md) the security model and
[docs/release.md](docs/release.md) the release process.

## License

[Apache-2.0](./LICENSE). The interface design is derived from
[Voltip](https://github.com/sunerpy/voltip); the bundled fonts are under the SIL Open Font License
1.1 ([NOTICE](./NOTICE)).
