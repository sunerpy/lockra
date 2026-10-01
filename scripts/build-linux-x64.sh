#!/usr/bin/env bash
# Build the Linux x64 desktop package (deb) with the Tauri CLI and check it: the web bundle carries
# no development code, the packaged executable resolves every library, and the control fields are
# what the package says. LOCKRA_APPIMAGE=1 adds an AppImage (the CLI downloads linuxdeploy for it).
# Packages go to dist/linux-x64 (git-ignored) with SHA256SUMS.txt.
# Usage: scripts/build-linux-x64.sh [out-dir]
set -euo pipefail
cd "$(dirname "$0")/.."
for tool in pnpm cargo node dpkg-deb ldd sha256sum; do
  command -v "$tool" >/dev/null || { echo "build-linux-x64: $tool not installed"; exit 2; }
done
out=${1:-dist/linux-x64}
bundles=deb
[ -n "${LOCKRA_APPIMAGE:-}" ] && bundles=deb,appimage

scripts/check-web-bundle.sh
(cd apps/desktop && pnpm exec tauri build --ci --bundles "$bundles")

version=$(node -p 'require("./package.json").version')
deb=target/release/bundle/deb/Lockra_${version}_amd64.deb
[ -f "$deb" ] || { echo "build-linux-x64: $deb was not produced"; exit 1; }
artefacts=("$deb")
if [ -n "${LOCKRA_APPIMAGE:-}" ]; then
  appimage=target/release/bundle/appimage/Lockra_${version}_amd64.AppImage
  [ -f "$appimage" ] || { echo "build-linux-x64: $appimage was not produced"; exit 1; }
  artefacts+=("$appimage")
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
dpkg-deb -x "$deb" "$work/root"
exe="$work/root/usr/bin/lockra-desktop"
[ -x "$exe" ] || { echo "build-linux-x64: the deb has no usr/bin/lockra-desktop"; exit 1; }
missing=$(ldd "$exe" | grep -c 'not found' || true)
[ "$missing" = 0 ] || { ldd "$exe" | grep 'not found'; echo "build-linux-x64: unresolved libraries"; exit 1; }

rm -rf "$out"
mkdir -p "$out"
cp "${artefacts[@]}" "$out"/
(cd "$out" && sha256sum ./* >SHA256SUMS.txt)
echo "deb control:"
dpkg-deb -f "$deb" Package Version Architecture Depends | sed 's/^/  /'
for f in "${artefacts[@]}"; do echo "$(basename "$f"): $(stat -c %s "$f") bytes"; done
echo "build-linux-x64: OK → $out"
