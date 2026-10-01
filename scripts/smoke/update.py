#!/usr/bin/env python3
"""Drive a packaged Lockra through an in-app update over WebDriver. scripts/smoke-update-linux.sh
owns the two builds, the manifest server, Xvfb and the driver; this checks that the copy says it
was installed as an AppImage at the old version, that a check finds the newer one, and starts the
install, which replaces the AppImage and restarts Lockra (the session ends with the old process).
Every wait is a condition with a deadline.
"""

import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from desktop import Session, until  # noqa: E402


def main():
    parser = argparse.ArgumentParser()
    for name in ["driver", "app", "from-version", "to-version"]:
        parser.add_argument(f"--{name}", required=True)
    args = parser.parse_args()
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

    web.invoke({"command": "update_install"})
    print("smoke-update: install started; the app replaces its AppImage and restarts")


if __name__ == "__main__":
    main()
