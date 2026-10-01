#!/usr/bin/env python3
"""Drive a packaged Lockra through an in-app update over WebDriver. scripts/smoke-update-linux.sh
owns the two builds, the manifest server, Xvfb and the driver; this checks that the copy says it
was installed as an AppImage at the old version, that a check finds the newer one, then, in the
unlocked app, that the title bar shows the new version, that Settings › General shows it and opens
the update dialog over Settings, and starts the install from the dialog's button, which replaces
the AppImage and restarts Lockra (the session ends with the old process). With --out, the title
bar, the General pane and the dialog are saved as screenshots.
Every wait is a condition with a deadline.
"""

import argparse
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from desktop import Session, until  # noqa: E402

PASSWORD = "correct horse battery"


def shot(web, out, name):
    """A screenshot once two in a row agree (the dialog's entrance has settled)."""
    if out is None:
        return
    last, deadline = None, time.monotonic() + 8
    while True:
        image = web.screenshot()
        if image == last or time.monotonic() > deadline:
            break
        last = image
        time.sleep(0.3)
    path = os.path.join(out, f"{name}.png")
    with open(path, "wb") as f:
        f.write(image)
    print(f"smoke-update: {path}")


def main():
    parser = argparse.ArgumentParser()
    for name in ["driver", "app", "from-version", "to-version"]:
        parser.add_argument(f"--{name}", required=True)
    parser.add_argument("--out", help="where the screenshots go")
    args = parser.parse_args()
    if args.out is not None:
        os.makedirs(args.out, exist_ok=True)
    web = Session(args.driver, args.app)
    state = until("the first state", lambda: web.invoke({"command": "app_state"}), timeout=60)
    if state["app_version"] != args.from_version:
        sys.exit(f"smoke-update: running {state['app_version']}, expected {args.from_version}")
    if state["update"] != {"method": "appimage", "status": {"state": "idle"}}:
        sys.exit(f"smoke-update: the copy does not see itself as an AppImage: {state['update']}")
    print(f"smoke-update: Lockra {args.from_version}, installed as an AppImage")

    web.invoke({"command": "update_check"})
    status = until(
        "the check's answer",
        lambda: (lambda s: s if s["state"] in ("available", "up_to_date", "failed") else None)(web.invoke({"command": "app_state"})["update"]["status"]),
        timeout=60,
    )
    if status.get("state") != "available" or status.get("version") != args.to_version:
        sys.exit(f"smoke-update: the check found {status}, expected {args.to_version}")
    print(f"smoke-update: the check found {args.to_version} ({status.get('date')})")

    # The title bar and the update dialog belong to the unlocked app.
    web.invoke({"command": "vault_create", "password": PASSWORD})
    settings = web.invoke({"command": "app_state"})["settings"]
    web.invoke({"command": "settings_set", "settings": {**settings, "locale": "zh-cn", "follow_system_theme": False, "theme": "light", "reduce_motion": True}})
    badge = web.wait(css="[data-testid=update-badge]")
    until("the note to name the new version", lambda: args.to_version in web.text(badge))
    shot(web, args.out, "update-badge-light")
    web.click(web.wait(xpath="//nav//button[normalize-space()='设置']"))
    section = web.wait(css="[data-testid=update-section]")
    until("Settings › General to name the new version", lambda: args.to_version in web.text(section))
    shot(web, args.out, "update-settings-light")
    web.click(web.wait(css="[data-testid=update-view]"))
    web.wait(css="[data-testid=update-dialog][data-state=available]")
    # The update dialog opens over Settings: find it by its content, not as the first dialog.
    dialog = "//*[@role='dialog'][.//*[@data-testid='update-dialog']]"
    title = web.text(web.wait(xpath=f"{dialog}//h2"))
    if args.to_version not in title:
        sys.exit(f"smoke-update: the update dialog's title does not name {args.to_version}: {title}")
    shot(web, args.out, "update-dialog-light")
    print(f"smoke-update: the title bar and Settings › General named {args.to_version}; the update dialog opened")

    # 「立即更新」: the dialog's primary action.
    web.click(web.wait(xpath=f"{dialog}//button[@data-autofocus]"))
    print("smoke-update: install started from the dialog; the app replaces its AppImage and restarts")


if __name__ == "__main__":
    main()
