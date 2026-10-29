import json
import tempfile
import unittest
from pathlib import Path
from omochi_ui import ResourceRegistry

FIXTURE = json.loads((Path(__file__).resolve().parents[4] / "spec/fixtures/resources.json").read_text())

class ResourceTests(unittest.TestCase):
    def test_shared_contract_and_immutable_files(self):
        registry = ResourceRegistry(max_file_bytes=10, max_total_bytes=20)
        try:
            with tempfile.TemporaryDirectory() as root:
                path = Path(root) / "clip.mp4"
                path.write_text(FIXTURE["content"])
                uri = registry.file(path)
                path.write_text("changed")
                request = {"omochi": "resources/1", "kind": "request", "id": "test", "resource": uri.rsplit("/", 1)[1]}
                self.assertEqual(registry.respond({**request, "op": "stat"})["size"], 10)
                for read in FIXTURE["reads"]:
                    response = registry.respond({**request, "op": "read", "offset": read["offset"], "length": read["length"]})
                    self.assertEqual(response["status"], read["status"])
                    self.assertEqual(response.get("data"), read.get("data"))
                with self.assertRaises(ValueError): registry.bytes(b"x" * 11, FIXTURE["mime"])
                registry.release(uri)
                self.assertEqual(registry.respond({**request, "op": "stat"})["status"], 404)
                self.assertNotEqual(registry.bytes(b"0123456789", FIXTURE["mime"]), uri)
        finally:
            registry.close()

    def test_initial_render_failure_closes_registry(self):
        import asyncio
        from omochi_ui import App
        from omochi_ui.transport import CallbackTransport
        app = App({})
        uri = app.resources.bytes(b"media", "video/mp4")
        @app.surface("main")
        def broken(ui, state):
            raise ValueError("render failed")
        async def send(message): pass
        async def receive():
            if False: yield {}
        with self.assertRaisesRegex(ValueError, "render failed"):
            asyncio.run(app.start(CallbackTransport(send, receive)))
        self.assertEqual(app.resources.respond({"kind": "request", "id": "test", "op": "stat", "resource": uri.rsplit("/", 1)[1]})["status"], 404)
