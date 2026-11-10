from __future__ import annotations

import asyncio
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

from omochi_ui import LocalServerTransport
from omochi_ui._native_local import request as native_request
from omochi_ui.receive_queue import MAX_PENDING_MESSAGES


def action_frame() -> bytes:
    message = {
        "version": "v0.9",
        "action": {
            "name": "click",
            "surfaceId": "main",
            "sourceComponentId": "button",
            "timestamp": "2026-11-11T00:00:00.000Z",
            "context": {},
        },
    }
    return (json.dumps(message) + "\n").encode()


class LocalServerLifecycleTests(unittest.IsolatedAsyncioTestCase):
    async def test_close_joins_a_runnable_client_with_buffered_actions(self) -> None:
        transport = LocalServerTransport("closing")
        reader = asyncio.StreamReader()
        reader.feed_data(action_frame() * (MAX_PENDING_MESSAGES + 1))
        reader.feed_eof()
        writer = Mock()
        # Both tasks are runnable before the event loop resumes: close() must
        # stop inbound processing before its separately scheduled teardown.
        closing = asyncio.create_task(transport.close())
        accepting = asyncio.create_task(transport._accept(reader, writer))
        try:
            async with asyncio.timeout(5):
                await asyncio.gather(asyncio.shield(closing), asyncio.shield(accepting))
            self.assertFalse(transport._client_tasks)
            self.assertIsNone(await transport._queue.get())
            writer.close.assert_called_once()
        finally:
            closing.cancel()
            accepting.cancel()
            await asyncio.gather(closing, accepting, return_exceptions=True)
            await transport.close()

    async def test_close_joins_a_client_whose_pending_read_completes(self) -> None:
        with tempfile.TemporaryDirectory(prefix="om-") as root:
            transport = LocalServerTransport("closing", runtime_dir=root)
            await transport.start()
            directory = transport.directory
            reader = asyncio.StreamReader()
            frame = action_frame()
            reader.feed_data(frame * MAX_PENDING_MESSAGES)
            reading = asyncio.Event()
            delivered = asyncio.Event()
            closing_native = asyncio.Event()
            native_can_close = asyncio.Event()
            reads = 0
            readline = reader.readline

            async def controlled_readline():
                nonlocal reads
                reads += 1
                if reads == MAX_PENDING_MESSAGES + 1:
                    reading.set()
                    line = await readline()
                    delivered.set()
                    return line
                return await readline()

            async def controlled_request(op, handle=0, data=b"", **options):
                if op == "close":
                    closing_native.set()
                    await native_can_close.wait()
                return await native_request(op, handle, data, **options)

            writer = Mock()
            closing = None
            with (
                patch.object(reader, "readline", controlled_readline),
                patch(
                    "omochi_ui._native_local.request",
                    controlled_request,
                ),
            ):
                accepting = asyncio.create_task(transport._accept(reader, writer))
                try:
                    async with asyncio.timeout(5):
                        await reading.wait()
                        closing = asyncio.create_task(transport.close())
                        await closing_native.wait()
                        # Complete the read only after shutdown is in progress.
                        reader.feed_data(frame)
                        reader.feed_eof()
                        await delivered.wait()
                        native_can_close.set()
                        await asyncio.gather(
                            asyncio.shield(closing), asyncio.shield(accepting)
                        )
                    self.assertFalse(transport._client_tasks)
                    self.assertIsNone(await transport._queue.get())
                    self.assertIsNotNone(directory)
                    self.assertFalse(directory.exists())
                    writer.transport.abort.assert_called_once()
                finally:
                    native_can_close.set()
                    tasks = [accepting] + ([closing] if closing is not None else [])
                    for task in tasks:
                        task.cancel()
                    await asyncio.gather(*tasks, return_exceptions=True)
                    await transport.close()

    async def test_close_waits_for_inflight_publication(self) -> None:
        with tempfile.TemporaryDirectory(prefix="om-") as root:
            transport = LocalServerTransport("closing", runtime_dir=root)
            directory = Path(root) / "svc-closing~"
            published = asyncio.Event()
            publishing_can_return = asyncio.Event()
            close_entered = asyncio.Event()

            async def controlled_request(op, handle=0, data=b"", **options):
                result = await native_request(op, handle, data, **options)
                if op == "publish":
                    published.set()
                    await publishing_can_return.wait()
                return result

            async def close_transport():
                close_entered.set()
                await transport.close()

            with patch("omochi_ui._native_local.request", controlled_request):
                starting = asyncio.create_task(transport.start())
                closing = None
                try:
                    async with asyncio.timeout(5):
                        await published.wait()
                        closing = asyncio.create_task(close_transport())
                        await close_entered.wait()
                        self.assertFalse(closing.done())
                        publishing_can_return.set()
                        await asyncio.gather(
                            asyncio.shield(starting), asyncio.shield(closing)
                        )
                    self.assertFalse(directory.exists())
                    self.assertIsNone(transport.directory)
                    successor = LocalServerTransport("closing", runtime_dir=root)
                    try:
                        await successor.start()
                    finally:
                        await successor.close()
                finally:
                    publishing_can_return.set()
                    tasks = [starting] + ([closing] if closing is not None else [])
                    for task in tasks:
                        task.cancel()
                    await asyncio.gather(*tasks, return_exceptions=True)
                    await transport.close()

    async def check_start_cancellation(self, stage: str) -> None:
        with tempfile.TemporaryDirectory(prefix="om-") as root:
            transport = LocalServerTransport("cancelled", runtime_dir=root)
            directory = Path(root) / "svc-cancelled~"
            published = asyncio.Event()
            publishing_can_return = asyncio.Event()
            closing = asyncio.Event()
            closing_can_finish = asyncio.Event()
            handles: list[int] = []

            async def controlled_request(op, handle=0, data=b"", **options):
                if op == "publish":
                    result = await native_request(op, handle, data, **options)
                    handles.append(json.loads(result)["id"])
                    # Native publication has succeeded, but its result has not
                    # reached start() yet, just as with a pending FFI callback.
                    published.set()
                    await publishing_can_return.wait()
                    return result
                if op == "close":
                    closing.set()
                    await closing_can_finish.wait()
                return await native_request(op, handle, data, **options)

            try:
                async with asyncio.timeout(5):
                    with patch("omochi_ui._native_local.request", controlled_request):
                        starting = asyncio.create_task(transport.start())
                        await published.wait()
                        self.assertTrue((directory / "manifest.json").is_file())
                        starting.cancel()
                        # Deliver the first cancellation at the startup await.
                        await asyncio.sleep(0)
                        if stage == "cleanup":
                            publishing_can_return.set()
                            await closing.wait()
                        # close() must serialize behind this cancelled start's
                        # cleanup, rather than declare teardown complete early.
                        closed = asyncio.create_task(transport.close())
                        await asyncio.sleep(0)
                        starting.cancel()
                        await asyncio.sleep(0)
                        publishing_can_return.set()
                        closing_can_finish.set()
                        with self.assertRaises(asyncio.CancelledError):
                            await starting
                        await closed
                    self.assertFalse(directory.exists())
                    # Cleanup must release ownership as well as remove the files.
                    successor = LocalServerTransport("cancelled", runtime_dir=root)
                    try:
                        await successor.start()
                    finally:
                        await successor.close()
            finally:
                publishing_can_return.set()
                closing_can_finish.set()
                for handle in handles:
                    await native_request("close", handle)
                await transport.close()

    async def test_repeated_cancellation_keeps_publication_cleanup(self) -> None:
        await self.check_start_cancellation("publication")

    async def test_cancellation_during_startup_cleanup_keeps_cleanup(self) -> None:
        await self.check_start_cancellation("cleanup")


if __name__ == "__main__":
    unittest.main()
