#!/usr/bin/env bash
# No two paths in one folder that differ only in case, and no two web modules there whose names
# without the extension do: macOS and Windows ignore case, so the first two cannot both be checked
# out, and an import without its extension may reach the other module. `../shared/Avatar` found
# `avatar.ts` before `Avatar.tsx` there, and the 0.11.0 release legs of both systems failed to build
# the extension (it built on Linux).
# Usage: scripts/check-case-paths.sh   (every tracked file of the repository)
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2
clashes=$(git ls-files | awk '
  function seen_as(key, shown) {
    if (!(key in first)) { first[key] = shown; return }
    if (first[key] != shown && !((key, shown) in told)) {
      told[key, shown] = 1
      print substr(first[key], 2) "  and  " substr(shown, 2)
    }
  }
  {
    n = split($0, parts, "/")
    dir = ""
    for (i = 1; i <= n; i++) {
      name = parts[i]
      # The name itself, folders on the way included.
      seen_as("n" dir "/" tolower(name), dir "/" name)
      # A web module by its name without the extension, as an import names it.
      if (i == n && name ~ /\.(ts|tsx|mts|cts|js|jsx|mjs|cjs)$/) {
        stem = name
        sub(/\.[^.]+$/, "", stem)
        seen_as("m" dir "/" tolower(stem), dir "/" stem)
      }
      dir = dir "/" name
    }
  }')
status=$?
if [ "$status" -ne 0 ]; then
  echo "check-case-paths: awk failed with status $status"
  exit 2
fi
if [ -n "$clashes" ]; then
  echo "check-case-paths: these differ only in case (macOS and Windows take one for the other):"
  printf '%s\n' "$clashes"
  exit 1
fi
echo "check-case-paths: ok"
