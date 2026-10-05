#!/usr/bin/env bash
# The keychain hand-over of a macOS in-app update end to end, with the real app, on this Mac's
# login keychain (CI `macos-keychain`; docs/security.md, "Keychain items across updates").
#
# check-keychain-handoff.sh proves the protocol between two harness builds. This one runs it the
# way an update does: the release app is a bundle signed with the hardened runtime, the updater
# hands its verified Lockra.app.tar.gz to keychain_handoff::prepare_update, and the installed copy
# is the same package expanded elsewhere. Both builds here are debug builds compiled with
# LOCKRA_DEV_RELEASE_REQUIREMENT, the requirement of a certificate made for this run (a debug
# build's stand-in for the release certificate, which never leaves the release workflow):
#
#   A  the running build: examples/keychain_harness.rs, whose `release …` commands are the app's
#      store and its updater's hand-over, in a bundle as the app is;
#   B  the new build: the app itself, built and bundled the same way.
#
#   1. A stores an entry, then prepares the update from B's package: the staged B takes the entry
#      into an item of its own before anything would install, and A's item stays;
#   2. the installed B reads the entry without asking (its debug-only `--lockra-keychain-probe`)
#      and removes A's copy;
#   3. a package signed otherwise (ad hoc) is refused before it is handed anything: no item is
#      made for it, and A's item stays.
#
# Usage: .github/scripts/check-keychain-preinstall.sh (macOS, with Homebrew's openssl@3)
set -euo pipefail
cd "$(dirname "$0")/../.."

if [ "$(uname -s)" != Darwin ]; then
	echo "check-keychain-preinstall: macOS only" >&2
	exit 2
fi

openssl=$(brew --prefix openssl@3)/bin/openssl
work=$(mktemp -d)
keychain="$work/ci-signing.keychain-db"
entry=$(uuidgen | tr '[:upper:]' '[:lower:]')
refused=$(uuidgen | tr '[:upper:]' '[:lower:]')
secret="device key $RANDOM$RANDOM$RANDOM"
existing=()
while IFS= read -r line; do
	line=${line//\"/}
	line=${line## }
	line=${line%% }
	[ -n "$line" ] && existing+=("$line")
done < <(security list-keychains -d user)

fail() {
	echo "::error::check-keychain-preinstall: $*" >&2
	exit 1
}

old="$work/old/Lockra.app/Contents/MacOS/lockra-desktop"
cleanup() {
	if [ -x "$old" ]; then
		"$old" release wipe "$entry" >/dev/null 2>&1 || true
		"$old" release wipe "$refused" >/dev/null 2>&1 || true
	fi
	security list-keychains -d user -s "${existing[@]}" 2>/dev/null || true
	security delete-keychain "$keychain" 2>/dev/null || true
	rm -rf "$work"
}
trap cleanup EXIT

# The run's code signing certificate, not trusted, as the release certificate is not on a
# user's Mac.
password=$("$openssl" rand -hex 16)
security create-keychain -p "$password" "$keychain"
security set-keychain-settings -lut 3600 "$keychain"
security unlock-keychain -p "$password" "$keychain"
security list-keychains -d user -s "${existing[@]}" "$keychain"
"$openssl" req -x509 -newkey rsa:2048 -sha256 -days 2 -nodes -subj "/CN=Lockra CI Release" \
	-addext "keyUsage=critical,digitalSignature" -addext "extendedKeyUsage=critical,codeSigning" \
	-addext "basicConstraints=critical,CA:false" -keyout "$work/release.key" -out "$work/release.pem" 2>/dev/null
"$openssl" pkcs12 -export -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1 -inkey "$work/release.key" \
	-in "$work/release.pem" -name "Lockra CI Release" -passout pass:ci -out "$work/release.p12"
security import "$work/release.p12" -k "$keychain" -f pkcs12 -P ci -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null
identity=$(security find-identity -p codesigning "$keychain" | awk '/Lockra CI Release/ {print $2; exit}')
[ -n "$identity" ] || fail "the run's signing identity was not found"
requirement="identifier \"dev.lockra.desktop\" and certificate leaf = H\"$(tr '[:upper:]' '[:lower:]' <<<"$identity")\""

LOCKRA_DEV_RELEASE_REQUIREMENT="$requirement" \
	cargo build --locked --quiet -p lockra-desktop --bin lockra-desktop --example keychain_harness

# A bundle as the release has one: the executable, its Info.plist, signed with the hardened
# runtime under the bundle's identifier.
bundle() {
	local executable=$1 app=$2 version=$3
	mkdir -p "$app/Contents/MacOS"
	cp "$executable" "$app/Contents/MacOS/lockra-desktop"
	cat >"$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>lockra-desktop</string>
<key>CFBundleIdentifier</key><string>dev.lockra.desktop</string>
<key>CFBundleName</key><string>Lockra</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
</dict></plist>
PLIST
	codesign --force --options runtime --sign "$identity" "$app"
	local designated
	designated=$(codesign --display --requirements - "$app" 2>&1 | sed -n 's/^designated => //p')
	[ "$designated" = "$requirement" ] || fail "$app is signed as '$designated', not '$requirement'"
}
bundle target/debug/examples/keychain_harness "$work/old/Lockra.app" 9.9.8
bundle target/debug/lockra-desktop "$work/new/Lockra.app" 9.9.9
cdhash() { codesign -dvvv "$1" 2>&1 | sed -n 's/^CDHash=//p'; }
[ "$(cdhash "$work/old/Lockra.app")" != "$(cdhash "$work/new/Lockra.app")" ] || fail "the builds share a cdhash; the check would prove nothing"
# The updater's package: the bundle at the top, nothing beside it (no AppleDouble files).
(cd "$work/new" && COPYFILE_DISABLE=1 tar -czf "$work/Lockra.app.tar.gz" Lockra.app)

expect() {
	local want=$1
	shift
	local got
	got=$("$@") || fail "$* failed"
	[ "$got" = "$want" ] || fail "$*: got '$got', want '$want'"
}
digest=$(printf '%s' "$secret" | shasum -a 256 | cut -c1-16)

# 1. Before installation: the staged B stores the entry in an item of its own; A's stays.
expect stored "$old" release put "$entry" "$secret"
account_old=$("$old" release account)
expect "handed 1" "$old" release preinstall "$work/Lockra.app.tar.gz"
accounts=$("$old" release accounts "$entry")
if [ "$(grep -c . <<<"$accounts")" != 2 ] || ! grep -qx "$account_old" <<<"$accounts"; then
	fail "after the hand-over the entry is held by '$accounts', not by A and the staged B"
fi
expect found "$old" release peek "$entry" "$account_old"

# 2. The installed B, the same package expanded elsewhere, reads it without asking and removes
#    A's copy.
mkdir -p "$work/Applications"
tar -xzf "$work/Lockra.app.tar.gz" -C "$work/Applications"
expect "found $digest" "$work/Applications/Lockra.app/Contents/MacOS/lockra-desktop" --lockra-keychain-probe "$entry"
accounts=$("$old" release accounts "$entry")
if [ "$(grep -c . <<<"$accounts")" != 1 ] || grep -qx "$account_old" <<<"$accounts"; then
	fail "after the installed build read the entry it is held by '$accounts', not by B alone"
fi

# 3. A package signed otherwise is handed nothing.
mkdir -p "$work/adhoc"
cp -R "$work/new/Lockra.app" "$work/adhoc/Lockra.app"
codesign --force --options runtime --sign - "$work/adhoc/Lockra.app"
(cd "$work/adhoc" && COPYFILE_DISABLE=1 tar -czf "$work/adhoc.app.tar.gz" Lockra.app)
expect stored "$old" release put "$refused" "$secret"
if "$old" release preinstall "$work/adhoc.app.tar.gz" >/dev/null 2>&1; then
	fail "an ad hoc package was handed the entries"
fi
expect "$account_old" "$old" release accounts "$refused"

echo "check-keychain-preinstall: the installed app reads its entry without asking, and nothing goes to a package signed otherwise"
