#!/usr/bin/env bash
# Offline test of scripts/install.sh: a fake GitHub (a `curl` on PATH answering from a fixture
# release), fake package managers and macOS tools that record what they are asked to do, and the
# script run as `sh` (dash on Debian and Ubuntu) for every platform it supports. Nothing is
# installed on this machine and nothing goes online. Run by scripts/verify-all.sh and CI.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
script="$root/scripts/install.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
VERSION=9.8.7

fail() {
	echo "test-install: FAIL: $*" >&2
	if [ -f "$tmp/stderr" ]; then sed 's/^/  | /' "$tmp/stderr" >&2; fi
	exit 1
}

# ---- the release ----------------------------------------------------------------------------
release="$tmp/release/v$VERSION"
mkdir -p "$release"
for name in "Lockra_${VERSION}_amd64.deb" "Lockra_${VERSION}_arm64.deb" "Lockra-${VERSION}-1.x86_64.rpm" \
	"Lockra-${VERSION}-1.aarch64.rpm" "Lockra_${VERSION}_aarch64.dmg" "Lockra_${VERSION}_x64.dmg"; do
	printf 'package %s\n' "$name" >"$release/$name"
done
# An AppImage answers --appimage-extract by writing the file it is asked for.
for arch in amd64 aarch64; do
	cat >"$release/Lockra_${VERSION}_${arch}.AppImage" <<EOF
#!/bin/sh
# AppImage $arch
if [ "\${1:-}" = --appimage-extract ]; then mkdir -p "squashfs-root/\$(dirname "\$2")" && printf 'png' >"squashfs-root/\$2"; fi
EOF
done
(cd "$release" && sha256sum Lockra* >SHA256SUMS)
# GitHub's latest-release redirect serves the newest release's assets.
mkdir -p "$tmp/release/latest/download"
cp "$release/SHA256SUMS" "$tmp/release/latest/download/SHA256SUMS"

# ---- the commands ---------------------------------------------------------------------------
fake="$tmp/fake"
mkdir -p "$fake"
log="$tmp/calls.log"
cat >"$fake/curl" <<EOF
#!/bin/sh
out= url=
while [ \$# -gt 0 ]; do
	case "\$1" in
	-o) out=\$2; shift 2 ;;
	--retry) shift 2 ;;
	-*) shift ;;
	*) url=\$1; shift ;;
	esac
done
echo "curl \$url" >>"$log"
case "\$url" in
https://github.com/sunerpy/lockra/releases/latest/download/*) src="$tmp/release/latest/download/\${url#https://github.com/sunerpy/lockra/releases/latest/download/}" ;;
https://github.com/sunerpy/lockra/releases/download/*) src="$tmp/release/\${url#https://github.com/sunerpy/lockra/releases/download/}" ;;
*) echo "curl: unexpected URL \$url" >&2; exit 2 ;;
esac
[ -f "\$src" ] || exit 22
if [ -n "\$out" ]; then cp "\$src" "\$out"; else cat "\$src"; fi
EOF
cat >"$fake/uname" <<'EOF'
#!/bin/sh
case "$1" in -s) echo "$FAKE_UNAME_S" ;; -m) echo "$FAKE_UNAME_M" ;; esac
EOF
cat >"$fake/sysctl" <<'EOF'
#!/bin/sh
echo "${FAKE_ARM64:-0}"
EOF
printf '#!/bin/sh\necho 1000\n' >"$fake/id"
cat >"$fake/sudo" <<EOF
#!/bin/sh
echo "sudo \$*" >>"$log"
exec "\$@"
EOF
# A package manager records its arguments and the package it was handed.
recorder() {
	cat >"$1" <<EOF
#!/bin/sh
echo "\$(basename "\$0") \$*" >>"$log"
for arg in "\$@"; do
	case "\$arg" in *.deb | *.rpm) echo "package: \$(cat "\$arg")" >>"$log" ;; esac
done
EOF
	chmod +x "$1"
}
for manager in apt-get dnf zypper; do
	mkdir -p "$tmp/pm-$manager"
	recorder "$tmp/pm-$manager/$manager"
done
mkdir -p "$tmp/pm-none"
cat >"$fake/hdiutil" <<EOF
#!/bin/sh
echo "hdiutil \$*" >>"$log"
if [ "\$1" = attach ]; then
	while [ \$# -gt 0 ]; do [ "\$1" = -mountpoint ] && mount=\$2; shift; done
	mkdir -p "\$mount/Lockra.app/Contents/MacOS" && printf 'app\n' >"\$mount/Lockra.app/Contents/MacOS/lockra-desktop"
fi
EOF
cat >"$fake/ditto" <<'EOF'
#!/bin/sh
cp -R "$1" "$2"
EOF
for tool in xattr lsregister; do
	cat >"$fake/$tool" <<EOF
#!/bin/sh
echo "$tool \$*" >>"$log"
EOF
done
cat >"$fake/pgrep" <<'EOF'
#!/bin/sh
[ -n "${FAKE_RUNNING:-}" ]
EOF
chmod +x "$fake"/*

# The real tools the script uses, and nothing else from this machine (its own package manager
# would otherwise decide the package).
system="$tmp/system"
mkdir -p "$system"
for tool in sh sed awk grep head mktemp rm mkdir chmod cp install cat sha256sum dirname basename; do
	ln -s "$(command -v "$tool")" "$system/$tool"
done

# run CASE OS ARCH MANAGER [VAR=VALUE...]: the script in a fresh home; its status in $status.
run() {
	local name=$1 os=$2 arch=$3 manager=$4
	shift 4
	home="$tmp/home-$name"
	mkdir -p "$home"
	: >"$log"
	set +e
	env -i PATH="$fake:$tmp/pm-$manager:$system" HOME="$home" FAKE_UNAME_S="$os" FAKE_UNAME_M="$arch" "$@" \
		sh "$script" >"$tmp/stdout" 2>"$tmp/stderr"
	status=$?
	set -e
}

expect_ok() {
	[ "$status" = 0 ] || fail "$1: exited $status"
}

expect_failure() {
	[ "$status" != 0 ] || fail "$1: expected a failure"
	grep -q -- "$2" "$tmp/stderr" || fail "$1: the error does not say '$2'"
	if grep -q -e '-get install' -e 'dnf install' -e 'zypper ' -e 'ditto' "$log"; then fail "$1: something was installed"; fi
}

logged() {
	grep -qF -- "$2" "$log" || {
		sed 's/^/  log: /' "$log" >&2
		fail "$1: expected '$2' in the calls"
	}
}

# ---- Linux ----------------------------------------------------------------------------------
run apt Linux x86_64 apt-get
expect_ok "x64 with apt"
logged "x64 with apt" "curl https://github.com/sunerpy/lockra/releases/latest/download/SHA256SUMS"
logged "x64 with apt" "curl https://github.com/sunerpy/lockra/releases/download/v${VERSION}/Lockra_${VERSION}_amd64.deb"
if grep -q "api.github.com" "$log"; then fail "the latest release is found without the rate-limited API"; fi
logged "x64 with apt" "sudo apt-get update -qq"
logged "x64 with apt" "apt-get install -y"
logged "x64 with apt" "package: package Lockra_${VERSION}_amd64.deb"

run dnf Linux aarch64 dnf
expect_ok "ARM64 with dnf"
logged "ARM64 with dnf" "sudo dnf install -y"
logged "ARM64 with dnf" "package: package Lockra-${VERSION}-1.aarch64.rpm"

run zypper Linux x86_64 zypper
expect_ok "x64 with zypper"
logged "x64 with zypper" "zypper --non-interactive install --allow-unsigned-rpm"
logged "x64 with zypper" "package: package Lockra-${VERSION}-1.x86_64.rpm"

run appimage Linux x86_64 none
expect_ok "x64 without a package manager"
appimage="$home/.local/bin/Lockra.AppImage"
[ -x "$appimage" ] || fail "the AppImage is not at $appimage"
grep -q "AppImage amd64" "$appimage" || fail "the x64 AppImage was not the one installed"
entry="$home/.local/share/applications/lockra.desktop"
grep -qx "Exec=\"$appimage\"" "$entry" || fail "the menu entry does not start the AppImage"
grep -qx "Icon=lockra" "$entry" || fail "the menu entry has no icon"
[ -s "$home/.local/share/icons/hicolor/128x128/apps/lockra.png" ] || fail "the icon was not extracted"

run forced Linux aarch64 apt-get LOCKRA_PACKAGE=appimage LOCKRA_INSTALL_DIR="$tmp/apps" LOCKRA_VERSION="v$VERSION"
expect_ok "LOCKRA_PACKAGE=appimage on apt"
grep -q "AppImage aarch64" "$tmp/apps/Lockra.AppImage" || fail "the ARM64 AppImage was not installed into LOCKRA_INSTALL_DIR"
if grep -q -e "apt-get" -e "releases/latest" "$log"; then fail "a given version and package: no apt and no lookup of the latest"; fi

mv "$tmp/release/latest/download/SHA256SUMS" "$tmp/sums.latest"
printf 'not a checksum list\n' >"$tmp/release/latest/download/SHA256SUMS"
run no-latest Linux x86_64 apt-get
expect_failure "a latest release without packages" "could not find the latest release"
mv "$tmp/sums.latest" "$tmp/release/latest/download/SHA256SUMS"

run bad-package Linux x86_64 apt-get LOCKRA_PACKAGE=snap
expect_failure "an unknown package" "must be deb, rpm or appimage"

run bad-version Linux x86_64 apt-get LOCKRA_VERSION=latest
expect_failure "a version that is not one" "not a release version"

run no-release Linux x86_64 apt-get LOCKRA_VERSION=9.8.6
expect_failure "a release that does not exist" "has no SHA256SUMS"

cp "$release/Lockra_${VERSION}_amd64.deb" "$tmp/deb.orig"
printf 'tampered\n' >>"$release/Lockra_${VERSION}_amd64.deb"
run tampered Linux x86_64 apt-get
expect_failure "a package that does not match SHA256SUMS" "checksum mismatch"
mv "$tmp/deb.orig" "$release/Lockra_${VERSION}_amd64.deb"

run riscv Linux riscv64 apt-get
expect_failure "an unsupported processor" "x64 and ARM64 Linux only"

# ---- macOS ----------------------------------------------------------------------------------
run rosetta Darwin x86_64 none FAKE_ARM64=1 LOCKRA_INSTALL_DIR="$tmp/Applications"
expect_ok "Apple silicon, from a Rosetta shell"
logged "Apple silicon" "Lockra_${VERSION}_aarch64.dmg"
[ -f "$tmp/Applications/Lockra.app/Contents/MacOS/lockra-desktop" ] || fail "Lockra.app was not copied"
logged "Apple silicon" "xattr -dr com.apple.quarantine"
logged "Apple silicon" "lsregister -f"
logged "Apple silicon" "hdiutil detach"

run intel Darwin x86_64 none FAKE_ARM64=0 LOCKRA_INSTALL_DIR="$tmp/Applications-intel"
expect_ok "an Intel Mac"
logged "an Intel Mac" "Lockra_${VERSION}_x64.dmg"

run running Darwin arm64 none FAKE_ARM64=1 FAKE_RUNNING=1 LOCKRA_INSTALL_DIR="$tmp/Applications-running"
expect_failure "Lockra still running" "Lockra is running"

run freebsd FreeBSD amd64 none
expect_failure "an unsupported system" "unsupported system"

echo "test-install: install.sh passed on every platform it supports"
