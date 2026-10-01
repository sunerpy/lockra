#!/usr/bin/env bash
# Screenshot the development showcase (every primitive, DESIGN.md §3) in the four themes, in the
# real Tauri window (WebKitGTK) under Xvfb. Usage: scripts/screenshot-showcase.sh [out-dir]
# Needs Xvfb, xdotool, xwininfo, scrot, python3 with Pillow, and the web workspace installed.
set -euo pipefail
cd "$(dirname "$0")/.."
out=${1:-docs/acceptance/screens/showcase}
mkdir -p "$out"
for tool in Xvfb xdotool xwininfo scrot python3; do
  command -v "$tool" >/dev/null || { echo "screenshot-showcase: $tool not installed"; exit 2; }
done
display=""
for n in $(seq 120 160); do
  if [ ! -e "/tmp/.X11-unix/X$n" ] && [ ! -e "/tmp/.X$n-lock" ]; then display=":$n"; break; fi
done
[ -n "$display" ] || { echo "screenshot-showcase: no free X display"; exit 2; }
work=$(mktemp -d)
xvfb_pid=""
app_pgid=""
cleanup() {
  [ -n "$app_pgid" ] && kill -- "-$app_pgid" 2>/dev/null || true
  [ -n "$xvfb_pid" ] && kill "$xvfb_pid" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT
Xvfb "$display" -screen 0 1280x800x24 -nolisten tcp >/dev/null 2>&1 &
xvfb_pid=$!
timeout 20 sh -c "until DISPLAY=$display xdpyinfo >/dev/null 2>&1; do sleep 0.5; done"
mkdir -p "$work/data" "$work/config/dev.lockra.desktop"
# Chinese copy for the acceptance screenshots (the container has no zh_CN locale for WebKit to read).
printf '{"locale":"zh-cn"}\n' >"$work/config/dev.lockra.desktop/settings.json"
(
  cd apps/desktop
  DISPLAY=$display LOCKRA_DEV_SECRET_STORE=memory XDG_DATA_HOME=$work/data XDG_CONFIG_HOME=$work/config \
    LANG=zh_CN.UTF-8 setsid pnpm exec tauri dev --config '{"build":{"devUrl":"http://localhost:1420/#showcase-freeze"}}' >"$work/dev.log" 2>&1 &
  echo $! >"$work/pid"
)
app_pgid=$(cat "$work/pid")
if ! timeout 900 sh -c "until DISPLAY=$display xdotool search --onlyvisible --name '^Lockra$' >/dev/null 2>&1; do sleep 1; done"; then
  echo "screenshot-showcase: the window did not appear"; tail -40 "$work/dev.log"; exit 1
fi
win=$(DISPLAY=$display xdotool search --onlyvisible --name '^Lockra$' | head -1)
DISPLAY=$display xdotool windowsize "$win" 1280 800
DISPLAY=$display xdotool windowmove "$win" 0 0
DISPLAY=$display xdotool windowactivate --sync "$win" 2>/dev/null || DISPLAY=$display xdotool windowfocus "$win"
# Each frame: shoot until two consecutive captures agree and differ from the previous theme's.
python3 - "$display" "$win" "$out" <<'PY'
import hashlib, subprocess, sys, time
from PIL import Image
display, win, out = sys.argv[1], sys.argv[2], sys.argv[3]
env = {"DISPLAY": display, "PATH": "/usr/bin:/bin:/usr/local/bin"}
def shoot(path):
    subprocess.run(["scrot", "--overwrite", path], env=env, check=True)
    geo = subprocess.run(["xwininfo", "-id", win], env=env, check=True, capture_output=True, text=True).stdout
    vals = {k: int(line.split(":")[1]) for line in geo.splitlines() for k in ("Absolute upper-left X", "Absolute upper-left Y", "Width", "Height") if line.strip().startswith(k)}
    x, y, w, h = vals["Absolute upper-left X"], vals["Absolute upper-left Y"], vals["Width"], vals["Height"]
    image = Image.open(path).crop((x, y, x + w, y + h))
    image.save(path)
    return hashlib.sha256(image.tobytes()).hexdigest()
# First wait for the page itself: a blank window "settles" too.
deadline = time.monotonic() + 120
while True:
    shoot(f"{out}/.probe.png")
    colors = Image.open(f"{out}/.probe.png").getcolors(maxcolors=1 << 20) or []
    if len(colors) > 200:
        break
    if time.monotonic() > deadline:
        sys.exit("screenshot-showcase: the page never rendered")
    time.sleep(0.5)
import os
os.remove(f"{out}/.probe.png")
previous = None
for key, theme in zip("1234", ["light", "dark", "warm", "graphite"]):
    subprocess.run(["xdotool", "key", "--window", win, key], env=env, check=True)
    path = f"{out}/showcase-{theme}.png"
    deadline = time.monotonic() + 30
    last = None
    while True:
        digest = shoot(path)
        if digest == last and digest != previous:
            break
        if time.monotonic() > deadline:
            sys.exit(f"screenshot-showcase: {theme} never settled")
        last = digest
        time.sleep(0.5)
    previous = digest
    print(f"screenshot-showcase: {path}")
    # The lower half: scroll the page body with the wheel, shoot, scroll back.
    subprocess.run(["xdotool", "mousemove", "--window", win, "640", "500", "click", "--repeat", "12", "5"], env=env, check=True)
    lower = f"{out}/showcase-{theme}-lower.png"
    last = None
    deadline = time.monotonic() + 30
    while True:
        lower_digest = shoot(lower)
        if lower_digest == last and lower_digest != digest:
            break
        if time.monotonic() > deadline:
            sys.exit(f"screenshot-showcase: {theme} lower half never settled")
        last = lower_digest
        time.sleep(0.5)
    print(f"screenshot-showcase: {lower}")
    subprocess.run(["xdotool", "click", "--repeat", "12", "4"], env=env, check=True)
PY
