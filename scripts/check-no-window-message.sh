#!/usr/bin/env bash
# No `window` message listener anywhere (plan §一 信任边界, §五): a page can post any message to any
# window, so nothing in Lockra listens for them. The browser extension talks to its own parts through
# the browser's runtime messaging and ports only, and its inline frame takes its token from its URL.
# Flags `addEventListener("message"…)` and `onmessage =` on window, self, globalThis, top, parent,
# frames or on nothing (the global), in the web sources. A listener on an object of its own (a
# WebSocket, a Tauri channel, a MessagePort) is not this.
# Usage: scripts/check-no-window-message.sh [dir…]   (default: every web source of the repository)
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2
dirs=("$@")
if [ "${#dirs[@]}" -eq 0 ]; then
  dirs=(apps/desktop/src apps/mobile/src apps/extension/src apps/extension/scripts packages/ui/src packages/shared/src scripts)
fi
global='(^|[^.$[:alnum:]_])'
owner='(window|self|globalThis|top|parent|frames)[[:space:]]*\.[[:space:]]*'
event="addEventListener[[:space:]]*\\([[:space:]]*['\"\`]message(error)?['\"\`]"
handler='onmessage(error)?[[:space:]]*='
pattern="(${owner}|${global})(${event}|${handler})"
matches=$(grep -rnE --include='*.ts' --include='*.tsx' --include='*.js' --include='*.mjs' --include='*.html' \
  --exclude-dir=node_modules --exclude-dir=dist --exclude-dir=coverage "$pattern" "${dirs[@]}")
status=$?
if [ "$status" -gt 1 ]; then
  echo "check-no-window-message: grep failed with status $status"
  exit 2
fi
if [ -n "$matches" ]; then
  echo "check-no-window-message: a window message listener (pages can post anything to it):"
  printf '%s\n' "$matches"
  exit 1
fi
echo "check-no-window-message: OK"
