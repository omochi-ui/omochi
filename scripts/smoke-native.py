#!/usr/bin/env python3
"""Verify/extract/install an archive, then serve its embedded UI with no runtimes on PATH."""

import argparse
import hashlib
import json
import os
import queue
import re
import shutil
import signal
import subprocess
import tarfile
import tempfile
import threading
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
            suffix = ".exe" if os.name == "nt" else ""
            installer = "install.ps1" if os.name == "nt" else "install.sh"
            allowed = {
                "bin/omochi-ui" + suffix,
                "bin/omochi-utils" + suffix,
                "LICENSE",
                "README.md",
                installer,
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
        if os.name == "nt":
            import ctypes

            # Headless CI can have a console without a window handle.
            if not ctypes.windll.kernel32.GetConsoleCP():
                assert ctypes.windll.kernel32.AllocConsole(), (
                    "console required for graceful shutdown test"
                )
            subprocess.run(
                [
                    shutil.which("pwsh") or "powershell.exe",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(root / installer),
                    "-Prefix",
                    str(prefix),
                ],
                check=True,
            )
        else:
            subprocess.run(["/bin/sh", str(root / installer), str(prefix)], check=True)
        env = {**os.environ, "PATH": "/nonexistent"}
        for name in ("omochi-ui", "omochi-utils"):
            output = subprocess.check_output(
                [str(prefix / (name + suffix)), "--help"], env=env, text=True
            )
            assert "Usage:" in output
        hub = subprocess.Popen(
            [
                str(prefix / ("omochi-ui" + suffix)),
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
            creationflags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0,
        )
        try:
            ready = queue.Queue()
            threading.Thread(
                target=lambda: ready.put(hub.stderr.readline()), daemon=True
            ).start()
            line = ready.get(timeout=10)
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
            hub.send_signal(
                signal.CTRL_BREAK_EVENT if os.name == "nt" else signal.SIGTERM
            )
            assert hub.wait(timeout=10) == 0
            assert not (root / "runtime/gateway.sock").exists()
            assert not (root / "runtime/gateway.json").exists()
        finally:
            if hub.poll() is None:
                hub.kill()
                hub.wait()
    print("Native archive checksum, installation and embedded dashboard smoke passed")


if __name__ == "__main__":
    main()
