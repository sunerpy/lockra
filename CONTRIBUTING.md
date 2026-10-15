# Contributing to Lockra

Thank you for helping. This page covers the workflow; [AGENTS.md](AGENTS.md) has the rules the code
follows and [docs/architecture.md](docs/architecture.md) explains the code.

## Set up

Rust 1.98 (`rust-toolchain.toml` installs it), Node ≥ 20.19 with pnpm 9, and on Linux webkit2gtk 4.1.

```bash
pnpm install --frozen-lockfile
make hooks                                # pre-commit: make fmt · pre-push: make lint test
pnpm --filter @lockra/desktop tauri dev   # the app with hot reload
pnpm dev                                  # the interface alone, on an in-memory core
```

The Android app (`apps/mobile`) also needs JDK 21, the Android SDK with platform 36 and the NDK
(`ANDROID_HOME`, `NDK_HOME`) and the Rust target `aarch64-linux-android`; `make android-apk` builds
its debug APK and `make android-clippy` lints the crates it links for the phone.

### The browser extension

`pnpm --filter @lockra/extension build` builds `apps/extension` into `apps/extension/dist/`:
`chromium` (Chrome and Edge as the stores get it, without a key), `chromium-dev` (the same with the
development key, so that it always has the id `egbpficliljjnpkjbigonldakfllebco`) and `firefox`.
`make extension` also writes the store packages there, deterministic (sorted entries, fixed
times): `lockra-extension_<version>_chrome.zip`, which the Chrome Web Store and Edge Add-ons both
take, and `lockra-extension_<version>_firefox.zip`. A release builds them with
`scripts/package-extension.sh`, beside the source package addons.mozilla.org asks for
([docs/release.md](docs/release.md), "The browser extension").

- **Chrome or Edge**: open `chrome://extensions` (`edge://extensions`), turn on developer mode,
  choose _Load unpacked_ and pick `apps/extension/dist/chromium-dev`. Branded Chrome ignores the
  `--load-extension` switch from 137 on; loading it by hand works.
- **Firefox** (140 or later): open `about:debugging#/runtime/this-firefox`, choose _Load Temporary
  Add-on…_ and pick `apps/extension/dist/firefox/manifest.json`. Its id is `lockra@firlab.app` in
  every build.

The extension reaches Lockra through the native-messaging host `dev.lockra.browser` (the desktop
app in host mode, which writes the browsers' manifests when browser integration is on). The ids
those manifests allow are defined once, in `crates/lockra-browser/src/ids.rs`; a debug build of
Lockra also allows the development id. Only the development key's public half is in the repository
(`apps/extension/manifest/dev.json`): an unpacked build needs nothing else, and nothing is signed
with it.

## Before you open a pull request

```bash
make check     # every gate: formatting, lint, tests with coverage floors, deny, bundle, actionlint
make pre-ci    # optional, Linux: the real app under Xvfb, the deb, the cross-built Windows installer
```

`make check` needs, besides the set-up above, Python ≥ 3.9, cargo-llvm-cov, cargo-deny, actionlint
and shellcheck; it names any that is missing before the first gate. With PowerShell 7 (`pwsh`)
installed it also tests `scripts/install.ps1`; without it, CI does.

- Write the test first for any change in behaviour, and see it fail for the right reason.
- Never weaken, skip or delete a test to get green; fix the code.
- User-visible changes update the pages under [`docs/site/`](docs/site/README.md) in English and
  Chinese in the same pull request; when the interface's text or layout changes, capture the
  site's screenshots again with `make site-screens`.
- Interface changes come with screenshots in the light and dark themes.

## Pull requests and commits

- Every change reaches `main` through a pull request that is **squash-merged**; nobody pushes to
  `main` directly, the owner included.
- The pull request **title** becomes the commit subject and must be a
  [Conventional Commit](https://www.conventionalcommits.org/): `type(scope): subject`, for example
  `fix(import): 跳过 PhoneFactor 中加密的账号`. The `PR Title` check enforces it, and release-please
  reads it to choose the next version: `feat` bumps the minor version, `fix` the patch, and while
  the version is `0.x` a breaking change (`feat!:`) bumps the minor too.
- Types: `feat`, `fix`, `docs`, `chore`, `refactor`, `perf`, `test`, `build`, `ci`, `style`,
  `revert`. Dependency updates are `build(deps)`.
- No AI-tool attribution trailers in commit messages.

## Releases

Maintainers only; [docs/release.md](docs/release.md) describes the flow.

## Security

Report vulnerabilities privately ([SECURITY.md](SECURITY.md)), never in a public issue.

By contributing you agree that your contribution is licensed under the Apache License 2.0.
