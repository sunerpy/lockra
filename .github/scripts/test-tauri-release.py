#!/usr/bin/env python3
"""Offline test of `tauri-release.py collect` and `updater-json` against Lockra's own target
manifest and its real updater configuration: every package of every leg is signed and collected,
the macOS legs archive the .app for the updater beside their dmg, a leg without a signature or an
archive fails, and latest.json carries one key per target plus one per installer, so a copy
installed from the .deb, the .rpm or the .msi updates through its own package. It also checks the
update settings in tauri.conf.json and that deny.toml's [graph] targets are exactly the shipped
targets. Run by scripts/verify-all.sh and the CI config job.

Stdlib only and Python 3.9 or later, like tauri-release.py, so it runs wherever `make check` does
(macOS ships Python 3.9); it needs no jq, GNU tool or TOML library.
"""

from __future__ import annotations

import base64
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / ".github" / "scripts" / "tauri-release.py"
MANIFEST = ROOT / ".github" / "release-targets.json"
TAURI_CONF = ROOT / "apps" / "desktop" / "src-tauri" / "tauri.conf.json"
DENY = ROOT / "deny.toml"
REPO = "sunerpy/lockra"
ENDPOINT = f"https://github.com/{REPO}/releases/latest/download/latest.json"
VERSION = "1.2.3"


def fail(message: str) -> None:
    sys.exit(f"test-tauri-release: {message}")


def signature(name: str) -> str:
    """A `.sig` as the Tauri CLI writes it: base64 of minisign text with a trusted comment."""
    text = (
        "untrusted comment: signature from tauri secret key\nRUQfixture\n"
        f"trusted comment: timestamp:1790000000\tfile:{name}\tversion:{VERSION}\nZml4dHVyZQ==\n"
    )
    return base64.b64encode(text.encode()).decode()


def check_update_config(config: dict) -> None:
    """The settings the in-app update relies on (docs/release.md, "In-app updates")."""
    if config.get("bundle", {}).get("createUpdaterArtifacts") is not True:
        fail("tauri.conf.json: bundle.createUpdaterArtifacts must be true")
    updater = config.get("plugins", {}).get("updater", {})
    try:
        pubkey = base64.b64decode(updater.get("pubkey", ""), validate=True).decode()
    except ValueError:
        fail("tauri.conf.json: plugins.updater.pubkey is not base64")
    if not pubkey.startswith("untrusted comment: minisign public key"):
        fail("tauri.conf.json: plugins.updater.pubkey is not a minisign public key")
    if updater.get("endpoints") != [ENDPOINT]:
        fail(f"tauri.conf.json: plugins.updater.endpoints must be [{ENDPOINT}]")
    if updater.get("requireSignedVersion") is not True:
        fail("tauri.conf.json: plugins.updater.requireSignedVersion must be true")
    if updater.get("windows", {}).get("installMode") != "passive":
        fail("tauri.conf.json: plugins.updater.windows.installMode must be passive")


class Fixture:
    """A Tauri project directory with the bundles each target's `tauri bundle` writes."""

    def __init__(self, root: Path, config: dict) -> None:
        self.root = root
        repo = root / "repo"
        self.project = repo / "src-tauri"
        self.project.mkdir(parents=True)
        (repo / "package.json").write_text(json.dumps({"name": "lockra", "version": VERSION}), encoding="utf-8")
        # The parts of Lockra's tauri.conf.json that collect reads, the updater block included.
        fixture = {"productName": "Lockra", "version": "../package.json", "identifier": "dev.lockra.desktop",
                   "bundle": config["bundle"], "plugins": config["plugins"]}
        (self.project / "tauri.conf.json").write_text(json.dumps(fixture), encoding="utf-8")
        self.runs = 0

    def bundle(self, target: str, directory: str, name: str, signed: bool = True) -> Path:
        path = self.project / "target" / target / "release" / "bundle" / directory / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f"bundle {target} {name}\n", encoding="utf-8")
        if signed:
            path.with_name(f"{name}.sig").write_text(signature(name), encoding="utf-8")
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
    config = json.loads(TAURI_CONF.read_text(encoding="utf-8"))
    check_update_config(config)

    with tempfile.TemporaryDirectory() as tmp:
        fx = Fixture(Path(tmp), config)
        arm_archive = fx.bundle("aarch64-apple-darwin", "macos", "Lockra.app.tar.gz")
        x64_msi = fx.bundle("x86_64-pc-windows-msvc", "msi", f"Lockra_{VERSION}_x64_en-US.msi")
        packages = {
            "linux-x86_64-deb": fx.bundle("x86_64-unknown-linux-gnu", "deb", f"Lockra_{VERSION}_amd64.deb").name,
            "linux-x86_64-rpm": fx.bundle("x86_64-unknown-linux-gnu", "rpm", f"Lockra-{VERSION}-1.x86_64.rpm").name,
            "linux-x86_64-appimage": fx.bundle("x86_64-unknown-linux-gnu", "appimage", f"Lockra_{VERSION}_amd64.AppImage").name,
            "linux-aarch64-deb": fx.bundle("aarch64-unknown-linux-gnu", "deb", f"Lockra_{VERSION}_arm64.deb").name,
            "linux-aarch64-rpm": fx.bundle("aarch64-unknown-linux-gnu", "rpm", f"Lockra-{VERSION}-1.aarch64.rpm").name,
            "linux-aarch64-appimage": fx.bundle("aarch64-unknown-linux-gnu", "appimage", f"Lockra_{VERSION}_aarch64.AppImage").name,
            "darwin-aarch64-app": "Lockra_aarch64.app.tar.gz",
            "darwin-x86_64-app": "Lockra_x64.app.tar.gz",
            "windows-x86_64-nsis": fx.bundle("x86_64-pc-windows-msvc", "nsis", f"Lockra_{VERSION}_x64-setup.exe").name,
            "windows-x86_64-msi": x64_msi.name,
            "windows-aarch64-nsis": fx.bundle("aarch64-pc-windows-msvc", "nsis", f"Lockra_{VERSION}_arm64-setup.exe").name,
        }
        fx.bundle("x86_64-apple-darwin", "macos", "Lockra.app.tar.gz")
        dmgs = [fx.bundle("aarch64-apple-darwin", "dmg", f"Lockra_{VERSION}_aarch64.dmg", signed=False).name,
                fx.bundle("x86_64-apple-darwin", "dmg", f"Lockra_{VERSION}_x64.dmg", signed=False).name]
        fx.app("aarch64-apple-darwin")
        fx.app("x86_64-apple-darwin")

        out, evidence = Path(tmp) / "dist", Path(tmp) / "evidence"
        for target in targets:
            run = fx.collect(target, out, evidence)
            if run.returncode != 0:
                fail(f"collect failed for {target}: {run.stderr.strip()}")
        actual = sorted(path.name for path in out.iterdir())
        wanted = sorted([*packages.values(), *(f"{name}.sig" for name in packages.values()), *dmgs])
        if actual != wanted:
            fail(f"collected {actual}, expected {wanted}")
        for target in targets:
            record = json.loads((evidence / f"{target}.json").read_text(encoding="utf-8"))
            if not record["updater_enabled"] or record["updater"] is None:
                fail(f"{target}: the updater bundle was not recorded: {record}")
        if len(list(evidence.glob("*.json"))) != len(targets):
            fail("one evidence file per target expected")

        latest = out / "latest.json"
        run = subprocess.run([sys.executable, str(SCRIPT), "updater-json", "--dist", str(out), "--evidence-dir", str(evidence),
                              "--repo", REPO, "--tag", f"v{VERSION}", "--version", VERSION, "--targets-file", str(MANIFEST),
                              "--out", str(latest)], capture_output=True, text=True)
        if run.returncode != 0:
            fail(f"updater-json failed: {run.stderr.strip()}")
        platforms = json.loads(latest.read_text(encoding="utf-8"))["platforms"]
        generic = {"linux-x86_64": "linux-x86_64-appimage", "linux-aarch64": "linux-aarch64-appimage",
                   "darwin-aarch64": "darwin-aarch64-app", "darwin-x86_64": "darwin-x86_64-app",
                   "windows-x86_64": "windows-x86_64-nsis", "windows-aarch64": "windows-aarch64-nsis"}
        if sorted(platforms) != sorted([*packages, *generic]):
            fail(f"latest.json platforms {sorted(platforms)}, expected {sorted([*packages, *generic])}")
        base = f"https://github.com/{REPO}/releases/download/v{VERSION}/"
        for key, name in packages.items():
            entry = platforms[key]
            if entry["url"] != base + name or entry["signature"] != (out / f"{name}.sig").read_text(encoding="utf-8").strip():
                fail(f"latest.json {key} does not point at {name} and its signature: {entry}")
        for key, specific in generic.items():
            if platforms[key] != platforms[specific]:
                fail(f"latest.json {key} is not its updater bundle {specific}")

        # A leg whose updater bundle has no signature fails, and so does a macOS leg whose bundle
        # step archived no .app or produced no dmg.
        (fx.project / "target/x86_64-unknown-linux-gnu/release/bundle/appimage" / f"Lockra_{VERSION}_amd64.AppImage.sig").unlink()
        fx.expect_failure("Linux leg without the AppImage signature", "x86_64-unknown-linux-gnu")
        arm_archive.unlink()
        fx.expect_failure("macOS leg without its .app archive", "aarch64-apple-darwin")
        x64_msi.unlink()
        fx.expect_failure("Windows leg without its msi", "x86_64-pc-windows-msvc")
    print(f"test-tauri-release: collect and latest.json passed for {len(targets)} targets "
          f"({len(packages) + len(generic)} updater platforms)")

    # cargo-deny checks the graph of exactly the targets that ship, so a target added to the
    # manifest cannot escape the licence, ban and advisory policy.
    policy, shipped = deny_targets(DENY.read_text(encoding="utf-8")), set(targets)
    if policy != shipped:
        fail("deny.toml [graph] targets differ from .github/release-targets.json: "
             f"missing {sorted(shipped - policy)}, extra {sorted(policy - shipped)}")
    print(f"test-tauri-release: deny.toml checks the graph of all {len(shipped)} shipped targets")


if __name__ == "__main__":
    main()
