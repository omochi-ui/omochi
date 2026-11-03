"""Test-only process controller: two real Gateways + SDKs, never an HTTP Hub.

Bind on loopback; adb reverse is only for invitations and process lifecycle.
All application traffic and resources travel over iroh/UDP directly.
"""

import argparse
import json
import os
import re
import select
import signal
import ssl
import struct
import subprocess
import tempfile
import threading
import time
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

APP = Path(__file__).resolve().parent.parent
REPO = Path(os.environ.get("OMOCHI_HUB_ROOT", APP.parents[1])).resolve()
CLI = (
    Path(os.environ.get("CARGO_TARGET_DIR", REPO / "crates/target")) / "debug/omochi-ui"
)


class Fixture:
    def __init__(self):
        self.temp = tempfile.TemporaryDirectory(prefix="omochi-native-")
        self.gateways = {}
        self.services = {}
        self.invitations = {}
        self.logs = []
        self.online = False
        self.counts = {0: 1, 1: 1}
        self.image_size = 0
        self.model_bytes = 0

    def root(self, index):
        path = Path(self.temp.name) / str(index)
        path.mkdir(mode=0o700, exist_ok=True)
        return path

    def stop(self, processes, index):
        process = processes.pop(index, None)
        if process and process.poll() is None:
            process.send_signal(signal.SIGINT)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()

    def gateway(self, index):
        self.stop(self.gateways, index)
        root = self.root(index)
        log = open(root / "gateway.log", "ab")
        self.logs.append(log)
        process = subprocess.Popen(
            [
                str(CLI),
                "--config-dir",
                str(root / "config"),
                "--runtime-dir",
                str(root / "omochi"),
                "gateway",
                "--remote",
                "--pair",
                *([] if self.online else ["--offline"]),
                "--name",
                f"Native test {index + 1}",
            ],
            cwd=REPO,
            stdout=subprocess.PIPE,
            stderr=log,
            text=True,
        )
        self.gateways[index] = process
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError("Gateway failed to start; inspect fixture logs")
            ready, _, _ = select.select([process.stdout], [], [], 0.2)
            if ready:
                line = process.stdout.readline().strip()
                if line.startswith("omochi://invite/"):
                    self.invitations[index] = line
                    return
        raise TimeoutError("Gateway did not create an invitation")

    def stop_services(self, index):
        for key in list(self.services):
            if key[0] == index:
                self.stop(self.services, key)

    def spawn_service(self, index, slot):
        key = (index, slot)
        self.stop(self.services, key)
        root = self.root(index)
        log = open(root / f"sdk-{slot}.log", "ab")
        self.logs.append(log)
        extra = {}
        if self.image_size:
            size = self.image_size

            def chunk(kind, data):
                return (
                    struct.pack("!I", len(data))
                    + kind
                    + data
                    + struct.pack("!I", zlib.crc32(kind + data))
                )

            png = b"\x89PNG\r\n\x1a\n" + chunk(
                b"IHDR", struct.pack("!2I5B", size, size, 8, 6, 0, 0, 0)
            )
            png += chunk(
                b"IDAT", zlib.compress((b"\0" + b"\x20\xb0\x6f\xff" * size) * size)
            ) + chunk(b"IEND", b"")
            path = root / "image.png"
            path.write_bytes(png)
            extra["OMOCHI_IMAGE_FILE"] = str(path)
        self.services[key] = subprocess.Popen(
            ["bun", "run", str(APP / "tool/sdk_fixture.ts")],
            cwd=REPO,
            env={
                **os.environ,
                "XDG_RUNTIME_DIR": str(root),
                "OMOCHI_SERVICE": "native-app" if slot == 0 else f"native-app-{slot}",
                "OMOCHI_IMAGE": "1",
                "OMOCHI_MODEL_BYTES": str(self.model_bytes),
                **extra,
            },
            stdout=log,
            stderr=log,
        )

    def service(self, index):
        self.stop_services(index)
        for slot in range(self.counts[index]):
            self.spawn_service(index, slot)

    def close(self):
        for index in (0, 1):
            self.stop_services(index)
            self.stop(self.gateways, index)
        for log in self.logs:
            log.close()
        self.temp.cleanup()


fixture = Fixture()
fixture_lock = threading.Lock()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass  # Never print bearer invitations.

    def do_POST(self):
        with fixture_lock:
            self.handle_fixture_request()

    def handle_fixture_request(self):
        try:
            if self.path == "/start":
                size = int(self.headers.get("Content-Length", "0"))
                if size > 1024:
                    raise ValueError("Invalid fixture payload")
                config = json.loads(self.rfile.read(size)) if size else {}
                count = config.get("services", 1)
                image_size = config.get("imageSize", 0)
                model_bytes = config.get("modelBytes", 0)
                if (
                    type(count) is not int
                    or not 1 <= count <= 6
                    or image_size not in (0, 1024, 2048)
                    or type(model_bytes) is not int
                    or not 0 <= model_bytes <= 1024 * 1024
                ):
                    raise ValueError("Invalid fixture load")
                fixture.counts = {0: count, 1: count}
                fixture.image_size = image_size
                fixture.model_bytes = model_bytes
                for index in (0, 1):
                    fixture.gateway(index)
                    fixture.service(index)
                value = {"invitations": [fixture.invitations[i] for i in (0, 1)]}
            else:
                kind, raw_index, action = self.path.strip("/").split("/")
                index = int(raw_index)
                if index not in (0, 1) or kind not in ("gateway", "service"):
                    raise ValueError("Invalid fixture target")
                if action == "stop":
                    if kind == "gateway":
                        fixture.stop(fixture.gateways, index)
                    else:
                        fixture.stop_services(index)
                elif action == "start":
                    getattr(fixture, kind)(index)
                elif action == "extra" and kind == "service":
                    slot = fixture.counts[index]
                    if slot >= 6:
                        raise ValueError("Fixture service limit")
                    fixture.spawn_service(index, slot)
                    fixture.counts[index] += 1
                elif action == "revoke" and kind == "gateway":
                    size = int(self.headers.get("Content-Length", "0"))
                    if size < 1 or size > 1024:
                        raise ValueError("Invalid fixture payload")
                    identity = json.loads(self.rfile.read(size))["identity"]
                    if not isinstance(identity, str) or not re.fullmatch(
                        "[a-f0-9]{64}", identity
                    ):
                        raise ValueError("Invalid fixture identity")
                    root = fixture.root(index)
                    subprocess.run(
                        [
                            str(CLI),
                            "--config-dir",
                            str(root / "config"),
                            "--runtime-dir",
                            str(root / "omochi"),
                            "trust",
                            "forget",
                            identity,
                        ],
                        stdout=subprocess.DEVNULL,
                        stderr=subprocess.DEVNULL,
                        check=True,
                        timeout=15,
                    )
                else:
                    raise ValueError("Invalid fixture action")
                value = {"ok": True}
            body = json.dumps(value).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        except Exception as error:
            self.send_error(500, type(error).__name__)


def interrupt(_signum, _frame):
    raise KeyboardInterrupt


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=5559)
    parser.add_argument(
        "--tls",
        action="store_true",
        help="HTTPS fixture for release validation; prints the temporary test CA path",
    )
    parser.add_argument(
        "--online",
        action="store_true",
        help="Enable iroh relays/address lookup for firewall/NAT tests",
    )
    args = parser.parse_args()
    fixture.online = args.online
    signal.signal(signal.SIGTERM, interrupt)
    signal.signal(signal.SIGINT, interrupt)
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    if args.tls:
        certificate = Path(fixture.temp.name) / "fixture-cert.pem"
        key = Path(fixture.temp.name) / "fixture-key.pem"
        subprocess.run(
            [
                "openssl",
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-keyout",
                str(key),
                "-out",
                str(certificate),
                "-days",
                "2",
                "-subj",
                "/CN=localhost",
                "-addext",
                "subjectAltName=IP:127.0.0.1,DNS:localhost",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=True,
        )
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(certificate, key)
        server.socket = context.wrap_socket(server.socket, server_side=True)
        print(f"Fixture CA: {certificate}", flush=True)
    print(
        f"Native fixture controller on 127.0.0.1:{args.port}; no HTTP Hub. Logs: {fixture.temp.name}",
        flush=True,
    )
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
        fixture.close()
