#!/usr/bin/env python3
"""Validate deployment config with an actual pinned relay and verified local TLS."""

import argparse
import json
import os
import re
import signal
import socket
import ssl
import subprocess
import tempfile
import time
import tomllib
from pathlib import Path
from urllib.request import urlopen


def port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    binary = parser.parse_args().binary.resolve()
    assert (
        subprocess.check_output([binary, "--version"], text=True).strip()
        == "iroh-relay 1.4.1"
    )
    template = Path("deploy/relay/relay.toml.example").read_text()
    production = tomllib.loads(template)
    assert production["access"] == "everyone"
    assert production["tls"]["cert_mode"] == "LetsEncrypt"
    assert production["metrics_bind_addr"].startswith("127.0.0.1:")
    with tempfile.TemporaryDirectory(prefix="omochi-relay-") as directory:
        root = Path(directory)
        cert, key = root / "default.crt", root / "default.key"
        subprocess.run(
            [
                "openssl",
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-days",
                "1",
                "-subj",
                "/CN=127.0.0.1",
                "-addext",
                "subjectAltName=IP:127.0.0.1",
                "-keyout",
                str(key),
                "-out",
                str(cert),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        http, https, metrics = port(), port(), port()
        config = template.replace("0.0.0.0:80", f"127.0.0.1:{http}")
        config = config.replace("0.0.0.0:443", f"127.0.0.1:{https}").replace(
            "0.0.0.0:7842", "127.0.0.1:0"
        )
        config = config.replace("127.0.0.1:9090", f"127.0.0.1:{metrics}")
        config = config.replace('cert_mode = "LetsEncrypt"', 'cert_mode = "Manual"')
        config = config.replace('"/var/lib/omochi-relay/certs"', json.dumps(str(root)))
        config_path = root / "relay.toml"
        config_path.write_text(config)
        with (root / "stderr").open("w+") as log:
            child = subprocess.Popen(
                [binary, "--config-path", config_path],
                stdout=subprocess.DEVNULL,
                stderr=log,
                env={**os.environ, "RUST_LOG": "info"},
            )
            try:
                context = ssl.create_default_context(cafile=str(cert))
                deadline = time.monotonic() + 15
                while True:
                    try:
                        with urlopen(
                            f"https://127.0.0.1:{https}/", context=context, timeout=1
                        ) as response:
                            assert response.status == 200
                        break
                    except OSError:
                        if child.poll() is not None or time.monotonic() >= deadline:
                            log.seek(0)
                            raise AssertionError(f"relay failed to start: {log.read()}")
                        time.sleep(0.05)
                with urlopen(
                    f"http://127.0.0.1:{metrics}/metrics", timeout=2
                ) as response:
                    text = response.read().decode()
                names = set(
                    re.findall(r"^([a-zA-Z_:][a-zA-Z0-9_:]*)[ {]", text, re.MULTILINE)
                )
                alerts = Path("deploy/relay/alerts.yml").read_text()
                for metric in re.findall(r"rate\((\w+)\{", alerts):
                    assert metric in names, (
                        f"alert uses missing metric {metric}; available: {sorted(names)}"
                    )
                child.send_signal(signal.SIGINT)
                assert child.wait(timeout=10) == 0
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait()
    print(
        "Relay config, verified TLS listener, metrics names and graceful shutdown passed"
    )


if __name__ == "__main__":
    main()
