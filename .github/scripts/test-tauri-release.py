#!/usr/bin/env python3
"""Offline test of `tauri-release.py collect` against Lockra's own target manifest with the updater
off (Lockra sets no bundle.createUpdaterArtifacts): every leg collects what Tauri writes then, the
macOS legs their dmg (the .app stays a directory, which only an updater build archives), and a leg
whose bundles are missing fails. It also checks that deny.toml's [graph] targets are exactly the
shipped targets. Run by scripts/verify-all.sh and the CI config job.

Stdlib only and Python 3.9 or later, like tauri-release.py, so it runs wherever `make check` does
(macOS ships Python 3.9); it needs no jq, GNU tool or TOML library.
"""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / ".github" / "scripts" / "tauri-release.py"
MANIFEST = ROOT / ".github" / "release-targets.json"
DENY = ROOT / "deny.toml"
VERSION = "1.2.3"


def fail(message: str) -> None:
    sys.exit(f"test-tauri-release: {message}")


class Fixture:
    """A Tauri project directory with the bundles each target's `tauri bundle` writes."""

    def __init__(self, root: Path) -> None:
        self.root = root
        repo = root / "repo"
        self.project = repo / "src-tauri"
        self.project.mkdir(parents=True)
        (repo / "package.json").write_text(json.dumps({"name": "lockra", "version": VERSION}), encoding="utf-8")
        # The shape of Lockra's tauri.conf.json that collect reads: no updater artifacts, no plugin.
        config = {"productName": "Lockra", "version": "../package.json", "identifier": "dev.lockra.desktop",
                  "bundle": {"active": True, "targets": "all"}}
        (self.project / "tauri.conf.json").write_text(json.dumps(config), encoding="utf-8")
        self.runs = 0

    def bundle(self, target: str, directory: str, name: str) -> Path:
        path = self.project / "target" / target / "release" / "bundle" / directory / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f"bundle {target} {name}\n", encoding="utf-8")
        return path

    def app(self, target: str) -> Path:
        path = self.project / "target" / target / "release" / "bundle" / "macos" / "Lockra.app"
        (path / "Contents" / "MacOS").mkdir(parents=True, exist_ok=True)
        return path

    def collect(self, target: str, out: Path | None = None, evidence: Path | None = None) -> subprocess.CompletedProcess:
        self.runs += 1
        out = out or self.root / f"out-{self.runs}"
        evidence = evidence or self.root / f"evidence-{self.runs}"
        command = [sys.executable, str(SCRIPT), "collect", "--project", str(self.project), "--target", target,
                   "--targets-file", str(MANIFEST), "--out", str(out), "--evidence", str(evidence / f"{target}.json")]
        return subprocess.run(command, capture_output=True, text=True)

    def expect_failure(self, label: str, target: str) -> None:
        if self.collect(target).returncode == 0:
            fail(f"expected failure did not happen: {label}")


def deny_targets(text: str) -> set[str]:
    """The `targets` array of deny.toml's [graph] table, read with a pattern (tomllib is 3.11+)."""
    table, body = None, []
    for line in text.splitlines():
        line = line.split("#", 1)[0]
        header = re.match(r"\s*\[\[?([^\[\]]+)\]\]?\s*$", line)
        if header:
            table = header.group(1).strip()
        elif table == "graph":
            body.append(line)
    found = re.search(r"\btargets\s*=\s*\[(.*?)\]", "\n".join(body), re.S)
    if not found:
        fail("deny.toml has no [graph] targets array")
    return set(re.findall(r'"([^"]+)"', found.group(1)))


def main() -> None:
    if sys.version_info < (3, 9):
        fail("needs Python 3.9 or later")
    targets = [entry["target"] for entry in json.loads(MANIFEST.read_text(encoding="utf-8"))["targets"]]

    with tempfile.TemporaryDirectory() as tmp:
        fx = Fixture(Path(tmp))
        arm_dmg = fx.bundle("aarch64-apple-darwin", "dmg", f"Lockra_{VERSION}_aarch64.dmg")
        x64_msi = fx.bundle("x86_64-pc-windows-msvc", "msi", f"Lockra_{VERSION}_x64_en-US.msi")
        expected = [
            arm_dmg,
            x64_msi,
            fx.bundle("x86_64-unknown-linux-gnu", "deb", f"Lockra_{VERSION}_amd64.deb"),
            fx.bundle("x86_64-unknown-linux-gnu", "rpm", f"Lockra-{VERSION}-1.x86_64.rpm"),
            fx.bundle("x86_64-unknown-linux-gnu", "appimage", f"Lockra_{VERSION}_amd64.AppImage"),
            fx.bundle("aarch64-unknown-linux-gnu", "deb", f"Lockra_{VERSION}_arm64.deb"),
            fx.bundle("aarch64-unknown-linux-gnu", "rpm", f"Lockra-{VERSION}-1.aarch64.rpm"),
            fx.bundle("aarch64-unknown-linux-gnu", "appimage", f"Lockra_{VERSION}_aarch64.AppImage"),
            fx.bundle("x86_64-apple-darwin", "dmg", f"Lockra_{VERSION}_x64.dmg"),
            fx.bundle("x86_64-pc-windows-msvc", "nsis", f"Lockra_{VERSION}_x64-setup.exe"),
            fx.bundle("aarch64-pc-windows-msvc", "nsis", f"Lockra_{VERSION}_arm64-setup.exe"),
        ]
        fx.app("aarch64-apple-darwin")
        fx.app("x86_64-apple-darwin")

        out, evidence = Path(tmp) / "dist", Path(tmp) / "evidence"
        for target in targets:
            run = fx.collect(target, out, evidence)
            if run.returncode != 0:
                fail(f"collect failed for {target}: {run.stderr.strip()}")
        actual = sorted(path.name for path in out.iterdir())
        wanted = sorted(path.name for path in expected)
        if actual != wanted:
            fail(f"collected {actual}, expected {wanted}")
        for target in ("aarch64-apple-darwin", "x86_64-apple-darwin"):
            record = json.loads((evidence / f"{target}.json").read_text(encoding="utf-8"))
            if record["updater_enabled"] or record["updater"] is not None or [f["kind"] for f in record["files"]] != ["dmg"]:
                fail(f"{target}: evidence is not the dmg alone: {record}")
        if len(list(evidence.glob("*.json"))) != len(targets):
            fail("one evidence file per target expected")

        # A macOS leg whose bundle step produced no .app, or no dmg, fails; so does any other leg
        # with a bundle missing.
        shutil.rmtree(fx.app("aarch64-apple-darwin"))
        fx.expect_failure("macOS leg without its .app", "aarch64-apple-darwin")
        fx.app("aarch64-apple-darwin")
        arm_dmg.unlink()
        fx.expect_failure("macOS leg without its dmg", "aarch64-apple-darwin")
        x64_msi.unlink()
        fx.expect_failure("Windows leg without its msi", "x86_64-pc-windows-msvc")
    print(f"test-tauri-release: collect passed for {len(targets)} targets with the updater off")

    # cargo-deny checks the graph of exactly the targets that ship, so a target added to the
    # manifest cannot escape the licence, ban and advisory policy.
    policy, shipped = deny_targets(DENY.read_text(encoding="utf-8")), set(targets)
    if policy != shipped:
        fail("deny.toml [graph] targets differ from .github/release-targets.json: "
             f"missing {sorted(shipped - policy)}, extra {sorted(policy - shipped)}")
    print(f"test-tauri-release: deny.toml checks the graph of all {len(shipped)} shipped targets")


if __name__ == "__main__":
    main()
