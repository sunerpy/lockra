# Developers

This page is for people who want to build Lockra, change it, or check how it works.

## Source

The code is on GitHub at [sunerpy/lockra](https://github.com/sunerpy/lockra), under the Apache
License 2.0. It is a Rust core in a Tauri 2 shell with a React interface:

| Part                             | What it holds                                                           |
| -------------------------------- | ----------------------------------------------------------------------- |
| `crates/lockra-otp`              | HOTP and TOTP (RFC 4226 and 6238), Base32, otpauth links                |
| `crates/lockra-vault`            | the encrypted container of the vault and the backups                    |
| `crates/lockra-transfer`         | Google's migration codes, Microsoft's database, otpauth lists, QR codes |
| `crates/lockra-core`             | the application: vault session, import, export, backups, settings       |
| `crates/lockra-bridge`           | the contract between the interface and the core                         |
| `apps/desktop`                   | the Tauri shell and the React app                                       |
| `apps/mobile`                    | the Android app, in progress: its Tauri shell and React app             |
| `packages/ui`, `packages/shared` | the design system; the contract, translations and test backend          |

## Building

You need Rust 1.98, Node 20.19 or later with pnpm 9, and on Linux WebKitGTK 4.1. `make check` also
needs Python 3.9 or later, cargo-llvm-cov, cargo-deny, actionlint and shellcheck.

```bash
pnpm install --frozen-lockfile
pnpm --filter @lockra/desktop tauri dev   # the app with hot reload
make check                                # every gate the CI runs
make help                                 # everything else
```

`CONTRIBUTING.md` and `AGENTS.md` in the repository describe the workflow and the rules.

## Design documents

These documents describe how Lockra is built. They are written in English and kept next to the
code.

- [Architecture](/dev/architecture): the parts, the IPC contract and the tests
- [File formats](/dev/formats): the vault, backups, settings, and each import and export format
- [Security model](/dev/security): what is protected, how, and the residual risks
- [Releasing](/dev/release): the release pipeline and the repository settings

## This site

The pages of this site live in the repository under `docs/site/`, in English and Chinese; a change
to the app updates its page in the same pull request.
