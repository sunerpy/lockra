#!/usr/bin/env bash
# Browser smoke test on Linux (plan §八): the development build of the extension in Chrome for
# Testing and in Firefox, paired with the real app (a debug build serving the release web bundle,
# with the in-memory keychain), under Xvfb, against the test sites of scripts/smoke/site/ (HTTPS
# with a certificate this run makes, and plain HTTP; every *.test name resolves to 127.0.0.1).
# scripts/smoke/browser.py drives the app over tauri-driver and each browser over its own WebDriver
# (chromedriver, geckodriver), with real X input where only a real key reaches the browser (the
# extension's shortcut). The browsers and the drivers are pinned below and checked by SHA-256,
# downloaded once into $LOCKRA_SMOKE_CACHE (~/.cache/lockra-smoke).
# Usage: scripts/smoke-browser-linux.sh [chrome|firefox|all] [out-dir]
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)
which=${1:-all}
out=${2:-$root/target/smoke-browser}
cache=${LOCKRA_SMOKE_CACHE:-$HOME/.cache/lockra-smoke}
case "$which" in chrome | firefox | all) ;; *) echo "smoke: $which is not chrome, firefox or all"; exit 2 ;; esac
for tool in Xvfb xdpyinfo xdotool tauri-driver WebKitWebDriver python3 cargo pnpm curl openssl sha256sum tar; do
  command -v "$tool" >/dev/null || { echo "smoke: $tool not installed"; exit 2; }
done

# Chrome for Testing (Chrome itself ignores --load-extension since 137) and its chromedriver;
# Firefox and geckodriver. Hashes of the files as published (Firefox's match Mozilla's SHA256SUMS).
chrome_version=155.0.8059.79
chrome_sha256=bcd9a79130d7e44d70f93be87042e9bcee7fe0f0b3898e06b8c9bed5fb619dcd
chromedriver_sha256=876f0ea4095237bb3e93321d95758d38c9195989935d7bdc9f51d8e26bc6a3a1
firefox_version=158.0
firefox_sha256=9549b054ec4cf85f50f6a19d8c6fb79580312e71ebc4ec2cb217ec9af09dc12a
geckodriver_version=0.37.1
geckodriver_sha256=e815130ea95983e162ae91843b48d3a3ce991735635fce83a647afde21e09f7e
cft=https://storage.googleapis.com/chrome-for-testing-public/$chrome_version/linux64

# fetch URL FILE SHA256: the file in the cache, downloaded if it is missing, checked every time.
fetch() {
  local file="$cache/downloads/$2"
  mkdir -p "$cache/downloads"
  if [ ! -s "$file" ]; then
    echo "smoke: downloading $1"
    curl -sSfL --retry 3 -o "$file.part" "$1"
    mv "$file.part" "$file"
  fi
  if ! echo "$3  $file" | sha256sum --check --quiet; then
    rm -f "$file"
    echo "smoke: $2 does not have the pinned SHA-256"
    exit 1
  fi
}

# unzip FILE DIR, keeping the files' modes (zipfile alone drops the executable bits).
unzip_to() {
  python3 - "$1" "$2" <<'PY'
import os, sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    for info in archive.infolist():
        path = archive.extract(info, sys.argv[2])
        mode = info.external_attr >> 16
        if mode:
            os.chmod(path, mode & 0o777)
PY
}

chrome="" chromedriver="" firefox="" geckodriver=""
if [ "$which" != firefox ]; then
  fetch "$cft/chrome-linux64.zip" "chrome-linux64-$chrome_version.zip" "$chrome_sha256"
  fetch "$cft/chromedriver-linux64.zip" "chromedriver-linux64-$chrome_version.zip" "$chromedriver_sha256"
  dir="$cache/chrome-$chrome_version"
  [ -x "$dir/chrome-linux64/chrome" ] || unzip_to "$cache/downloads/chrome-linux64-$chrome_version.zip" "$dir"
  [ -x "$dir/chromedriver-linux64/chromedriver" ] || unzip_to "$cache/downloads/chromedriver-linux64-$chrome_version.zip" "$dir"
  chrome="$dir/chrome-linux64/chrome"
  chromedriver="$dir/chromedriver-linux64/chromedriver"
fi
if [ "$which" != chrome ]; then
  fetch "https://ftp.mozilla.org/pub/firefox/releases/$firefox_version/linux-x86_64/en-US/firefox-$firefox_version.tar.xz" \
    "firefox-$firefox_version.tar.xz" "$firefox_sha256"
  fetch "https://github.com/mozilla/geckodriver/releases/download/v$geckodriver_version/geckodriver-v$geckodriver_version-linux64.tar.gz" \
    "geckodriver-v$geckodriver_version-linux64.tar.gz" "$geckodriver_sha256"
  dir="$cache/firefox-$firefox_version"
  [ -x "$dir/firefox/firefox" ] || { mkdir -p "$dir" && tar -xJf "$cache/downloads/firefox-$firefox_version.tar.xz" -C "$dir"; }
  gdir="$cache/geckodriver-$geckodriver_version"
  [ -x "$gdir/geckodriver" ] || { mkdir -p "$gdir" && tar -xzf "$cache/downloads/geckodriver-v$geckodriver_version-linux64.tar.gz" -C "$gdir"; }
  firefox="$dir/firefox/firefox"
  geckodriver="$gdir/geckodriver"
fi

echo "smoke: building the extension, the web bundle and the app"
# The extension first: the app carries its Chromium build (build.rs), the folder Chrome loads here.
pnpm --filter @lockra/extension run build >/dev/null
pnpm --filter @lockra/desktop build >/dev/null
# custom-protocol: the binary serves the bundle it embeds instead of the Vite dev server.
LOCKRA_EXTENSION_REQUIRED=1 cargo build -q -p lockra-desktop --features tauri/custom-protocol
app="$root/target/debug/lockra-desktop"

display=""
for n in $(seq 160 199); do
  if [ ! -e "/tmp/.X11-unix/X$n" ] && [ ! -e "/tmp/.X$n-lock" ]; then display=":$n"; break; fi
done
[ -n "$display" ] || { echo "smoke: no free X display"; exit 2; }
free_port() { python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'; }
driver_port=$(free_port)
native_port=$(free_port)
https_port=$(free_port)
http_port=$(free_port)
webdriver_port=$(free_port)

work=$(mktemp -d)
pids=()
cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    echo "smoke: processes at the failure:"
    pgrep -a -f 'tauri-driver|WebKitWebDriver|lockra-desktop|chromedriver|geckodriver|chrome-linux64|firefox/firefox' || echo "  none"
    for log in "$work"/*.log; do
      if [ -s "$log" ]; then
        echo "smoke: the end of $(basename "$log"):"
        tail -n 40 "$log"
      fi
    done
  fi
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  rm -rf "$work"
}
trap cleanup EXIT

Xvfb "$display" -screen 0 1440x900x24 -nolisten tcp >/dev/null 2>&1 &
pids+=($!)
timeout 20 sh -c "until DISPLAY=$display xdpyinfo >/dev/null 2>&1; do sleep 0.5; done"

# One certificate for every test host, signed by a CA of this run's own.
openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj "/CN=Lockra smoke CA" \
  -keyout "$work/ca.key" -out "$work/ca.pem" 2>/dev/null
openssl req -newkey rsa:2048 -nodes -subj "/CN=login.test" -keyout "$work/site.key" -out "$work/site.csr" 2>/dev/null
printf 'subjectAltName=DNS:login.test,DNS:shop.test,DNS:pay.test,DNS:signup.test,DNS:otp.test,DNS:many.test\n' >"$work/san.cnf"
openssl x509 -req -in "$work/site.csr" -CA "$work/ca.pem" -CAkey "$work/ca.key" -CAcreateserial -days 2 \
  -extfile "$work/san.cnf" -out "$work/site.pem" 2>/dev/null
python3 scripts/smoke/site.py scripts/smoke/site "$work/site.pem" "$work/site.key" "$https_port" "$http_port" >"$work/site.log" 2>&1 &
pids+=($!)
timeout 20 sh -c "until grep -q 'site: ready' '$work/site.log'; do sleep 0.2; done"

# The app and the browsers share one home and runtime folder: the browsers start the host (the
# app's own binary) with their environment, and the host finds the app's socket under
# XDG_RUNTIME_DIR; Firefox reads its manifest under HOME. Chrome's manifest goes to its test
# profile (LOCKRA_DEV_BROWSER_MANIFEST_DIR, a debug build's), never a real profile.
mkdir -p "$work/home" "$work/data" "$work/config/dev.lockra.desktop" "$work/chrome-profile" "$out"
mkdir -m 700 "$work/run"
printf '{"locale":"zh-cn"}\n' >"$work/config/dev.lockra.desktop/settings.json"
export DISPLAY="$display" HOME="$work/home" XDG_RUNTIME_DIR="$work/run" XDG_DATA_HOME="$work/data" \
  XDG_CONFIG_HOME="$work/config" LOCKRA_DEV_SECRET_STORE=memory LOCKRA_DEV_BROWSER_MANIFEST_DIR="$work/chrome-profile"

tauri-driver --port "$driver_port" --native-port "$native_port" >"$work/driver.log" 2>&1 &
pids+=($!)
if ! timeout 30 sh -c "until curl -sf http://127.0.0.1:$driver_port/status >/dev/null; do sleep 0.5; done"; then
  echo "smoke: tauri-driver did not start"; exit 1
fi

# Chrome's sandbox needs user namespaces it may map itself into; where the host forbids that (an
# AppArmor userns restriction, a container), Chrome cannot start with it and runs without.
sandbox=()
if ! unshare -Ur true 2>/dev/null; then
  echo "smoke: no user namespaces for Chrome's sandbox here: Chrome runs with --no-sandbox"
  sandbox=(--no-sandbox)
fi

run_browser() {
  local name=$1 driver=$2 binary=$3
  local flags=()
  # geckodriver: the chrome context, which opens the extension's own pages (browser.py).
  [ "$name" = firefox ] && flags=(--allow-system-access)
  "$driver" --port="$webdriver_port" "${flags[@]}" >"$work/$name-driver.log" 2>&1 &
  local driver_pid=$!
  pids+=("$driver_pid")
  if ! timeout 30 sh -c "until curl -sf http://127.0.0.1:$webdriver_port/status >/dev/null; do sleep 0.5; done"; then
    echo "smoke: $name's WebDriver did not start"; exit 1
  fi
  python3 scripts/smoke/browser.py --browser "$name" --binary "$binary" \
    --webdriver "http://127.0.0.1:$webdriver_port" --driver "http://127.0.0.1:$driver_port" --app "$app" \
    --extension "$root/apps/extension/dist" --work "$work" --out "$out" \
    --https-port "$https_port" --http-port "$http_port" "${sandbox[@]}"
  kill "$driver_pid" 2>/dev/null || true
  wait "$driver_pid" 2>/dev/null || true
}

[ "$which" = firefox ] || run_browser chrome "$chromedriver" "$chrome"
[ "$which" = chrome ] || run_browser firefox "$geckodriver" "$firefox"
echo "smoke: passed"
