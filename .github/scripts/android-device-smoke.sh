#!/usr/bin/env bash
# The Android app on a device: install the APK, start it, create a vault, leave the app (it locks)
# and unlock it again; it stays up throughout with nothing fatal in its log, and the page gives way
# to the keyboard rather than lie under it. A package that passes every check can still close on
# start (a Tauri app that panics before its first screen), so the app itself has to run. CI runs this against an emulator (`android-device` in ci.yml); it runs the
# same against a phone over adb.
#
# Usage: android-device-smoke.sh <apk or directory holding one> <out dir>
# Needs `adb` on PATH with one device online. Writes into <out>: install.txt, start.txt,
# logcat.txt, crash.txt, exit-info.txt, ui.xml and screen.png, whatever the outcome, and
# app-logcat.txt (the app's process alone) when it came up.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <apk or directory> <out dir>" >&2
  exit 2
fi
apk=$1 out=$2
if [ -d "$apk" ]; then
  apk=$(find "$apk" -name '*.apk' | sort | head -1)
fi
[ -f "$apk" ] || { echo "android-device-smoke: no APK at $1" >&2; exit 2; }
package=dev.lockra.mobile
# Typed with `input text`, which needs no spaces; a throwaway vault on a throwaway device.
password=lockra-smoke-7f3a
mkdir -p "$out"

collect() {
  adb logcat -d >"$out/logcat.txt" 2>&1 || true
  adb logcat -b crash -d >"$out/crash.txt" 2>&1 || true
  # Android 11+ keeps why the process last ended (a crash, a native crash, an exit code).
  adb shell dumpsys activity exit-info "$package" >"$out/exit-info.txt" 2>&1 || true
  adb exec-out screencap -p >"$out/screen.png" 2>/dev/null || true
}
fail() {
  collect
  echo "::error title=Android device::$*" >&2
  echo "--- crash buffer" >&2
  head -60 "$out/crash.txt" >&2 || true
  echo "--- the app's fatal lines" >&2
  grep -E "FATAL EXCEPTION|AndroidRuntime|panicked at|Fatal signal|$package" "$out/logcat.txt" | head -60 >&2 || true
  exit 1
}
running() {
  [ -n "$(adb shell pidof "$package" 2>/dev/null | tr -d '\r')" ]
}
dump() {
  adb shell uiautomator dump /sdcard/ui.xml >/dev/null 2>&1 && adb pull /sdcard/ui.xml "$out/ui.xml" >/dev/null 2>&1
}
# Wait (at most $2 s, default 60) until the screen shows text matching the extended regex $1. FLAG_SECURE
# keeps screenshots black but not the accessibility tree, which the dump reads.
showing() {
  local deadline=$((SECONDS + ${2:-60}))
  until dump && grep -qE "$1" "$out/ui.xml"; do
    running || fail "the app closed while waiting for: $1"
    [ "$SECONDS" -lt "$deadline" ] || fail "the screen did not show $1 within ${2:-60} s"
    sleep 1
  done
}
# The centre of the node labelled with one of the words: the text or description equal to it,
# else containing it.
centre() {
  python3 - "$out/ui.xml" "$@" <<'PY'
import re, sys, xml.etree.ElementTree as ET
path, *words = sys.argv[1:]
nodes = []
for node in ET.parse(path).getroot().iter("node"):
    m = re.fullmatch(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", node.get("bounds", ""))
    if m:
        x1, y1, x2, y2 = map(int, m.groups())
        if x2 > x1 and y2 > y1:
            nodes.append(((node.get("text") or "").strip(), (node.get("content-desc") or "").strip(), (x1 + x2) // 2, (y1 + y2) // 2))
for exact in (True, False):
    for text, desc, x, y in nodes:
        if any((w in (text, desc)) if exact else (w in text or w in desc) for w in words):
            print(x, y)
            sys.exit(0)
sys.exit(1)
PY
}
tap() {
  local xy
  dump || fail "the screen could not be read"
  xy=$(centre "$@") || fail "nothing on the screen reads $*"
  # shellcheck disable=SC2086 # "x y"
  adb shell input tap $xy
}
# The webview's bottom edge on the screen, from the last dump.
webview_bottom() {
  python3 - "$out/ui.xml" <<'PY'
import re, sys, xml.etree.ElementTree as ET
for node in ET.parse(sys.argv[1]).getroot().iter("node"):
    m = re.fullmatch(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", node.get("bounds", ""))
    if node.get("class") == "android.webkit.WebView" and m:
        print(m.group(4))
        sys.exit(0)
sys.exit(1)
PY
}
# Wait (at most 20 s) until the webview ends above the keyboard ("up": MainActivity pads the
# content by the keyboard's height, edge to edge nothing else would) or at the screen's bottom
# again ("down").
keyboard() {
  local deadline=$((SECONDS + 20)) bottom=
  while :; do
    running || fail "the app closed while waiting for the keyboard to go $1"
    if dump && bottom=$(webview_bottom); then
      if [ "$1" = up ] && [ "$bottom" -lt "$full_bottom" ]; then return 0; fi
      if [ "$1" = down ] && [ "$bottom" -eq "$full_bottom" ]; then return 0; fi
    fi
    if [ "$SECONDS" -ge "$deadline" ]; then
      if [ "$1" = up ]; then fail "the keyboard opened over the page: the webview still ends at ${bottom:-?}"; fi
      fail "the keyboard did not close: the webview ends at ${bottom:-?}, not $full_bottom"
    fi
    sleep 1
  done
}
# Type the password into the field labelled with one of the words, then close the keyboard, so the
# next tap lands on the page and not on a key.
type_password() {
  tap "$@"
  keyboard up
  adb shell input text "$password"
  adb shell input keyevent KEYCODE_BACK
  keyboard down
}

adb wait-for-device
# A build signed with another key cannot replace the installed one.
adb uninstall "$package" >/dev/null 2>&1 || true
adb install -r -g "$apk" >"$out/install.txt" 2>&1 || fail "the APK did not install: $(tail -3 "$out/install.txt")"
adb logcat -c
adb shell am start -W -n "$package/.MainActivity" >"$out/start.txt" 2>&1 || fail "the activity did not start: $(tail -5 "$out/start.txt")"

# Up: the welcome screen (English on the emulator, Chinese on a phone set to it). At most 120 s:
# an emulator running arm64 code through its ARM translation is slow.
showing 'Create vault|创建保险库' 120
full_bottom=$(webview_bottom) || fail "no webview on the screen"

# A vault: the password typed twice, then the codes screen (Argon2id runs here: give it time).
type_password 'Master password' '主密码'
type_password 'Repeat it' '再输入一次'
tap 'Create vault' '创建保险库'
showing 'No accounts yet|还没有账号' 120

# Leaving the app locks the vault: Home, then back to it.
adb shell input keyevent KEYCODE_HOME
adb shell am start -n "$package/.MainActivity" >/dev/null 2>&1 || fail "the activity did not come back"
showing 'The vault is locked|保险库已锁定' 60

# The master password opens it again.
tap 'Master password' '主密码'
adb shell input text "$password"
adb shell input keyevent KEYCODE_ENTER
showing 'No accounts yet|还没有账号' 120

# And it stays up. The 20 s are the check itself (an app that closes a few seconds after its
# screen fails here), not a wait for something to finish.
for _ in $(seq 1 10); do
  sleep 2
  running || fail "the app closed after unlocking"
done
collect
# The app's own lines only: other processes on the device may log their own failures.
pid=$(adb shell pidof "$package" | tr -d '\r')
adb logcat -d --pid="$pid" >"$out/app-logcat.txt" 2>&1 || true
if grep -qE "FATAL EXCEPTION|panicked at|Fatal signal" "$out/app-logcat.txt" || grep -q "$package" "$out/crash.txt"; then
  fail "the app logged a fatal error although it is still running"
fi
echo "android-device-smoke: $package created a vault, locked on leaving and unlocked again ($(basename "$apk"))"
