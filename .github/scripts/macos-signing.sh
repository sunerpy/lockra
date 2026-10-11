#!/usr/bin/env bash
# The macOS release signing of one bundle leg (release.yml; docs/release.md, "macOS signing"): the
# project's self-signed "Lockra Code Signing" certificate, the same for every release, so each
# build can hand its keychain items to the next (docs/security.md, "Keychain items across updates").
#
#   import   decode MACOS_CERTIFICATE (base64 .p12) into a keychain of this job only, unlocked for
#            codesign, trust the certificate for code signing (passwordless sudo on GitHub's Macs),
#            and check that the identity is the one MACOS_SIGNING_IDENTITY and the pin
#            (.github/macos-signing.json) name. Tauri then signs with APPLE_SIGNING_IDENTITY.
#   check    the bundled app and the app in the updater's archive: signed, and with the designated
#            requirement the app trusts (identifier and certificate from the pin).
#   remove   delete that keychain again, and with it the private key. The trust setting stays:
#            removing it waits for an authorisation dialog nobody answers on a runner (Voltip,
#            2026-09-29), and GitHub discards the runner with the job.
#
# Usage: .github/scripts/macos-signing.sh import
#        .github/scripts/macos-signing.sh check <bundle dir>   (target/<triple>/release/bundle)
#        .github/scripts/macos-signing.sh remove
# Nothing secret is printed: the certificate and its password travel in the environment only.
set -euo pipefail
cd "$(dirname "$0")/../.."

pin=.github/macos-signing.json
sha1=$(jq -r .certificate_sha1 "$pin")
name=$(jq -r .certificate "$pin")
identifier=$(jq -r .identifier "$pin")
keychain="${RUNNER_TEMP:?RUNNER_TEMP}/lockra-signing.keychain-db"
requirement="identifier \"$identifier\" and certificate leaf = H\"$(tr '[:upper:]' '[:lower:]' <<<"$sha1")\""

check_app() {
	local app=$1
	codesign --verify --deep --strict --verbose=2 "$app" 2>&1 | tail -n 2
	local designated
	designated=$(codesign --display --requirements - "$app" 2>&1 | sed -n 's/^designated => //p')
	if [ "$(tr '[:upper:]' '[:lower:]' <<<"$designated")" != "$(tr '[:upper:]' '[:lower:]' <<<"$requirement")" ]; then
		echo "::error::$app is signed as '$designated', not '$requirement' (the release certificate in $pin)" >&2
		exit 1
	fi
	echo "macos-signing: $app: $designated"
}

case "${1:-}" in
import)
	: "${MACOS_CERTIFICATE:?MACOS_CERTIFICATE (base64 .p12) is not set}"
	: "${MACOS_CERTIFICATE_PASSWORD:?MACOS_CERTIFICATE_PASSWORD is not set}"
	: "${MACOS_SIGNING_IDENTITY:?MACOS_SIGNING_IDENTITY (certificate SHA-1) is not set}"
	if [ "$(tr '[:lower:]' '[:upper:]' <<<"$MACOS_SIGNING_IDENTITY")" != "$(tr '[:lower:]' '[:upper:]' <<<"$sha1")" ]; then
		echo "::error::MACOS_SIGNING_IDENTITY is not the certificate $pin pins ($sha1)" >&2
		exit 1
	fi
	umask 077
	p12="$RUNNER_TEMP/lockra-signing.p12"
	certificate="$RUNNER_TEMP/lockra-signing.pem"
	trap 'rm -f "$p12" "$certificate"' EXIT
	printf '%s' "$MACOS_CERTIFICATE" | base64 --decode >"$p12"
	password=$(openssl rand -base64 24)
	security create-keychain -p "$password" "$keychain"
	security set-keychain-settings -lut 21600 "$keychain"
	security unlock-keychain -p "$password" "$keychain"
	security import "$p12" -k "$keychain" -f pkcs12 -P "$MACOS_CERTIFICATE_PASSWORD" -T /usr/bin/codesign >/dev/null
	security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null
	# Ours first in the search list, the runner's own after it.
	existing=()
	while IFS= read -r line; do
		line=${line//\"/}
		line=${line## }
		line=${line%% }
		[ -n "$line" ] && existing+=("$line")
	done < <(security list-keychains -d user)
	security list-keychains -d user -s "$keychain" "${existing[@]}"
	# A self-signed certificate is its own root: trusted for code signing on this runner only.
	security find-certificate -c "$name" -p "$keychain" >"$certificate"
	sudo security add-trusted-cert -d -r trustRoot -p codeSign -k /Library/Keychains/System.keychain "$certificate"
	identities=$(security find-identity -v -p codesigning "$keychain")
	grep -qi "$sha1" <<<"$identities" || {
		echo "::error::the imported identity is not $sha1" >&2
		exit 1
	}
	echo "macos-signing: $name ($sha1) ready in $keychain"
	;;
check)
	bundle=${2:?bundle directory}
	apps=("$bundle"/macos/*.app)
	[ "${#apps[@]}" -eq 1 ] && [ -d "${apps[0]}" ] || {
		echo "::error::expected one app in $bundle/macos" >&2
		exit 1
	}
	check_app "${apps[0]}"
	archives=("$bundle"/macos/*.app.tar.gz)
	if [ -f "${archives[0]}" ]; then
		staged=$(mktemp -d)
		tar -xzf "${archives[0]}" -C "$staged"
		check_app "$staged/$(basename "${apps[0]}")"
		rm -rf "$staged"
	fi
	;;
remove)
	security delete-keychain "$keychain" 2>/dev/null || true
	echo "macos-signing: removed"
	;;
*)
	echo "macos-signing: import | check <bundle dir> | remove" >&2
	exit 2
	;;
esac
