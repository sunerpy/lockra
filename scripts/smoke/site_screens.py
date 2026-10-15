#!/usr/bin/env python3
"""Screenshots for the documentation site (docs/site/public/screens), from the real app in one
language: the codes page, the Passwords page (a login selected, with its code), the import preview
of a Google Authenticator export and the backup page, at 1440 x 900, as <page>-<lang>-<scheme>.webp: the site's light figure in 澄空 and its dark one in
澄空 · 夜, Lockra's default theme on a light and on a dark system. The accounts are made up
(example.com addresses, secrets and passwords derived from their names). Run by scripts/capture-site-screens.sh,
which owns Xvfb, the driver and the throwaway folders; the WebDriver session is the smoke test's.
--state gets the accounts and the moment of each codes shot, for scripts/smoke/phone_screens.mjs to
show the same accounts and codes on the phone. Every wait is a condition with a deadline.
"""

import argparse
import base64
import hashlib
import io
import json
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
# The site's scheme, and the theme its figure is shot in.
SCHEMES = {"light": "glass", "dark": "glass_night"}

COPY = {
    "en": {
        "locale": "en",
        "nav_label": "Main navigation",
        "nav": {"codes": "Codes", "passwords": "Passwords", "import": "Import", "backup": "Backup"},
        "work": "Work",
        "dev": "Dev",
        "bank": ("Bank", "Card ending 1234"),
        "bank_login": "Bank",
        "vpn": "Legacy VPN",
    },
    "zh": {
        "locale": "zh-cn",
        "nav_label": "主导航",
        "nav": {"codes": "验证码", "passwords": "密码", "import": "导入", "backup": "备份"},
        "work": "工作",
        "dev": "开发",
        "bank": ("银行", "尾号 1234"),
        "bank_login": "网上银行",
        "vpn": "旧版 VPN",
    },
}


def secret(*seed):
    """Ten bytes derived from the account's name: a believable secret that belongs to no one."""
    return hashlib.sha1(":".join(("lockra-site", *seed)).encode()).digest()[:10]


def password(*seed):
    """A strong password derived from the login's name: shown as dots, and no one's."""
    return "-".join(b32(secret("password", *seed))[i : i + 5] for i in (0, 5, 10))


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
        self.web.gone('[data-testid="toast-viewport"]', timeout=20)

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

    def shot(self, page, scheme):
        """A screenshot once two in a row agree (the countdown ring steps once a second), as WebP;
        returns the moment it was taken, in Unix milliseconds."""
        self.rest()
        last, deadline = None, time.monotonic() + 8
        while True:
            image = self.web.screenshot()
            taken = int(time.time() * 1000)
            if image == last or time.monotonic() > deadline:
                break
            last = image
            time.sleep(0.3)
        picture = Image.open(io.BytesIO(image)).convert("RGB")
        if picture.size != (WIDTH, HEIGHT):
            sys.exit(f"site-screens: the screenshot is {picture.size}, expected {(WIDTH, HEIGHT)}")
        path = os.path.join(self.args.out, f"{page}-{self.args.lang}-{scheme}.webp")
        picture.save(path, "WEBP", quality=85, method=6)
        print(f"site-screens: {path}")
        return taken

    def run(self):
        web, copy = self.web, self.copy
        web.wait('[data-testid="page-welcome"]')
        self.resize()
        web.invoke({"command": "vault_create", "password": PASSWORD})
        web.wait('[data-testid="page-codes"]')
        self.settings(locale=copy["locale"], reduce_motion=True, follow_system_theme=False, theme="glass")

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
        # The same accounts for the phone (the mock's mockEntry options).
        state = {"accounts": [], "shots": {}}
        for issuer, account, params in accounts:
            options = {"secret": b32(secret(issuer, account)), "digits": params.get("digits", 6)}
            if issuer in ("GitHub", "Google"):
                options["favorite"] = True
            if issuer in ("AWS", "Slack", "Cloudflare"):
                options["group"] = copy["work"]
            state["accounts"].append({"issuer": issuer, "account": account, "options": options})
        state["accounts"].append({"issuer": bank, "account": card, "options": {"secret": b32(secret("bank", card)), "kind": {"type": "hotp", "counter": 12}}})
        web.invoke({"command": "import_text", "text": "\n".join(lines)})
        web.invoke({"command": "import_commit"})
        entries = until("eight accounts", lambda: (lambda e: e if len(e) == 8 else None)(web.invoke({"command": "app_state"})["entries"]))
        for entry in entries:
            if entry["issuer"] in ("GitHub", "Google"):
                web.invoke({"command": "entry_update", "id": entry["id"], "patch": {"favorite": True}})
            if entry["issuer"] in ("AWS", "Slack", "Cloudflare"):
                web.invoke({"command": "entry_update", "id": entry["id"], "patch": {"group": copy["work"]}})

        # The logins of the Passwords page, three with their account's code.
        code = {e["issuer"]: e["id"] for e in entries}
        logins = [
            ("GitHub", {"username": "octocat", "email": "alex@example.com", "url": "https://github.com", "group": copy["work"], "tags": [copy["dev"]], "favorite": True, "totp_entry": code["GitHub"]}),
            ("Google", {"email": "alex@example.com", "url": "https://accounts.google.com", "favorite": True, "totp_entry": code["Google"]}),
            ("AWS", {"username": "admin", "email": "admin@example.com", "url": "https://console.aws.amazon.com", "group": copy["work"], "totp_entry": code["AWS"]}),
            ("Slack", {"email": "alex@example.org", "url": "https://slack.com", "group": copy["work"]}),
            ("Cloudflare", {"email": "alex@example.com", "url": "https://dash.cloudflare.com", "group": copy["work"], "tags": [copy["dev"]]}),
            ("Dropbox", {"email": "alex@example.com", "url": "https://www.dropbox.com"}),
            ("Netflix", {"email": "alex@example.com", "url": "https://www.netflix.com"}),
            (copy["bank_login"], {"username": "alex", "url": "https://bank.example.com", "protection": "confirm"}),
        ]
        # The same logins for the phone (the mock's sampleItems, its account linked by issuer).
        state["logins"] = []
        issuers = {v: k for k, v in code.items()}
        for title, login in logins:
            urls = [{"url": login.pop("url"), "matching": "domain"}]
            draft = {"title": title, "password": {"type": "set", "value": password(title)}, "urls": urls, **login}
            web.invoke({"command": "item_add", "draft": draft})
            view = {k: v for k, v in login.items() if k != "totp_entry"}
            state["logins"].append({"title": title, "view": {**view, "urls": urls}, "password": password(title), "code": issuers.get(login.get("totp_entry"))})

        # An automatic backup that has run once, into a folder named like a synchronised one.
        self.settings(auto_backup={"enabled": True, "dir": self.args.backup_dir, "keep": 10})
        web.invoke({"command": "backup_auto_now"})
        until("the automatic backup", lambda: web.invoke({"command": "app_state"})["backup"]["last_auto_file"])

        self.nav("codes")
        until("eight rows", lambda: len(web.find_all('[data-testid="entry-row"]')) == 8)
        self.quiet()
        for scheme, theme in SCHEMES.items():
            self.theme(theme)
            # Mid-window: no code in its last five seconds, when the next one would show beside it.
            until("the middle of a 30-second window", lambda: 5 <= time.time() % 30 <= 15, timeout=40)
            state["shots"][f"codes-{scheme}"] = self.shot("codes", scheme)
        with open(self.args.state, "w") as f:
            json.dump(state, f, ensure_ascii=False, indent=1)

        # The Passwords page, GitHub's login in the pane beside the list.
        self.nav("passwords")
        rows = until("eight logins", lambda: (lambda r: r if len(r) == 8 else None)(web.find_all('[data-testid="password-row"]')))
        github = next(r for r in rows if web.text(r).startswith("GitHub"))
        web.click(github)
        web.wait('[data-testid="item-detail"]')
        self.quiet()
        for scheme, theme in SCHEMES.items():
            self.theme(theme)
            until("the middle of a 30-second window", lambda: 5 <= time.time() % 30 <= 15, timeout=40)
            self.shot("passwords", scheme)

        self.nav("backup")
        for scheme, theme in SCHEMES.items():
            self.theme(theme)
            self.shot("backup", scheme)

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
        for scheme, theme in SCHEMES.items():
            self.theme(theme)
            self.shot("import", scheme)
        web.invoke({"command": "import_cancel"})
        web.quit()


def main():
    parser = argparse.ArgumentParser()
    for name in ["driver", "app", "out", "encode-qr", "display", "backup-dir", "state"]:
        parser.add_argument(f"--{name}", required=True)
    parser.add_argument("--lang", required=True, choices=sorted(COPY))
    Capture(parser.parse_args()).run()


if __name__ == "__main__":
    main()
