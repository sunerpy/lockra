# AGENTS.md

How to work in this repository, for coding agents and people alike.

## Layout

- `crates/` — the Rust workspace: `lockra-otp` (codes, Base32, URIs), `lockra-vault` (the
  encrypted container), `lockra-transfer` (Google, Microsoft, otpauth lists, QR codes),
  `lockra-core` (the platform-free application core: commands in, state and notices out, ports for
  everything native), `lockra-bridge` (the IPC contract), `lockra-sync` (the end-to-end encrypted
  sync: keyring, device snapshots, merge; no I/O), `lockra-remote` (its S3 and WebDAV storage) and
  `lockra-lan` (the LAN sync: the hub's folder, its server and its clients over Noise).
- `apps/desktop` — the React app (`src/`) and the Tauri 2 shell (`src-tauri/`, crate
  `lockra-desktop`).
- `apps/mobile` — the Android app (released from 0.7.0): the React phone app (`src/`) and its Tauri 2
  shell (`src-tauri/`, crate `lockra-mobile`; `gen/android` is the Gradle project with the Kotlin
  plugins, generated once and then kept).
- `packages/shared` (zod contract, backends, i18n, labels), `packages/ui` (design system).
- `docs/` — `architecture.md` first, then `formats.md`, `security.md`, `release.md`; `DESIGN.md` at
  the root is the design system's reading guide; `docs/acceptance/` holds the visual QA and its
  screenshots.
- `docs/site/` — the pages of the documentation site at <https://firlab.app/lockra/> (English, with
  Chinese under `zh/`); the site itself (VitePress, theme, deploy) lives in `sunerpy/firlab` under
  `lockra/`. `docs/site/README.md` has the writing rules.
- `.github/` — CI, the release workflow and its scripts, rendered from the github-project-scaffold
  tauri profile (`.github/scaffold.json` records the profile and the files allowed to differ).

## Commands

```bash
pnpm install --frozen-lockfile   # once
make hooks                       # pre-commit runs make fmt, pre-push make lint test
make check                       # every gate, stopping at the first failure; run before pushing
make fmt | make lint | make test # the narrower loops
make coverage                    # crates/* line coverage, floor 90 %
make sync-it                     # the sync storage against S3 and WebDAV servers in Docker
make pre-ci                      # the real app under Xvfb, the deb, the cross-built installer
make smoke-desktop               # the real app alone: flows and screenshots
make showcase                    # the component showcase in the four themes
make android-apk                 # the phone app's debug APK (JDK 21, Android SDK 36, NDK)
make site-screens                # the documentation site's screenshots (docs/site/README.md)
make help                        # everything else
```

## Rules

- **Tests first** for behaviour: show the test failing for the right reason, then the smallest
  change that makes it pass. Never weaken or skip an existing test to get green; fix the code.
- **Coverage floors** are part of the gates: crates 90 % of lines (`make coverage`, `:=` in the
  Makefile so the environment cannot lower it), each web package 85 % (`vitest.config.ts`).
- **The IPC contract**: after changing a type that crosses the bridge, regenerate and commit the
  fixtures: `UPDATE_IPC_FIXTURES=1 cargo test -p lockra-bridge --test contract`. They are compared
  byte for byte, so formatters must not touch them (`.oxfmtignore`).
- **No path and no secret to the webview**: files are opened in Rust after a native dialog or a
  drop (on the phone, the photo picker or the camera); only `entry_reveal`, `export_page`, `sync_create` (the new sync key), `sync_invite`
  and `sync_lan_offer` (a pairing offer) answer with secret material (docs/security.md). A new command needs its zod schema, its fixture
  and its i18n strings.
- **Commands are `async`** in the shell; Argon2 and file I/O go through `spawn_blocking` in the core.
- **Copy** lives in `packages/shared/src/i18n/zh-CN.ts` (the keys) and `en.ts` (same shape, tested).
  No colour literals in components (`scripts/check-no-literal-colors.sh`); use the tokens.
- **Layout** follows DESIGN.md §2: only the page body scrolls, grid tracks are `minmax(0, 1fr)`.
- **Waiting** in scripts is a condition with a deadline (`timeout … until …`), never a fixed sleep.
- **Dependencies** are pinned exactly (`=x.y.z`); `cargo deny check licenses bans sources` must
  pass. Tauri's runtime crates are pinned beside `tauri` (see `apps/desktop/src-tauri/Cargo.toml`).
- **Merging**: every change reaches `main` through a pull request, squash-merged
  (`gh pr merge --squash --match-head-commit <verified head>`, never `--admin`); release pull
  requests too. Never push to `main` directly, even when the token could bypass the ruleset: the
  owner is usually on its bypass list and an agent on the owner's token inherits it. The first
  push of the new repository is the only exception (`docs/release.md`).
- **Titles and commits**: the pull request title is the squash subject and release-please reads it,
  so it is a Conventional Commit, `type(scope): subject`, with a Chinese subject in the imperative
  (the `PR Title` check enforces the type); no AI-tool attribution trailers.
- **Releases** happen only through release-please's pull request and `release.yml`; never create
  or move a `v*` tag, a Release or a release asset by hand (`docs/release.md`).
- **Coverage** is a local hard floor, not a remote service: `make coverage` (crates, 90 %) and each
  package's `vitest.config.ts` (85 %) fail the build below it; there is no Codecov upload.
- **Docs**: a change users can see updates `docs/site/` in both languages in the same pull request,
  and `make site-screens` when a page's screenshot changes. `docs-site.yml` builds the site from the
  pull request; `publish-site.yml` pushes the merged pages to firlab (`FIRLAB_DOCS_TOKEN`).
