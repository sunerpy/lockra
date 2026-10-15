#!/usr/bin/env bash
# The browser extension's release build (apps/extension, plan §九): the content script, which runs
# in every frame of every page, stays under 30 KB gzip, and the popup (its scripts and styles) under
# 150 KB; the content script and the background print nothing (no `console`), and nothing names a
# host it could load from: the only URLs allowed are XML namespaces, JSON Schema identifiers, the
# error decoder and the licence links React and Tailwind leave (none is fetched; the CSP would refuse
# it) and the source link the options page shows and copies. Every manifest carries the root
# package.json's version.
# Usage: scripts/check-extension-bundle.sh   (builds apps/extension first)
set -euo pipefail
cd "$(dirname "$0")/.."
pnpm --filter @lockra/extension build >/dev/null
status=0
version=$(python3 -c 'import json; print(json.load(open("package.json"))["version"])')

gz() { gzip -9 -c "$1" | wc -c; }

for target in chromium chromium-unpacked chromium-dev firefox; do
  dist=apps/extension/dist/$target
  manifest_version=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["version"])' "$dist/manifest.json")
  if [ "$manifest_version" != "$version" ]; then
    echo "check-extension-bundle: $target's manifest says $manifest_version, package.json $version"
    status=1
  fi

  content=$(gz "$dist/content.js")
  if [ "$content" -gt 30720 ]; then
    echo "check-extension-bundle: $target content.js is $content bytes gzip, over 30 KB"
    status=1
  fi

  popup=0
  while IFS= read -r asset; do
    popup=$((popup + $(gz "$dist/$asset")))
  done < <(grep -oE 'assets/[^"]+\.(js|css)' "$dist/popup.html" | sort -u)
  if [ "$popup" -gt 153600 ]; then
    echo "check-extension-bundle: $target popup is $popup bytes gzip (scripts and styles), over 150 KB"
    status=1
  fi

  for script in content.js background.js; do
    if grep -qE '\bconsole\.' "$dist/$script"; then
      echo "check-extension-bundle: $target $script writes to the console"
      status=1
    fi
  done

  allowed='^https?://(www\.w3\.org/|json-schema\.org/|react\.dev/errors/|tailwindcss\.com$|firlab\.app/lockra/$)'
  while IFS= read -r url; do
    if ! [[ $url =~ $allowed ]]; then
      echo "check-extension-bundle: unexpected URL in $target: $url"
      status=1
    fi
  done < <(grep -rohE 'https?://[A-Za-z0-9./_-]+' "$dist" --include='*.js' --include='*.css' --include='*.html' | sort -u)

  if [ "$target" = chromium ]; then
    sizes="content.js $((content / 1024)) KB gzip, popup $((popup / 1024)) KB gzip"
  fi
done

[ "$status" -eq 0 ] && echo "check-extension-bundle: ok ($sizes)"
exit "$status"
