#!/usr/bin/env bash
# Refresh the third-party data compiled into crates/lockra-autofill (Apple's
# password-manager-resources lists and the EFF's large wordlist) from the sources that
# crates/lockra-autofill/data/SOURCES pins: every file is downloaded from its URL (at an upstream
# commit, never a branch) and checked against the SHA-256 written there. One mismatch and nothing
# is written. This is the only way those files change (AGENTS.md). To move to a newer upstream
# commit: put it in the URLs, run this, read the upstream change between the two commits, pin the
# SHA-256 values it printed, and run it again; then `cargo test -p lockra-autofill`.
# Usage: scripts/update-vendored-data.sh
set -euo pipefail
cd "$(dirname "$0")/.."
data=crates/lockra-autofill/data

command -v curl >/dev/null || { echo "update-vendored-data: curl is not installed" >&2; exit 2; }
if command -v sha256sum >/dev/null; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null; then
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  echo "update-vendored-data: neither sha256sum nor shasum is installed" >&2
  exit 2
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
files=()
status=0
while read -r file pin url extra; do
  if [ -z "$url" ] || [ -n "${extra:-}" ] || [[ $file == /* || $file == *..* ]] || ! [[ $pin =~ ^[0-9a-f]{64}$ ]] || [[ $url != https://* ]]; then
    echo "update-vendored-data: $data/SOURCES: not a path, a SHA-256 and an https URL: $file $pin $url ${extra:-}" >&2
    exit 2
  fi
  download=$work/${#files[@]}
  files+=("$file")
  # GitHub is slow from some networks (tens of KB/s): a generous deadline, a few retries.
  curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 --retry 3 --max-time 600 --output "$download" "$url"
  actual=$(sha256 "$download")
  if [ "$actual" != "$pin" ]; then
    echo "update-vendored-data: $file: SHA-256 $actual, but $data/SOURCES pins $pin"
    status=1
  fi
done < <(grep -vE '^[[:space:]]*(#|$)' "$data/SOURCES")

if [ "${#files[@]}" -eq 0 ]; then
  echo "update-vendored-data: $data/SOURCES names no file" >&2
  exit 2
fi
if [ "$status" -ne 0 ]; then
  echo "update-vendored-data: nothing written; pin the new SHA-256 only after reading the upstream change"
  exit 1
fi
for index in "${!files[@]}"; do
  target=$data/${files[$index]}
  if [ -f "$target" ] && cmp -s "$work/$index" "$target"; then
    echo "update-vendored-data: ${files[$index]} unchanged"
  else
    mkdir -p "$(dirname "$target")"
    mv "$work/$index" "$target"
    echo "update-vendored-data: ${files[$index]} updated"
  fi
done
