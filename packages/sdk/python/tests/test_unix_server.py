from __future__ import annotations

import asyncio
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from omochi_ui import (
    App,
    LocalServerTransport,
    ResourceRegistry,
    schema,
)
from omochi_ui.local_server import validate_service_id


def counter_app() -> App:
    app = App({"count": 0})

    @app.action("increment", schema=schema.literal(None))
    async def increment(input, context):
        await context.update(lambda current: current.update(count=current["count"] + 1))

    @app.surface("main")
    def main(ui, current):
        ui.text(f"Count: {current['count']}", key="count")
        ui.button("Increment", increment.call(None), key="increment")

    return app


async def read_messages(reader: asyncio.StreamReader, count: int) -> list[dict]:
    messages = []
    while len(messages) < count:
        line = await asyncio.wait_for(reader.readline(), timeout=5)
        if not line:
            raise AssertionError("the service closed the connection")
        if line.strip():
            message = json.loads(line)
            if message.get("omochi") != "resources/1":
                messages.append(message)
    return messages


class LocalServerTransportTests(unittest.TestCase):
    def test_isolates_malformed_client_frames(self) -> None:
        fixtures = Path(__file__).parents[4] / "spec/fixtures"
        actions = json.loads((fixtures / "client-actions.json").read_text())
        errors = json.loads((fixtures / "client-errors.json").read_text())

        async def scenario() -> None:
            with tempfile.TemporaryDirectory() as root:
                transport = LocalServerTransport("demo", runtime_dir=root)
                resources = ResourceRegistry()
                transport.resources = resources
                uri = resources.bytes(b"media", "image/png")
                writers = []

                async def connect():
                    reader, writer = await asyncio.open_unix_connection(
                        str(transport.endpoint["path"])
                    )
                    writers.append(writer)
                    self.assertEqual(
                        json.loads(await reader.readline())["kind"], "hello"
                    )
                    return reader, writer

                def send(writer, message):
                    writer.write((json.dumps(message) + "\n").encode())

                try:
                    await transport.start()
                    reader, writer = await connect()
                    received = transport.receive()

                    async def verify(action):
                        send(writer, action)
                        expected = {
                            key: value for key, value in action.items() if key != "type"
                        }
                        self.assertEqual(
                            await asyncio.wait_for(anext(received), 5), expected
                        )
                        send(
                            writer,
                            {
                                "omochi": "resources/1",
                                "kind": "request",
                                "id": "read",
                                "op": "stat",
                                "resource": uri.rsplit("/", 1)[1],
                            },
                        )
                        self.assertEqual(
                            json.loads(await asyncio.wait_for(reader.readline(), 5))[
                                "status"
                            ],
                            200,
                        )

                    for message in actions["invalid"] + errors["invalid"]:
                        bad_reader, bad_writer = await connect()
                        tail = {
                            **actions["actions"][0],
                            "action": {
                                **actions["actions"][0]["action"],
                                "name": "disconnected-peer",
                            },
                        }
                        bad_writer.write(
                            (
                                json.dumps(message) + "\n" + json.dumps(tail) + "\n"
                            ).encode()
                        )
                        self.assertEqual(
                            await asyncio.wait_for(bad_reader.readline(), 5),
                            b"",
                            message,
                        )
                        await verify(actions["actions"][0])
                    for report in errors["reports"]:
                        send(writer, report)
                        await verify(actions["actions"][0])
                    for action in actions["actions"]:
                        await verify(action)
                finally:
                    for writer in writers:
                        writer.close()
                        await writer.wait_closed()
                    await transport.close()
                    resources.close()

        asyncio.run(scenario())

    def test_publishes_and_cleans_up_a_discoverable_service(self) -> None:
        async def scenario() -> None:
            with tempfile.TemporaryDirectory() as root:
                transport = LocalServerTransport(
                    "demo", name="Demo", icon="🎤", runtime_dir=root
                )
                running = asyncio.create_task(counter_app().start(transport))
                await transport.start()

                directory = Path(root) / "svc-demo~"
                self.assertEqual(transport.directory, directory)
                self.assertEqual(
                    json.loads((directory / "manifest.json").read_text()),
                    {
                        "version": 2,
                        "serviceId": "demo",
                        "name": "Demo",
                        "endpoint": {"kind": "unix", "path": "a2ui.sock"},
                        "icon": "🎤",
                    },
                )
                self.assertEqual(directory.stat().st_mode & 0o777, 0o700)

                reader, writer = await asyncio.open_unix_connection(
                    str(transport.endpoint["path"])
                )
                initial = await read_messages(reader, 3)
                self.assertEqual(initial[0]["createSurface"]["surfaceId"], "main")
                writer.close()

                await transport.close()
                await running
                self.assertFalse(directory.exists())

        asyncio.run(scenario())

    def test_fans_out_and_snapshots_for_a_late_client(self) -> None:
        async def scenario() -> None:
            with tempfile.TemporaryDirectory() as root:
                transport = LocalServerTransport("demo", runtime_dir=root)
                running = asyncio.create_task(counter_app().start(transport))
                await transport.start()
                path = str(transport.endpoint["path"])

                reader, writer = await asyncio.open_unix_connection(path)
                await read_messages(reader, 3)
                writer.write(
                    (
                        json.dumps(
                            {
                                "version": "v0.9",
                                "action": {
                                    "name": "increment",
                                    "surfaceId": "main",
                                    "sourceComponentId": "increment",
                                    "timestamp": "2026-08-12T00:00:00.000Z",
                                    "context": {},
                                },
                            }
                        )
                        + "\n"
                    ).encode()
                )
                await writer.drain()
                await read_messages(reader, 1)

                late_reader, late_writer = await asyncio.open_unix_connection(path)
                replay = await read_messages(late_reader, 3)
                components = next(m for m in replay if "updateComponents" in m)
                labels = {
                    c["id"]: c.get("text")
                    for c in components["updateComponents"]["components"]
                }
                self.assertEqual(labels["count"], "Count: 1")

                writer.close()
                late_writer.close()
                await transport.close()
                await running

        asyncio.run(scenario())

    def test_validates_the_entire_service_id(self) -> None:
        for service_id in ("demo", "demo_1.2-3", "x" * 64):
            with self.subTest(service_id=service_id):
                self.assertEqual(validate_service_id(service_id), service_id)
        for service_id in ("Demo", "demo\n", "demo\r\n", "x" * 65):
            with self.subTest(service_id=service_id), self.assertRaises(ValueError):
                validate_service_id(service_id)

    def test_rejects_invalid_service_ids_before_publication(self) -> None:
        async def scenario() -> None:
            for service_id in (".", "..", "a/b", "", "x" * 65, "demo\n", "demo\r\n"):
                with (
                    self.subTest(service_id=service_id),
                    tempfile.TemporaryDirectory() as root,
                ):
                    runtime_dir = Path(root) / "runtime"
                    transport = None
                    try:
                        with self.assertRaises(ValueError):
                            transport = LocalServerTransport(
                                service_id, runtime_dir=runtime_dir
                            )
                            await transport.start()
                        self.assertFalse(runtime_dir.exists())
                    finally:
                        if transport is not None:
                            await transport.close()

        asyncio.run(scenario())

    def test_sdk_import_does_not_require_unix_fcntl(self) -> None:
        subprocess.run(
            [
                sys.executable,
                "-c",
                """
import builtins
original = builtins.__import__
def without_fcntl(name, *args, **kwargs):
    if name == 'fcntl':
        raise ModuleNotFoundError('No module named fcntl')
    return original(name, *args, **kwargs)
builtins.__import__ = without_fcntl
import omochi_ui
""",
            ],
            check=True,
        )

    def test_refuses_a_second_instance_and_recovers_a_stale_socket(self) -> None:
        async def scenario() -> None:
            with tempfile.TemporaryDirectory() as root:
                first = LocalServerTransport("demo", runtime_dir=root)
                running = asyncio.create_task(counter_app().start(first))
                await first.start()

                duplicate = LocalServerTransport("demo", runtime_dir=root)
                with self.assertRaises(OSError):
                    await duplicate.start()
                await duplicate.close()

                await first.close()
                await running

                # A socket file left behind by a crash must not block the next start.
                (Path(root) / "svc-demo~").mkdir(exist_ok=True)
                import socket

                stale = socket.socket(socket.AF_UNIX)
                stale.bind(str(Path(root) / "svc-demo~" / "a2ui.sock"))
                stale.close()
                restarted = LocalServerTransport("demo", runtime_dir=root)
                second = asyncio.create_task(counter_app().start(restarted))
                await restarted.start()
                self.assertIsNotNone(restarted.endpoint["path"])
                await restarted.close()
                await second

        asyncio.run(scenario())


if __name__ == "__main__":
    unittest.main()
