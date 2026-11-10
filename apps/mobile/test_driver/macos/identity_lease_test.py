"""Run on macOS: python3 apps/mobile/test_driver/macos/identity_lease_test.py."""

import pathlib
import subprocess
import tempfile
import uuid

mobile = pathlib.Path(__file__).resolve().parents[2]
source = (mobile / "macos/Runner/MainFlutterWindow.swift").read_text()
lease = source[source.index("private enum IdentityLeases") :]
harness = """
let token = try IdentityLeases.acquire(namespace: CommandLine.arguments[1])
print("acquired")
fflush(stdout)
_ = readLine()
IdentityLeases.release(token: token)
"""
with tempfile.TemporaryDirectory() as directory:
    swift = pathlib.Path(directory) / "lease.swift"
    binary = pathlib.Path(directory) / "lease"
    swift.write_text(
        "import Foundation\nimport CryptoKit\nimport Darwin\n" + lease + harness
    )
    subprocess.run(["swiftc", str(swift), "-o", str(binary)], check=True)
    namespace = "omochi.test.lease." + str(uuid.uuid4())

    def start(name):
        return subprocess.Popen(
            [str(binary), name],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )

    for crash in (False, True):
        with start(namespace) as owner:
            assert owner.stdout.readline().strip() == "acquired"
            blocked = subprocess.run(
                [str(binary), namespace],
                check=False,
                input="",
                capture_output=True,
                text=True,
                timeout=10,
            )
            assert blocked.returncode != 0, "second process acquired the same identity"
            other = subprocess.run(
                [str(binary), namespace + ".other"],
                check=False,
                input="",
                capture_output=True,
                text=True,
                timeout=10,
            )
            assert other.returncode == 0, other.stderr
            if crash:
                owner.kill()
                owner.wait(timeout=10)
            else:
                owner.communicate("\n", timeout=10)
                assert owner.returncode == 0
        retry = subprocess.run(
            [str(binary), namespace],
            check=False,
            input="",
            capture_output=True,
            text=True,
            timeout=10,
        )
        assert retry.returncode == 0, retry.stderr
print("PASS: cross-process exclusion, separate identities, normal/crash release")
