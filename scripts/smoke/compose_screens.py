#!/usr/bin/env python3
"""The documentation site's two device figures, put together from real screenshots in one
language and scheme (docs/site/README.md, "Screenshots"):

- devices-<lang>-<scheme>.webp: the desktop codes page and the phone codes page, the same accounts
  and codes on both (scripts/smoke/phone_screens.mjs runs the phone at the desktop shot's moment);
- phones-<lang>-<scheme>.webp: the phone's codes, add-an-account and sync pages side by side;
- phones-passwords-<lang>-<scheme>.webp: the phone's logins and a login's page.

1440 x 900, WebP quality 85, like the other figures. The screenshots keep their pixels: each is
scaled, given rounded corners, a hairline and a shadow, on a blurred copy of itself. No device or
window chrome is drawn. Run by scripts/capture-site-screens.sh.
"""

import argparse
import os

from PIL import Image, ImageChops, ImageDraw, ImageFilter

WIDTH, HEIGHT = 1440, 900
# The 澄空 / 澄空 · 夜 canvas (packages/ui/src/tokens.css), the backdrop's tint, and the edges.
CANVAS = {"light": (238, 242, 250), "dark": (11, 16, 32)}
HAIRLINE = {"light": (17, 24, 39, 30), "dark": (255, 255, 255, 34)}
SHADOW = {"light": 70, "dark": 150}


def rounded_mask(size, radius):
    """An antialiased rounded rectangle: drawn at four times the size, then scaled down."""
    w, h = size
    big = Image.new("L", (w * 4, h * 4), 0)
    ImageDraw.Draw(big).rounded_rectangle((0, 0, w * 4 - 1, h * 4 - 1), radius * 4, fill=255)
    return big.resize(size, Image.LANCZOS)


def backdrop(source, scheme):
    """The source blurred to fill the canvas, tinted towards the theme's canvas."""
    scale = max(WIDTH / source.width, HEIGHT / source.height)
    cover = source.resize((round(source.width * scale), round(source.height * scale)), Image.LANCZOS)
    left, top = (cover.width - WIDTH) // 2, (cover.height - HEIGHT) // 2
    blurred = cover.crop((left, top, left + WIDTH, top + HEIGHT)).filter(ImageFilter.GaussianBlur(48))
    return Image.blend(blurred, Image.new("RGB", (WIDTH, HEIGHT), CANVAS[scheme]), 0.45)


def place(canvas, shot, box, radius, scheme):
    """Paste the screenshot into box (x, y, w, h) with its shadow, rounded corners and hairline."""
    x, y, w, h = box
    picture = shot.resize((w, h), Image.LANCZOS)
    mask = rounded_mask((w, h), radius)
    shadow = Image.new("L", (WIDTH, HEIGHT), 0)
    shadow.paste(Image.new("L", (w, h), SHADOW[scheme]), (x, y + 18), mask)
    shadow = shadow.filter(ImageFilter.GaussianBlur(28))
    canvas.paste(Image.new("RGB", (WIDTH, HEIGHT), (0, 0, 0)), (0, 0), shadow)
    canvas.paste(picture, (x, y), mask)
    edge = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    ImageDraw.Draw(edge).rounded_rectangle((0, 0, w - 1, h - 1), radius, outline=HAIRLINE[scheme], width=1)
    canvas.paste(edge, (x, y), ImageChops.multiply(edge.getchannel("A"), mask))


def save(canvas, path):
    canvas.save(path, "WEBP", quality=85, method=6)
    print(f"compose-screens: {path}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--lang", required=True)
    parser.add_argument("--desktop", required=True, help="the folder with codes-<lang>-<scheme>.webp")
    parser.add_argument("--phone", required=True, help="the folder with <scheme>-<page>.png")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    for scheme in ("light", "dark"):
        desktop = Image.open(os.path.join(args.desktop, f"codes-{args.lang}-{scheme}.webp")).convert("RGB")
        phones = {page: Image.open(os.path.join(args.phone, f"{scheme}-{page}.png")).convert("RGB") for page in ("codes", "add", "sync", "logins", "login")}
        for shot in phones.values():
            if shot.size != (1080, 1920):
                raise SystemExit(f"compose-screens: a phone screenshot is {shot.size}, expected (1080, 1920)")

        # The phone in front of the desktop window's sidebar, the codes of both in view.
        canvas = backdrop(desktop, scheme)
        place(canvas, desktop, (264, 100, 1120, 700), 14, scheme)
        place(canvas, phones["codes"], (56, 66, 432, 768), 36, scheme)
        save(canvas, os.path.join(args.out, f"devices-{args.lang}-{scheme}.webp"))

        canvas = backdrop(phones["codes"], scheme)
        for i, page in enumerate(("codes", "add", "sync")):
            place(canvas, phones[page], (45 + i * 470, 85, 410, 729), 34, scheme)
        save(canvas, os.path.join(args.out, f"phones-{args.lang}-{scheme}.webp"))

        # The two passwords pages, centred as two of the three above would be.
        canvas = backdrop(phones["logins"], scheme)
        for i, page in enumerate(("logins", "login")):
            place(canvas, phones[page], (280 + i * 470, 85, 410, 729), 34, scheme)
        save(canvas, os.path.join(args.out, f"phones-passwords-{args.lang}-{scheme}.webp"))


if __name__ == "__main__":
    main()
