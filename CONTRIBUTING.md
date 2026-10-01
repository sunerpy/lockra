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
