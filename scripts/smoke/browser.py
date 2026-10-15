#!/usr/bin/env python3
"""Browser smoke test of the extension (plan §八), run by scripts/smoke-browser-linux.sh, which owns
Xvfb, the drivers, the test sites and the shared home and runtime folders.

The real app (tauri-driver) gets a vault, two logins (login.test with a linked code, pay.test) and
the browser integration on; the browser (chromedriver or geckodriver) loads the development build of
the extension, with site access granted up front (a copy whose manifest asks for <all_urls>
outright: the request flow and its prompt are the unit tests' and the manual matrix's). Then:
pairing, the code compared on both sides and approved in the app; filling from the inline menu with
the keyboard; the linked code on the next page, in one field and in six; the extension's shortcut
on a sign-in in two pages (a real X key, which no WebDriver key reaches); the policy's limits (an
HTTP page for an HTTPS login, a frame of another site); the pages that try to hijack a click (made
transparent under the menu, covered by an element and by a popover opened after it, a frame of the
menu's page of their own); two thousand fields; a sign-up form's generated password; the save and
the update prompts changing the app's logins; and locking the app locking the extension.
Every wait is a condition with a deadline. The fixed pauses: the inline menu's own rule (a key or a
click counts only once the menu has kept still for 300 ms, content/clickjack.ts), and one bounded
look for an effect that must not come (a click on the page's own frame of the menu).
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
import zipfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from desktop import ELEMENT, Session, totp, until  # noqa: E402 - beside this file

PASSWORD = "smoke test pass phrase"
LOGIN_USER = "alice"
LOGIN_PASSWORD = "Correct-Horse-Battery-7"
LOGIN_SECRET = "JBSWY3DPEHPK3PXP"
PAY_USER = "carol"
PAY_PASSWORD = "Pay-Frame-Secret-3"
NEW_USER = "bob"
NEW_PASSWORD = "Fresh-Login-Secret-9"
CHROME_ID = "egbpficliljjnpkjbigonldakfllebco"
FIREFOX_ID = "lockra@firlab.app"
# The add-on's internal UUID, fixed so its pages have a known address (extensions.webextensions.uuids).
FIREFOX_UUID = "6b3e1f2a-4c5d-4e6f-8a9b-0c1d2e3f4a5b"
HOSTS = ["login.test", "shop.test", "pay.test", "signup.test", "otp.test", "many.test"]
# The inline menu counts a key or a click only once it has shown unmoved this long (clickjack.ts).
MENU_SETTLE = 0.45
ARROW_DOWN = "\ue015"
ENTER = "\ue007"
ESCAPE = "\ue00c"


class Browser(Session):
    """A W3C WebDriver session with a browser (chromedriver or geckodriver)."""

    def __init__(self, base, capabilities):  # noqa: D107 - not the app's session
        self.base = base
        self.id = self.call("POST", "/session", {"capabilities": {"alwaysMatch": capabilities}})["sessionId"]

    def go(self, url):
        self.call("POST", self.s("/url"), {"url": url})

    def keys(self, *keys):
        """Keys pressed and released one after the other, to whatever has the focus."""
        actions = []
        for key in keys:
            actions += [{"type": "keyDown", "value": key}, {"type": "keyUp", "value": key}]
        self.call("POST", self.s("/actions"), {"actions": [{"type": "key", "id": "keyboard", "actions": actions}]})

    def values(self, *selectors):
        return self.run("return arguments[0].map((s) => document.querySelector(s)?.value ?? null)", list(selectors))

    def menu_open(self):
        """Whether the extension's frame (the inline menu or a prompt) is in the top layer and has
        drawn itself (its height is the frame's own measure, 0 until its page has loaded)."""
        return self.run(
            "const host = document.querySelector('[popover]:popover-open');"
            "return !!host && host.getBoundingClientRect().height > 0;"
        )

    def menu_on_top(self):
        """Whether what is at the menu's middle is the menu itself (nothing the page put over it)."""
        return self.run(
            "const host = [...document.querySelectorAll('[popover]')].find((el) => el.id !== 'late' && el.matches(':popover-open'));"
            "if (!host) return false; const r = host.getBoundingClientRect();"
            "return document.elementsFromPoint(r.left + r.width / 2, r.top + r.height / 2)[0] === host;"
        )

    def click_at(self, x, y):
        """A real pointer click at the viewport's (x, y)."""
        self.call(
            "POST",
            self.s("/actions"),
            {
                "actions": [
                    {
                        "type": "pointer",
                        "id": "mouse",
                        "parameters": {"pointerType": "mouse"},
                        "actions": [
                            {"type": "pointerMove", "x": x, "y": y, "origin": "viewport"},
                            {"type": "pointerDown", "button": 0},
                            {"type": "pointerUp", "button": 0},
                        ],
                    }
                ]
            },
        )

    def title(self):
        return self.call("GET", self.s("/title"))

    def menu_debug(self):
        return self.run(
            "const host = document.querySelector('[popover]:popover-open');"
            "return host ? { rect: host.getBoundingClientRect().toJSON(), style: host.getAttribute('style') } : null;"
        )


class Smoke:
    def __init__(self, args):
        self.args = args
        self.chrome = args.browser == "chrome"
        self.app = None
        self.web = None

    def url(self, host, path, secure=True):
        port = self.args.https_port if secure else self.args.http_port
        return f"{'https' if secure else 'http'}://{host}:{port}/{path}"

    def extension_page(self, page):
        return f"chrome-extension://{CHROME_ID}/{page}" if self.chrome else f"moz-extension://{FIREFOX_UUID}/{page}"

    def shot(self, name):
        path = os.path.join(self.args.out, f"{self.args.browser}-{name}.png")
        with open(path, "wb") as f:
            f.write(self.web.screenshot())
        print(f"smoke: {path}")

    # ---- setup --------------------------------------------------------------------------------

    def start_app(self):
        """A vault with a login for login.test, and the integration on for this browser."""
        self.app = Session(self.args.driver, self.args.app)
        until("the app's page", lambda: self.app.run("return document.readyState") == "complete", 60)
        # The browser before this one left its vault (locked, at the end of its run): start afresh.
        if self.app.invoke({"command": "app_state"})["phase"] == "locked":
            self.app.invoke({"command": "vault_reset"})
        self.app.invoke({"command": "vault_create", "password": PASSWORD})
        self.app.invoke({"command": "entry_add_uri", "uri": f"otpauth://totp/Login%20test:{LOGIN_USER}?secret={LOGIN_SECRET}&issuer=Login%20test"})
        code = next(e["id"] for e in self.app.invoke({"command": "app_state"})["entries"] if e["issuer"] == "Login test")
        self.app.invoke(
            {
                "command": "item_add",
                "draft": {
                    "title": "Login test",
                    "username": LOGIN_USER,
                    "password": {"type": "set", "value": LOGIN_PASSWORD},
                    "urls": [{"url": self.url("login.test", "")}],
                    "totp_entry": code,
                },
            }
        )
        self.app.invoke(
            {
                "command": "item_add",
                "draft": {
                    "title": "Pay test",
                    "username": PAY_USER,
                    "password": {"type": "set", "value": PAY_PASSWORD},
                    "urls": [{"url": self.url("pay.test", "")}],
                },
            }
        )
        statuses = self.app.invoke_shell("browser_enable", {"browsers": [self.args.browser]})
        if not next(s for s in statuses if s["browser"] == self.args.browser)["manifest"]:
            sys.exit(f"smoke: no manifest for {self.args.browser}: {statuses}")
        print("smoke: the app has a vault, a login for login.test and the integration on")

    def extension(self):
        """The development build, with site access asked for outright (a copy in the run's folder)."""
        source = os.path.join(self.args.extension, "chromium-dev" if self.chrome else "firefox")
        target = os.path.join(self.args.work, f"extension-{self.args.browser}")
        shutil.rmtree(target, ignore_errors=True)
        shutil.copytree(source, target)
        path = os.path.join(target, "manifest.json")
        with open(path) as f:
            manifest = json.load(f)
        manifest["host_permissions"] = ["<all_urls>"]
        manifest.pop("optional_host_permissions", None)
        with open(path, "w") as f:
            json.dump(manifest, f, indent=2)
        if self.chrome:
            return target
        packed = target + ".xpi"
        with zipfile.ZipFile(packed, "w", zipfile.ZIP_DEFLATED) as archive:
            for folder, _, files in os.walk(target):
                for name in sorted(files):
                    full = os.path.join(folder, name)
                    archive.write(full, os.path.relpath(full, target))
        return packed

    def start_browser(self):
        extension = self.extension()
        if self.chrome:
            capabilities = {
                "browserName": "chrome",
                "acceptInsecureCerts": True,
                "goog:chromeOptions": {
                    "binary": self.args.binary,
                    "prefs": {"credentials_enable_service": False, "profile.password_manager_enabled": False, "autofill.profile_enabled": False},
                    "args": [
                        f"--user-data-dir={os.path.join(self.args.work, 'chrome-profile')}",
                        f"--load-extension={extension}",
                        f"--disable-extensions-except={extension}",
                        "--host-resolver-rules=MAP *.test 127.0.0.1",
                        # The test sites are this machine's: never through a proxy of the environment.
                        "--no-proxy-server",
                        "--no-first-run",
                        "--no-default-browser-check",
                        "--disable-search-engine-choice-screen",
                        "--lang=en-US",
                        "--window-size=1280,900",
                        "--enable-logging",
                        f"--log-file={os.path.join(self.args.work, 'chrome.log')}",
                        "--v=0",
                        *(["--no-sandbox"] if self.args.no_sandbox else []),
                    ],
                },
            }
        else:
            capabilities = {
                "browserName": "firefox",
                "acceptInsecureCerts": True,
                "moz:firefoxOptions": {
                    "binary": self.args.binary,
                    "args": ["-width", "1280", "-height", "900"],
                    "prefs": {
                        "network.dns.localDomains": ",".join(HOSTS),
                        "network.proxy.type": 0,
                        "extensions.webextensions.uuids": json.dumps({FIREFOX_ID: FIREFOX_UUID}),
                        "intl.locale.requested": "en-US",
                    },
                },
            }
        self.web = Browser(self.args.webdriver, capabilities)
        if not self.chrome:
            self.web.call("POST", self.web.s("/moz/addon/install"), {"path": extension, "temporary": True})
        else:
            self.chrome_shortcut()
        print(f"smoke: {self.args.browser} runs the extension")

    def chrome_shortcut(self):
        """Chrome leaves the fill command without its suggested key when the extension is loaded
        from the command line (`commands.getAll` says ""; the action's key it does assign): set it
        the way chrome://extensions/shortcuts does."""
        self.web.go(self.extension_page("options.html"))
        commands = self.web.call(
            "POST",
            self.web.s("/execute/async"),
            {"script": "const done = arguments[arguments.length - 1]; chrome.commands.getAll().then(done);", "args": []},
        )
        if next(c for c in commands if c["name"] == "fill-login")["shortcut"]:
            return
        self.web.go("chrome://extensions/shortcuts")
        answer = self.web.call(
            "POST",
            self.web.s("/execute/async"),
            {
                "script": "const done = arguments[arguments.length - 1];"
                "chrome.developerPrivate.updateExtensionCommand({ extensionId: arguments[0], commandName: 'fill-login', keybinding: 'Ctrl+Shift+L' })"
                ".then(() => done('ok'), (e) => done(String(e && e.message || e)));",
                "args": [CHROME_ID],
            },
        )
        if answer != "ok":
            sys.exit(f"smoke: Chrome did not take Ctrl+Shift+L for the fill command: {answer}")
        print("smoke: Chrome had left the fill command without a key; set to Ctrl+Shift+L")

    # ---- the flows ----------------------------------------------------------------------------

    def open_extension_page(self, page):
        """The extension's `page` in the current tab. Firefox lets no content navigation reach a
        moz-extension:// page: its own browser chrome loads it, as the toolbar button would."""
        if self.chrome:
            self.web.go(self.extension_page(page))
            return
        self.web.call("POST", self.web.s("/moz/context"), {"context": "chrome"})
        try:
            self.web.run(
                "const browser = Services.wm.getMostRecentWindow('navigator:browser').gBrowser;"
                "browser.loadURI(Services.io.newURI(arguments[0]), {"
                " triggeringPrincipal: Services.scriptSecurityManager.getSystemPrincipal() });",
                self.extension_page(page),
            )
        finally:
            self.web.call("POST", self.web.s("/moz/context"), {"context": "content"})
        until(f"the extension's {page}", lambda: self.web.call("GET", self.web.s("/url")).startswith("moz-extension://"), 15)

    def popup_state(self):
        element = self.web.find('[data-testid="popup-state"]')
        return None if element is None else self.web.text(element)

    def pair(self):
        """Connect from the popup; the code the popup shows is the one the app shows; approved there."""
        self.open_extension_page("popup.html")
        until("the popup to offer pairing", lambda: self.popup_state() == "Not connected", 60)
        self.web.click(self.web.wait(xpath="//button[normalize-space()='Connect']"))
        code = until(
            "the popup's pairing code",
            lambda: (lambda c: c if c and "·" not in c else None)(self.web.text(self.web.wait('[data-testid="pairing-code"]'))),
            60,
        )
        shown = until("the app's pairing dialog", lambda: self.app.find('[data-testid="pair-code"]'), 60)
        if self.app.text(shown) != code:
            sys.exit(f"smoke: the app shows {self.app.text(shown)}, the popup {code}")
        dialog = '//*[@role="dialog"][.//*[@data-testid="pair-code"]]'
        self.app.type(self.app.wait(xpath=f"{dialog}//input[@type='password']"), PASSWORD)
        self.app.click(self.app.wait(xpath=f"{dialog}//button[normalize-space()='批准']"))
        until("the popup to say Lockra is unlocked", lambda: self.popup_state() == "Unlocked", 60)
        print(f"smoke: paired; both sides showed {code}")

    def fill_from_menu(self):
        """The login page: the field's focus opens the menu, ↓ and Enter fill the login."""
        self.web.go(self.url("login.test", "login.html"))
        self.open_menu("#username")
        self.shot("menu")
        self.web.keys(ARROW_DOWN, ENTER)
        until(
            "the login filled from the menu",
            lambda: self.web.values("#username", "#password") == [LOGIN_USER, LOGIN_PASSWORD],
            30,
        )
        print("smoke: the inline menu filled the login with the keyboard")

    def activity(self):
        return self.app.invoke({"command": "app_state"})["browser"]["activity"]

    def items(self):
        return self.app.invoke({"command": "items_list"})

    def focus(self, selector):
        """The field focused the way a click does, without a click (a page may cover it)."""
        self.web.run("document.querySelector(arguments[0]).focus()", selector)

    def x_key(self, keys):
        """A real X key to the browser's window: the extension's shortcut reaches the browser only
        this way (no WebDriver or DevTools key does)."""
        title = re.escape(self.web.title())
        window = until(
            "the browser's X window",
            lambda: (subprocess.run(["xdotool", "search", "--onlyvisible", "--name", title], capture_output=True, text=True).stdout.split() or [None])[0],
            15,
        )
        subprocess.run(["xdotool", "windowfocus", "--sync", window], check=True)
        subprocess.run(["xdotool", "key", "--clearmodifiers", keys], check=True)

    def open_menu(self, selector, click=True):
        """Focus `selector`; the menu has drawn itself and shown unmoved long enough to count."""
        if click:
            self.web.click(self.web.wait(selector))
        else:
            self.web.wait(selector)
            self.focus(selector)

        since = time.monotonic()

        def shown():
            nonlocal since
            if self.web.menu_open():
                return True
            # The content script may start after the field was focused (it runs at document_idle and
            # asks the background first): after a second with no menu at all, focus it again, as a
            # user clicking once more would. A menu on its way is left alone (another focus would
            # make another).
            if time.monotonic() - since > 1.0 and not self.web.run("return !!document.querySelector('[popover]:popover-open')"):
                self.web.run("document.activeElement?.blur(); document.querySelector(arguments[0]).focus()", selector)
                since = time.monotonic()
            return False

        try:
            until("the inline menu", shown, 15)
        except SystemExit:
            print(f"smoke: the menu's host: {self.web.menu_debug()}; active: {self.web.run('return document.activeElement && document.activeElement.id')}")
            raise
        self.settle()

    def settle(self):
        """The menu has kept its place and size for longer than its rule asks (it measures itself
        again as its content arrives, and every move restarts its 300 ms)."""
        rect = lambda: self.web.run(
            "const host = document.querySelector('[popover]:popover-open');"
            "return host ? JSON.stringify(host.getBoundingClientRect()) : null;"
        )

        def still():
            before = rect()
            time.sleep(MENU_SETTLE)
            return before is not None and rect() == before

        until("the menu to hold still", still, 15, step=0)

    def otp_follows(self):
        """Right after its login filled, the site's next page gets the linked code by itself: in
        one field, and in six of one digit each."""
        self.web.go(self.url("login.test", "otp.html"))
        value = until("the code filled on the next page", lambda: (lambda v: v if v and len(v) == 6 else None)(self.web.values("#code")[0]), 30)
        self.is_code(value)
        self.web.go(self.url("login.test", "otp-split.html"))
        digits = [f"#d{i}" for i in range(1, 7)]
        value = until("the code filled digit by digit", lambda: (lambda v: "".join(v) if all(v) else None)(self.web.values(*digits)), 30)
        self.is_code(value)
        print("smoke: the next pages' code fields got the login's code, whole and digit by digit")

    def is_code(self, value):
        now = time.time()
        if value not in {totp(LOGIN_SECRET, now), totp(LOGIN_SECRET, now - 30)}:
            sys.exit(f"smoke: the code filled is {value}, not the login's")

    def shortcut_two_steps(self):
        """The extension's shortcut on a sign-in in two pages: the user name, then the password."""
        self.web.go(self.url("login.test", "two-step.html"))
        self.web.click(self.web.wait("#username"))
        if self.web.menu_open():
            self.web.keys(ESCAPE)
        self.x_key("ctrl+shift+l")
        until("the shortcut to fill the user name", lambda: self.web.values("#username") == [LOGIN_USER], 30)
        self.web.click(self.web.wait("#next"))
        self.web.click(self.web.wait("#password"))
        if self.web.menu_open():
            self.web.keys(ESCAPE)
        self.x_key("ctrl+shift+l")
        until("the shortcut to fill the password on the next page", lambda: self.web.values("#password") == [LOGIN_PASSWORD], 30)
        print("smoke: the shortcut filled a sign-in in two pages")

    def http_page(self):
        """An HTTPS login on an HTTP page: the menu warns, and nothing goes in without consent."""
        self.web.go(self.url("login.test", "login.html", secure=False))
        self.open_menu("#username")
        height = self.web.run("return document.querySelector('[popover]:popover-open').getBoundingClientRect().height")
        self.web.keys(ARROW_DOWN, ENTER)
        # The menu turns to its warning (it measures itself again); the fields stay empty.
        until(
            "the HTTP warning",
            lambda: self.web.run(
                "const host = document.querySelector('[popover]:popover-open');"
                "return !!host && host.getBoundingClientRect().height !== arguments[0];",
                height,
            ),
            10,
        )
        if self.web.values("#username", "#password") != ["", ""]:
            sys.exit("smoke: an HTTPS login was filled into an HTTP page without consent")
        self.shot("http-warning")
        self.web.keys(ESCAPE)
        print("smoke: an HTTP page got no HTTPS login without consent")

    def cross_site_frame(self):
        """A frame of another site: with the default policy, nothing is filled, the shortcut neither."""
        before = len(self.activity())
        self.web.go(self.url("shop.test", "shop.html"))
        frame = self.web.wait("#pay")
        self.web.call("POST", self.web.s("/frame"), {"id": {ELEMENT: frame}})
        self.web.click(self.web.wait("#username"))
        self.x_key("ctrl+shift+l")

        def refusal():
            # The activity is newest first: what came since this page.
            news = self.activity()
            return next((a for a in news[: len(news) - before] if a["event"] == "refused"), None)

        refused = until("the app to refuse the frame", refusal, 30)
        if refused["refusal"] != "cross_origin" or self.web.values("#username", "#password") != ["", ""]:
            sys.exit(f"smoke: the frame of another site: {refused}, {self.web.values('#username', '#password')}")
        self.web.call("POST", self.web.s("/frame"), {"id": None})
        print("smoke: a frame of another site got nothing (refused: cross_origin)")

    def hijack_pages(self):
        """Pages that try to steer a click into the menu get nothing filled."""
        # The page turns transparent once the menu is up: a real click on its first login fills
        # nothing, and the menu goes.
        self.web.go(self.url("login.test", "cj-opacity.html"))
        self.open_menu("#username")
        until("the page to hide itself", lambda: self.web.run("return document.documentElement.style.opacity === '0'"), 10)
        left, top = self.web.run(
            "const r = document.querySelector('[popover]:popover-open').getBoundingClientRect();"
            "return [Math.round(r.left), Math.round(r.top)];"
        )
        # The first row: under the 36 px header, 52 px high (board 33).
        self.web.click_at(left + 120, top + 62)
        until("the menu to close on a hidden page", lambda: not self.web.menu_open(), 10)
        if self.web.values("#username", "#password") != ["", ""]:
            sys.exit("smoke: a transparent page got a login filled")
        print("smoke: a transparent page got nothing filled")
        # An element over everything and a popover opened after the menu: the menu comes back on top.
        self.web.go(self.url("login.test", "cj-cover.html"))
        self.open_menu("#username", click=False)
        until("the menu back on top of the page's popover", self.web.menu_on_top, 10)
        self.settle()
        self.web.keys(ARROW_DOWN, ENTER)
        until("the login filled with the menu on top", lambda: self.web.values("#username", "#password") == [LOGIN_USER, LOGIN_PASSWORD], 30)
        print("smoke: a page covering the menu did not stay over it")
        # The page frames the menu's page itself, without a token: a click on it fills nothing.
        fake = self.extension_page("inline.html")
        self.web.go(self.url("login.test", f"cj-fake.html?menu={fake}"))
        self.web.wait("#fake")
        x, y = self.web.run("const r = document.getElementById('fake').getBoundingClientRect(); return [Math.round(r.left + 40), Math.round(r.top + 40)];")
        self.web.click_at(x, y)
        # Nothing is to happen: the one bounded look for an effect that must not come.
        time.sleep(1.5)
        if self.web.values("#username", "#password") != ["", ""]:
            sys.exit("smoke: the page's own menu frame filled a login")
        print("smoke: the page's own frame of the menu filled nothing")

    def many_fields(self):
        """Two thousand fields: the menu still comes within the content script's budget."""
        self.web.go(self.url("login.test", "many.html"))
        start = time.monotonic()
        self.open_menu("#username")
        took = time.monotonic() - start - MENU_SETTLE
        self.web.keys(ARROW_DOWN, ENTER)
        until("the login filled on the long page", lambda: self.web.values("#username", "#password") == [LOGIN_USER, LOGIN_PASSWORD], 30)
        print(f"smoke: two thousand fields: the menu came in {took:.2f} s")

    def signup(self):
        """A sign-up form: the menu offers a password of the form's rules, saved in Lockra at once."""
        before = len(self.items())
        self.web.go(self.url("signup.test", "signup.html"))
        self.web.type(self.web.wait("#username"), "dana")
        self.open_menu("#password")
        self.web.keys(ARROW_DOWN, ENTER)
        password, _ = until("the generated password", lambda: (lambda v: v if v[0] and v[0] == v[1] else None)(self.web.values("#password", "#confirm")), 30)
        if len(password) < 20 or not re.search(r"[A-Z]", password) or not re.search(r"[0-9]", password):
            sys.exit(f"smoke: the generated password breaks the form's rules: {password!r}")
        until("the generated login saved in Lockra", lambda: len(self.items()) == before + 1, 30)
        print("smoke: a sign-up form got a password of its rules, saved in Lockra")

    def sign_in(self, username, password):
        """Type a sign-in on login.test by hand (the menu sent away) and send it."""
        self.web.go(self.url("login.test", "login.html"))
        # The site has a login: its menu comes, and goes with Esc.
        self.open_menu("#username")
        self.web.keys(ESCAPE)
        until("the menu to go", lambda: not self.web.menu_open(), 10)
        self.web.type(self.web.find("#username"), username)
        self.web.type(self.web.find("#password"), password)
        self.web.click(self.web.find("#submit"))
        self.web.wait("#welcome")

    def answer_prompt(self, name):
        """The prompt in the page's corner: its primary button (bottom right) clicked for real once
        it has shown unmoved long enough to count."""
        until(f"the {name} prompt", self.web.menu_open, 30)
        self.settle()
        self.shot(f"{name}-prompt")
        right, bottom = self.web.run(
            "const r = document.querySelector('[popover]:popover-open').getBoundingClientRect();"
            "return [Math.round(r.right), Math.round(r.bottom)];"
        )
        self.web.click_at(right - 30, bottom - 26)
        until(f"the {name} prompt to go", lambda: not self.web.menu_open(), 15)

    def reveal(self, item):
        return self.app.invoke({"command": "item_reveal", "id": item["id"], "parts": {"password": True}})["password"]

    def save_prompt(self):
        """A sign-in Lockra does not know: after it, the prompt saves it in the app; a known one
        with a new password: the prompt updates it."""
        before = len(self.items())
        self.sign_in(NEW_USER, NEW_PASSWORD)
        self.answer_prompt("save")
        saved = until(
            "the new login in Lockra",
            lambda: next((i for i in self.items() if i["username"] == NEW_USER), None),
            30,
        )
        if len(self.items()) != before + 1 or self.reveal(saved) != NEW_PASSWORD:
            sys.exit(f"smoke: the saved login: {saved}")
        print("smoke: the save prompt added the sign-in to Lockra")
        changed = "Changed-Horse-Battery-8"
        self.sign_in(LOGIN_USER, changed)
        self.answer_prompt("update")
        alice = next(i for i in self.items() if i["username"] == LOGIN_USER)
        until("the updated password in Lockra", lambda: self.reveal(alice) == changed, 30)
        print("smoke: the update prompt changed the login's password in Lockra")

    def lock_follows(self):
        """Locking the app locks the extension."""
        self.app.invoke({"command": "vault_lock"})
        self.open_extension_page("popup.html")
        until("the popup to say Lockra is locked", lambda: self.popup_state() == "Locked", 60)
        print("smoke: locking Lockra locked the extension")

    def run(self):
        os.makedirs(self.args.out, exist_ok=True)
        try:
            self.start_app()
            self.start_browser()
            self.pair()
            self.fill_from_menu()
            self.otp_follows()
            self.shortcut_two_steps()
            self.http_page()
            self.cross_site_frame()
            self.hijack_pages()
            self.many_fields()
            self.signup()
            self.save_prompt()
            self.lock_follows()
        except SystemExit:
            if self.app is not None:
                try:
                    state = self.app.invoke({"command": "app_state"})
                    print(f"smoke: the app's browser activity: {json.dumps(state['browser']['activity'], ensure_ascii=False)}")
                except SystemExit:
                    pass
            if self.web is not None:
                try:
                    self.shot("failure")
                except SystemExit:
                    pass
            raise
        finally:
            if self.web is not None:
                self.web.quit()
            if self.app is not None:
                self.app.quit()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--browser", choices=["chrome", "firefox"], required=True)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--webdriver", required=True)
    parser.add_argument("--driver", required=True)
    parser.add_argument("--app", required=True)
    parser.add_argument("--extension", required=True)
    parser.add_argument("--work", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--https-port", type=int, required=True)
    parser.add_argument("--http-port", type=int, required=True)
    parser.add_argument("--no-sandbox", action="store_true", help="Chrome without its sandbox (no user namespaces)")
    Smoke(parser.parse_args()).run()


if __name__ == "__main__":
    main()
