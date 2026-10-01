#!/usr/bin/env bash
# Desktop smoke test on Linux: the real app (a debug build serving the release web bundle, with
# the in-memory keychain of debug builds) under Xvfb, in throwaway data and config folders.
# scripts/smoke/desktop.py drives it over WebDriver (tauri-driver + WebKitWebDriver) with real X
# input; then the app is started once more on its own and closed with the title bar's button,
# which must end the process with status 0. Screenshots go to the given folder.
# Usage: scripts/smoke-desktop-linux.sh [out-dir]
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)
out=${1:-docs/acceptance/screens/desktop}
for tool in Xvfb xdpyinfo xdotool xclip tauri-driver WebKitWebDriver python3 cargo pnpm curl; do
  command -v "$tool" >/dev/null || { echo "smoke: $tool not installed"; exit 2; }
done

echo "smoke: building the web bundle and the app"
pnpm --filter @lockra/desktop build >/dev/null
# custom-protocol: the binary serves the bundle it embeds instead of the Vite dev server.
cargo build -q -p lockra-desktop --features tauri/custom-protocol
cargo build -q -p lockra-transfer --example decode-qr
app="$root/target/debug/lockra-desktop"
decode_qr="$root/target/debug/examples/decode-qr"

display=""
for n in $(seq 120 160); do
  if [ ! -e "/tmp/.X11-unix/X$n" ] && [ ! -e "/tmp/.X$n-lock" ]; then display=":$n"; break; fi
done
[ -n "$display" ] || { echo "smoke: no free X display"; exit 2; }
port=""
for p in $(seq 4444 2 4498); do
  if ! (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null && ! (exec 3<>"/dev/tcp/127.0.0.1/$((p + 1))") 2>/dev/null; then port=$p; break; fi
done
[ -n "$port" ] || { echo "smoke: no free port pair"; exit 2; }

work=$(mktemp -d)
xvfb_pid=""
driver_pid=""
app_pid=""
cleanup() {
  for pid in "$app_pid" "$driver_pid" "$xvfb_pid"; do
    [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
  done
  rm -rf "$work"
}
trap cleanup EXIT

Xvfb "$display" -screen 0 1440x900x24 -nolisten tcp >/dev/null 2>&1 &
xvfb_pid=$!
timeout 20 sh -c "until DISPLAY=$display xdpyinfo >/dev/null 2>&1; do sleep 0.5; done"

mkdir -p "$work/data" "$work/config/dev.lockra.desktop"
# Chinese copy (the container has no zh_CN locale for the webview to follow).
printf '{"locale":"zh-cn"}\n' >"$work/config/dev.lockra.desktop/settings.json"
export DISPLAY="$display" XDG_DATA_HOME="$work/data" XDG_CONFIG_HOME="$work/config" LOCKRA_DEV_SECRET_STORE=memory

tauri-driver --port "$port" --native-port "$((port + 1))" >"$work/driver.log" 2>&1 &
driver_pid=$!
if ! timeout 30 sh -c "until curl -sf http://127.0.0.1:$port/status >/dev/null; do sleep 0.5; done"; then
  echo "smoke: tauri-driver did not start"; cat "$work/driver.log"; exit 1
fi

python3 scripts/smoke/desktop.py --driver "http://127.0.0.1:$port" --app "$app" --out "$out" \
  --decode-qr "$decode_qr" --display "$display" --work "$work"
kill "$driver_pid" 2>/dev/null || true
wait "$driver_pid" 2>/dev/null || true
driver_pid=""

# The title bar's close button ends the process cleanly.
"$app" >"$work/app.log" 2>&1 &
app_pid=$!
if ! timeout 60 sh -c 'until xdotool search --onlyvisible --name "^Lockra$" >/dev/null 2>&1; do sleep 0.5; done'; then
  echo "smoke: the window did not appear"; tail -20 "$work/app.log"; exit 1
fi
win=$(xdotool search --onlyvisible --name '^Lockra$' | head -1)
xdotool windowmove "$win" 0 0
read -r x y < <(python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(round(r[0]), round(r[1]))' "$work/close-button.json")
# A click that lands before the page has loaded does nothing: try again, at most three times.
for attempt in 1 2 3; do
  xdotool mousemove --window "$win" "$x" "$y" click 1
  if timeout 10 tail --pid="$app_pid" -f /dev/null; then break; fi
  echo "smoke: the window is still open after click $attempt"
done
set +e
wait "$app_pid"
status=$?
set -e
app_pid=""
[ "$status" -eq 0 ] || { echo "smoke: the app exited with $status after its close button"; tail -20 "$work/app.log"; exit 1; }
echo "smoke: the close button ended the app with status 0"
echo "smoke: passed"
