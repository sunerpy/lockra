#!/usr/bin/env bash
# Sync smoke test on Linux: two copies of the real app (a debug build serving the release web
# bundle, in-memory keychain) syncing through the Versity S3 gateway in Docker, under Xvfb.
# scripts/smoke/sync.py drives each start over WebDriver: device A sets up the space, device B
# joins it with the sync key and changes the accounts, device A takes the changes in and refuses an
# older snapshot of B's put back on the storage. Then two more devices sync through lockra-relay on
# 127.0.0.1: C sets up a space on it and shows an invitation, D joins from the invitation and its
# code alone, and C takes D's change in. Each device has its own data and config folders; they run
# one after the other. Screenshots go to the given folder. Every wait is a condition with a
# deadline.
# Usage: scripts/smoke-sync-linux.sh [out-dir]   (make smoke-sync)
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)
out=${1:-docs/acceptance/screens/sync}
for tool in Xvfb xdpyinfo xdotool tauri-driver WebKitWebDriver python3 cargo pnpm curl docker; do
  command -v "$tool" >/dev/null || { echo "smoke-sync: $tool not installed"; exit 2; }
done
s3_image=${LOCKRA_IT_S3_IMAGE:-versity/versitygw:v1.8.0}

echo "smoke-sync: building the web bundle and the app"
pnpm --filter @lockra/desktop build >/dev/null
cargo build -q -p lockra-desktop --features tauri/custom-protocol
cargo build -q -p lockra-relay
app="$root/target/debug/lockra-desktop"

free_port() {
  for p in $(seq "$1" "$2"); do
    if ! (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null; then echo "$p"; return; fi
  done
  echo "smoke-sync: no free port in $1-$2" >&2
  exit 2
}
display=""
for n in $(seq 161 200); do
  if [ ! -e "/tmp/.X11-unix/X$n" ] && [ ! -e "/tmp/.X$n-lock" ]; then display=":$n"; break; fi
done
[ -n "$display" ] || { echo "smoke-sync: no free X display"; exit 2; }
s3_port=$(free_port 19200 19299)
relay_port=$(free_port 19300 19399)
port=""
for p in $(seq 4500 2 4598); do
  if ! (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null && ! (exec 3<>"/dev/tcp/127.0.0.1/$((p + 1))") 2>/dev/null; then port=$p; break; fi
done
[ -n "$port" ] || { echo "smoke-sync: no free port pair"; exit 2; }

work=$(mktemp -d)
s3=lockra-smoke-s3-$$
xvfb_pid=""
driver_pid=""
relay_pid=""
cleanup() {
  status=$?
  if [ "$status" -ne 0 ] && [ -s "$work/driver.log" ]; then
    echo "smoke-sync: the end of the driver's log:"
    tail -n 60 "$work/driver.log"
  fi
  for pid in "$driver_pid" "$relay_pid" "$xvfb_pid"; do
    if [ -n "$pid" ]; then kill "$pid" 2>/dev/null || true; fi
  done
  docker rm -f "$s3" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

user=lockra-smoke
umask 077
od -An -N18 -tx1 /dev/urandom | tr -d ' \n' >"$work/s3-secret"
docker run -d --name "$s3" -p "127.0.0.1:$s3_port:7070" -e ROOT_ACCESS_KEY_ID="$user" -e ROOT_SECRET_ACCESS_KEY="$(cat "$work/s3-secret")" \
  --entrypoint sh "$s3_image" -c 'mkdir -p /gw/lockra-it && exec /usr/local/bin/versitygw posix /gw' >/dev/null
timeout 60 sh -c "until [ \"\$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:$s3_port/)\" != 000 ]; do sleep 0.5; done" ||
  { docker logs "$s3" | tail -n 20; exit 1; }

"$root/target/debug/lockra-relay" --bind "127.0.0.1:$relay_port" --data "$work/relay" >"$work/relay.log" 2>&1 &
relay_pid=$!
timeout 30 sh -c "until curl -sf http://127.0.0.1:$relay_port/healthz >/dev/null; do sleep 0.5; done" ||
  { cat "$work/relay.log"; exit 1; }

Xvfb "$display" -screen 0 1440x900x24 -nolisten tcp >/dev/null 2>&1 &
xvfb_pid=$!
timeout 20 sh -c "until DISPLAY=$display xdpyinfo >/dev/null 2>&1; do sleep 0.5; done"
export DISPLAY="$display" LOCKRA_DEV_SECRET_STORE=memory

# One app start: device `$1` (its own folders) runs phase `$2`.
phase() {
  local device=$1 name=$2
  mkdir -p "$work/$device/data" "$work/$device/config/dev.lockra.desktop"
  [ -f "$work/$device/config/dev.lockra.desktop/settings.json" ] ||
    printf '{"locale":"zh-cn"}\n' >"$work/$device/config/dev.lockra.desktop/settings.json"
  XDG_DATA_HOME="$work/$device/data" XDG_CONFIG_HOME="$work/$device/config" \
    tauri-driver --port "$port" --native-port "$((port + 1))" >"$work/driver.log" 2>&1 &
  driver_pid=$!
  if ! timeout 30 sh -c "until curl -sf http://127.0.0.1:$port/status >/dev/null; do sleep 0.5; done"; then
    echo "smoke-sync: tauri-driver did not start"; cat "$work/driver.log"; exit 1
  fi
  python3 scripts/smoke/sync.py --phase "$name" --driver "http://127.0.0.1:$port" --app "$app" --out "$out" \
    --work "$work" --s3 "http://127.0.0.1:$s3_port" --s3-user "$user" --secret-file "$work/s3-secret" \
    --relay "http://127.0.0.1:$relay_port" --relay-data "$work/relay"
  kill "$driver_pid" 2>/dev/null || true
  wait "$driver_pid" 2>/dev/null || true
  driver_pid=""
  # The session's app quits with its driver: the next start must not find its window.
  if ! timeout 30 sh -c 'while xdotool search --onlyvisible --name "^Lockra$" >/dev/null 2>&1; do sleep 0.5; done'; then
    echo "smoke-sync: device $device's window is still open"; exit 1
  fi
}

phase A a
phase B b
phase A c
phase C r1
phase D r2
phase C r3
echo "smoke-sync: passed"
