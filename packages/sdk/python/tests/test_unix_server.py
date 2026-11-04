from __future__ import annotations

import asyncio
import json
import os
import tempfile
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

from omochi_ui import schema, Expr, InputSchema, ActionContext, UI, App, ResourceRegistry, UnixSocketServerTransport
from omochi_ui.unix_server import validate_service_id


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


class UnixSocketServerTransportTests(unittest.TestCase):
    def test_isolates_malformed_client_frames(self) -> None:
        fixtures = Path(__file__).parents[4] / "spec/fixtures"
        actions = json.loads((fixtures / "client-actions.json").read_text())
        errors = json.loads((fixtures / "client-errors.json").read_text())

        async def scenario() -> None:
            with tempfile.TemporaryDirectory() as root:
                transport = UnixSocketServerTransport("demo", runtime_dir=root)
                resources = ResourceRegistry()
                transport.resources = resources
                uri = resources.bytes(b"media", "image/png")
                writers = []

                async def connect():
                    reader, writer = await asyncio.open_unix_connection(str(transport.socket_path))
                    writers.append(writer)
                    self.assertEqual(json.loads(await reader.readline())["kind"], "hello")
                    return reader, writer

                def send(writer, message):
                    writer.write((json.dumps(message) + "\n").encode())

                try:
                    await transport.start()
                    reader, writer = await connect()
                    received = transport.receive()

                    async def verify(action):
                        send(writer, action)
                        expected = {key: value for key, value in action.items() if key != "type"}
                        self.assertEqual(await asyncio.wait_for(anext(received), 5), expected)
                        send(writer, {"omochi": "resources/1", "kind": "request", "id": "read", "op": "stat", "resource": uri.rsplit("/", 1)[1]})
                        self.assertEqual(json.loads(await asyncio.wait_for(reader.readline(), 5))["status"], 200)

                    for message in actions["invalid"] + errors["invalid"]:
                        bad_reader, bad_writer = await connect()
                        tail = {**actions["actions"][0], "action": {**actions["actions"][0]["action"], "name": "disconnected-peer"}}
                        bad_writer.write((json.dumps(message) + "\n" + json.dumps(tail) + "\n").encode())
                        self.assertEqual(await asyncio.wait_for(bad_reader.readline(), 5), b"", message)
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

    def test_close_releases_ownership_with_an_inherited_file_description(self) -> None:
        async def scenario() -> None:
            with tempfile.TemporaryDirectory() as root:
                owner = UnixSocketServerTransport("demo", runtime_dir=root)
                successor = UnixSocketServerTransport("demo", runtime_dir=root)
                contender = UnixSocketServerTransport("demo", runtime_dir=root)
                await owner.start()
                assert owner._lock_fd is not None
                # dup and fork retain the same open file description. Keep one
                # alive through close, independent of subprocess scheduling.
                inherited = os.dup(owner._lock_fd)
                try:
                    await owner.close()
                    await successor.start()
                finally:
                    os.close(inherited)
                try:
                    owner._cleanup_sync()
                    with self.assertRaisesRegex(RuntimeError, "another omochi service"):
                        await contender.start()
                    reader, writer = await asyncio.open_unix_connection(str(successor.socket_path))
                    writer.close()
                    await writer.wait_closed()
                finally:
                    await contender.close()
                    await successor.close()

        asyncio.run(scenario())

    def test_close_waits_for_start_and_retracts_the_service(self) -> None:
        async def scenario() -> None:
            with tempfile.TemporaryDirectory() as root:
                transport = UnixSocketServerTransport("demo", runtime_dir=root)
                binding = asyncio.Event()
                release_bind = asyncio.Event()
                closing = asyncio.Event()
                start_unix_server = asyncio.start_unix_server
                servers = []

                async def blocked_bind(*args, **kwargs):
                    binding.set()
                    await release_bind.wait()
                    server = await start_unix_server(*args, **kwargs)
                    servers.append(server)
                    return server

                async def close() -> None:
                    closing.set()
                    await transport.close()

                with patch("omochi_ui.unix_server.asyncio.start_unix_server", blocked_bind):
                    started = asyncio.create_task(transport.start())
                    await asyncio.wait_for(binding.wait(), timeout=5)
                    closed = asyncio.create_task(close())
                    await asyncio.wait_for(closing.wait(), timeout=5)
                    try:
                        self.assertFalse(closed.done())
                    finally:
                        release_bind.set()
                        await asyncio.wait_for(asyncio.gather(started, closed), timeout=5)

                await transport.close()
                await transport.start()
                self.assertFalse(servers[0].is_serving())
                self.assertIsNone(transport._server)
                self.assertFalse((Path(root) / "demo").exists())
                with self.assertRaises(OSError):
                    await asyncio.open_unix_connection(str(Path(root) / "demo" / "a2ui.sock"))

        asyncio.run(scenario())

    def test_close_after_bind_failure_is_idempotent(self) -> None:
        async def scenario() -> None:
            with tempfile.TemporaryDirectory() as root:
                transport = UnixSocketServerTransport("demo", runtime_dir=root)
                with patch("omochi_ui.unix_server.asyncio.start_unix_server", side_effect=OSError("bind failed")):
                    with self.assertRaisesRegex(OSError, "bind failed"):
                        await transport.start()
                await transport.close()
                await transport.close()
                await transport.start()
                self.assertIsNone(transport._server)
                self.assertFalse((Path(root) / "demo").exists())

        asyncio.run(scenario())

    def test_cancelled_close_still_releases_receive_and_retracts_the_service(self) -> None:
        async def scenario() -> None:
            with tempfile.TemporaryDirectory() as root:
                transport = UnixSocketServerTransport("demo", runtime_dir=root)
                await transport.start()
                server = transport._server
                assert server is not None
                waiting_for_message = asyncio.Event()
                waiting_for_close = asyncio.Event()
                queue_get = transport._queue.get

                async def get_message():
                    waiting_for_message.set()
                    return await queue_get()

                async def wait_closed() -> None:
                    waiting_for_close.set()
                    await asyncio.Event().wait()

                with patch.object(transport._queue, "get", get_message):
                    receiving = asyncio.create_task(anext(transport.receive()))
                    await asyncio.wait_for(waiting_for_message.wait(), timeout=5)
                    with patch.object(server, "wait_closed", wait_closed):
                        closing = asyncio.create_task(transport.close())
                        await asyncio.wait_for(waiting_for_close.wait(), timeout=5)
                        closing.cancel()
                        with self.assertRaises(asyncio.CancelledError):
                            await closing
                    await transport.close()
                    with self.assertRaises(StopAsyncIteration):
                        await asyncio.wait_for(receiving, timeout=5)

                await transport.start()
                self.assertFalse(server.is_serving())
                self.assertIsNone(transport._server)
                self.assertFalse((Path(root) / "demo").exists())

        asyncio.run(scenario())

    def test_publishes_and_cleans_up_a_discoverable_service(self) -> None:
        async def scenario() -> None:
            with tempfile.TemporaryDirectory() as root:
                transport = UnixSocketServerTransport("demo", name="Demo", icon="🎤", runtime_dir=root)
                running = asyncio.create_task(counter_app().start(transport))
                await transport.start()

                directory = Path(root) / "demo"
                self.assertEqual(transport.directory, directory)
                self.assertEqual(
                    json.loads((directory / "manifest.json").read_text()),
                    {"name": "Demo", "socket": "a2ui.sock", "icon": "🎤"},
                )
                self.assertEqual(directory.stat().st_mode & 0o777, 0o700)

                reader, writer = await asyncio.open_unix_connection(str(transport.socket_path))
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
                transport = UnixSocketServerTransport("demo", runtime_dir=root)
                running = asyncio.create_task(counter_app().start(transport))
                await transport.start()
                path = str(transport.socket_path)

                reader, writer = await asyncio.open_unix_connection(path)
                await read_messages(reader, 3)
                writer.write((json.dumps({
                    "version": "v0.9",
                    "action": {
                        "name": "increment",
                        "surfaceId": "main",
                        "sourceComponentId": "increment",
                        "timestamp": "2026-08-12T00:00:00.000Z",
                        "context": {},
                    },
                }) + "\n").encode())
                await writer.drain()
                await read_messages(reader, 1)

                late_reader, late_writer = await asyncio.open_unix_connection(path)
                replay = await read_messages(late_reader, 3)
                components = next(m for m in replay if "updateComponents" in m)
                labels = {c["id"]: c.get("text") for c in components["updateComponents"]["components"]}
                self.assertEqual(labels["count"], "Count: 1")

                writer.close()
                late_writer.close()
                await transport.close()
                await running

        asyncio.run(scenario())

    def test_validates_the_entire_service_id(self) -> None:
        for service_id in ("demo", "Demo_1.2-3", "x" * 64):
            with self.subTest(service_id=service_id):
                self.assertEqual(validate_service_id(service_id), service_id)
        for service_id in ("demo\n", "demo\r\n", "x" * 65):
            with self.subTest(service_id=service_id):
                with self.assertRaises(ValueError):
                    validate_service_id(service_id)

    def test_rejects_invalid_service_ids_before_publication(self) -> None:
        async def scenario() -> None:
            for service_id in (".", "..", "a/b", "", "x" * 65, "demo\n", "demo\r\n"):
                with self.subTest(service_id=service_id), tempfile.TemporaryDirectory() as root:
                    runtime_dir = Path(root) / "runtime"
                    transport = None
                    try:
                        with self.assertRaises(ValueError):
                            transport = UnixSocketServerTransport(service_id, runtime_dir=runtime_dir)
                            await transport.start()
                        self.assertFalse(runtime_dir.exists())
                    finally:
                        if transport is not None:
                            await transport.close()

        asyncio.run(scenario())

    def test_sdk_import_does_not_require_unix_fcntl(self) -> None:
        subprocess.run([sys.executable, "-c", """
import builtins
original = builtins.__import__
def without_fcntl(name, *args, **kwargs):
    if name == 'fcntl':
        raise ModuleNotFoundError('No module named fcntl')
    return original(name, *args, **kwargs)
builtins.__import__ = without_fcntl
import omochi_ui
"""], check=True)

    def test_ownership_survives_bind_and_cleanup_barriers(self) -> None:
        async def scenario() -> None:
            with tempfile.TemporaryDirectory() as root:
                first = UnixSocketServerTransport("demo", runtime_dir=root)
                paused, resume = asyncio.Event(), asyncio.Event()
                listen = first._listen

                async def paused_listen(directory):
                    paused.set()
                    await resume.wait()
                    await listen(directory)

                first._listen = paused_listen
                started = asyncio.create_task(first.start())
                await paused.wait()

                async def reject_contender():
                    contender = UnixSocketServerTransport("demo", runtime_dir=root)
                    with self.assertRaisesRegex(RuntimeError, "already owns"):
                        await contender.start()
                    await contender.close()

                await reject_contender()
                resume.set()
                await started
                await reject_contender()
                paused.clear()
                resume.clear()
                server = first._server
                wait_closed = server.wait_closed

                async def paused_close():
                    paused.set()
                    await resume.wait()
                    await wait_closed()

                server.wait_closed = paused_close
                stopped = asyncio.create_task(first.close())
                await paused.wait()
                await reject_contender()
                resume.set()
                await stopped
                successor = UnixSocketServerTransport("demo", runtime_dir=root)
                await successor.start()
                await first.close()
                first._cleanup_sync()
                reader, writer = await asyncio.open_unix_connection(str(successor.socket_path))
                writer.close()
                await writer.wait_closed()
                await successor.close()

        asyncio.run(scenario())

    def test_refuses_a_second_instance_and_recovers_a_stale_socket(self) -> None:
        async def scenario() -> None:
            with tempfile.TemporaryDirectory() as root:
                first = UnixSocketServerTransport("demo", runtime_dir=root)
                running = asyncio.create_task(counter_app().start(first))
                await first.start()

                duplicate = UnixSocketServerTransport("demo", runtime_dir=root)
                with self.assertRaises(RuntimeError):
                    await duplicate.start()
                await duplicate.close()

                await first.close()
                await running

                # A socket file left behind by a crash must not block the next start.
                (Path(root) / "demo").mkdir(exist_ok=True)
                (Path(root) / "demo" / "a2ui.sock").touch()
                restarted = UnixSocketServerTransport("demo", runtime_dir=root)
                second = asyncio.create_task(counter_app().start(restarted))
                await restarted.start()
                self.assertIsNotNone(restarted.socket_path)
                await restarted.close()
                await second

        asyncio.run(scenario())


if __name__ == "__main__":
    unittest.main()
