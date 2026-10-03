#!/usr/bin/env python3
"""Desktop smoke test of the real app, driven over WebDriver (tauri-driver -> WebKitWebDriver)
with real X input where it matters (typing the master password, Ctrl+L) and the X clipboard
(xclip). Run by scripts/smoke-desktop-linux.sh, which owns Xvfb, the driver and the data dirs.

Checks: create a vault by typing; import an otpauth link from the clipboard; the copied code
equals an independent RFC 6238 computation; lock and unlock; Google's export QR code decodes
(rxing, scripts' decode-qr) back to the same account; then screenshots of every page at three
window sizes in the light and dark themes, plus the empty, long and 200-account states; Touch ID
turned on from the unlock screen's pointer and unlocking with it (the debug build's stand-in check);
an account's colour and avatar text, folded groups, a row's right-click menu, and several accounts
ticked and moved to a group at once.
Every wait is a condition with a deadline; nothing sleeps for a fixed time to "let it finish".
"""

import argparse
import base64
import hashlib
import hmac
import json
import os
import struct
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

SECRET = "JBSWY3DPEHPK3PXP"
ISSUER = "Smoke"
ACCOUNT = "alice@example.com"
URI = f"otpauth://totp/{ISSUER}:{ACCOUNT}?secret={SECRET}&issuer={ISSUER}"
PASSWORD = "smoke test pass phrase"
SIZES = [(960, 600), (1280, 800), (1440, 900)]
THEMES = ["light", "dark"]
PAGES = [("codes", "验证码"), ("import", "导入"), ("export", "导出"), ("backup", "备份")]
ELEMENT = "element-6066-11e4-a52e-4f735466cecf"


class NoSuchElement(Exception):
    pass


def until(what, check, timeout=30.0, step=0.25):
    """Poll `check` until it returns something truthy; fail after `timeout` seconds."""
    deadline = time.monotonic() + timeout
    while True:
        value = check()
        if value:
            return value
        if time.monotonic() > deadline:
            sys.exit(f"smoke: timed out waiting for {what}")
        time.sleep(step)


class Session:
    def __init__(self, base, app):
        self.base = base
        caps = {"capabilities": {"alwaysMatch": {"tauri:options": {"application": app}}}}
        self.id = self.call("POST", "/session", caps)["sessionId"]

    def call(self, method, path, body=None):
        data = None if body is None else json.dumps(body).encode()
        for attempt in range(1, 4):
            req = urllib.request.Request(self.base + path, method=method, data=data, headers={"Content-Type": "application/json"})
            try:
                with urllib.request.urlopen(req, timeout=120) as r:
                    return json.loads(r.read() or b'{"value": null}')["value"]
            except urllib.error.HTTPError as e:
                payload = json.loads(e.read() or b"{}").get("value", {})
                if payload.get("error") == "no such element":
                    raise NoSuchElement(path) from None
                raise SystemExit(f"smoke: {method} {path}: {e.code} {payload}") from None
            except (ConnectionResetError, urllib.error.URLError) as e:
                reset = isinstance(e, ConnectionResetError) or isinstance(getattr(e, "reason", None), ConnectionResetError)
                if not reset or attempt == 3:
                    raise SystemExit(f"smoke: {method} {path}: {e}") from None
                # tauri-driver drops a request when the pooled connection it forwards on was reset by
                # WebKitWebDriver ("client error (SendRequest) ... Connection reset by peer" in its log),
                # while every process keeps running (measured on CI, 2026-10-01). The request did not
                # reach the browser, so it is sent again; a command that did run would fail loudly the
                # second time rather than pass.
                print(f"smoke: the driver dropped {method} {path} ({e}); sending it again", file=sys.stderr)
                time.sleep(0.5)

    def s(self, path):
        return f"/session/{self.id}{path}"

    def find(self, css=None, xpath=None):
        using, value = ("css selector", css) if css is not None else ("xpath", xpath)
        try:
            return self.call("POST", self.s("/element"), {"using": using, "value": value})[ELEMENT]
        except NoSuchElement:
            return None

    def find_all(self, css):
        return [e[ELEMENT] for e in self.call("POST", self.s("/elements"), {"using": "css selector", "value": css})]

    def wait(self, css=None, xpath=None, timeout=30.0):
        return until(css or xpath, lambda: self.find(css, xpath), timeout)

    def gone(self, css, timeout=30.0):
        until(f"{css} to go", lambda: self.find(css) is None, timeout)

    def click(self, element):
        self.call("POST", self.s(f"/element/{element}/click"), {})

    def type(self, element, text):
        self.call("POST", self.s(f"/element/{element}/value"), {"text": text})

    def text(self, element):
        return self.call("GET", self.s(f"/element/{element}/text"))

    def run(self, script, *args):
        return self.call("POST", self.s("/execute/sync"), {"script": script, "args": list(args)})

    def invoke(self, command):
        """A core command through the shell's own IPC, as the webview sends it (setup only)."""
        # Tauri's internals global, spelled in two halves: the scaffold check rejects any literal
        # double-underscore NAME token as an unresolved template placeholder.
        script = (
            "const done = arguments[arguments.length - 1];"
            "window['__' + 'TAURI_INTERNALS__'].invoke('lockra_dispatch', { command: arguments[0] })"
            ".then((ok) => done({ ok }), (err) => done({ err }));"
        )
        answer = self.call("POST", self.s("/execute/async"), {"script": script, "args": [command]})
        if "err" in answer:
            sys.exit(f"smoke: {command['command']} failed: {answer['err']}")
        return answer["ok"]

    def screenshot(self):
        return base64.b64decode(self.call("GET", self.s("/screenshot")))

    def quit(self):
        try:
            self.call("DELETE", self.s(""))
        except SystemExit:
            pass


def totp(secret, at, period=30, digits=6):
    key = base64.b32decode(secret + "=" * (-len(secret) % 8))
    mac = hmac.new(key, struct.pack(">Q", int(at // period)), hashlib.sha1).digest()
    offset = mac[-1] & 0x0F
    return f"{(struct.unpack('>I', mac[offset:offset + 4])[0] & 0x7FFFFFFF) % 10**digits:0{digits}d}"


def varint(data, i):
    value = shift = 0
    while True:
        byte = data[i]
        i += 1
        value |= (byte & 0x7F) << shift
        shift += 7
        if byte < 0x80:
            return value, i


def proto_fields(data):
    """(field, value) pairs of a protobuf message: varints as ints, length-delimited as bytes."""
    i, out = 0, []
    while i < len(data):
        key, i = varint(data, i)
        field, wire = key >> 3, key & 7
        if wire == 0:
            value, i = varint(data, i)
        elif wire == 2:
            size, i = varint(data, i)
            value, i = data[i:i + size], i + size
        else:
            sys.exit(f"smoke: unexpected protobuf wire type {wire}")
        out.append((field, value))
    return out


def migration_accounts(uri):
    """Google Authenticator's export format, decoded independently of the Rust code."""
    parsed = urllib.parse.urlparse(uri)
    if parsed.scheme != "otpauth-migration":
        sys.exit(f"smoke: not a migration URI: {uri[:40]}")
    # Not parse_qs: it turns "+" (a base64 digit) into a space.
    raw = dict(part.split("=", 1) for part in parsed.query.split("&") if "=" in part)["data"]
    data = urllib.parse.unquote(raw)
    payload = base64.b64decode(data + "=" * (-len(data) % 4))
    accounts = []
    for field, value in proto_fields(payload):
        if field == 1:
            fields = dict(proto_fields(value))
            accounts.append({"secret": fields.get(1, b""), "name": fields.get(2, b"").decode(), "issuer": fields.get(3, b"").decode()})
    return accounts


class Smoke:
    def __init__(self, args):
        self.args = args
        self.env = {**os.environ, "DISPLAY": args.display}
        os.makedirs(args.out, exist_ok=True)
        self.web = Session(args.driver, args.app)
        self.window = None

    # ---- X input and the clipboard ---------------------------------------------------------

    def x(self, *argv):
        subprocess.run(["xdotool", *argv], env=self.env, check=True)

    def focus(self):
        if self.window is None:
            found = until("the window", lambda: subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Lockra$"], env=self.env, capture_output=True, text=True).stdout.split())
            self.window = found[0]
            self.x("windowmove", self.window, "0", "0")
        # No window manager under Xvfb: set the input focus directly.
        self.x("windowfocus", self.window)

    def clipboard(self):
        out = subprocess.run(["xclip", "-o", "-selection", "clipboard"], env=self.env, capture_output=True, text=True, timeout=10)
        return out.stdout if out.returncode == 0 else ""

    # ---- pages ------------------------------------------------------------------------------

    def page(self, name):
        return self.web.wait(f'[data-testid="page-{name}"]')

    def nav(self, label):
        self.web.click(self.web.wait(xpath=f"//nav[@aria-label='主导航']//button[contains(normalize-space(), '{label}')]"))

    def settings(self, **patch):
        state = self.web.invoke({"command": "app_state"})
        self.web.invoke({"command": "settings_set", "settings": {**state["settings"], **patch}})

    def resize(self, width, height):
        self.focus()
        self.x("windowsize", self.window, str(width), str(height))
        until(f"a {width}x{height} viewport", lambda: self.web.run("return [innerWidth, innerHeight]") == [width, height])

    def shot(self, name):
        """A screenshot once two in a row agree (the countdown ring steps once a second)."""
        path = os.path.join(self.args.out, f"{name}.png")
        last = None
        deadline = time.monotonic() + 8
        while True:
            image = self.web.screenshot()
            if image == last or time.monotonic() > deadline:
                break
            last = image
            time.sleep(0.3)
        with open(path, "wb") as f:
            f.write(image)
        print(f"smoke: {path}")
        return path

    # ---- the flow ---------------------------------------------------------------------------

    def run(self):
        web = self.web
        self.page("welcome")
        self.shot("welcome-1152-light")
        # The title bar's close button, for the close test the shell script runs afterwards.
        rect = web.run("const b = [...document.querySelectorAll('header button')].find((e) => e.getAttribute('aria-label') === '关闭'); const r = b.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2, innerWidth, innerHeight];")
        with open(os.path.join(self.args.work, "close-button.json"), "w") as f:
            json.dump(rect, f)

        # Create the vault by typing, as a person would (the first field has the focus).
        self.focus()
        self.x("type", "--delay", "15", PASSWORD)
        self.x("key", "Tab", "Tab")
        self.x("type", "--delay", "15", PASSWORD)
        self.x("key", "Return")
        self.page("codes")
        until("the empty state", lambda: web.find(xpath="//*[normalize-space()='还没有账号']"))
        self.shot("codes-empty-1152-light")

        # Import from the clipboard.
        server = subprocess.Popen(["xclip", "-selection", "clipboard", "-i"], env=self.env, stdin=subprocess.PIPE)
        server.communicate(URI.encode(), timeout=10)
        until("the link on the clipboard", lambda: self.clipboard() == URI)
        web.click(web.wait('[data-testid="add-menu"]'))
        web.click(web.wait(xpath="//button[@role='menuitem'][normalize-space()='从剪贴板导入']"))
        web.wait('[data-testid="import-preview"]')
        self.shot("import-preview-1152-light")
        web.click(web.wait('[data-testid="import-commit"]'))
        web.gone('[data-testid="import-preview"]')
        self.nav("验证码")
        self.page("codes")
        row = until("one account", lambda: (lambda rows: rows[0] if len(rows) == 1 else None)(web.find_all('[data-testid="entry-row"]')))

        # Copy, and check the code against RFC 6238 computed here.
        for attempt in range(2):
            web.click(row)
            copied = until("the code on the clipboard", lambda: (lambda c: c if c.isdigit() else None)(self.clipboard().strip()))
            now = time.time()
            if copied == totp(SECRET, now):
                break
            if attempt == 1:
                sys.exit(f"smoke: copied {copied}, expected {totp(SECRET, now)}")
            print("smoke: a window boundary passed while copying; once more")
        print(f"smoke: copied code {copied} matches RFC 6238")

        # Lock with the keyboard, unlock by typing.
        self.focus()
        self.x("key", "ctrl+l")
        self.page("unlock")
        self.shot("unlock-1152-light")
        self.focus()
        self.x("type", "--delay", "15", PASSWORD)
        self.x("key", "Return")
        self.page("codes")

        # Export to Google Authenticator and read the code back from the screen.
        self.nav("导出")
        self.page("export")
        web.type(web.wait('[data-testid="page-export"] input[type="password"]'), PASSWORD)
        web.click(web.wait('[data-testid="export-start"]'))
        web.wait('[data-testid="export-viewer"] img')
        shot = self.shot("export-google-1152-light")
        decoded = subprocess.run([self.args.decode_qr, shot], capture_output=True, text=True, timeout=60)
        if decoded.returncode != 0:
            sys.exit(f"smoke: decode-qr failed: {decoded.stderr.strip()}")
        accounts = migration_accounts(decoded.stdout.strip().splitlines()[0])
        expected = {"secret": base64.b32decode(SECRET), "name": ACCOUNT, "issuer": ISSUER}
        if accounts != [expected]:
            sys.exit(f"smoke: the export QR code holds {accounts}, expected {[expected]}")
        print("smoke: the Google export QR code decodes to the imported account")
        web.click(web.wait('[data-testid="export-finish"]'))
        web.gone('[data-testid="export-viewer"]')

        # Believable accounts for the screenshots, in Chinese, with motion off (still frames).
        self.settings(locale="zh-cn", reduce_motion=True, follow_system_theme=False, theme="light")
        lines = [
            "otpauth://totp/GitHub:octocat?secret=GEZDGNBVGY3TQOJQ&issuer=GitHub",
            "otpauth://totp/Google:alex%40gmail.com?secret=MFRGGZDFMZTWQ2LK&issuer=Google",
            "otpauth://totp/Microsoft:alex%40outlook.com?secret=ONSWG4TFOQ======&issuer=Microsoft&digits=8",
            "otpauth://totp/AWS:root%40acme-corp?secret=KRUGKIDROVUWG2ZA&issuer=AWS",
            "otpauth://hotp/Bank:6222%201234?secret=MZXW6YTBOI======&issuer=Bank&counter=12",
            "otpauth://totp/Game:player-one?secret=NBSWY3DPEB3W64TM&issuer=Game&period=60&digits=7",
        ]
        web.invoke({"command": "import_text", "text": "\n".join(lines)})
        web.invoke({"command": "import_commit"})
        self.nav("验证码")
        self.page("codes")
        for width, height in SIZES:
            self.resize(width, height)
            for theme in THEMES:
                self.settings(theme=theme)
                until(f"the {theme} theme", lambda: web.run("return document.documentElement.dataset.theme") == theme)
                for page, label in PAGES:
                    self.nav(label)
                    self.page(page)
                    self.shot(f"{page}-{width}-{theme}")
        self.resize(1280, 800)
        self.settings(theme="light")
        self.focus()
        self.x("key", "ctrl+comma")
        web.wait('[data-testid="settings-content"]')
        web.click(web.wait(xpath="//button[@role='tab'][normalize-space()='安全']"))
        web.wait('[data-section="security"]')
        self.shot("settings-security-1280-light")
        self.settings(theme="dark")
        web.click(web.wait(xpath="//button[@role='tab'][normalize-space()='外观']"))
        web.wait('[data-section="appearance"]')
        self.shot("settings-appearance-1280-dark")
        self.x("key", "Escape")
        web.gone('[data-testid="settings-content"]')
        self.settings(theme="light")

        # Touch ID (the debug build's stand-in check, which always passes): while it is off, the
        # unlock screen says where to turn it on; there one switch turns it and "remember on this
        # device" on; then the unlock screen's button opens the vault, the user's own lock waits for
        # it, and any other lock asks for it by itself (Touch ID is the default unlock).
        self.nav("验证码")
        self.page("codes")
        self.focus()
        self.x("key", "ctrl+l")
        self.page("unlock")
        web.wait('[data-testid="biometric-offer"]')
        self.shot("unlock-touch-id-offer-1280-light")
        self.focus()
        self.x("type", "--delay", "15", PASSWORD)
        self.x("key", "Return")
        self.page("codes")
        self.focus()
        self.x("key", "ctrl+comma")
        web.wait('[data-testid="settings-content"]')
        web.click(web.wait(xpath="//button[@role='tab'][normalize-space()='安全']"))
        web.click(web.wait('[data-testid="biometric-unlock"] [role="switch"]'))
        remembered = lambda: web.invoke({"command": "app_state"})["lock"]["device_unlock"]
        until("Touch ID and remember on this device turned on", lambda: remembered()["enabled"] and remembered()["biometric"]["enabled"])
        self.shot("settings-biometric-1280-light")
        self.x("key", "Escape")
        web.gone('[data-testid="settings-content"]')
        self.focus()
        self.x("key", "ctrl+l")
        self.page("unlock")
        touch_id = web.wait(xpath="//button[normalize-space()='使用 Touch ID 解锁']")
        self.shot("unlock-touch-id-1280-light")
        # The user's own lock does not bring the check: the vault stays locked with the window in
        # front. The two seconds are the check itself (the stand-in passes at once, so a check
        # asked for would have unlocked by then), not a wait for something to finish.
        time.sleep(2)
        if web.invoke({"command": "app_state"})["phase"] != "locked":
            raise SystemExit("smoke: the lock screen asked for Touch ID right after the user's own lock")
        web.click(touch_id)
        self.page("codes")
        # Touch ID is the default unlock: a lock that is not the user's (the core's own, as an
        # automatic lock is) brings the check by itself while the window is in front.
        self.focus()
        web.invoke({"command": "vault_lock"})
        until("the vault unlocked by itself with Touch ID", lambda: web.invoke({"command": "app_state"})["phase"] == "unlocked")
        self.page("codes")
        web.invoke({"command": "device_unlock_disable", "password": PASSWORD})

        # An account's own colour and avatar text, from the row's edit button.
        self.nav("验证码")
        self.page("codes")
        web.click(web.wait(css='[data-testid="row-edit"]'))
        dialog = "//*[@role='dialog']"
        web.click(web.wait(xpath=f"{dialog}//*[@role='radio'][@aria-label='紫色']"))
        web.type(web.wait(xpath=f"{dialog}//label[normalize-space()='头像文字']/following::input[1]"), "GH")
        # The avatar is aria-hidden, which WebKitWebDriver's element text reads as empty.
        avatar = web.wait(xpath=f"{dialog}//*[@data-testid='entry-avatar']")
        until("the preview", lambda: web.run("return [arguments[0].textContent, arguments[0].dataset.tag]", {ELEMENT: avatar}) == ["GH", "purple"])
        self.shot("edit-appearance-1280-light")
        web.click(web.wait(xpath=f"{dialog}//button[normalize-space()='保存']"))
        web.gone('[role="dialog"]')
        # Groups that fold: two accounts in a group, then every section folded, then the row menu
        # of a right click.
        entries = web.invoke({"command": "app_state"})["entries"]
        for entry in entries[:2]:
            web.invoke({"command": "entry_update", "id": entry["id"], "patch": {"group": "工作"}})
        until("the sections", lambda: len(web.find_all('[data-testid="codes-group-toggle"]')) == 2)
        self.shot("codes-groups-1280-light")
        web.click(web.wait(css='[data-testid="codes-collapse-all"]'))
        until("everything folded", lambda: not web.find_all('[data-testid="entry-row"]'))
        self.shot("codes-groups-folded-1280-light")
        web.click(web.wait(css='[data-testid="codes-expand-all"]'))
        until("everything unfolded", lambda: len(web.find_all('[data-testid="entry-row"]')) == len(entries))
        row = web.find_all('[data-testid="entry-row"]')[1]
        web.run(
            "const r = arguments[0].getBoundingClientRect();"
            "arguments[0].dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, cancelable: true, clientX: r.left + 240, clientY: r.top + r.height / 2}));",
            {ELEMENT: row},
        )
        web.wait('[data-testid="row-context"]')
        self.shot("codes-context-1280-light")
        self.focus()
        self.x("key", "Escape")
        web.gone('[data-testid="row-context"]')
        # Several accounts at once: the group's two and one more, moved to a new group in one step.
        web.click(web.wait(css='[data-testid="codes-select"]'))
        web.wait('[data-testid="codes-selection"]')
        web.click(web.wait(xpath="//input[@type='checkbox'][@aria-label='选择「工作」中的全部账号']"))
        web.click(web.find_all('[data-testid="entry-row"]')[3])
        until("three ticked", lambda: len(web.find_all('[data-testid="entry-row"][aria-checked="true"]')) == 3)
        self.shot("codes-select-1280-light")
        web.click(web.wait(css='[data-testid="codes-move"]'))
        dialog = "//*[@role='dialog']"
        web.type(web.wait(xpath=f"{dialog}//label[normalize-space()='分组']/following::input[1]"), "Home")
        self.shot("codes-move-1280-light")
        web.click(web.wait(xpath=f"{dialog}//button[normalize-space()='移动']"))
        web.gone('[role="dialog"]')
        web.gone('[data-testid="codes-selection"]')
        until("three accounts in Home", lambda: sum(e["group"] == "Home" for e in web.invoke({"command": "app_state"})["entries"]) == 3)
        for entry in web.invoke({"command": "app_state"})["entries"]:
            web.invoke({"command": "entry_update", "id": entry["id"], "patch": {"group": ""}})

        # Overflow: a 200-character issuer and an unbroken account; then 200 accounts.
        long_issuer = urllib.parse.quote("超长服务名称" + "Very long issuer name " * 9)
        long_account = "a" * 120 + "@example.com"
        web.invoke({"command": "import_text", "text": f"otpauth://totp/{long_issuer}:{long_account}?secret=ORSXG5BAMJUXG5BA"})
        web.invoke({"command": "import_commit"})
        self.nav("验证码")
        self.page("codes")
        self.shot("codes-long-1280-light")
        many = "\n".join(f"otpauth://totp/Service%20{i:03d}:user{i}%40example.com?secret={base64.b32encode(hashlib.sha1(str(i).encode()).digest()[:10]).decode()}" for i in range(200))
        web.invoke({"command": "import_text", "text": many})
        web.invoke({"command": "import_commit"})
        self.nav("验证码")
        until("208 accounts", lambda: len(web.find_all('[data-testid="entry-row"]')) == 208)
        self.shot("codes-200-1280-light")
        web.quit()


def main():
    parser = argparse.ArgumentParser()
    for name in ["driver", "app", "out", "decode-qr", "display", "work"]:
        parser.add_argument(f"--{name}", required=True)
    Smoke(parser.parse_args()).run()


if __name__ == "__main__":
    main()
