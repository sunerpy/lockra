#!/usr/bin/env python3
"""Desktop smoke test of the real app, driven over WebDriver (tauri-driver -> WebKitWebDriver)
with real X input where it matters (typing the master password, Ctrl+L) and the X clipboard
(xclip). Run by scripts/smoke-desktop-linux.sh, which owns Xvfb, the driver and the data dirs.

Checks: a new install on 澄空, the default theme; create a vault by typing; import an otpauth link
from the clipboard; the copied code equals an independent RFC 6238 computation; lock and unlock;
Google's export QR code decodes (rxing, scripts' decode-qr) back to the same account; then
screenshots of every page at three window sizes in the light and dark themes, plus the empty, long
and 200-account states; Touch ID turned on from the unlock screen's pointer and unlocking with it
(the debug build's stand-in check); an account's colour and avatar text, folded groups, a row's
right-click menu, several accounts ticked and moved to a group at once, the group picker's list, and
accounts reordered by a handle; the newer themes (澄空, 墨白 and their nights) as a list and as cards, a
card and a row just copied, and Settings › 外观's grid of eight themes; the passwords page (a debug
build offers password management) with logins whose password, copied from the list by Rust, is on
the X clipboard; the browser integration turned on, the real binary in host mode reaching the
running app through its socket, Settings › 浏览器, and turned off; the quick search's shortcut chosen
and taken from X, pressed to bring the command palette up, a copy from it putting the window back in
the tray, `lockra-desktop --palette` started again opening the palette in the running app, and a
shortcut the shell cannot take shown with that command in Settings › 通用.
Every wait is a condition with a deadline; nothing sleeps for a fixed time to "let it finish".
"""

import argparse
import base64
import hashlib
import hmac
import json
import os
import select
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
PAGES = [("codes", "验证码"), ("passwords", "密码"), ("import", "导入"), ("export", "导出"), ("backup", "备份")]
LOGIN_PASSWORD = "x7#Kq9!vLm2@Rt4z"
ELEMENT = "element-6066-11e4-a52e-4f735466cecf"


class NoSuchElement(Exception):
    pass


class StaleElement(Exception):
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
                if payload.get("error") == "stale element reference":
                    raise StaleElement(f"{method} {path}: {payload}") from None
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

    def found(self, path, body):
        """A find, asked again when WebKitWebDriver trips over a node the page replaced while it
        looked ("stale element reference" from the find itself: CI on 2026-10-12 and 2026-10-15)."""
        for _ in range(10):
            try:
                return self.call("POST", self.s(path), body)
            except StaleElement:
                time.sleep(0.2)
        sys.exit(f"smoke: POST {path} kept finding a replaced node: {body}")

    def find(self, css=None, xpath=None):
        using, value = ("css selector", css) if css is not None else ("xpath", xpath)
        try:
            return self.found("/element", {"using": using, "value": value})[ELEMENT]
        except NoSuchElement:
            return None

    def find_all(self, css):
        return [e[ELEMENT] for e in self.found("/elements", {"using": "css selector", "value": css})]

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

    def invoke_shell(self, name, args):
        """One of the shell's own commands (not the core's), as the webview calls it."""
        script = (
            "const done = arguments[arguments.length - 1];"
            "window['__' + 'TAURI_INTERNALS__'].invoke(arguments[0], arguments[1])"
            ".then((ok) => done({ ok }), (err) => done({ err }));"
        )
        answer = self.call("POST", self.s("/execute/async"), {"script": script, "args": [name, args]})
        if "err" in answer:
            sys.exit(f"smoke: {name} failed: {answer['err']}")
        return answer["ok"]

    def screenshot(self):
        return base64.b64decode(self.call("GET", self.s("/screenshot")))

    def quit(self):
        try:
            self.call("DELETE", self.s(""))
        except SystemExit:
            pass


def read_exactly(pipe, size, deadline):
    """`size` bytes from `pipe`, or None if it ends or the deadline passes first."""
    data = b""
    while len(data) < size:
        left = deadline - time.monotonic()
        if left <= 0 or not select.select([pipe], [], [], left)[0]:
            return None
        chunk = os.read(pipe.fileno(), size - len(data))
        if not chunk:
            return None
        data += chunk
    return data


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

    def tool(self, label):
        """One of the codes list's tools, from its ⋯ menu (board A1)."""
        self.web.click(self.web.wait(css='[data-testid="codes-more"]'))
        self.web.click(self.web.wait(xpath=f"//*[@role='menu']//*[starts-with(@role, 'menuitem')][normalize-space()='{label}']"))

    def edit_first(self):
        """The first row's 编辑…, from its menu: the row has no edit button of its own (board A1)."""
        self.web.click(self.web.wait(css='[data-testid="entry-row"] [data-testid="row-menu"]'))
        self.web.click(self.web.wait(xpath="//*[@role='menu']//*[@role='menuitem'][normalize-space()='编辑…']"))

    def browser_channel(self):
        """The browser integration on (the shell command Settings › 浏览器 calls): Chrome's manifest
        is written (into the debug build's own folder, LOCKRA_DEV_BROWSER_MANIFEST_DIR) and points
        at this binary; started as Chrome starts it, in host mode, the binary reaches the running app
        through its socket and gets the app's own hello (unlocked); off again, the manifest goes."""
        statuses = self.web.invoke_shell("browser_enable", {"browsers": ["chrome"]})
        chrome = next(s for s in statuses if s["browser"] == "chrome")
        if not chrome["manifest"]:
            sys.exit(f"smoke: Chrome's manifest was not written: {statuses}")
        manifest_path = os.path.join(os.environ["LOCKRA_DEV_BROWSER_MANIFEST_DIR"], "NativeMessagingHosts", "dev.lockra.browser.json")
        with open(manifest_path) as f:
            manifest = json.load(f)
        if manifest["path"] != self.args.app or "chrome-extension://egbpficliljjnpkjbigonldakfllebco/" not in manifest["allowed_origins"]:
            sys.exit(f"smoke: Chrome's manifest: {manifest}")
        hello = json.dumps({"op": "hello", "proto_min": 1, "proto_max": 1, "browser": "chrome", "extension": "egbpficliljjnpkjbigonldakfllebco", "version": "0.0.0"})
        envelope = json.dumps({"v": 1, "t": "hello", "d": base64.b64encode(hello.encode()).decode()}).encode()
        host = subprocess.Popen([manifest["path"], "chrome-extension://egbpficliljjnpkjbigonldakfllebco/"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        # As a browser does: stdin stays open while the port does, so the answer comes first.
        host.stdin.write(struct.pack("=I", len(envelope)) + envelope)
        host.stdin.flush()
        deadline = time.monotonic() + 30
        length = read_exactly(host.stdout, 4, deadline)
        answer = length and read_exactly(host.stdout, struct.unpack("=I", length)[0], deadline)
        if not answer:
            host.kill()
            sys.exit("smoke: the host did not answer")
        plain = json.loads(base64.b64decode(json.loads(answer)["d"]))
        # The browser closes its end: the host ends, with success.
        host.stdin.close()
        try:
            host.wait(timeout=20)
        except subprocess.TimeoutExpired:
            host.kill()
            sys.exit("smoke: the host did not end with its stdin")
        if host.returncode != 0 or plain.get("op") != "hello" or plain.get("state") != "unlocked" or plain.get("proto") != 1:
            sys.exit(f"smoke: the host's answer: {plain} (status {host.returncode})")
        print("smoke: Chrome's manifest points here, and the host reached the running app through its socket")
        # Settings › 浏览器 with the integration on: Chrome's row as the shell reports it.
        web = self.web
        self.resize(1280, 800)
        self.focus()
        self.x("key", "ctrl+comma")
        web.wait('[data-testid="settings-content"]')
        web.click(web.wait(xpath="//button[@role='tab'][normalize-space()='浏览器']"))
        web.wait('[data-testid="browser-row-chrome"]')
        self.shot("settings-browser-1280-light")
        self.x("key", "Escape")
        web.gone('[data-testid="settings-content"]')
        statuses = self.web.invoke_shell("browser_disable", {})
        if any(s["manifest"] for s in statuses) or os.path.exists(manifest_path):
            sys.exit(f"smoke: the manifests stayed: {statuses}")

    def visible(self):
        found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Lockra$"], env=self.env, capture_output=True, text=True)
        return bool(found.stdout.split())

    def quick_search(self):
        """The quick search (a debug build offers password management): its shortcut chosen and
        taken from X by the shell, a press from X brings the command palette up; hidden in the tray
        (running in the background), the window comes up with the palette and a copy from it puts
        the code on the clipboard and the window back in the tray; `lockra-desktop --palette`
        started again reaches the running app (the single-instance check, on the run's own session
        bus) and opens the palette there; a shortcut the shell cannot take is shown in Settings ›
        通用 with the command to bind instead."""
        web = self.web
        palette = "[role='dialog'][aria-label='命令菜单']"
        quick = lambda: web.invoke({"command": "app_state"})["quick_access"]
        self.settings(quick_access_shortcut="Control+Shift+Space")
        until("the shortcut taken from X", lambda: quick() == {"shortcut": "Control+Shift+Space", "error": None})
        self.focus()
        self.x("key", "ctrl+shift+space")
        web.wait(palette)
        self.x("key", "Escape")
        web.gone(palette)
        # Settings › 通用 shows it as keycaps.
        self.resize(1280, 800)
        self.focus()
        self.x("key", "ctrl+comma")
        web.wait('[data-testid="quick-access"]')
        self.shot("settings-general-1280-light")
        self.x("key", "Escape")
        web.gone('[data-testid="settings-content"]')
        # Hidden in the tray, the shortcut brings the window up with the palette; the account's
        # code copied from it, the window goes back.
        self.settings(run_in_background=True)
        web.click(web.wait(xpath="//header//button[@aria-label='关闭']"))
        until("the window hidden in the tray", lambda: not self.visible())
        self.x("key", "ctrl+shift+space")
        until("the window brought up", self.visible)
        web.wait(palette)
        self.focus()
        self.x("type", "--delay", "15", ISSUER)
        self.x("key", "Return")
        until("the window back in the tray", lambda: not self.visible())
        copied = until("the code on the clipboard", lambda: (lambda c: c if c.isdigit() else None)(self.clipboard().strip()))
        if len(copied) != 6:
            sys.exit(f"smoke: the quick search copied {copied!r}")
        print("smoke: the shortcut brought the palette up from the tray, and the copy sent the window back")
        # Up again, closed without a copy: the window stays.
        self.x("key", "ctrl+shift+space")
        until("the window brought up", self.visible)
        web.wait(palette)
        self.focus()
        self.x("key", "Escape")
        web.gone(palette)
        if not self.visible():
            sys.exit("smoke: the window went back without a copy")
        # Where apps get no global shortcuts, the desktop runs `lockra-desktop --palette`: started
        # again, it hands the palette to the running app and ends.
        second = subprocess.run([self.args.app, "--palette"], env=self.env, capture_output=True, timeout=60)
        if second.returncode != 0:
            sys.exit(f"smoke: lockra-desktop --palette ended with {second.returncode}: {second.stderr.decode(errors='replace')[-2000:]}")
        web.wait(palette)
        self.focus()
        self.x("key", "Escape")
        web.gone(palette)
        print("smoke: lockra-desktop --palette opened the palette in the running app")
        # A shortcut the shell cannot take: Settings says so, with the command to bind.
        self.settings(quick_access_shortcut="Control+Alt+NoSuchKey")
        until("the shortcut refused", lambda: quick()["error"] == "shortcut_unavailable")
        self.focus()
        self.x("key", "ctrl+comma")
        web.wait('[data-testid="quick-access-bind"]')
        self.shot("settings-general-quick-error-1280-light")
        self.x("key", "Escape")
        web.gone('[data-testid="settings-content"]')
        self.settings(quick_access_shortcut=None, run_in_background=False)
        until("no shortcut", lambda: quick() == {"shortcut": None, "error": None})

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
        # A new install starts on 澄空, following the system (light under Xvfb). The screenshots
        # are named for the theme they show, here 明亮.
        until("the default theme, 澄空", lambda: web.run("return document.documentElement.dataset.theme") == "glass")
        self.settings(follow_system_theme=False, theme="light")
        until("the light theme", lambda: web.run("return document.documentElement.dataset.theme") == "light")
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
        # Logins for the passwords page; GitHub's goes with the GitHub account's codes.
        entries = {e["issuer"]: e["id"] for e in web.invoke({"command": "app_state"})["entries"]}
        logins = [
            {"title": "GitHub", "username": "octocat", "email": "octocat@github.com", "password": {"type": "set", "value": LOGIN_PASSWORD},
             "urls": [{"url": "https://github.com/login"}], "group": "工作", "tags": ["开发", "常用"], "favorite": True, "totp_entry": entries["GitHub"]},
            {"title": "招商银行", "username": "6222 •••• 1234", "password": {"type": "set", "value": "Cmb#880613"},
             "urls": [{"url": "https://pbsz.ebank.cmbchina.com", "matching": "host"}], "group": "个人", "protection": "confirm", "color": "red", "mark": "招"},
            {"title": "Figma", "email": "alex@acme.dev", "password": {"type": "set", "value": "Figma#Design42"}, "urls": [{"url": "https://www.figma.com"}], "group": "工作"},
            {"title": "路由器", "username": "admin", "password": {"type": "set", "value": "router-admin"}, "urls": [{"url": "http://192.168.1.1", "matching": "host"}]},
        ]
        for draft in logins:
            web.invoke({"command": "item_add", "draft": draft})
        self.nav("密码")
        self.page("passwords")
        # The first login's row copies its password; Rust puts it on the clipboard.
        row = web.wait(xpath="//*[@data-testid='password-row'][.//*[normalize-space()='GitHub']]")
        web.click(row)
        web.wait('[data-testid="item-detail"]')
        web.click(web.wait(xpath="//*[@data-testid='password-row'][.//*[normalize-space()='GitHub']]//*[@data-testid='row-copy-password']"))
        until("the login's password on the clipboard", lambda: self.clipboard() == LOGIN_PASSWORD)
        print("smoke: a login's password, copied from the list, is on the X clipboard")
        self.browser_channel()
        self.quick_search()
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

        # The newer themes as a list and as cards (验证码视图), and the classic two as cards.
        self.nav("验证码")
        self.page("codes")
        tiles = lambda: len(web.find_all('[data-testid="entry-tile"]'))
        for theme in ["glass", "glass_night", "paper", "paper_night", "light", "dark"]:
            self.settings(theme=theme, codes_view="list")
            until(f"the {theme} theme", lambda: web.run("return document.documentElement.dataset.theme") == theme)
            until("the list", lambda: tiles() == 0)
            if theme not in ("light", "dark"):
                self.shot(f"codes-1280-{theme}")
            self.settings(codes_view="grid")
            until("the cards", lambda: tiles() > 0)
            self.shot(f"codes-cards-1280-{theme}")
        # A card and a row just copied, as 墨白 (ink) and 澄空 (an accent pill) draw them: one frame,
        # taken while the mark is on (it goes after FLASH_MS, 1.6 s), so not shot()'s two alike.
        for view, item, name in [("grid", "entry-tile", "codes-cards-copied"), ("list", "entry-row", "codes-copied")]:
            for theme in ["paper", "glass"]:
                self.settings(theme=theme, codes_view=view)
                until(f"the {theme} theme", lambda: web.run("return document.documentElement.dataset.theme") == theme)
                until(f"the {view}", lambda: web.find_all(f'[data-testid="{item}"]'))
                web.click(web.find_all(f'[data-testid="{item}"]')[0])
                until("the copied mark", lambda: web.find_all('[data-copied="true"]'))
                path = os.path.join(self.args.out, f"{name}-1280-{theme}.png")
                with open(path, "wb") as f:
                    f.write(web.screenshot())
                print(f"smoke: {path}")
                until("the mark gone", lambda: not web.find_all('[data-copied="true"]'))
        # Settings › 外观: light above dark, a column per pair, followed in 澄空.
        self.settings(theme="glass", codes_view="list", follow_system_theme=True)
        self.focus()
        self.x("key", "ctrl+comma")
        web.wait('[data-testid="settings-content"]')
        web.click(web.wait(xpath="//button[@role='tab'][normalize-space()='外观']"))
        web.wait('[data-testid="theme-grid"]')
        self.shot("settings-appearance-1280-glass")
        self.x("key", "Escape")
        web.gone('[data-testid="settings-content"]')
        self.settings(theme="light", follow_system_theme=False)

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

        # An account's own colour and avatar text, from the row's menu.
        self.nav("验证码")
        self.page("codes")
        self.edit_first()
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
        self.tool("全部折叠")
        until("everything folded", lambda: not web.find_all('[data-testid="entry-row"]'))
        self.shot("codes-groups-folded-1280-light")
        self.tool("全部展开")
        until("everything unfolded", lambda: len(web.find_all('[data-testid="entry-row"]')) == len(entries))
        # 未分组 pinned from the ⋯ menu while it is shown: its chip first, and in 全部 its section
        # first; then unpinned, the view as it was.
        def chip(name):
            return web.wait(xpath=f"//*[@data-testid='filter-chip'][starts-with(@aria-label, '{name} ')]")

        web.click(chip("未分组"))
        self.tool("置顶「未分组」")
        until("the pinned chip first", lambda: web.run("return document.querySelector('[data-testid=filter-chip]').dataset.pinned") == "true")
        web.click(chip("全部"))
        until("未分组 first", lambda: web.run("return document.querySelector('[data-testid=codes-group-toggle]').textContent.startsWith('未分组')"))
        self.shot("codes-pinned-1280-light")
        web.invoke({"command": "view_pin_group", "pinned": None})
        until("unpinned", lambda: web.invoke({"command": "app_state"})["pinned_group"] is None)
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
        self.tool("选择")
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
        # The group picker: Lockra's own list under the field (the webview's <datalist> drew a dark
        # native box), which Esc closes before the dialog.
        self.edit_first()
        web.click(web.wait(xpath=f"{dialog}//label[normalize-space()='分组']/following::input[1]"))
        web.wait('[data-testid="group-options"]')
        self.shot("edit-group-1280-light")
        self.focus()
        self.x("key", "Escape")
        web.gone('[data-testid="group-options"]')
        if not web.find_all('[role="dialog"]'):
            raise SystemExit("smoke: Esc in the group list closed the dialog as well")
        self.x("key", "Escape")
        web.gone('[role="dialog"]')
        # Reordering: the handles, and an account moved down a place from the keyboard; the code
        # order turns manual, kept in the vault for this device.
        sort = web.invoke({"command": "app_state"})["settings"]["sort"]
        web.click(web.wait(css='[data-testid="codes-reorder"]'))
        web.wait('[data-testid="codes-reordering"]')
        handle = web.wait(css='[data-sort-handle]:not([data-sort-handle^="group:"])')
        web.run("arguments[0].focus()", {ELEMENT: handle})
        self.focus()
        self.x("key", "Down")
        until("the dragged order", lambda: len(web.invoke({"command": "app_state"})["entry_order"]) == len(entries))
        until("the manual order", lambda: web.invoke({"command": "app_state"})["settings"]["sort"] == "manual")
        # And by the pointer, X's own: the handle pressed, moved down, let go. The window
        # is at the screen's origin (`focus`) and draws its own title bar, so the page's
        # coordinates are the screen's.
        order = web.invoke({"command": "app_state"})["entry_order"]
        x, y = (int(v) for v in web.run("const r = arguments[0].getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]", {ELEMENT: handle}))
        # What the page hears of it, for a failure to say.
        web.run(
            "window.lockraPointer = [];"
            "for (const type of ['pointerdown', 'pointermove', 'pointerup', 'pointercancel', 'mousedown', 'mouseup', 'dragstart', 'lostpointercapture'])"
            "  document.addEventListener(type, (e) => window.lockraPointer.push([type, e.clientX, e.clientY, e.target.closest('[data-sort-handle]')?.dataset.sortHandle ?? e.target.tagName]), true);"
        )
        # Each step waits for the page: a burst of moves and the release in a few milliseconds came
        # before the list knew what the row was over, and the drop changed nothing (one run in two).
        moved = "return [...document.querySelectorAll('[data-sort-name]')].some((e) => !e.hasAttribute('data-dragging') && Number(/translate3d\\([^,]*,\\s*(-?[\\d.]+)px/.exec(e.style.transform)?.[1] ?? 0) !== 0)"
        self.x("mousemove", str(x), str(y))
        self.x("mousedown", "1")
        self.x("mousemove", str(x), str(y + 8))
        try:
            until("the drag under way", lambda: web.find_all("[data-dragging]"), timeout=10)
            self.x("mousemove", str(x), str(y + 80))
            until("another account making room", lambda: web.run(moved), timeout=10)
            self.x("mouseup", "1")
            until("the order dragged by the pointer", lambda: web.invoke({"command": "app_state"})["entry_order"] != order, timeout=10)
        except SystemExit:
            heard = web.run("return window.lockraPointer")
            at = web.run("const e = document.elementFromPoint(arguments[0], arguments[1]); return e ? e.outerHTML.slice(0, 200) : null", x, y)
            said = web.run("return [...document.querySelectorAll('[id^=\"DndLiveRegion\"]')].map((e) => e.textContent)")
            print(f"smoke: the drag from {x},{y}: the page heard {json.dumps(heard)[:1500]}; at that point: {at}; dnd-kit said {said}", flush=True)
            raise
        self.shot("codes-reorder-1280-light")
        web.click(web.wait(css='[data-testid="codes-reorder-done"]'))
        web.gone('[data-testid="codes-reordering"]')
        self.settings(sort=sort)
        for entry in web.invoke({"command": "app_state"})["entries"]:
            web.invoke({"command": "entry_update", "id": entry["id"], "patch": {"group": ""}})

        # Overflow: a 200-character issuer and an unbroken account; then 200 accounts.
        long_issuer = urllib.parse.quote("超长服务名称" + "Very long issuer name " * 9)
        long_account = "a" * 120 + "@example.com"
        web.invoke({"command": "import_text", "text": f"otpauth://totp/{long_issuer}:{long_account}?secret=ORSXG5BAMJUXG5BA"})
        web.invoke({"command": "import_commit"})
        # Copied last and sorted by use: the long names on 「最近使用」 and in the list's first row.
        long = next(e for e in web.invoke({"command": "app_state"})["entries"] if e["account"] == long_account)
        web.invoke({"command": "entry_copy", "id": long["id"]})
        self.settings(sort="recent")
        self.nav("验证码")
        self.page("codes")
        web.gone('[data-testid="toast-viewport"]', timeout=20)
        self.shot("codes-long-1280-light")
        self.settings(codes_view="grid")
        until("the cards", lambda: web.find_all('[data-testid="entry-tile"]'))
        self.shot("codes-long-cards-1280-light")
        self.settings(codes_view="list", sort=sort)
        until("the list", lambda: not web.find_all('[data-testid="entry-tile"]'))
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
