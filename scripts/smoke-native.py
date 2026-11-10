#!/usr/bin/env python3
"""Verify/extract/install an archive, then serve its embedded UI with no runtimes on PATH."""

import argparse
import hashlib
import json
import os
import re
import selectors
import subprocess
import tarfile
import tempfile
from pathlib import Path
from urllib.request import urlopen


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path)
    archive = parser.parse_args().archive.resolve()
    checksum = archive.with_suffix(".gz.sha256").read_text().split()[0]
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == checksum
    with tempfile.TemporaryDirectory(prefix="omochi-native-") as directory:
        root = Path(directory)
        with tarfile.open(archive) as tar:
            allowed = {
                "bin/omochi-ui",
                "bin/omochi-utils",
                "LICENSE",
                "README.md",
                "install.sh",
                "manifest.json",
            }
            assert {item.name for item in tar.getmembers()} == allowed
            for item in tar.getmembers():
                assert item.isfile() and not item.name.startswith("/")
                target = root / item.name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(tar.extractfile(item).read())
                target.chmod(item.mode)
        manifest = json.loads((root / "manifest.json").read_text())
        for name, checksum in manifest["sha256"].items():
            assert name in allowed
            assert hashlib.sha256((root / name).read_bytes()).hexdigest() == checksum
        prefix = root / "installed with spaces"
        subprocess.run(["/bin/sh", str(root / "install.sh"), str(prefix)], check=True)
        env = {**os.environ, "PATH": "/nonexistent"}
        for name in ("omochi-ui", "omochi-utils"):
            output = subprocess.check_output(
                [str(prefix / name), "--help"], env=env, text=True
            )
            assert "Usage:" in output
        hub = subprocess.Popen(
            [
                str(prefix / "omochi-ui"),
                "--config-dir",
                str(root / "config"),
                "--runtime-dir",
                str(root / "runtime"),
                "hub",
                "--port",
                "0",
            ],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            selector = selectors.DefaultSelector()
            selector.register(hub.stderr, selectors.EVENT_READ)
            assert selector.select(10), "hub startup timed out"
            line = hub.stderr.readline()
            assert line.startswith("[omochi-ui] dashboard "), line
            url = line.split("dashboard ", 1)[1].strip()
            with urlopen(url, timeout=5) as response:
                assert response.status == 200
                html = response.read().decode()
            assets = re.findall(r'(?:src|href)="(/assets/[^"]+)"', html)
            assert len(assets) >= 2
            for asset in assets:
                with urlopen(url.rstrip("/") + asset, timeout=5) as response:
                    assert response.status == 200 and response.read()
            for endpoint in ("services", "api/presets"):
                with urlopen(url + endpoint, timeout=5) as response:
                    assert json.load(response) == []
            hub.terminate()
            assert hub.wait(timeout=10) == 0
            assert not (root / "runtime/gateway.sock").exists()
        finally:
            if hub.poll() is None:
                hub.kill()
                hub.wait()
    print("Native archive checksum, installation and embedded dashboard smoke passed")


if __name__ == "__main__":
    main()
