#!/usr/bin/env bash
# The Android release key's pin, .github/android-signing.json (docs/release.md, "Android"): the
# SHA-256 of the certificate every release's APK and AAB must carry (check-android-package.sh), and
# the alias that names the key inside the keystore, which is not a secret. Prints
# `<certificate> <alias>` when the file is well formed, and fails otherwise.
#
# Usage: android-signing.sh [file]   (default .github/android-signing.json)
set -euo pipefail

file=${1:-.github/android-signing.json}
fail() {
  echo "::error title=Android signing pin::$file: $*" >&2
  exit 1
}
[ -f "$file" ] || fail "missing"
jq -e 'type == "object" and keys == ["certificate_sha256", "key_alias"]' "$file" >/dev/null ||
  fail "must hold certificate_sha256 and key_alias, nothing else"
certificate=$(jq -r .certificate_sha256 "$file")
alias=$(jq -r .key_alias "$file")
[[ $certificate =~ ^[0-9a-f]{64}$ ]] || fail "certificate_sha256 must be 64 lowercase hex digits"
[[ $alias =~ ^[a-z0-9][a-z0-9._-]*$ ]] || fail "key_alias must be a plain name"
echo "$certificate $alias"
