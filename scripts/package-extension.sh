#!/usr/bin/env bash
# The browser extension's release files, made from the checked-out commit (release.yml's
# bundle-extension job; docs/release.md, "The browser extension"):
#   lockra-extension_<version>_chromium-store.zip     the package for Edge Add-ons and the Chrome
#                                                     Web Store
#   lockra-extension_<version>_chromium-unpacked.zip  the self-install package for Chrome, Edge and
#                                                     Brave (loaded unpacked; Edge Add-ons' id)
#   lockra-extension_<version>_firefox.zip            the package for addons.mozilla.org (AMO)
#   lockra-extension_<version>_source.zip             the commit's tree (git archive) with BUILD.md
#                                                     at its root: AMO asks for the source of a
#                                                     bundled, minified add-on and the commands
#                                                     that rebuild it
# and their SHA-256 in sha256sum's format, the names relative to DIST. The packages are what
# `make extension` builds (pnpm --filter @lockra/extension run package), built twice: both builds
# must give the same bytes, and exactly these three zips. Their manifests must carry the root
# package.json's version; the store package no `key` (Edge Add-ons refuses one), the self-install
# package the key of apps/extension/manifest/unpacked.json, and the Firefox package the add-on id
# of apps/extension/manifest/firefox.json.
#
# Usage: scripts/package-extension.sh [--version X.Y.Z] DIST CHECKSUMS
#          from a clean tree (the source zip is the commit, so the packages must be built from it)
#          into an empty or new DIST; --version is the release's, which package.json must carry.
#        scripts/package-extension.sh --check
#          the same into a temporary folder from the tree as it is, then the checksums checked as
#          release.yml's updater job checks them (make check, CI's repo job).
set -euo pipefail
cd "$(dirname "$0")/.."

fail() {
  echo "package-extension: $*" >&2
  exit 1
}

check=0
expect=
operands=()
while [ $# -gt 0 ]; do
  case $1 in
  --check)
    check=1
    shift
    ;;
  --version)
    [ $# -ge 2 ] || fail "--version needs a version"
    expect=$2
    shift 2
    ;;
  -*) fail "unknown option $1" ;;
  *)
    operands+=("$1")
    shift
    ;;
  esac
done
if [ "$check" -eq 1 ]; then
  { [ "${#operands[@]}" -eq 0 ] && [ -z "$expect" ]; } || fail "usage: scripts/package-extension.sh --check"
elif [ "${#operands[@]}" -ne 2 ]; then
  fail "usage: scripts/package-extension.sh [--version X.Y.Z] DIST CHECKSUMS"
fi
for tool in git pnpm node python3 sha256sum; do
  command -v "$tool" >/dev/null || fail "$tool is not installed"
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# package DIST CHECKSUMS DIRTY_OK
package() {
  local dist=$1 checksums=$2 dirty_ok=$3
  local version built=apps/extension/dist
  version=$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["version"])' package.json)
  if [ -n "$expect" ] && [ "$version" != "$expect" ]; then
    fail "package.json says $version, the release is $expect"
  fi
  if [ "$dirty_ok" -eq 0 ] && [ -n "$(git status --porcelain --untracked-files=all)" ]; then
    fail "the tree has changes: the source zip is the commit, so build the packages from a clean checkout"
  fi
  if [ -e "$dist" ] && [ -n "$(ls -A -- "$dist")" ]; then
    fail "$dist is not empty"
  fi
  mkdir -p -- "$dist" "$(dirname "$checksums")"
  local store="lockra-extension_${version}_chromium-store.zip"
  local unpacked="lockra-extension_${version}_chromium-unpacked.zip"
  local firefox="lockra-extension_${version}_firefox.zip"
  local source="lockra-extension_${version}_source.zip"

  pnpm --filter @lockra/extension run package
  local path
  local found=()
  for path in "$built"/*.zip; do
    if [ -e "$path" ]; then
      found+=("${path##*/}")
    fi
  done
  if [ "${found[*]-}" != "$store $unpacked $firefox" ]; then
    fail "the build wrote ${found[*]:-no zip}, the release ships $store, $unpacked and $firefox: change apps/extension/scripts/build.mjs and this script together"
  fi
  local first second
  first=$(cd "$built" && sha256sum -- "$store" "$unpacked" "$firefox")
  pnpm --filter @lockra/extension run package
  second=$(cd "$built" && sha256sum -- "$store" "$unpacked" "$firefox")
  if [ "$first" != "$second" ]; then
    fail "two builds of the same tree gave different packages, so a reviewer could not rebuild them:"$'\n'"$first"$'\n'"$second"
  fi
  cp -- "$built/$store" "$built/$unpacked" "$built/$firefox" "$dist/"

  # BUILD.md, for the reviewer who rebuilds the packages from the source zip: the environment
  # these were built in, the commands, the bytes that came out.
  local os='' runner=''
  if [ -r /etc/os-release ]; then
    os=$(sed -n 's/^PRETTY_NAME="\{0,1\}\([^"]*\)"\{0,1\}$/\1/p' /etc/os-release)
  fi
  os="${os:-$(uname -sr)}, $(uname -m)"
  if [ "${GITHUB_ACTIONS:-}" = true ] && [ -n "${ImageOS:-}" ]; then
    runner="GitHub Actions runner image ${ImageOS:-} ${ImageVersion:-}"
  fi
  python3 -I - "$work/BUILD.md" "$version" "$(git rev-parse HEAD)" "$os" "$runner" \
    "$(node --version)" "$(pnpm --version)" "$second" <<'PY'
import sys
from pathlib import Path

out, version, commit, system, runner, node, pnpm, sums = sys.argv[1:]
where = f"- {system}" + (f" ({runner.strip()})" if runner else "")
Path(out).write_text(f"""# Building Lockra's browser extension {version}

This is the source of Lockra {version} at commit {commit}, exported with `git archive`, with this
file added. The browser extension is `apps/extension`; it is built with the workspace packages
`packages/shared` and `packages/ui`, and every tool and library it uses is pinned in
`pnpm-lock.yaml`. Nothing else here (the Rust crates, the desktop and phone apps) is needed.

## Environment

The release's packages were built with:

{where}
- Node.js {node}: https://nodejs.org/dist/{node}/
- pnpm {pnpm}, the version `packageManager` names in `package.json`: `corepack enable`, or
  `npm install --global pnpm@{pnpm}`

## Commands

From the folder this file is in, with access to the npm registry:

    pnpm install --frozen-lockfile
    pnpm --filter @lockra/extension run package

The second command (what `make extension` runs) builds the extension into `apps/extension/dist/`:

- `firefox/` and `lockra-extension_{version}_firefox.zip`: the add-on for addons.mozilla.org;
- `chromium/` and `lockra-extension_{version}_chromium-store.zip`: the extension for Microsoft Edge
  Add-ons and the Chrome Web Store;
- `chromium-unpacked/` and `lockra-extension_{version}_chromium-unpacked.zip`: the same with Edge
  Add-ons' public key, for loading unpacked in Chrome, Edge and Brave;
- `chromium-dev/`: the same with a development key, for loading unpacked; never published.

The zips are deterministic (entries sorted, fixed times, no extra fields). The release's are:

""" + "".join(f"    {line}\n" for line in sums.splitlines()), encoding="utf-8")
PY
  # git writes a zip's times in the local time zone: UTC, so the zip depends on the commit alone.
  TZ=UTC git archive --format=zip --prefix="lockra-${version}/" --add-file="$work/BUILD.md" \
    -o "$dist/$source" HEAD

  python3 -I - "$dist" "$version" apps/extension/manifest/firefox.json apps/extension/manifest/unpacked.json <<'PY'
import json
import sys
import zipfile
from pathlib import Path

dist, version, firefox_manifest, unpacked_manifest = Path(sys.argv[1]), sys.argv[2], Path(sys.argv[3]), Path(sys.argv[4])
kinds = ("chromium-store", "chromium-unpacked", "firefox", "source")
store, unpacked, firefox, source = (f"lockra-extension_{version}_{kind}.zip" for kind in kinds)
problems = []


def opened(name):
    archive = zipfile.ZipFile(dist / name)
    names = archive.namelist()
    if len(names) != len(set(names)):
        problems.append(f"{name} holds an entry twice")
    broken = archive.testzip()
    if broken is not None:
        problems.append(f"{name}: {broken} does not match its CRC")
    return archive


def manifest(name, key=None):
    with opened(name) as archive:
        data = json.loads(archive.read("manifest.json"))
    if data.get("version") != version:
        problems.append(f"{name}: the manifest says version {data.get('version')!r}, not {version}")
    if data.get("key") != key:
        problems.append(f"{name}: the manifest's key is not {'the one of ' + str(unpacked_manifest) if key else 'absent'}")
    return data


found = sorted(path.name for path in dist.iterdir())
if found != sorted([store, unpacked, firefox, source]):
    problems.append(f"{dist} holds {found}, not {store}, {unpacked}, {firefox} and {source}")
# The store package without a key (Edge Add-ons refuses one, the stores assign the ids); the
# self-install package with Edge Add-ons' public key, so that it has the store's id.
self_install_key = json.loads(unpacked_manifest.read_text(encoding="utf-8"))["key"]
for name, key in ((store, None), (unpacked, self_install_key)):
    if "browser_specific_settings" in manifest(name, key):
        problems.append(f"{name}: the manifest is Firefox's")
gecko_id = json.loads(firefox_manifest.read_text(encoding="utf-8"))["browser_specific_settings"]["gecko"]["id"]
packed_id = manifest(firefox).get("browser_specific_settings", {}).get("gecko", {}).get("id")
if packed_id != gecko_id:
    problems.append(f"{firefox}: the add-on id is {packed_id!r}, not {gecko_id}")
root = f"lockra-{version}/"
with opened(source) as archive:
    names = set(archive.namelist())
    stray = sorted(name for name in names if not name.startswith(root))
    if stray:
        problems.append(f"{source} holds files outside {root}: {stray[:3]}")
    needed = ["BUILD.md", "package.json", "pnpm-lock.yaml", "apps/extension/package.json"]
    missing = [name for name in needed if root + name not in names]
    if missing:
        problems.append(f"{source} lacks {missing}")
    elif json.loads(archive.read(root + "package.json")).get("version") != version:
        problems.append(f"{source}: its package.json is not version {version}")
for problem in problems:
    print(f"package-extension: {problem}", file=sys.stderr)
sys.exit(1 if problems else 0)
PY
  (cd "$dist" && sha256sum -- "$store" "$unpacked" "$firefox" "$source") >"$checksums"
  cat -- "$checksums"
}

if [ "$check" -eq 0 ]; then
  package "${operands[0]}" "${operands[1]}" 0
  exit 0
fi
# As the updater job downloads them: bundle/dist and bundle/evidence.
package "$work/bundle/dist" "$work/bundle/evidence/extension.sha256" 1
(cd "$work/bundle/dist" && sha256sum --check --strict ../evidence/extension.sha256)
echo "package-extension: ok (the release's four files, the packages the same in two builds)"
