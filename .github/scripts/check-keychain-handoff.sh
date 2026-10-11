#!/usr/bin/env bash
# The keychain hand-over of a macOS update, on this Mac's login keychain (CI `macos-check`;
# docs/security.md, "Keychain items across updates").
#
# A keychain made with `security create-keychain` skips the partition check that makes macOS ask
# when one build reads an item another build created, so this runs on the login keychain, where
# it applies as on a user's Mac. Three builds of examples/keychain_harness.rs, whose code (and so
# cdhash and partition) differ: "one" and "two" signed with a certificate made for this run (the
# release certificate's stand-in), "three" with another one. The harness never shows the dialog:
# a read that would need it fails.
#
#   1. "two" cannot read "one"'s item without asking (the partition, as on a real Mac);
#   2. "one" hands its entry to "two" before the update would install; "two" stores it in an item
#      of its own, and the installed "two" reads it without asking and removes "one"'s copy;
#   3. a build signed with another certificate is handed nothing, and gives nothing: "two"
#      refuses to hand over to "three", and "one" refuses "three" as the parent of a hand-over.
#
# Usage: .github/scripts/check-keychain-handoff.sh (macOS, with Homebrew's openssl@3, as on GitHub's Macs)
set -euo pipefail
cd "$(dirname "$0")/../.."

if [ "$(uname -s)" != Darwin ]; then
	echo "check-keychain-handoff: macOS only" >&2
	exit 2
fi

# Homebrew's OpenSSL 3: the system's LibreSSL is not what the release certificate was made with.
openssl=$(brew --prefix openssl@3)/bin/openssl
work=$(mktemp -d)
keychain="$work/ci-signing.keychain-db"
entry="ci-${GITHUB_RUN_ID:-local}-$$"
secret="device key $RANDOM$RANDOM"
existing=()
while IFS= read -r line; do
	line=${line//\"/}
	line=${line## }
	line=${line%% }
	[ -n "$line" ] && existing+=("$line")
done < <(security list-keychains -d user)

fail() {
	echo "::error::check-keychain-handoff: $*" >&2
	exit 1
}

cleanup() {
	if [ -x "$work/one" ]; then "$work/one" wipe "$entry" >/dev/null 2>&1 || true; fi
	security list-keychains -d user -s "${existing[@]}" 2>/dev/null || true
	security delete-keychain "$keychain" 2>/dev/null || true
	rm -rf "$work"
}
trap cleanup EXIT

# Two throwaway code signing certificates in a keychain of this run. Neither is trusted, as the
# release certificate is not on a user's Mac: code signed with them still checks against a
# requirement that names the certificate.
password=$("$openssl" rand -hex 16)
security create-keychain -p "$password" "$keychain"
security set-keychain-settings -lut 3600 "$keychain"
security unlock-keychain -p "$password" "$keychain"
security list-keychains -d user -s "${existing[@]}" "$keychain"

certificate() {
	local name=$1
	"$openssl" req -x509 -newkey rsa:2048 -sha256 -days 2 -nodes -subj "/CN=$name" \
		-addext "keyUsage=critical,digitalSignature" -addext "extendedKeyUsage=critical,codeSigning" \
		-addext "basicConstraints=critical,CA:false" -keyout "$work/$name.key" -out "$work/$name.pem" 2>/dev/null
	"$openssl" pkcs12 -export -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1 -inkey "$work/$name.key" \
		-in "$work/$name.pem" -name "$name" -passout pass:ci -out "$work/$name.p12"
	security import "$work/$name.p12" -k "$keychain" -f pkcs12 -P ci -T /usr/bin/codesign >/dev/null
}
certificate "Lockra CI Release"
certificate "Lockra CI Other"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null
identity() { security find-identity -p codesigning "$keychain" | awk -v name="$1" 'index($0, name) {print $2; exit}'; }
release=$(identity "Lockra CI Release")
other=$(identity "Lockra CI Other")
[ -n "$release" ] && [ -n "$other" ] || fail "the run's signing identities were not found"
requirement="identifier \"dev.lockra.desktop\" and certificate leaf = H\"$(tr '[:upper:]' '[:lower:]' <<<"$release")\""

# Three builds whose code differs by the word each was built with.
build() {
	local name=$1 identity=$2
	LOCKRA_HARNESS_REQUIREMENT="$requirement" LOCKRA_HARNESS_BUILD="$name" \
		cargo build --locked --quiet -p lockra-desktop --example keychain_harness
	cp target/debug/examples/keychain_harness "$work/$name"
	codesign --force --sign "$identity" --identifier dev.lockra.desktop "$work/$name"
}
build one "$release"
build two "$release"
build three "$other"
cdhash() { codesign -dvvv "$1" 2>&1 | sed -n 's/^CDHash=//p'; }
[ "$(cdhash "$work/one")" != "$(cdhash "$work/two")" ] || fail "the builds share a cdhash; the check would prove nothing"

expect() {
	local want=$1
	shift
	local got
	got=$("$@") || fail "$* failed"
	[ "$got" = "$want" ] || fail "$*: got '$got', want '$want'"
}

one="$work/one"
two="$work/two"
three="$work/three"
expect stored "$one" put "$entry" "$secret"
account_one=$("$one" account)
account_two=$("$two" account)
expect "found $secret" "$one" peek "$entry" "$account_one"

# 1. The partition: another build's item needs the dialog.
expect would-ask "$two" peek "$entry" "$account_one"

# 2. The hand-over before installation, then the installed build.
expect "handed 1" "$one" handoff "$two"
expect "found $secret" "$two" peek "$entry" "$account_two"
expect "found $secret" "$one" peek "$entry" "$account_one"
expect "found $secret" "$two" get "$entry"
expect missing "$one" peek "$entry" "$account_one"

# 3. Another certificate: nothing handed to it, nothing taken from it.
if "$two" handoff "$three" >/dev/null 2>&1; then fail "a build signed with another certificate was handed the entries"; fi
if "$three" force-handoff "$one" >/dev/null 2>&1; then fail "a parent signed with another certificate handed over"; fi
expect missing "$one" peek forced "$account_one"

echo "check-keychain-handoff: no dialog across the update, and nothing to or from another certificate"
