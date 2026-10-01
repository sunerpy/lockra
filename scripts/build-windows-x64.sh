#!/usr/bin/env bash
# Cross-build the Windows x64 installer (NSIS) from Linux with cargo-xwin, which downloads the
# MSVC CRT and Windows SDK on its first run. The MSVC C runtime is linked statically
# (.cargo/config.toml), so the exe needs no Visual C++ Redistributable; the check below reads its
# imports to prove it. The installer is not Authenticode-signed (SmartScreen will warn).
# Needs cargo-xwin, clang, lld-link, llvm-rc, llvm-readobj, makensis and the Rust target
# x86_64-pc-windows-msvc. Output: dist/windows-x64 with SHA256SUMS.txt.
# Usage: scripts/build-windows-x64.sh [out-dir]
set -euo pipefail
cd "$(dirname "$0")/.."
for tool in pnpm node cargo-xwin clang lld-link llvm-rc llvm-readobj makensis sha256sum; do
  command -v "$tool" >/dev/null || { echo "build-windows-x64: $tool not installed"; exit 2; }
done
rustup target list --installed | grep -q '^x86_64-pc-windows-msvc$' || { echo "build-windows-x64: run: rustup target add x86_64-pc-windows-msvc"; exit 2; }
out=${1:-dist/windows-x64}

scripts/check-web-bundle.sh
(cd apps/desktop && pnpm exec tauri build --ci --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis)

version=$(node -p 'require("./package.json").version')
release=target/x86_64-pc-windows-msvc/release
exe=$release/lockra-desktop.exe
setup=$release/bundle/nsis/Lockra_${version}_x64-setup.exe
for f in "$exe" "$setup"; do
  [ -f "$f" ] || { echo "build-windows-x64: $f was not produced"; exit 1; }
done
# A fresh Windows has no VCRUNTIME140.dll: the static CRT must leave no import of it.
if llvm-readobj --coff-imports "$exe" | grep -qiE 'Name: (vcruntime|msvcp)[0-9]*\.dll'; then
  llvm-readobj --coff-imports "$exe" | grep -iE 'Name: (vcruntime|msvcp)'
  echo "build-windows-x64: the exe imports the Visual C++ runtime (crt-static not applied)"
  exit 1
fi

rm -rf "$out"
mkdir -p "$out"
cp "$exe" "$setup" "$out"/
(cd "$out" && sha256sum ./*.exe >SHA256SUMS.txt && cat SHA256SUMS.txt)
for f in "$exe" "$setup"; do echo "$(basename "$f"): $(stat -c %s "$f") bytes"; done
echo "build-windows-x64: OK → $out"
