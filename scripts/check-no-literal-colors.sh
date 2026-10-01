#!/usr/bin/env bash
# No colour literal in a component: every colour comes from packages/ui/src/tokens.css through the
# Tailwind utilities (DESIGN.md §1). The logo is the one exception (its colours are the mark's own),
# and test files are skipped (they assert on token values).
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2
pattern='(#[0-9a-fA-F]{6}\b|#[0-9a-fA-F]{3}\b|\brgba?\(|\bhsla?\(|\boklch\()'
matches=$(grep -rnE --include='*.tsx' "$pattern" packages/ui/src apps/desktop/src)
status=$?
if [ "$status" -gt 1 ]; then
  echo "check-no-literal-colors: grep failed with status $status"
  exit 2
fi
offenders=$(printf '%s\n' "$matches" | grep -vE '(\.test\.tsx|/Logo\.tsx):' | grep -v '^$')
if [ -n "$offenders" ]; then
  echo "check-no-literal-colors: colour literals in components (use a token from tokens.css):"
  printf '%s\n' "$offenders"
  exit 1
fi
echo "check-no-literal-colors: OK"
