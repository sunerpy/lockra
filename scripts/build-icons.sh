#!/usr/bin/env bash
# Render apps/desktop/src-tauri/icons/icon.svg to a 1024 px PNG and let the Tauri CLI derive every
# platform icon from it, render the browser extension's (apps/extension/public/icons, 16–128 px)
# and the tray's (icons/tray/, from tray.svg and tray-template.svg), then write contact sheets to
# check the small sizes by eye: the app icon at 16/32/48 px, the tray's on a light and a dark tray.
# `--tray` renders the tray's alone.
# Needs python3 with cairosvg and Pillow, and the Tauri CLI (`pnpm --filter @lockra/desktop tauri`).
# Usage: scripts/build-icons.sh [--tray] [contact-sheet-dir]
set -euo pipefail
cd "$(dirname "$0")/.."
icons=apps/desktop/src-tauri/icons
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
tray_only=""
if [ "${1:-}" = --tray ]; then
  tray_only=1
  shift
fi
sheets=${1:-$work}

# The tray (src/background.rs): the colour padlock at each size a Windows notification area draws
# (16 px times the display's scale), what Linux panels get (32), and the macOS menu bar's template
# at 36 px (18 pt at 2x). Each size is rendered from the vector, never scaled from another.
render_tray() {
  mkdir -p "$icons/tray"
  python3 - "$icons" "$sheets/tray-sheet.png" <<'PY'
import io
import sys

import cairosvg
from PIL import Image

icons, sheet_path = sys.argv[1], sys.argv[2]
for size in (16, 20, 24, 28, 32, 40, 48, 64):
    cairosvg.svg2png(url=f"{icons}/tray.svg", write_to=f"{icons}/tray/tray-{size}.png", output_width=size, output_height=size)
cairosvg.svg2png(url=f"{icons}/tray-template.svg", write_to=f"{icons}/tray/tray-template-36.png", output_width=36, output_height=36)
# The small sizes eight times over, on Windows' light and dark taskbars and on the macOS menu bar.
sizes, cell = (16, 20, 24, 32), 8
rows = [("tray-{}.png", (243, 243, 243)), ("tray-{}.png", (32, 32, 32)), ("template", (236, 236, 236))]
sheet = Image.new("RGB", (sum(sizes) * cell + 10 * (len(sizes) + 1), len(rows) * (32 * cell + 20) + 10), (128, 128, 128))
for row, (name, back) in enumerate(rows):
    x, y = 10, 10 + row * (32 * cell + 20)
    for size in sizes:
        if name == "template":
            png = cairosvg.svg2png(url=f"{icons}/tray-template.svg", output_width=size, output_height=size)
            image = Image.open(io.BytesIO(png)).convert("RGBA")
        else:
            image = Image.open(f"{icons}/tray/{name.format(size)}").convert("RGBA")
        tile = Image.new("RGBA", (size, size), (*back, 255))
        tile.alpha_composite(image)
        sheet.paste(tile.convert("RGB").resize((size * cell, size * cell), Image.NEAREST), (x, y))
        x += size * cell + 10
sheet.save(sheet_path)
print(f"build-icons: tray contact sheet {sheet_path}")
PY
}

render_tray
if [ -n "$tray_only" ]; then
  echo "build-icons: $(find "$icons/tray" -maxdepth 1 -type f | wc -l) files in $icons/tray"
  exit 0
fi
python3 -c 'import cairosvg, sys; cairosvg.svg2png(url=sys.argv[1], write_to=sys.argv[2], output_width=1024, output_height=1024)' "$icons/icon.svg" "$work/icon.png"
(cd apps/desktop && pnpm exec tauri icon "$work/icon.png" -o src-tauri/icons >/dev/null 2>&1)
# Desktop only: the mobile sets the CLI also writes are not shipped.
rm -rf "$icons/android" "$icons/ios"
python3 -c 'import cairosvg, sys
for size in (16, 32, 48, 128):
    cairosvg.svg2png(url=sys.argv[1], write_to=f"{sys.argv[2]}/icon-{size}.png", output_width=size, output_height=size)' \
  "$icons/icon.svg" apps/extension/public/icons
python3 - "$work/icon.png" "$sheets/contact-sheet.png" <<'PY'
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
