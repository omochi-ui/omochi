from __future__ import annotations

import asyncio
import json
from itertools import product
import tempfile
import unittest

from omochi_ui import App, UnixSocketServerTransport, schema
from omochi_ui.receive_queue import (
    MAX_PENDING_BYTES, MAX_PENDING_MESSAGES, ReceiveQueue, ReceiveQueueOverflowError,
)
from omochi_ui.transport import CallbackTransport


def action_message(payload=""):
    return {
        "version": "v0.9",
        "action": {
            "name": "block", "surfaceId": "main", "sourceComponentId": "block",
            "timestamp": "2026-08-12T00:00:00.000Z", "context": {},
        },
        "metadata": {"a2uiClientDataModel": {
            "version": "v0.9", "surfaces": {"main": {"inputs": {"payload": payload}}},
        }},
    }


def encoded(message):
    return json.dumps(message, separators=(",", ":"), ensure_ascii=False).encode()


class ReceiveQueueTests(unittest.IsolatedAsyncioTestCase):
    def blocked_app(self):
        app = App({})
        entered, release = asyncio.Event(), asyncio.Event()
        calls = []

        @app.action("block", schema=schema.literal(None))
        async def block(input, context):
            calls.append(input)
            entered.set()
            await release.wait()

        @app.surface("main")
        def main(ui, state):
            ui.button("Block", block.call(None), key="block")

        uri = app.resources.bytes(b"media", "video/mp4")
        stat = {"omochi": "resources/1", "kind": "request", "id": "barrier",
                "op": "stat", "resource": uri.rsplit("/", 1)[1]}
        return app, entered, release, calls, stat

    async def test_unix_overflow_serves_resources_then_disconnects_and_fails(self):
        for budget, ending in product(("count", "bytes"), ("overflow", "shutdown")):
            with self.subTest(budget=budget, ending=ending), tempfile.TemporaryDirectory() as root:
                message = action_message("é" * (512 * 1024) if budget == "bytes" else "")
                capacity = min(MAX_PENDING_MESSAGES, MAX_PENDING_BYTES // len(encoded(message)))
                app, entered, release, calls, stat = self.blocked_app()
                transport = UnixSocketServerTransport("queue", runtime_dir=root)
                await transport.start()
                running = asyncio.create_task(app.start(transport))
                reader, writer = await asyncio.open_unix_connection(str(transport.socket_path))
                try:
                    writer.write(encoded(action_message()) + b"\n")
                    await writer.drain()
                    await asyncio.wait_for(entered.wait(), 5)
                    for _ in range(capacity):
                        writer.write(encoded(message) + b"\n")
                    writer.write(encoded(stat) + b"\n")
                    await writer.drain()
                    while True:
                        line = await asyncio.wait_for(reader.readline(), 5)
                        self.assertTrue(line, "disconnected before the queue reached capacity")
                        response = json.loads(line)
                        if response.get("id") == "barrier":
                            self.assertEqual(response["status"], 200)
                            self.assertEqual(response["size"], 5)
                            break
                    self.assertEqual(calls, [None])
                    if ending == "overflow":
                        writer.write(encoded(message) + b"\n")
                        await writer.drain()
                    else:
                        await asyncio.wait_for(transport.close(), 5)
                    self.assertEqual(await asyncio.wait_for(reader.read(), 5), b"")
                    release.set()
                    if ending == "overflow":
                        with self.assertRaises(ReceiveQueueOverflowError):
                            await asyncio.wait_for(running, 5)
                    else:
                        await asyncio.wait_for(running, 5)
                    self.assertEqual(calls, [None])
                    await asyncio.wait_for(transport.close(), 5)
                    self.assertIsNone(transport.directory)
                finally:
                    release.set()
                    writer.close()
                    await writer.wait_closed()
                    await transport.close()
                    await asyncio.gather(running, return_exceptions=True)

    async def test_resource_pump_overflow_closes_source_and_fails(self):
        for budget in ("count", "bytes"):
            with self.subTest(budget=budget):
                message = action_message("é" * (512 * 1024) if budget == "bytes" else "")
                capacity = min(MAX_PENDING_MESSAGES, MAX_PENDING_BYTES // len(encoded(message)))
                app, entered, release, calls, stat = self.blocked_app()
                responded, overflow, source_closed = asyncio.Event(), asyncio.Event(), asyncio.Event()

                async def send(frame):
                    if frame.get("id") == "barrier":
                        self.assertEqual(frame["status"], 200)
                        responded.set()

                async def source():
                    try:
                        yield action_message()
                        await entered.wait()
                        for _ in range(capacity):
                            yield message
                        yield stat
                        await overflow.wait()
                        yield message
                        self.fail("pump must stop consuming after overflow")
                    finally:
                        source_closed.set()

                running = asyncio.create_task(app.start(CallbackTransport(send, source)))
                try:
                    await asyncio.wait_for(responded.wait(), 5)
                    self.assertEqual(calls, [None])
                    overflow.set()
                    await asyncio.wait_for(source_closed.wait(), 5)
                    release.set()
                    with self.assertRaises(ReceiveQueueOverflowError):
                        await asyncio.wait_for(running, 5)
                    self.assertEqual(calls, [None])
                finally:
                    overflow.set()
                    release.set()
                    await asyncio.gather(running, return_exceptions=True)

    async def test_dequeue_and_close_release_capacity_without_a_sentinel_slot(self):
        queue = ReceiveQueue()
        value = {"value": "é" * ((MAX_PENDING_BYTES - len(encoded({"value": ""}))) // 2)}
        queue.put_nowait(value)
        with self.assertRaises(ReceiveQueueOverflowError):
            queue.put_nowait({})
        self.assertEqual(await queue.get(), value)
        queue.put_nowait(value)
        queue.finish(discard=True)
        self.assertIsNone(await queue.get())
        self.assertEqual(queue._bytes, 0)

        queue = ReceiveQueue()
        for _ in range(MAX_PENDING_MESSAGES):
            queue.put_nowait({})
        with self.assertRaises(ReceiveQueueOverflowError):
            queue.put_nowait({})
        self.assertEqual(await queue.get(), {})
        queue.put_nowait({})
        queue.finish(discard=True)
        self.assertIsNone(await queue.get())

    async def test_resource_pump_shutdown_cancels_a_pending_read(self):
        waiting, source_closed = asyncio.Event(), asyncio.Event()

        async def source():
            try:
                yield action_message()
                waiting.set()
                await asyncio.Event().wait()
            finally:
                source_closed.set()

        async def send(frame):
            pass

        stream = CallbackTransport(send, source).receive()
        self.assertEqual(await anext(stream), action_message())
        await asyncio.wait_for(waiting.wait(), 5)
        await asyncio.wait_for(stream.aclose(), 5)
        self.assertTrue(source_closed.is_set())
