#!/bin/sh
# One-line install of Lockra on Linux (x64 and ARM64) and macOS (Apple silicon and Intel) from a
# GitHub release (Windows: scripts/install.ps1):
#
#   curl -fsSL https://raw.githubusercontent.com/sunerpy/lockra/main/scripts/install.sh | sh
#
# It picks the package for this computer: the .deb where apt is, the .rpm where dnf, zypper or yum
# is, the AppImage elsewhere, and on a Mac the dmg for its processor (also from a Rosetta shell).
# It downloads the package and SHA256SUMS from the same release and installs nothing unless the
# package's SHA-256 matches its line there. Run it again to update; from Lockra 0.2.0 the app also
# updates itself (Settings › About), through the package it was installed from.
#
#   LOCKRA_VERSION=0.2.0        a given release instead of the latest
#   LOCKRA_PACKAGE=appimage     Linux: deb, rpm or appimage instead of the one picked
#   LOCKRA_INSTALL_DIR=DIR      the AppImage's directory (default ~/.local/bin), or the Mac app's
#                               (default /Applications, ~/Applications when that is not writable)
set -eu

REPO="sunerpy/lockra"
CHECKSUM_FILE="SHA256SUMS"

err() {
	printf 'lockra-install: %s\n' "$1" >&2
	exit 1
}

info() {
	printf 'lockra-install: %s\n' "$1" >&2
}

if command -v curl >/dev/null 2>&1; then
	download() { curl -fsSL --retry 3 "$1" -o "$2"; }
	fetch() { curl -fsSL --retry 3 "$1"; }
elif command -v wget >/dev/null 2>&1; then
	download() { wget -qO "$2" "$1"; }
	fetch() { wget -qO - "$1"; }
else
	err "curl or wget is required"
fi

# The package for this computer.
case "$(uname -s)" in
Linux)
	case "$(uname -m)" in
	x86_64 | amd64) deb_arch=amd64 rpm_arch=x86_64 appimage_arch=amd64 ;;
	aarch64 | arm64) deb_arch=arm64 rpm_arch=aarch64 appimage_arch=aarch64 ;;
	*) err "Lockra ships for x64 and ARM64 Linux only (this is $(uname -m))" ;;
	esac
	package=${LOCKRA_PACKAGE:-}
	if [ -z "$package" ]; then
		if command -v apt-get >/dev/null 2>&1; then
			package=deb
		elif command -v dnf >/dev/null 2>&1 || command -v zypper >/dev/null 2>&1 || command -v yum >/dev/null 2>&1; then
			package=rpm
		else
			package=appimage
		fi
	fi
	case "$package" in
	deb | rpm | appimage) ;;
	*) err "LOCKRA_PACKAGE must be deb, rpm or appimage, not '$package'" ;;
	esac
	;;
Darwin)
	# A shell under Rosetta reports x86_64 on Apple silicon; the native build is the one to take.
	if [ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" = 1 ]; then
		mac_arch=aarch64
	else
		case "$(uname -m)" in
		x86_64) mac_arch=x64 ;;
		*) err "unsupported Mac processor: $(uname -m)" ;;
		esac
	fi
	package=dmg
	;;
*) err "unsupported system: $(uname -s) (on Windows, use scripts/install.ps1)" ;;
esac

if [ -n "${LOCKRA_VERSION:-}" ]; then
	version=$(printf '%s' "$LOCKRA_VERSION" | sed 's/^v//')
else
	# The latest release's SHA256SUMS through GitHub's latest-release redirect, and the version
	# from the package names in it: no API call, so no rate limit (an address many computers share
	# runs out of anonymous API calls within the hour).
	info "finding the latest release"
	latest=$(fetch "https://github.com/${REPO}/releases/latest/download/${CHECKSUM_FILE}") ||
		err "could not find the latest release"
	version=$(printf '%s\n' "$latest" |
		sed -n 's/^[0-9a-fA-F]\{64\}[[:space:]][[:space:]]*\*\{0,1\}Lockra_\([0-9][^_]*\)_.*/\1/p' | head -1)
	[ -n "$version" ] || err "could not find the latest release"
fi
# The version goes into URLs and file names: digits and dots, and an optional pre-release tail.
printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$' ||
	err "not a release version: '$version'"

case "$package" in
deb) asset="Lockra_${version}_${deb_arch}.deb" ;;
rpm) asset="Lockra-${version}-1.${rpm_arch}.rpm" ;;
appimage) asset="Lockra_${version}_${appimage_arch}.AppImage" ;;
dmg) asset="Lockra_${version}_${mac_arch}.dmg" ;;
esac
base_url="https://github.com/${REPO}/releases/download/v${version}"

tmp=$(mktemp -d 2>/dev/null || mktemp -d -t lockra)
cleanup() {
	if [ -n "${mounted:-}" ]; then hdiutil detach "$mounted" -quiet >/dev/null 2>&1 || true; fi
	rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

info "downloading ${asset} (Lockra ${version})"
download "${base_url}/${CHECKSUM_FILE}" "$tmp/$CHECKSUM_FILE" ||
	err "release v${version} has no ${CHECKSUM_FILE} (is ${version} a Lockra release?)"
expected=$(awk -v name="$asset" '{
	file = $2
	sub(/^\*/, "", file)
	if (file == name) { print $1; exit }
}' "$tmp/$CHECKSUM_FILE")
[ -n "$expected" ] || err "release v${version} has no ${asset}"
download "${base_url}/${asset}" "$tmp/$asset" || err "could not download ${asset}"

if command -v sha256sum >/dev/null 2>&1; then
	actual=$(sha256sum "$tmp/$asset" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
	actual=$(shasum -a 256 "$tmp/$asset" | awk '{print $1}')
else
	err "sha256sum or shasum is required"
fi
[ "$actual" = "$expected" ] || err "checksum mismatch for ${asset}: nothing was installed"
info "SHA-256 matches ${CHECKSUM_FILE}"

as_root() {
	if [ "$(id -u)" = 0 ]; then
		"$@"
	elif command -v sudo >/dev/null 2>&1; then
		sudo "$@"
	else
		err "installing the ${package} needs root; run as root, or set LOCKRA_PACKAGE=appimage"
	fi
}

case "$package" in
deb)
	# apt reads the file as its own user: let it, instead of warning about the private temp dir.
	chmod 755 "$tmp"
	chmod 644 "$tmp/$asset"
	info "installing with apt (it asks for your password when it needs to)"
	# The dependencies come from the distribution's mirror. Package lists older than the mirror
	# make apt ask for versions it no longer has (404), so refresh them first; if that fails, the
	# install still tries with the lists as they are.
	as_root apt-get update -qq || info "apt-get update failed; installing with the package lists as they are"
	as_root apt-get install -y "$tmp/$asset"
	info "installed Lockra ${version}; start it from the applications menu or run lockra-desktop"
	;;
rpm)
	chmod 755 "$tmp"
	chmod 644 "$tmp/$asset"
	# The package is not GPG-signed: its SHA-256 was checked against the release above.
	if command -v dnf >/dev/null 2>&1; then
		info "installing with dnf (it asks for your password when it needs to)"
		as_root dnf install -y "$tmp/$asset"
	elif command -v zypper >/dev/null 2>&1; then
		info "installing with zypper (it asks for your password when it needs to)"
		as_root zypper --non-interactive install --allow-unsigned-rpm "$tmp/$asset"
	elif command -v yum >/dev/null 2>&1; then
		info "installing with yum (it asks for your password when it needs to)"
		as_root yum install -y "$tmp/$asset"
	else
		info "installing with rpm (dependencies are not resolved)"
		as_root rpm -U --replacepkgs "$tmp/$asset"
	fi
	info "installed Lockra ${version}; start it from the applications menu or run lockra-desktop"
	;;
appimage)
	dir=${LOCKRA_INSTALL_DIR:-$HOME/.local/bin}
	mkdir -p "$dir"
	install -m 0755 "$tmp/$asset" "$dir/Lockra.AppImage"
	data="${XDG_DATA_HOME:-$HOME/.local/share}"
	mkdir -p "$data/applications"
	# The menu icon, from inside the AppImage (its runtime extracts without FUSE); the entry goes
	# without one when that does not work.
	icon=
	icon_path="usr/share/icons/hicolor/128x128/apps/lockra-desktop.png"
	if (cd "$tmp" && "$dir/Lockra.AppImage" --appimage-extract "$icon_path" >/dev/null 2>&1) &&
		[ -s "$tmp/squashfs-root/$icon_path" ]; then
		mkdir -p "$data/icons/hicolor/128x128/apps"
		cp "$tmp/squashfs-root/$icon_path" "$data/icons/hicolor/128x128/apps/lockra.png"
		icon="Icon=lockra"
	fi
	cat >"$data/applications/lockra.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Lockra
Comment=Two-factor codes that stay on your computer
Exec="$dir/Lockra.AppImage"
$icon
Terminal=false
Categories=Utility;Security;
EOF
	info "installed Lockra ${version} to $dir/Lockra.AppImage (and the applications menu)"
	info "an AppImage needs FUSE 2 (libfuse2) to start"
	;;
dmg)
	dir=${LOCKRA_INSTALL_DIR:-/Applications}
	if [ -z "${LOCKRA_INSTALL_DIR:-}" ] && [ ! -w "$dir" ]; then dir="$HOME/Applications"; fi
	mkdir -p "$dir"
	if pgrep -f 'Lockra.app/Contents/MacOS/' >/dev/null 2>&1; then
		err "Lockra is running: quit it (Lockra › Quit, or Cmd Q) and run this again"
	fi
	mounted="$tmp/mnt"
	mkdir -p "$mounted"
	hdiutil attach -nobrowse -readonly -quiet -mountpoint "$mounted" "$tmp/$asset" ||
		{
			mounted=
			err "could not open ${asset}"
		}
	[ -d "$mounted/Lockra.app" ] || err "${asset} holds no Lockra.app"
	rm -rf "$dir/Lockra.app"
	ditto "$mounted/Lockra.app" "$dir/Lockra.app"
	hdiutil detach "$mounted" -quiet >/dev/null 2>&1 || true
	mounted=
	# Downloaded here rather than by a browser, so the app carries no quarantine flag and opens
	# without the Gatekeeper prompt; clear one anyway in case an older copy left it.
	xattr -dr com.apple.quarantine "$dir/Lockra.app" 2>/dev/null || true
	# Launchpad and Spotlight list the apps LaunchServices knows; a copy made here would only be
	# registered at its first start, so register it now.
	lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
	if command -v lsregister >/dev/null 2>&1; then lsregister=$(command -v lsregister); fi
	if [ -x "$lsregister" ]; then
		"$lsregister" -f "$dir/Lockra.app" || info "LaunchServices did not take the app; it appears in Launchpad after its first start"
	fi
	info "installed Lockra ${version} to $dir/Lockra.app; open it from Launchpad or with: open -a Lockra"
	;;
esac
