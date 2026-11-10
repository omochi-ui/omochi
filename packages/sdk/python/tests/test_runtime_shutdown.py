from __future__ import annotations

import asyncio
import tempfile
import unittest
from pathlib import Path

from omochi_ui import App, CallbackTransport, LocalServerTransport, schema


class RuntimeShutdownTests(unittest.IsolatedAsyncioTestCase):
    def runtime(self):
        state = {"calls": 0}
        app = App(state)

        @app.action("increment", schema=schema.literal(None))
        async def increment(_, context):
            await context.update(lambda state: state.update(calls=state["calls"] + 1))

        runtime = app.build()
        runtime.resources.bytes(b"one", "image/png")
        return runtime, state

    async def closed(self, runtime, started):
        await asyncio.wait_for(runtime.wait_closed(), 2)
        await asyncio.wait_for(started, 2)
        with self.assertRaisesRegex(ValueError, "resource registry closed"):
            runtime.resources.bytes(b"", "image/png")

    async def test_shutdown_cancels_idle_custom_read_without_closing_caller_transport(self):
        runtime, state = self.runtime()
        reading, canceled = asyncio.Event(), asyncio.Event()
        queue = asyncio.Queue()

        class CustomTransport:
            capabilities = set()
            closed = False

            async def send(self, message): pass

            async def receive(self):
                try:
                    while True:
                        reading.set()
                        yield await queue.get()
                finally:
                    await runtime.shutdown()
                    canceled.set()

            async def close(self): self.closed = True

        transport = CustomTransport()
        started = asyncio.create_task(runtime.start(transport))
        await asyncio.wait_for(reading.wait(), 2)
        await asyncio.wait_for(runtime.shutdown(), 2)
        await self.closed(runtime, started)
        queue.put_nowait({"type": "invoke", "action": "increment", "input": None})
        self.assertEqual(state["calls"], 0)
        self.assertTrue(canceled.is_set())
        self.assertFalse(transport.closed)
        await runtime.shutdown()

    async def test_callback_shutdown_closes_resource_pump_and_source(self):
        runtime, state = self.runtime()
        reading, canceled = asyncio.Event(), asyncio.Event()
        queue = asyncio.Queue()
        sent = []

        async def receive():
            try:
                while True:
                    reading.set()
                    yield await queue.get()
            finally:
                canceled.set()

        async def send(message): sent.append(message)

        started = asyncio.create_task(runtime.start(CallbackTransport(send, receive)))
        await asyncio.wait_for(reading.wait(), 2)
        await asyncio.wait_for(runtime.shutdown(), 2)
        await self.closed(runtime, started)
        self.assertTrue(canceled.is_set())
        self.assertEqual(state["calls"], 0)
        self.assertEqual(len(sent), 1)

    async def test_action_can_shutdown_and_discard_queued_actions(self):
        app = App({})
        calls = []

        @app.action("stop", schema=schema.literal(None))
        async def stop(_, context):
            calls.append("stop")
            await runtime.shutdown()

        @app.action("late", schema=schema.literal(None))
        async def late(_, context): calls.append("late")

        runtime = app.build()

        async def receive():
            for action in ["stop", "late"]:
                yield {"type": "invoke", "action": action, "input": None}

        async def send(message): pass

        await asyncio.wait_for(runtime.start(CallbackTransport(send, receive)), 2)
        self.assertEqual(calls, ["stop"])

    async def test_unix_shutdown_disconnects_clients_and_retracts_publication(self):
        runtime, state = self.runtime()
        with tempfile.TemporaryDirectory(prefix="omochi-shutdown-") as root:
            transport = LocalServerTransport("app", runtime_dir=root)
            started = asyncio.create_task(runtime.start(transport))
            try:
                await asyncio.wait_for(runtime.ready(), 2)
                socket_path = transport.endpoint["path"]
                reader, writer = await asyncio.open_unix_connection(str(socket_path))
                try:
                    # Receiving the hello proves the server has accepted this client.
                    await asyncio.wait_for(reader.readline(), 2)
                    await asyncio.wait_for(runtime.shutdown(), 2)
                    await self.closed(runtime, started)
                    self.assertEqual(await asyncio.wait_for(reader.read(), 2), b"")
                    self.assertFalse((Path(root) / "app").exists())
                    with self.assertRaises(FileNotFoundError):
                        await asyncio.open_unix_connection(str(socket_path))
                    self.assertEqual(state["calls"], 0)
                finally:
                    writer.close()
                    await writer.wait_closed()
            finally:
                await transport.close()
                if not started.done():
                    started.cancel()
                    await asyncio.gather(started, return_exceptions=True)

    async def test_background_task_finally_can_repeat_shutdown(self):
        runtime, _ = self.runtime()
        entered, cleaned = asyncio.Event(), asyncio.Event()

        async def worker():
            try:
                entered.set()
                await asyncio.Event().wait()
            finally:
                await runtime.shutdown()
                cleaned.set()

        runtime.spawn(worker())
        await entered.wait()
        await asyncio.wait_for(runtime.shutdown(), 2)
        await runtime.wait_closed()
        self.assertTrue(cleaned.is_set())

    async def test_background_task_can_request_shutdown(self):
        runtime, _ = self.runtime()
        task = runtime.spawn(runtime.shutdown())
        await asyncio.wait_for(task, 2)
        await runtime.wait_closed()

    async def test_cleanup_error_still_closes_resources_and_preserves_receive_error(self):
        for fail_receive in [False, True]:
            with self.subTest(fail_receive=fail_receive):
                runtime, _ = self.runtime()
                reading, cleaned = asyncio.Event(), asyncio.Event()
                primary = RuntimeError("receive failed")
                cleanup = RuntimeError("stop failed")

                class CustomTransport:
                    capabilities = set()

                    async def send(self, message): pass

                    async def receive(self):
                        reading.set()
                        if fail_receive: raise primary
                        await asyncio.Event().wait()
                        yield {}

                    async def stop_receiving(self): raise cleanup

                async def worker():
                    try: await asyncio.Event().wait()
                    finally: cleaned.set()

                runtime.spawn(worker())
                started = asyncio.create_task(runtime.start(CustomTransport()))
                await asyncio.wait_for(reading.wait(), 2)
                if not fail_receive:
                    with self.assertRaisesRegex(RuntimeError, "stop failed"):
                        await asyncio.wait_for(runtime.shutdown(), 2)
                with self.assertRaises(RuntimeError) as result:
                    await asyncio.wait_for(started, 2)
                self.assertIs(result.exception, primary if fail_receive else cleanup)
                await runtime.wait_closed()
                self.assertTrue(cleaned.is_set())
                with self.assertRaisesRegex(ValueError, "resource registry closed"):
                    runtime.resources.bytes(b"", "image/png")
