#!/usr/bin/env bash
# Every gate of the repository, in order, stopping at the first failure: formatting (Rust, and
# oxfmt over TS, JSON, YAML, TOML and Markdown), clippy, the Rust tests (the IPC contract fixtures
# are compared byte for byte there), the crates' line coverage floor, the web packages' lint,
# types and tests with their own coverage floors, the release configuration, no colour literal in
# a component, a release bundle free of development code, cargo-deny, actionlint, shellcheck and
# the one-line installers (offline).
# The desktop smoke test and the packages are separate (make pre-ci): they need a display server
# stack and the cross toolchain. Usage: scripts/verify-all.sh   (make check)
set -euo pipefail
cd "$(dirname "$0")/.."
step() { printf '\n== verify: %s\n' "$*"; }

# Every tool the gates call, named before the first gate rather than discovered at the one that
# needs it (CONTRIBUTING.md, "Before you open a pull request").
missing=()
for tool in cargo pnpm python3 cargo-llvm-cov cargo-deny actionlint shellcheck; do
  command -v "$tool" >/dev/null || missing+=("$tool")
done
if command -v python3 >/dev/null && ! python3 -c 'import sys; sys.exit(sys.version_info < (3, 9))'; then
  missing+=("python3 3.9 or later")
fi
if [ "${#missing[@]}" -gt 0 ]; then
  echo "verify: missing ${missing[*]}; see CONTRIBUTING.md" >&2
  exit 2
fi

step "format";           make -s fmt-check
# The shells' generate_context! needs the web apps' output folders, even empty (a fresh clone).
mkdir -p apps/desktop/dist apps/mobile/dist
step "clippy";           cargo clippy --workspace --all-targets -- -D warnings
step "rust tests";       cargo test --workspace --all-targets
step "crate coverage";   make -s coverage
step "web lint + types"; pnpm -r run lint
step "web tests";        pnpm -r run test:coverage
step "release config";   python3 .github/scripts/tauri-release.py check-config --project apps/desktop/src-tauri --targets-file .github/release-targets.json >/dev/null && python3 .github/scripts/tauri-release.py matrix --file .github/release-targets.json >/dev/null && python3 .github/scripts/test-tauri-release.py
step "colour literals";  scripts/check-no-literal-colors.sh
step "release bundle";   scripts/check-web-bundle.sh
step "cargo-deny";       cargo deny check licenses bans sources
step "actionlint";       actionlint
step "shellcheck";       shellcheck scripts/*.sh .github/scripts/*.sh
step "install scripts";  scripts/test-install.sh
if command -v pwsh >/dev/null; then
  pwsh -NoProfile -File scripts/test-install.ps1
else
  echo "verify: pwsh is not installed here; scripts/install.ps1 is tested in CI"
fi
printf '\nverify: all gates passed\n'
