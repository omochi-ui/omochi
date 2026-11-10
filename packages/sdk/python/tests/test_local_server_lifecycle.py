from __future__ import annotations

import asyncio
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from omochi_ui import LocalServerTransport
from omochi_ui._native_local import request as native_request


class LocalServerLifecycleTests(unittest.IsolatedAsyncioTestCase):
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
