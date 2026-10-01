#!/usr/bin/env python3
"""Screenshots for the documentation site (docs/site/public/screens), from the real app in one
language: the codes page, the import preview of a Google Authenticator export and the backup page,
in the light and dark themes at 1440 x 900, as <page>-<lang>-<theme>.webp. The accounts are made up
(example.com addresses, secrets derived from their names). Run by scripts/capture-site-screens.sh,
which owns Xvfb, the driver and the throwaway folders; the WebDriver session is the smoke test's.
Every wait is a condition with a deadline.
"""

import argparse
import base64
import hashlib
import io
import os
import subprocess
import sys
import time
import urllib.parse

import cairosvg
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from desktop import Session, until  # noqa: E402

WIDTH, HEIGHT = 1440, 900
PASSWORD = "site screenshots pass phrase"
THEMES = ["light", "dark"]

COPY = {
    "en": {
        "locale": "en",
        "nav_label": "Main navigation",
        "nav": {"codes": "Codes", "import": "Import", "backup": "Backup"},
        "work": "Work",
        "bank": ("Bank", "Card ending 1234"),
        "vpn": "Legacy VPN",
    },
    "zh": {
        "locale": "zh-cn",
        "nav_label": "主导航",
        "nav": {"codes": "验证码", "import": "导入", "backup": "备份"},
        "work": "工作",
        "bank": ("银行", "尾号 1234"),
        "vpn": "旧版 VPN",
    },
}


def secret(*seed):
    """Ten bytes derived from the account's name: a believable secret that belongs to no one."""
    return hashlib.sha1(":".join(("lockra-site", *seed)).encode()).digest()[:10]


def b32(raw):
    return base64.b32encode(raw).decode().rstrip("=")


def otpauth(issuer, account, raw, kind="totp", **params):
    label = f"{urllib.parse.quote(issuer)}:{urllib.parse.quote(account)}"
    query = urllib.parse.urlencode({"secret": b32(raw), "issuer": issuer, **params})
    return f"otpauth://{kind}/{label}?{query}"


def pb_varint(n):
    out = bytearray()
    while True:
        byte, n = n & 0x7F, n >> 7
        out.append(byte | (0x80 if n else 0))
        if not n:
            return bytes(out)


def pb_field(field, value):
    if isinstance(value, int):
        return pb_varint(field << 3) + pb_varint(value)
    data = value.encode() if isinstance(value, str) else value
    return pb_varint(field << 3 | 2) + pb_varint(len(data)) + data


def migration_uri(accounts, size, index, batch_id):
    """Google Authenticator's export code (`otpauth-migration`), encoded here from the schema."""
    payload = b"".join(
        pb_field(1, pb_field(1, raw) + pb_field(2, account) + pb_field(3, issuer) + pb_field(4, algorithm) + pb_field(5, 1) + pb_field(6, 2))
        for issuer, account, raw, algorithm in accounts
    )
    payload += pb_field(2, 1) + pb_field(3, size) + pb_field(4, index) + pb_field(5, batch_id)
    return "otpauth-migration://offline?data=" + urllib.parse.quote(base64.b64encode(payload).decode(), safe="")


class Capture:
    def __init__(self, args):
        self.args = args
        self.copy = COPY[args.lang]
        self.env = {**os.environ, "DISPLAY": args.display}
        os.makedirs(args.out, exist_ok=True)
        self.web = Session(args.driver, args.app)
        self.window = None

    def settings(self, **patch):
        state = self.web.invoke({"command": "app_state"})
        self.web.invoke({"command": "settings_set", "settings": {**state["settings"], **patch}})

    def theme(self, theme):
        self.settings(theme=theme)
        until(f"the {theme} theme", lambda: self.web.run("return document.documentElement.dataset.theme") == theme)

    def nav(self, page):
        label, nav = self.copy["nav"][page], self.copy["nav_label"]
        self.web.click(self.web.wait(xpath=f"//nav[@aria-label='{nav}']//button[contains(normalize-space(), '{label}')]"))
        return self.web.wait(f'[data-testid="page-{page}"]')

    def quiet(self):
        """No toast on screen."""
        self.web.gone("div.pointer-events-none.fixed.bottom-6", timeout=20)

    def resize(self):
        self.window = until("the window", lambda: subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Lockra$"], env=self.env, capture_output=True, text=True).stdout.split())[0]
        subprocess.run(["xdotool", "windowmove", self.window, "0", "0"], env=self.env, check=True)
        subprocess.run(["xdotool", "windowsize", self.window, str(WIDTH), str(HEIGHT)], env=self.env, check=True)
        until(f"a {WIDTH}x{HEIGHT} viewport", lambda: self.web.run("return [innerWidth, innerHeight]") == [WIDTH, HEIGHT])

    def rest(self):
        """The pointer in the empty margin of the page and no focused control, so nothing shows
        a hover or focus state. The X pointer moves too: WebKit keeps the hover of the last
        WebDriver click until a real motion event arrives."""
        x, y = WIDTH - 40, HEIGHT - 120
        move = {"type": "pointerMove", "duration": 0, "origin": "viewport", "x": x, "y": y}
        self.web.call("POST", self.web.s("/actions"), {"actions": [{"type": "pointer", "id": "mouse", "parameters": {"pointerType": "mouse"}, "actions": [move]}]})
        subprocess.run(["xdotool", "mousemove", "--window", self.window, str(x - 1), str(y - 1), "mousemove", "--window", self.window, str(x), str(y)], env=self.env, check=True)
        self.web.run("document.activeElement?.blur()")

    def shot(self, page, theme):
        """A screenshot once two in a row agree (the countdown ring steps once a second), as WebP."""
        self.rest()
        last, deadline = None, time.monotonic() + 8
        while True:
            image = self.web.screenshot()
            if image == last or time.monotonic() > deadline:
                break
            last = image
            time.sleep(0.3)
        picture = Image.open(io.BytesIO(image)).convert("RGB")
        if picture.size != (WIDTH, HEIGHT):
            sys.exit(f"site-screens: the screenshot is {picture.size}, expected {(WIDTH, HEIGHT)}")
        path = os.path.join(self.args.out, f"{page}-{self.args.lang}-{theme}.webp")
        picture.save(path, "WEBP", quality=85, method=6)
        print(f"site-screens: {path}")

    def run(self):
        web, copy = self.web, self.copy
        web.wait('[data-testid="page-welcome"]')
        self.resize()
        web.invoke({"command": "vault_create", "password": PASSWORD})
        web.wait('[data-testid="page-codes"]')
        self.settings(locale=copy["locale"], reduce_motion=True, follow_system_theme=False, theme="light")

        bank, card = copy["bank"]
        accounts = [
            ("GitHub", "octocat", {}),
            ("Google", "alex@example.com", {}),
            ("Microsoft", "alex@example.com", {"digits": 8}),
            ("AWS", "admin@example.com", {}),
            ("Slack", "alex@example.org", {}),
            ("Cloudflare", "alex@example.com", {}),
            ("Dropbox", "alex@example.com", {}),
        ]
        lines = [otpauth(issuer, account, secret(issuer, account), **params) for issuer, account, params in accounts]
        lines.append(otpauth(bank, card, secret("bank", card), kind="hotp", counter=12))
        web.invoke({"command": "import_text", "text": "\n".join(lines)})
        web.invoke({"command": "import_commit"})
        entries = until("eight accounts", lambda: (lambda e: e if len(e) == 8 else None)(web.invoke({"command": "app_state"})["entries"]))
        for entry in entries:
            if entry["issuer"] in ("GitHub", "Google"):
                web.invoke({"command": "entry_update", "id": entry["id"], "patch": {"favorite": True}})
            if entry["issuer"] in ("AWS", "Slack", "Cloudflare"):
                web.invoke({"command": "entry_update", "id": entry["id"], "patch": {"group": copy["work"]}})

        # An automatic backup that has run once, into a folder named like a synchronised one.
        self.settings(auto_backup={"enabled": True, "dir": self.args.backup_dir, "keep": 10})
        web.invoke({"command": "backup_auto_now"})
        until("the automatic backup", lambda: web.invoke({"command": "app_state"})["backup"]["last_auto_file"])

        self.nav("codes")
        until("eight rows", lambda: len(web.find_all('[data-testid="entry-row"]')) == 8)
        self.quiet()
        for theme in THEMES:
            self.theme(theme)
            # Mid-window: no code in its last five seconds, when the next one would show beside it.
            until("the middle of a 30-second window", lambda: 5 <= time.time() % 30 <= 15, timeout=40)
            self.shot("codes", theme)

        self.nav("backup")
        for theme in THEMES:
            self.theme(theme)
            self.shot("backup", theme)

        # The first of two export codes, with one account of each kind the preview tells apart.
        batch = [
            ("GitHub", "octocat", secret("GitHub", "octocat"), 1),
            ("Slack", "alex@example.org", secret("Slack", "alex@example.org", "new"), 1),
            ("Notion", "alex@example.com", secret("Notion", "alex@example.com"), 1),
            ("Figma", "alex@example.com", secret("Figma", "alex@example.com"), 1),
            ("Linear", "alex@example.com", secret("Linear", "alex@example.com"), 1),
            (copy["vpn"], "alex", secret("vpn", "alex"), 4),
        ]
        uri = migration_uri(batch, size=2, index=0, batch_id=1_234_567_891)
        svg = subprocess.run([self.args.encode_qr, uri], capture_output=True, check=True, timeout=30).stdout
        png = cairosvg.svg2png(bytestring=svg, output_width=720)
        with subprocess.Popen(["xclip", "-selection", "clipboard", "-t", "image/png", "-i"], env=self.env, stdin=subprocess.PIPE) as server:
            server.communicate(png, timeout=10)
        until("the picture on the clipboard", lambda: "image/png" in subprocess.run(["xclip", "-o", "-selection", "clipboard", "-t", "TARGETS"], env=self.env, capture_output=True, text=True, timeout=10).stdout)
        self.nav("import")
        web.invoke({"command": "import_clipboard"})
        web.wait('[data-testid="import-preview"]')
        until("six accounts in the preview", lambda: len(web.invoke({"command": "app_state"})["import"]["candidates"]) == 6)
        self.quiet()
        for theme in THEMES:
            self.theme(theme)
            self.shot("import", theme)
        web.invoke({"command": "import_cancel"})
        web.quit()


def main():
    parser = argparse.ArgumentParser()
    for name in ["driver", "app", "out", "encode-qr", "display", "backup-dir"]:
        parser.add_argument(f"--{name}", required=True)
    parser.add_argument("--lang", required=True, choices=sorted(COPY))
    Capture(parser.parse_args()).run()


if __name__ == "__main__":
    main()
