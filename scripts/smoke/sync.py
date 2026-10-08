#!/usr/bin/env python3
"""Two real Lockra apps syncing through a real S3 server, over WebDriver. scripts/smoke-sync-linux.sh
owns the server (the Versity S3 gateway in Docker), Xvfb, the drivers and two data folders, and runs
one phase per app start:

  a  device A: a vault with two accounts; Settings › Sync, fill in the S3 form and start syncing;
     the sync key is shown once and kept (in the work folder, for device B).
  b  device B, no vault: the welcome screen's "join sync" with the storage and the sync key; A's
     accounts arrive; B renames one, deletes the other and adds a third. B's first snapshot is
     fetched from the storage for phase c.
  c  device A again: unlock, the changes arrive, both devices are listed; then B's older snapshot
     is put back on the storage and A refuses it as a rollback.

Then the same through a Lockra relay (lockra-relay on 127.0.0.1, its data folder read back):

  r1 device C: the storage form starts on Lockra's built-in relay; C sets up a space on a relay of
     its own instead, and shows an invitation (the sealed text and its code kept for device D).
     The relay holds one snapshot, nothing readable, and the SHA-256 of the access token derived
     from the sync key (computed here apart), never the sync key.
  r2 device D, no vault: the welcome screen's "join sync" with C's invitation and code alone (no
     storage settings); C's accounts arrive; D adds one.
  r3 device C again: unlock, D's account arrives, both devices are listed.

Screenshots go to --out. Every wait is a condition with a deadline.
"""

import argparse
import base64
import datetime
import hashlib
import hmac
import json
import os
import sys
import time
import urllib.parse
import urllib.request
import uuid
import xml.etree.ElementTree as ET

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from desktop import ELEMENT, Session, until  # noqa: E402

# React keeps an input's value in its state: emptying it means the native setter and an input
# event, as typing would send.
CLEAR = (
    "const el = arguments[0];"
    "Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value').set.call(el, '');"
    "el.dispatchEvent(new Event('input', { bubbles: true }));"
)

# A native <select> chosen as a user would: React reads the value from the change event.
SELECT = (
    "const el = arguments[0];"
    "Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(el, arguments[1]);"
    "el.dispatchEvent(new Event('change', { bubbles: true }));"
)

PASSWORD = "smoke sync pass phrase"
BUCKET = "lockra-it"
PREFIX = "lockra"
REGION = "us-east-1"
GITHUB = "otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP&issuer=GitHub"
MAIL = "otpauth://totp/Mail:me%40example.com?secret=GEZDGNBVGY3TQOJQ&issuer=Mail"
BANK = "otpauth://totp/Bank:card?secret=MZXW6YTBOI&issuer=Bank"


class S3:
    """The few S3 calls the rollback needs, signed with AWS Signature Version 4."""

    def __init__(self, endpoint, access_key, secret_key):
        self.endpoint, self.access_key, self.secret_key = endpoint.rstrip("/"), access_key, secret_key

    def request(self, method, key, query="", body=None):
        url = urllib.parse.urlparse(f"{self.endpoint}/{BUCKET}/{key}")
        now = datetime.datetime.now(datetime.timezone.utc)
        stamp, day = now.strftime("%Y%m%dT%H%M%SZ"), now.strftime("%Y%m%d")
        payload_hash = hashlib.sha256(body or b"").hexdigest()
        headers = {"host": url.netloc, "x-amz-content-sha256": payload_hash, "x-amz-date": stamp}
        signed = ";".join(sorted(headers))
        canonical_query = "&".join(
            f"{urllib.parse.quote(k, safe='-_.~')}={urllib.parse.quote(v, safe='-_.~')}" for k, v in sorted(urllib.parse.parse_qsl(query, keep_blank_values=True))
        )
        canonical = "\n".join(
            [method, urllib.parse.quote(url.path, safe="/-_.~"), canonical_query, *(f"{k}:{headers[k]}" for k in sorted(headers)), "", signed, payload_hash]
        )
        scope = f"{day}/{REGION}/s3/aws4_request"
        to_sign = "\n".join(["AWS4-HMAC-SHA256", stamp, scope, hashlib.sha256(canonical.encode()).hexdigest()])
        key_bytes = ("AWS4" + self.secret_key).encode()
        for part in (day, REGION, "s3", "aws4_request"):
            key_bytes = hmac.new(key_bytes, part.encode(), hashlib.sha256).digest()
        signature = hmac.new(key_bytes, to_sign.encode(), hashlib.sha256).hexdigest()
        headers["authorization"] = f"AWS4-HMAC-SHA256 Credential={self.access_key}/{scope}, SignedHeaders={signed}, Signature={signature}"
        target = f"{self.endpoint}{url.path}" + (f"?{query}" if query else "")
        req = urllib.request.Request(target, method=method, data=body, headers=headers)
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.read()

    def keys(self, prefix):
        xml = self.request("GET", "", query=f"list-type=2&prefix={prefix}")
        root = ET.fromstring(xml)
        return [el.text for el in root.iter() if el.tag.endswith("}Key") or el.tag == "Key"]

    def get(self, key):
        return self.request("GET", key)

    def put(self, key, body):
        self.request("PUT", key, body=body)


def hkdf32(salt, ikm, info):
    """HKDF-SHA256 to 32 bytes (RFC 5869), as lockra-sync derives its keys."""
    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    return hmac.new(prk, info + b"\x01", hashlib.sha256).digest()


def relay_access(sync_key):
    """The space id and the access token's 32 bytes a sync key gives (docs/formats.md §9), computed
    apart from the Rust code: the relay must keep their SHA-256 and nothing else of the key."""
    body = sync_key.replace("-", "").upper().removeprefix("LKS1")
    raw = base64.b32decode(body + "=" * (-len(body) % 8))
    key = raw[:32]
    if hashlib.sha256(key).digest()[:3] != raw[32:]:
        sys.exit("smoke-sync: the sync key's checksum does not hold")
    space = uuid.UUID(bytes=hkdf32(b"LKS1", key, b"lockra-sync v1 space id")[:16], version=4)
    token = hkdf32(space.bytes, key, b"lockra-relay v1 access token")
    return key, space, token


class Phase:
    def __init__(self, args):
        self.args = args
        os.makedirs(args.out, exist_ok=True)
        with open(args.secret_file) as f:
            self.secret = f.read().strip()
        self.web = Session(args.driver, args.app)
        self.s3 = S3(args.s3, args.s3_user, self.secret)

    def relay_files(self):
        """Every file the relay keeps, by its path under the data folder."""
        files = {}
        for folder, _, names in os.walk(self.args.relay_data):
            for name in names:
                path = os.path.join(folder, name)
                with open(path, "rb") as f:
                    files[os.path.relpath(path, self.args.relay_data)] = f.read()
        return files

    # ---- helpers ------------------------------------------------------------------------------

    def state(self):
        return self.web.invoke({"command": "app_state"})

    def shot(self, name):
        """A screenshot once two in a row agree (the dialog's entrance has settled)."""
        path = os.path.join(self.args.out, f"{name}.png")
        last, deadline = None, time.monotonic() + 8
        while True:
            image = self.web.screenshot()
            if image == last or time.monotonic() > deadline:
                break
            last = image
            time.sleep(0.3)
        with open(path, "wb") as f:
            f.write(image)
        print(f"smoke-sync: {path}")

    def field(self, label, scope="//*[@data-testid='settings-content']"):
        """The input a visible label names, inside `scope`."""
        return self.web.wait(xpath=f"{scope}//input[@id = {scope}//label[normalize-space()='{label}']/@for] | {scope}//textarea[@id = {scope}//label[normalize-space()='{label}']/@for]")

    def fill(self, label, text, scope="//*[@data-testid='settings-content']"):
        """Empty the field, then type `text` into it as key events."""
        element = self.field(label, scope)
        self.web.run(CLEAR, {ELEMENT: element})
        self.web.type(element, text)
        until(f"{label} to read what was typed", lambda: self.web.run("return arguments[0].value", {ELEMENT: element}) == text)

    def button(self, text, scope="//*[@data-testid='settings-content']"):
        return self.web.wait(xpath=f"{scope}//button[normalize-space()='{text}']")

    def synced_after(self, at_ms, timeout=60):
        """The space's status once a run that finished after `at_ms` says synced."""

        def check():
            space = self.state()["sync"]["space"]
            if space is None:
                return None
            status = space["status"]
            if status["state"] == "failed":
                sys.exit(f"smoke-sync: the sync failed: {status}")
            return space if status["state"] == "synced" and status["at_ms"] > at_ms else None

        return until("a finished sync", check, timeout=timeout)

    def issuers(self):
        return sorted(e["issuer"] for e in self.state()["entries"])

    def open_sync(self):
        self.web.click(self.web.wait(xpath="//nav//button[normalize-space()='设置']"))
        self.web.click(self.web.wait(xpath="//*[@role='tab'][normalize-space()='同步']"))
        self.web.wait(css="[data-testid=settings-content][data-section=sync]")

    def light(self):
        settings = self.state()["settings"]
        self.web.invoke({"command": "settings_set", "settings": {**settings, "locale": "zh-cn", "follow_system_theme": False, "theme": "light", "reduce_motion": True}})

    def storage_form(self, scope):
        # The form starts on Lockra's relay; this smoke test syncs through S3.
        self.web.click(self.web.wait(xpath=f"{scope}//*[@role='radio'][normalize-space()='S3 兼容']"))
        self.fill("服务地址", self.args.s3, scope)
        self.fill("区域", REGION, scope)
        self.fill("存储桶", BUCKET, scope)
        self.fill("访问密钥 ID", self.args.s3_user, scope)
        self.fill("访问密钥", self.secret, scope)
        self.fill("文件夹（可选）", PREFIX, scope)
        toggle = self.web.wait(xpath=f"{scope}//*[@role='switch'][@aria-label='路径式访问']")
        if self.web.run("return arguments[0].getAttribute('aria-checked')", {ELEMENT: toggle}) != "true":
            self.web.click(toggle)

    # ---- the phases -----------------------------------------------------------------------------

    def a(self):
        web = self.web
        until("the first state", self.state, timeout=60)
        web.invoke({"command": "vault_create", "password": PASSWORD})
        self.light()
        web.invoke({"command": "entry_add_uri", "uri": GITHUB})
        web.invoke({"command": "entry_add_uri", "uri": MAIL})
        self.open_sync()
        self.shot("sync-off-light")
        # Joining a space from a vault: this device's master password, and another one apart.
        web.click(web.wait(css="[data-testid=sync-join-open]"))
        join = "//*[@data-testid='sync-join']"
        self.fill("这台设备的主密码", PASSWORD, join)
        self.shot("sync-join-settings-light")
        web.click(self.button("取消", join))
        web.gone("[data-testid=sync-join]")
        web.click(web.wait(css="[data-testid=sync-create-open]"))
        scope = "//*[@data-testid='sync-create']"
        self.storage_form(scope)
        self.fill("这台设备的名称", "台式机", scope)
        self.fill("主密码", PASSWORD, scope)
        self.shot("sync-create-light")
        before = int(time.time() * 1000)
        web.click(self.button("开始同步", scope))
        key = web.text(web.wait(css="[data-testid=sync-created] [data-testid=sync-key]", timeout=60)).strip()
        if not key.startswith("LKS1-"):
            sys.exit(f"smoke-sync: the sync key does not read as one: {key[:8]}…")
        with open(os.path.join(self.args.work, "sync-key"), "w") as f:
            os.chmod(f.name, 0o600)
            f.write(key)
        self.shot("sync-key-light")
        web.click(self.button("我已保存", "//*[@role='dialog']"))
        web.gone("[data-testid=sync-created]")
        space = self.synced_after(before)
        if [d["name"] for d in space["devices"]] != ["台式机"]:
            sys.exit(f"smoke-sync: device A's list: {space['devices']}")
        objects = self.s3.keys(f"{PREFIX}/lockra-sync-v1/")
        # One object per device and nothing else: device A's snapshot, its keyring inside.
        if len(objects) != 1 or "/devices/" not in objects[0]:
            sys.exit(f"smoke-sync: the storage holds {objects}")
        for key_name in objects:
            body = self.s3.get(key_name)
            for clear in [b"GitHub", b"octocat", b"JBSWY3DPEHPK3PXP", "台式机".encode()]:
                if clear in body:
                    sys.exit(f"smoke-sync: {clear!r} is readable in {key_name}")
        self.shot("sync-on-light")
        print(f"smoke-sync: A set up the space (1 object, nothing readable) and kept the sync key")
        web.quit()

    def b(self):
        web = self.web
        until("the first state", self.state, timeout=60)
        with open(os.path.join(self.args.work, "sync-key")) as f:
            key = f.read().strip()
        web.wait(css="[data-testid=page-welcome]")
        web.click(web.wait(css="[data-testid=welcome-join-open]"))
        scope = "//*[@data-testid='welcome-join']"
        web.click(web.wait(xpath=f"{scope}//*[@role='radio'][normalize-space()='同步密钥']"))
        self.storage_form(scope)
        self.fill("同步密钥", key, scope)
        self.fill("这台设备的名称", "笔记本", scope)
        self.fill("同步空间的主密码", PASSWORD, scope)
        self.shot("sync-join-welcome-light")
        before = int(time.time() * 1000)
        web.click(self.button("加入", scope))
        web.wait(css="[data-testid=page-codes]", timeout=60)
        self.synced_after(before)
        self.light()
        until("A's accounts on B", lambda: self.issuers() == ["GitHub", "Mail"])
        # B's own first snapshot, for the rollback in phase c.
        tag = next(d["tag"] for d in self.state()["sync"]["space"]["devices"] if d["this_device"])
        own = next(k for k in self.s3.keys(f"{PREFIX}/lockra-sync-v1/") if k.endswith(f"/devices/{tag}.lks"))
        with open(os.path.join(self.args.work, "b-old.lks"), "wb") as f:
            f.write(self.s3.get(own))
        with open(os.path.join(self.args.work, "b-key"), "w") as f:
            f.write(own)

        entries = {e["issuer"]: e["id"] for e in self.state()["entries"]}
        web.invoke({"command": "entry_update", "id": entries["GitHub"], "patch": {"issuer": "GitHub Enterprise"}})
        web.invoke({"command": "entry_delete", "id": entries["Mail"]})
        web.invoke({"command": "entry_add_uri", "uri": BANK})
        before = int(time.time() * 1000)
        web.invoke({"command": "sync_now"})
        self.synced_after(before)
        self.open_sync()
        self.shot("sync-on-b-light")
        print("smoke-sync: B joined with the sync key, got A's accounts, and renamed, deleted and added one")
        web.quit()

    def c(self):
        web = self.web
        until("the first state", self.state, timeout=60)
        before = int(time.time() * 1000)
        web.invoke({"command": "vault_unlock", "password": PASSWORD})
        space = self.synced_after(before)
        until("B's changes on A", lambda: self.issuers() == ["Bank", "GitHub Enterprise"])
        names = [(d["name"], d["this_device"]) for d in space["devices"]]
        if names != [("台式机", True), ("笔记本", False)]:
            sys.exit(f"smoke-sync: device A lists {names}")
        print("smoke-sync: A has B's rename, deletion and new account, and lists both devices")

        # The storage serves B's first snapshot again: A refuses it.
        with open(os.path.join(self.args.work, "b-key")) as f:
            own = f.read().strip()
        with open(os.path.join(self.args.work, "b-old.lks"), "rb") as f:
            self.s3.put(own, f.read())
        before = int(time.time() * 1000)
        web.invoke({"command": "sync_now"})
        space = self.synced_after(before)
        if space["rolled_back"] != [next(d["tag"] for d in space["devices"] if not d["this_device"])]:
            sys.exit(f"smoke-sync: the rollback was not reported: {space['rolled_back']}")
        if self.issuers() != ["Bank", "GitHub Enterprise"]:
            sys.exit(f"smoke-sync: the rollback changed A's accounts: {self.issuers()}")
        self.open_sync()
        banner = web.wait(css="[data-testid=sync-rolled-back]")
        if "笔记本" not in web.text(banner):
            sys.exit(f"smoke-sync: the banner does not name B: {web.text(banner)}")
        self.shot("sync-rolled-back-light")
        print("smoke-sync: A refused B's older snapshot, kept its accounts and named B in the banner")
        web.quit()


    # ---- through a relay -------------------------------------------------------------------------

    def r1(self):
        web = self.web
        until("the first state", self.state, timeout=60)
        web.invoke({"command": "vault_create", "password": PASSWORD})
        self.light()
        web.invoke({"command": "entry_add_uri", "uri": GITHUB})
        web.invoke({"command": "entry_add_uri", "uri": MAIL})
        self.open_sync()
        web.click(web.wait(css="[data-testid=sync-create-open]"))
        scope = "//*[@data-testid='sync-create']"
        # The form starts on Lockra's built-in relay, with nothing to fill in.
        web.wait(xpath=f"{scope}//*[@role='radio'][@aria-checked='true'][normalize-space()='Lockra 中继']")
        address = web.text(web.wait(xpath=f"{scope}//*[@data-testid='storage-address']"))
        if "https://lockra-relay.onethinker.top" not in address:
            sys.exit(f"smoke-sync: the form does not start on the built-in relay: {address}")
        self.fill("这台设备的名称", "台式机", scope)
        self.fill("主密码", PASSWORD, scope)
        self.shot("sync-relay-create-light")
        # This smoke test's relay is one of its own, on this computer.
        web.run(SELECT, {ELEMENT: web.wait(xpath=f"{scope}//select[@data-testid='storage-provider']")}, "relay-custom")
        self.fill("中继地址", self.args.relay, scope)
        before = int(time.time() * 1000)
        web.click(self.button("开始同步", scope))
        key = web.text(web.wait(css="[data-testid=sync-created] [data-testid=sync-key]", timeout=60)).strip()
        web.click(self.button("我已保存", "//*[@role='dialog']"))
        web.gone("[data-testid=sync-created]")
        space = self.synced_after(before)
        if space["storage"] != {"kind": "relay", "url": self.args.relay}:
            sys.exit(f"smoke-sync: C's storage reads {space['storage']}")

        raw_key, space_id, token = relay_access(key)
        bearer = base64.urlsafe_b64encode(token).rstrip(b"=")
        files = self.relay_files()
        base = f"lockra-relay-v1/spaces/{space_id}"
        snapshots = [p for p in files if p.startswith(f"{base}/devices/") and p.endswith(".lks")]
        if sorted(files) != sorted([f"{base}/access", *snapshots]) or len(snapshots) != 1:
            sys.exit(f"smoke-sync: the relay keeps {sorted(files)}")
        if files[f"{base}/access"].decode().strip() != hashlib.sha256(token).hexdigest():
            sys.exit("smoke-sync: the relay's access file is not the SHA-256 of the access token")
        for path, body in files.items():
            for clear in [b"GitHub", b"octocat", b"JBSWY3DPEHPK3PXP", "台式机".encode(), key.encode(), raw_key, token, bearer]:
                if clear in body:
                    sys.exit(f"smoke-sync: {clear[:12]!r}… is readable in the relay's {path}")
        print("smoke-sync: C set up a space on its relay (1 snapshot, nothing readable, only the token's SHA-256)")

        web.click(web.wait(css="[data-testid=sync-invite-open]"))
        dialog = "//*[@role='dialog']"
        self.fill("主密码", PASSWORD, dialog)
        web.click(self.button("显示邀请码", dialog))
        invite = web.wait(css="[data-testid=sync-invite]")
        if "中继地址" not in web.text(invite):
            sys.exit(f"smoke-sync: the invitation does not say it holds the relay's address: {web.text(invite)[:80]}")
        text = web.text(web.wait(css="[data-testid=sync-invite] [data-testid=invite-text]")).strip()
        code = web.text(web.wait(css="[data-testid=sync-invite] [data-testid=invite-code]")).strip()
        if not text.startswith("lockra-invite:2:"):
            sys.exit(f"smoke-sync: the invitation to send is not sealed: {text[:20]}…")
        for name, value in [("relay-invite", text), ("relay-code", code)]:
            with open(os.path.join(self.args.work, name), "w") as f:
                os.chmod(f.name, 0o600)
                f.write(value)
        self.shot("sync-relay-invite-light")
        web.click(self.button("完成", dialog))
        web.gone("[data-testid=sync-invite]")
        print("smoke-sync: C showed a sealed invitation for its relay space and kept it with its code")
        web.quit()

    def r2(self):
        web = self.web
        until("the first state", self.state, timeout=60)
        with open(os.path.join(self.args.work, "relay-invite")) as f:
            text = f.read().strip()
        with open(os.path.join(self.args.work, "relay-code")) as f:
            code = f.read().strip()
        web.wait(css="[data-testid=page-welcome]")
        web.click(web.wait(css="[data-testid=welcome-join-open]"))
        scope = "//*[@data-testid='welcome-join']"
        # The invitation and its code: no storage settings to type.
        self.fill("邀请码", text, scope)
        self.fill("口令", code, scope)
        self.fill("这台设备的名称", "笔记本", scope)
        self.fill("同步空间的主密码", PASSWORD, scope)
        self.shot("sync-relay-join-light")
        before = int(time.time() * 1000)
        web.click(self.button("加入", scope))
        web.wait(css="[data-testid=page-codes]", timeout=60)
        space = self.synced_after(before)
        self.light()
        if space["storage"] != {"kind": "relay", "url": self.args.relay}:
            sys.exit(f"smoke-sync: D's storage reads {space['storage']}")
        until("C's accounts on D", lambda: self.issuers() == ["GitHub", "Mail"])
        web.invoke({"command": "entry_add_uri", "uri": BANK})
        before = int(time.time() * 1000)
        web.invoke({"command": "sync_now"})
        self.synced_after(before)
        snapshots = [p for p in self.relay_files() if p.endswith(".lks")]
        if len(snapshots) != 2:
            sys.exit(f"smoke-sync: the relay keeps {snapshots}")
        print("smoke-sync: D joined from C's invitation and code alone, got C's accounts, and added one")
        web.quit()

    def r3(self):
        web = self.web
        until("the first state", self.state, timeout=60)
        before = int(time.time() * 1000)
        web.invoke({"command": "vault_unlock", "password": PASSWORD})
        space = self.synced_after(before)
        until("D's account on C", lambda: self.issuers() == ["Bank", "GitHub", "Mail"])
        names = [(d["name"], d["this_device"]) for d in space["devices"]]
        if names != [("台式机", True), ("笔记本", False)]:
            sys.exit(f"smoke-sync: device C lists {names}")
        self.open_sync()
        storage = web.text(web.wait(css="[data-testid=sync-storage]"))
        if "Lockra 中继" not in storage:
            sys.exit(f"smoke-sync: C's storage shows as {storage}")
        self.shot("sync-relay-on-light")
        print("smoke-sync: C has D's new account through the relay, and lists both devices")
        web.quit()


def main():
    parser = argparse.ArgumentParser()
    for name in ["phase", "driver", "app", "out", "work", "s3", "s3-user", "secret-file", "relay", "relay-data"]:
        parser.add_argument(f"--{name}", required=True)
    args = parser.parse_args()
    phase = Phase(args)
    {"a": phase.a, "b": phase.b, "c": phase.c, "r1": phase.r1, "r2": phase.r2, "r3": phase.r3}[args.phase]()


if __name__ == "__main__":
    main()
