#!/usr/bin/env bash
# The in-app update end to end on Linux, with real AppImages: two release builds (9.0.0 and
# 9.0.1) signed with a throwaway key, both trusting that key and asking a manifest on 127.0.0.1;
# the manifest served here; 9.0.0 started under Xvfb and driven over WebDriver to check and to
# install (scripts/smoke/update.py). Then the AppImage on disk must be the 9.0.1 package, and a
# Lockra must be running from it again. The release key and GitHub are never involved.
# Usage: scripts/smoke-update-linux.sh   (make smoke-update; docs/acceptance/updates.md)
set -euo pipefail
cd "$(dirname "$0")/.."
for tool in Xvfb xdpyinfo tauri-driver WebKitWebDriver python3 pnpm curl sha256sum pgrep; do
  command -v "$tool" >/dev/null || { echo "smoke-update: $tool not installed"; exit 2; }
done
from=9.0.0
to=9.0.1

display=""
for n in $(seq 161 199); do
  if [ ! -e "/tmp/.X11-unix/X$n" ] && [ ! -e "/tmp/.X$n-lock" ]; then display=":$n"; break; fi
done
[ -n "$display" ] || { echo "smoke-update: no free X display"; exit 2; }
free_port() {
  for p in $(seq "$1" 2 "$2"); do
    if ! (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null && ! (exec 3<>"/dev/tcp/127.0.0.1/$((p + 1))") 2>/dev/null; then echo "$p"; return; fi
  done
  echo "smoke-update: no free port" >&2
  exit 2
}
driver_port=$(free_port 4500 4598)
manifest_port=$(free_port 18600 18698)

work=$(mktemp -d)
pids=()
cleanup() {
  status=$?
  if [ "$status" -ne 0 ] && [ -s "$work/driver.log" ]; then
    echo "smoke-update: the end of the driver's log:"
    tail -n 40 "$work/driver.log"
  fi
  pkill -f "$work/apps/Lockra.AppImage" 2>/dev/null || true
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  rm -rf "$work"
}
trap cleanup EXIT

# A throwaway key: an update signed with it installs only into these two builds.
(cd apps/desktop && pnpm exec tauri signer generate --ci -w "$work/key" >/dev/null 2>&1)
pubkey=$(cat "$work/key.pub")

build() {
  local version=$1 config
  config=$(python3 -c 'import json, sys; print(json.dumps({"version": sys.argv[1], "bundle": {"createUpdaterArtifacts": True}, "plugins": {"updater": {"pubkey": sys.argv[2], "endpoints": [f"http://127.0.0.1:{sys.argv[3]}/latest.json"], "requireSignedVersion": True, "dangerousInsecureTransportProtocol": True}}}))' "$version" "$pubkey" "$manifest_port")
  echo "smoke-update: building the AppImage of $version"
  (cd apps/desktop && TAURI_SIGNING_PRIVATE_KEY="$(cat "$work/key")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    pnpm exec tauri build --ci --bundles appimage --config "$config" >"$work/build-$version.log" 2>&1) ||
    { tail -n 30 "$work/build-$version.log"; exit 1; }
  cp "target/release/bundle/appimage/Lockra_${version}_amd64.AppImage" "target/release/bundle/appimage/Lockra_${version}_amd64.AppImage.sig" "$work/"
}
build "$to"
build "$from"

mkdir -p "$work/serve" "$work/apps"
cp "$work/Lockra_${to}_amd64.AppImage" "$work/serve/"
python3 - "$work" "$to" "$manifest_port" <<'EOF'
import json, sys
work, version, port = sys.argv[1], sys.argv[2], sys.argv[3]
name = f"Lockra_{version}_amd64.AppImage"
entry = {"url": f"http://127.0.0.1:{port}/{name}", "signature": open(f"{work}/{name}.sig").read().strip()}
manifest = {"version": version, "notes": f"## {version}\n\n- An update for the smoke test", "pub_date": "2026-10-01T12:00:00Z",
            "platforms": {"linux-x86_64-appimage": entry, "linux-x86_64": entry}}
json.dump(manifest, open(f"{work}/serve/latest.json", "w"))
EOF
python3 -m http.server "$manifest_port" --bind 127.0.0.1 --directory "$work/serve" >"$work/serve.log" 2>&1 &
pids+=($!)
timeout 20 sh -c "until curl -sf http://127.0.0.1:$manifest_port/latest.json >/dev/null; do sleep 0.5; done"

installed="$work/apps/Lockra.AppImage"
install -m 0755 "$work/Lockra_${from}_amd64.AppImage" "$installed"
new_sum=$(sha256sum <"$work/Lockra_${to}_amd64.AppImage" | cut -d' ' -f1)

Xvfb "$display" -screen 0 1440x900x24 -nolisten tcp >/dev/null 2>&1 &
pids+=($!)
timeout 20 sh -c "until DISPLAY=$display xdpyinfo >/dev/null 2>&1; do sleep 0.5; done"
mkdir -p "$work/data" "$work/config"
# Extract-and-run: no FUSE needed; the runtime still sets APPIMAGE, the file the updater replaces.
export DISPLAY="$display" XDG_DATA_HOME="$work/data" XDG_CONFIG_HOME="$work/config" APPIMAGE_EXTRACT_AND_RUN=1
tauri-driver --port "$driver_port" --native-port "$((driver_port + 1))" >"$work/driver.log" 2>&1 &
pids+=($!)
timeout 30 sh -c "until curl -sf http://127.0.0.1:$driver_port/status >/dev/null; do sleep 0.5; done"

python3 scripts/smoke/update.py --driver "http://127.0.0.1:$driver_port" --app "$installed" --from-version "$from" --to-version "$to"

if ! timeout 120 sh -c "until [ \"\$(sha256sum <'$installed' | cut -d' ' -f1)\" = '$new_sum' ]; do sleep 1; done"; then
  echo "smoke-update: the AppImage was not replaced by $to"; exit 1
fi
echo "smoke-update: the AppImage on disk is now $to"
grep -q "GET /Lockra_${to}_amd64.AppImage" "$work/serve.log" || { echo "smoke-update: the package was not downloaded from the manifest server"; exit 1; }
# The restart: a new process runs from the replaced file (the extract-and-run runtime keeps
# APPIMAGE pointing at it).
if ! timeout 60 sh -c "until pgrep -f '$work/apps/Lockra.AppImage' >/dev/null || pgrep -af lockra-desktop | grep -q '$work'; do sleep 1; done"; then
  echo "smoke-update: Lockra did not start again after the update"; pgrep -af 'Lockra|lockra' || true; exit 1
fi
echo "smoke-update: Lockra restarted from the updated AppImage"
echo "smoke-update: OK, $from updated itself to $to"
