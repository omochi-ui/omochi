"""Cross-process publisher ownership regression. Build the TS SDK before running.

Runs Node, Bun, Python, Rust SDK and native utility publishers against the same
runtime directory. Every transition uses stdin/stdout barriers, with timeouts
only to detect hangs.
"""
from __future__ import annotations

import asyncio
import fcntl
import json
import os
from pathlib import Path
import selectors
import socket
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "packages/sdk/python/src"))


async def python_owner():
    from omochi_ui import UnixSocketServerTransport
    print("OWNER WAITING", flush=True)
    input()
    transport = UnixSocketServerTransport("demo", runtime_dir=os.environ["OMOCHI_OWNER_ROOT"])
    try:
        await transport.start()
    except Exception:
        await transport.close()
        print("OWNER REJECTED", flush=True)
        return
    print("OWNER READY", flush=True)
    await asyncio.to_thread(input)
    await transport.close()
    print("OWNER CLOSED", flush=True)
    await asyncio.to_thread(input)
    await transport.close()
    transport._cleanup_sync()


class Owner:
    def __init__(self, command, root):
        self.process = subprocess.Popen(command, cwd=ROOT, env={**os.environ, "OMOCHI_OWNER_ROOT": str(root)},
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.buffer = b""
        assert self.read() == "WAITING"

    def read(self):
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout, selectors.EVENT_READ)
            while True:
                while b"\n" in self.buffer:
                    line, self.buffer = self.buffer.split(b"\n", 1)
                    if b"OWNER " in line:
                        return line.split(b"OWNER ", 1)[1].decode()
                assert selector.select(15), "fixture response timed out"
                chunk = os.read(self.process.stdout.fileno(), 4096)
                assert chunk, self.process.stderr.read().decode()
                self.buffer += chunk

    def command(self):
        self.process.stdin.write(b"continue\n")
        self.process.stdin.flush()

    def finish(self):
        assert self.process.wait(timeout=15) == 0, self.process.stderr.read().decode()
        self.dispose()

    def dispose(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait(timeout=15)
        for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
            stream.close()


def connectable(root):
    manifest = json.loads((root / "demo/manifest.json").read_text())
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(5)
        client.connect(str(root / "demo" / manifest["socket"]))


def main():
    built = subprocess.run(["cargo", "test", "--manifest-path", "packages/sdk/rust/Cargo.toml", "--test", "unix_server",
                            "--no-run", "--message-format=json"], cwd=ROOT, check=True, text=True, capture_output=True)
    executable = next(item["executable"] for line in built.stdout.splitlines()
                      if (item := json.loads(line)).get("executable"))
    utility_built = subprocess.run(["cargo", "test", "--locked", "--manifest-path", "crates/Cargo.toml",
                                   "-p", "omochi-utils", "--lib", "--no-run", "--message-format=json"],
                                  cwd=ROOT, check=True, text=True, capture_output=True)
    utility_executable = next(item["executable"] for line in utility_built.stdout.splitlines()
                              if (item := json.loads(line)).get("executable"))
    commands = {
        "node": ["node", "tests/sdk/socket-owner.mjs"],
        "bun": ["bun", "tests/sdk/socket-owner.mjs"],
        "python": [sys.executable, __file__, "--child"],
        "rust": [executable, "--exact", "ownership_process_fixture", "--ignored", "--nocapture"],
        "native-utility": [utility_executable, "--exact", "local::tests::ownership_process_fixture",
                           "--ignored", "--nocapture"],
    }
    children = []
    try:
        with tempfile.TemporaryDirectory(prefix="omochi-owner-") as temporary:
            root = Path(temporary)
            # Pause an owner after stale recovery but before bind. Every publisher must
            # reject even though the socket is absent, and leave its directory intact.
            (root / "demo").mkdir()
            with (root / ".demo.lock~").open("a") as lock:
                os.fchmod(lock.fileno(), 0o600)
                lock_inode = os.fstat(lock.fileno()).st_ino
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                for command in commands.values():
                    contender = Owner(command, root)
                    children.append(contender)
                    contender.command()
                    assert contender.read() == "REJECTED"
                    contender.finish()
                    assert (root / "demo").is_dir()
            # Simultaneous starts in separate processes: exactly one ready result.
            racers = [Owner(command, root) for command in commands.values()]
            children.extend(racers)
            for racer in racers:
                racer.command()
            results = [racer.read() for racer in racers]
            assert results.count("READY") == 1, results
            for racer, result in zip(racers, results):
                if result == "REJECTED":
                    racer.finish()
            connectable(root)
            winner = racers[results.index("READY")]
            winner.command()
            assert winner.read() == "CLOSED"
            # Each publisher owns in turn. Others finish rejected close() while it remains
            # reachable. SIGKILL leaves a real stale socket and kernel-released lock.
            previous = winner
            for name, command in commands.items():
                owner = Owner(command, root)
                children.append(owner)
                owner.command()
                assert owner.read() == "READY", name
                if previous is not None:
                    previous.command()  # Old close/atexit after new ownership.
                    previous.finish()
                    previous = None
                for other_command in commands.values():
                    contender = Owner(other_command, root)
                    children.append(contender)
                    contender.command()
                    assert contender.read() == "REJECTED"
                    contender.finish()
                    connectable(root)
                owner.command()
                assert owner.read() == "CLOSED"
                replacement = Owner(command, root)
                children.append(replacement)
                replacement.command()
                assert replacement.read() == "READY", name
                owner.command()  # Every publisher's obsolete close/Drop is harmless.
                owner.finish()
                connectable(root)
                replacement.process.kill()
                replacement.process.wait(timeout=15)
                assert (root / "demo/a2ui.sock").exists()
                assert (root / ".demo.lock~").stat().st_ino == lock_inode
                print(f"{name}: publisher exclusion, obsolete cleanup and crash recovery passed")
            # Recover the final crashed native utility owner as well.
            final = Owner(commands["node"], root)
            children.append(final)
            final.command()
            assert final.read() == "READY"
            connectable(root)
            final.command()
            assert final.read() == "CLOSED"
            final.command()
            final.finish()
            assert (root / ".demo.lock~").stat().st_ino == lock_inode
    finally:
        for child in children:
            child.dispose()


if __name__ == "__main__":
    if "--child" in sys.argv:
        asyncio.run(python_owner())
    else:
        main()
