"""Headless discovery regression for the real mobile Gateway/SDK fixture."""

import json
import os
import select
import subprocess
import time
import unittest
import urllib.error
import urllib.request
from unittest.mock import patch
from urllib.parse import urljoin

import native_fixture


class NativeFixtureTests(unittest.TestCase):
    def test_gateways_discover_their_sdks_with_an_inherited_runtime_override(self):
        fixture = native_fixture.fixture
        hubs = {}
        self.addCleanup(fixture.close)
        self.addCleanup(lambda: [fixture.stop(hubs, index) for index in list(hubs)])
        inherited = fixture.root(2) / "unrelated-runtime"
        with patch.dict(os.environ, {"OMOCHI_RUNTIME_DIR": str(inherited)}):
            for index in (0, 1):
                fixture.gateway(index)
                fixture.service(index)
                hub = subprocess.Popen(
                    [
                        str(native_fixture.CLI),
                        "--config-dir",
                        str(fixture.root(index) / "hub-config"),
                        "--runtime-dir",
                        str(fixture.runtime(index)),
                        "hub",
                        "--no-auto-gateway",
                        "--offline",
                        "--port",
                        "0",
                    ],
                    cwd=native_fixture.REPO,
                    env=fixture.environment(index),
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                hubs[index] = hub
                deadline = time.monotonic() + 20
                url = None
                while time.monotonic() < deadline:
                    self.assertIsNone(hub.poll(), "Fixture Hub exited before readiness")
                    ready, _, _ = select.select([hub.stderr], [], [], 0.1)
                    if ready:
                        line = hub.stderr.readline().strip()
                        if line.startswith("[omochi-ui] dashboard "):
                            url = line.removeprefix("[omochi-ui] dashboard ")
                            break
                self.assertIsNotNone(url, "Fixture Hub did not become ready")
                opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
                services = []
                while time.monotonic() < deadline:
                    try:
                        with opener.open(
                            urljoin(url, "/services"), timeout=1
                        ) as response:
                            services = json.load(response)
                        if any(
                            service["id"].endswith("~native-app")
                            for service in services
                        ):
                            break
                    except (OSError, urllib.error.URLError):
                        pass
                    time.sleep(0.05)
                self.assertEqual(
                    [service["id"].split("~", 1)[1] for service in services],
                    ["native-app"],
                )
                self.assertTrue(
                    (fixture.runtime(index) / "svc-native-app~/manifest.json").is_file()
                )
            self.assertFalse(inherited.exists(), "Fixture used the inherited runtime")


if __name__ == "__main__":
    unittest.main()
