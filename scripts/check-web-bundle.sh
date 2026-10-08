#!/usr/bin/env bash
# The release web bundle carries no development code (the in-memory mock core, the component
# showcase) and names no host it could load from: the only URLs allowed are XML namespaces,
# schema identifiers and the error-decoder links React and Tailwind leave in comments and
# messages, none of which is ever fetched (the CSP would refuse it anyway). Besides those, the
# sync storage presets (packages/shared/src/storage-presets.ts) carry the start of the addresses
# they fill in and the built-in relay's address, exactly as listed: the core connects to the
# storage from Rust, the webview never. The relay's address field shows a reserved example.com
# address as its example.
# Usage: scripts/check-web-bundle.sh   (builds apps/desktop first)
set -euo pipefail
cd "$(dirname "$0")/.."
pnpm --filter @lockra/desktop build >/dev/null
dist=apps/desktop/dist
status=0

for marker in "MockBackend" "correct horse battery" "sampleEntries" "placeholderSvg" "showcase-freeze" "FROZEN_NOW" "awaitBackup"; do
  if grep -rqF -- "$marker" "$dist"; then
    echo "check-web-bundle: development code shipped: $marker"
    status=1
  fi
done

allowed='^https?://(www\.w3\.org/|json-schema\.org/|react\.dev/errors/|tailwindcss\.com$)'
presets='^https://(s3\.|s3\.oss-|cos\.|dav\.jianguoyun\.com/dav/?|lockra-relay\.onethinker\.top|relay\.example\.com\.?)$'
while IFS= read -r url; do
  if ! [[ $url =~ $allowed ]] && ! [[ $url =~ $presets ]]; then
    echo "check-web-bundle: unexpected URL in the bundle: $url"
    status=1
  fi
done < <(grep -rohE 'https?://[A-Za-z0-9./_-]+' "$dist" | sort -u)

if grep -qE 'https?://' "$dist/index.html"; then
  echo "check-web-bundle: index.html references a remote resource"
  status=1
fi

[ "$status" -eq 0 ] && echo "check-web-bundle: ok ($(du -sh "$dist" | cut -f1))"
exit "$status"
