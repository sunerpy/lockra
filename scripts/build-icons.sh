#!/usr/bin/env bash
# Render apps/desktop/src-tauri/icons/icon.svg to a 1024 px PNG and let the Tauri CLI derive every
# platform icon from it, then write a 16/32/48 px contact sheet to check the small sizes by eye.
# Needs python3 with cairosvg and Pillow, and the Tauri CLI (`pnpm --filter @lockra/desktop tauri`).
set -euo pipefail
cd "$(dirname "$0")/.."
icons=apps/desktop/src-tauri/icons
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
python3 -c 'import cairosvg, sys; cairosvg.svg2png(url=sys.argv[1], write_to=sys.argv[2], output_width=1024, output_height=1024)' "$icons/icon.svg" "$work/icon.png"
(cd apps/desktop && pnpm exec tauri icon "$work/icon.png" -o src-tauri/icons >/dev/null 2>&1)
# Desktop only: the mobile sets the CLI also writes are not shipped.
rm -rf "$icons/android" "$icons/ios"
python3 - "$work/icon.png" "${1:-$work/contact-sheet.png}" <<'PY'
import sys
from PIL import Image
source = Image.open(sys.argv[1]).convert("RGBA")
sizes = [16, 32, 48]
sheet = Image.new("RGBA", (sum(sizes) + 40, 64), (242, 243, 246, 255))
x = 10
for size in sizes:
    sheet.paste(source.resize((size, size), Image.LANCZOS), (x, 8), source.resize((size, size), Image.LANCZOS))
    x += size + 10
sheet.save(sys.argv[2])
print(f"build-icons: contact sheet {sys.argv[2]}")
PY
echo "build-icons: $(find "$icons" -maxdepth 1 -type f | wc -l) files in $icons"
