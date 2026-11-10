from __future__ import annotations

import asyncio
import json
import tempfile
import unittest
from pathlib import Path

from omochi_ui import LocalServerTransport


class LocalServerMetadataTests(unittest.TestCase):
    def test_publishes_large_unicode_metadata_and_cleans_up(self) -> None:
        async def scenario() -> None:
            with tempfile.TemporaryDirectory(prefix="om-") as temp:
                root = Path(temp) / "v2"
                # Valid UTF-8 metadata exceeds the native command limit if ASCII-escaped.
                name = "団子 Demo"
                icon = "🍡" * 12000
                transport = LocalServerTransport("unicode", name=name, icon=icon, runtime_dir=root)
                directory = root / "svc-unicode~"
                try:
                    await transport.start()
                    self.assertEqual(transport.directory, directory)
                    encoded = (directory / "manifest.json").read_bytes()
                    self.assertLessEqual(len(encoded), 128 * 1024)
                    manifest = json.loads(encoded)
                    self.assertEqual(manifest["version"], 2)
                    self.assertEqual(manifest["serviceId"], "unicode")
                    self.assertEqual(manifest["name"], name)
                    self.assertEqual(manifest["icon"], icon)
                finally:
                    await transport.close()
                self.assertFalse(directory.exists())

        asyncio.run(scenario())


if __name__ == "__main__":
    unittest.main()
