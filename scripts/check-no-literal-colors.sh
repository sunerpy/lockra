#!/usr/bin/env bash
# No colour literal in a component: every colour comes from packages/ui/src/tokens.css through the
# Tailwind utilities (DESIGN.md §1); the in-app logo too, since it sits on the theme's card (its
# colours are `--logo-body` / `--logo-dots`). The browser extension's pages count too, its plain
# TypeScript ones included (the inline frame draws its DOM by hand). Test files are skipped (they
# assert on token values).
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2
pattern='(#[0-9a-fA-F]{6}\b|#[0-9a-fA-F]{3}\b|\brgba?\(|\bhsla?\(|\boklch\()'
components=$(grep -rnE --include='*.tsx' "$pattern" packages/ui/src apps/desktop/src apps/mobile/src apps/extension/src)
components_status=$?
pages=$(grep -rnE --include='*.ts' "$pattern" apps/extension/src)
pages_status=$?
for status in "$components_status" "$pages_status"; do
  if [ "$status" -gt 1 ]; then
    echo "check-no-literal-colors: grep failed with status $status"
    exit 2
  fi
done
offenders=$(printf '%s\n%s\n' "$components" "$pages" | grep -vE '\.test\.tsx?:' | grep -v '^$')
if [ -n "$offenders" ]; then
  echo "check-no-literal-colors: colour literals in components (use a token from tokens.css):"
  printf '%s\n' "$offenders"
  exit 1
fi
echo "check-no-literal-colors: OK"
