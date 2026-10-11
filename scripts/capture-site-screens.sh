#!/usr/bin/env bash
# Screenshots for the documentation site: the real app (a debug build serving the release web
# bundle, with the in-memory keychain of debug builds) under Xvfb, once per language, each time in
# fresh throwaway folders. scripts/smoke/site_screens.py drives it over WebDriver with made-up
# accounts and writes <page>-<lang>-<theme>.webp. Look at every image before committing it
# (docs/site/README.md, "Screenshots").
# Usage: scripts/capture-site-screens.sh [out-dir]
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)
out=${1:-docs/site/public/screens}
for tool in Xvfb xdpyinfo xdotool xclip tauri-driver WebKitWebDriver python3 cargo pnpm curl; do
  command -v "$tool" >/dev/null || { echo "site-screens: $tool not installed"; exit 2; }
done
python3 -c 'import PIL, cairosvg' 2>/dev/null || { echo "site-screens: needs Pillow and cairosvg for python3"; exit 2; }

echo "site-screens: building the web bundle and the app"
pnpm --filter @lockra/desktop build >/dev/null
# custom-protocol: the binary serves the bundle it embeds instead of the Vite dev server.
cargo build -q -p lockra-desktop --features tauri/custom-protocol
cargo build -q -p lockra-transfer --example encode-qr
app="$root/target/debug/lockra-desktop"
encode_qr="$root/target/debug/examples/encode-qr"

display=""
for n in $(seq 120 160); do
  if [ ! -e "/tmp/.X11-unix/X$n" ] && [ ! -e "/tmp/.X$n-lock" ]; then display=":$n"; break; fi
done
[ -n "$display" ] || { echo "site-screens: no free X display"; exit 2; }
# A free pair of ports for tauri-driver and WebKitWebDriver, looked up again for every driver.
free_ports() {
  for p in $(seq 4444 2 4498); do
    if ! (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null && ! (exec 3<>"/dev/tcp/127.0.0.1/$((p + 1))") 2>/dev/null; then
      echo "$p"
      return
    fi
  done
  echo "site-screens: no free port pair" >&2
  exit 2
}

work=$(mktemp -d)
xvfb_pid=""
driver_pid=""
cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    for log in "$work"/driver-*.log; do
      [ -f "$log" ] && { echo "site-screens: the end of $(basename "$log"):"; tail -n 30 "$log"; }
    done
  fi
  for pid in "$driver_pid" "$xvfb_pid"; do
    if [ -n "$pid" ]; then kill "$pid" 2>/dev/null || true; fi
  done
  rm -rf "$work"
}
trap cleanup EXIT

Xvfb "$display" -screen 0 1440x900x24 -nolisten tcp >/dev/null 2>&1 &
xvfb_pid=$!
timeout 20 sh -c "until DISPLAY=$display xdpyinfo >/dev/null 2>&1; do sleep 0.5; done"
export DISPLAY="$display" LOCKRA_DEV_SECRET_STORE=memory

# With bubblewrap the app runs where /home is a throwaway folder, so the backup folder on screen
# reads like a real one (/home/alex/OneDrive/Lockra) and nothing of this machine shows. Without
# it, the folders stay in the temporary directory and the screenshot shows that path.
sandbox=""
if command -v bwrap >/dev/null && bwrap --dev-bind / / --bind "$work" /home true 2>/dev/null; then
  sandbox=1
fi

for lang in en zh; do
  dir="$work/$lang"
  port=$(free_ports)
  # tauri-driver starts the app with its own environment: one driver per language and folder set.
  if [ -n "$sandbox" ]; then
    mkdir -p "$dir/home/alex/OneDrive/Lockra" "$dir/home/alex/.local/share" "$dir/home/alex/.config"
    home=/home/alex
    bwrap --dev-bind / / --bind "$dir/home" /home --unshare-pid --die-with-parent \
      env HOME="$home" XDG_DATA_HOME="$home/.local/share" XDG_CONFIG_HOME="$home/.config" \
      tauri-driver --port "$port" --native-port "$((port + 1))" >"$work/driver-$lang.log" 2>&1 &
  else
    home="$dir"
    mkdir -p "$dir/OneDrive/Lockra" "$dir/data" "$dir/config"
    XDG_DATA_HOME="$dir/data" XDG_CONFIG_HOME="$dir/config" \
      tauri-driver --port "$port" --native-port "$((port + 1))" >"$work/driver-$lang.log" 2>&1 &
  fi
  driver_pid=$!
  if ! timeout 30 sh -c "until curl -sf http://127.0.0.1:$port/status >/dev/null; do sleep 0.5; done"; then
    echo "site-screens: tauri-driver did not start"; cat "$work/driver-$lang.log"; exit 1
  fi
  python3 scripts/smoke/site_screens.py --driver "http://127.0.0.1:$port" --app "$app" --out "$out" \
    --encode-qr "$encode_qr" --display "$display" --lang "$lang" --backup-dir "$home/OneDrive/Lockra"
  kill "$driver_pid" 2>/dev/null || true
  wait "$driver_pid" 2>/dev/null || true
  driver_pid=""
done
echo "site-screens: done; look at every image in $out before committing"
