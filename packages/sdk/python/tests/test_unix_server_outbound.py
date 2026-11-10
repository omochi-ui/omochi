from __future__ import annotations

import asyncio
import json
import tempfile
import unittest
from unittest.mock import patch

from omochi_ui import ResourceRegistry, LocalServerTransport
from omochi_ui.local_server import _MAX_PENDING_BYTES, _OutboundPeer, _encode_frame


class ControlledWriter:
    def __init__(self):
        self.transport = self
        self.frames = []
        self.started = asyncio.Queue()
        self.releases = asyncio.Queue()
        self.aborted = False
        self.watermarks = None

    def set_write_buffer_limits(self, *, high, low):
        self.watermarks = (high, low)

    def write(self, frame):
        self.frames.append(frame)

    async def drain(self):
        self.started.put_nowait(None)
        await self.releases.get()

    def abort(self):
        self.aborted = True


async def wait_until(predicate):
    async with asyncio.timeout(10):
        while not predicate():
            await asyncio.sleep(0)


def create(surface):
    return {"version": "v0.9", "createSurface": {"surfaceId": surface, "catalogId": "test"}}


def update(surface, value):
    return {"version": "v0.9", "updateDataModel": {"surfaceId": surface, "path": "/", "value": value}}


class UnixSocketOutboundTests(unittest.IsolatedAsyncioTestCase):
    async def test_budget_charges_utf8_newline_and_active_write_until_drain(self):
        writer = ControlledWriter()
        peer = _OutboundPeer(writer)
        self.assertEqual(writer.watermarks, (0, 0))
        unicode_frame = _encode_frame({"text": "🎤"})
        self.assertEqual(len(unicode_frame), len('{"text":"🎤"}') + 4)
        budget = len(unicode_frame) * 2
        with patch("omochi_ui.local_server._MAX_PENDING_BYTES", budget):
            peer.task = asyncio.create_task(peer.pump([]))
            peer.enqueue(unicode_frame)
            await asyncio.wait_for(writer.started.get(), 5)
            self.assertEqual(peer.pending_bytes, len(unicode_frame))
            peer.enqueue(unicode_frame)
            self.assertEqual(peer.pending_bytes, budget)
            self.assertFalse(peer.closed)
            # Removing the active frame from the queue must not free its budget.
            peer.enqueue(b"\n")
            self.assertTrue(peer.closed)
            self.assertTrue(writer.aborted)
            self.assertEqual(peer.pending_bytes, 0)
            self.assertFalse(peer._frames)
            await asyncio.gather(peer.task, return_exceptions=True)
            self.assertEqual(writer.frames, [unicode_frame])

    async def test_drain_releases_budget_and_oversized_frame_aborts(self):
        writer = ControlledWriter()
        peer = _OutboundPeer(writer)
        with patch("omochi_ui.local_server._MAX_PENDING_BYTES", 8):
            peer.task = asyncio.create_task(peer.pump([]))
            peer.enqueue(b"1234567\n")
            await asyncio.wait_for(writer.started.get(), 5)
            writer.releases.put_nowait(None)
            await wait_until(lambda: peer.pending_bytes == 0)
            peer.enqueue(b"12345678\n")
            self.assertTrue(peer.closed)
            await asyncio.gather(peer.task, return_exceptions=True)

    async def test_bootstrap_is_streamed_before_live_and_resource_frames(self):
        writer = ControlledWriter()
        peer = _OutboundPeer(writer)
        bootstrap = [{"n": n} for n in range(8)]
        expected = [_encode_frame(message) for message in bootstrap]
        live = _encode_frame({"live": 1})
        resource = _encode_frame({"resource": 1})
        with patch("omochi_ui.local_server._MAX_PENDING_BYTES", 40):
            peer.task = asyncio.create_task(peer.pump(bootstrap))
            await asyncio.wait_for(writer.started.get(), 5)
            peer.enqueue(live)
            peer.enqueue(resource)
            for index in range(len(expected) + 2):
                self.assertLessEqual(peer.pending_bytes, 40)
                writer.releases.put_nowait(None)
                if index < len(expected) + 1:
                    await asyncio.wait_for(writer.started.get(), 5)
            await wait_until(lambda: peer.pending_bytes == 0)
            self.assertEqual(writer.frames, expected + [live, resource])
            self.assertFalse(peer.closed)
            peer.abort()
            await asyncio.gather(peer.task, return_exceptions=True)

    async def test_bootstrap_transition_aborts_when_live_leaves_no_room(self):
        writer = ControlledWriter()
        peer = _OutboundPeer(writer)
        with patch("omochi_ui.local_server._MAX_PENDING_BYTES", 20):
            peer.task = asyncio.create_task(peer.pump([{}, {"long": "1234567"}]))
            await asyncio.wait_for(writer.started.get(), 5)
            peer.enqueue(b"123456789\n")
            writer.releases.put_nowait(None)
            await asyncio.wait_for(peer.task, 5)
            self.assertTrue(writer.aborted)
            self.assertEqual(writer.frames, [b"{}\n"])
            self.assertEqual(peer.pending_bytes, 0)

    async def test_close_cancels_a_blocked_writer_and_discards_queued_frames(self):
        with tempfile.TemporaryDirectory() as root:
            transport = LocalServerTransport("demo", runtime_dir=root)
            entered = asyncio.Event()
            writer = None

            async def blocked_drain(_writer):
                entered.set()
                await asyncio.Event().wait()

            try:
                await transport.start()
                with patch("omochi_ui._native_local.Writer.drain", blocked_drain):
                    _, writer = await asyncio.open_unix_connection(str(transport.endpoint["path"]))
                    await wait_until(lambda: bool(transport._clients))
                    peer = next(iter(transport._clients))
                    await transport.send(create("main"))
                    await asyncio.wait_for(entered.wait(), 5)
                    await transport.send(update("main", "queued"))
                    self.assertTrue(peer._frames)
                    await asyncio.wait_for(transport.close(), 5)
                    self.assertTrue(peer.closed)
                    self.assertTrue(peer.task.done())
                    self.assertEqual(peer.pending_bytes, 0)
                    self.assertFalse(peer._frames)
                    self.assertFalse(transport._client_tasks)
            finally:
                if writer is not None:
                    writer.transport.abort()
                await transport.close()

    async def test_paused_reader_is_evicted_while_healthy_peer_and_reconnect_progress(self):
        with tempfile.TemporaryDirectory() as root:
            transport = LocalServerTransport("demo", runtime_dir=root)
            resources = ResourceRegistry()
            transport.resources = resources
            writers = []
            try:
                await transport.send(create("main"))
                path = str(transport.endpoint["path"])
                slow_reader, slow_writer = await asyncio.open_unix_connection(path, limit=_MAX_PENDING_BYTES)
                writers.append(slow_writer)
                self.assertEqual(json.loads(await slow_reader.readline())["kind"], "hello")
                await slow_reader.readline()
                slow_peer = next(iter(transport._clients))
                slow_writer.transport.pause_reading()
                reader, writer = await asyncio.open_unix_connection(path, limit=_MAX_PENDING_BYTES)
                writers.append(writer)
                await reader.readline()
                await reader.readline()
                payload = "x" * (1024 * 1024)
                for index in range(40):
                    await transport.send(update("main", {"index": index, "payload": payload}))
                    message = json.loads(await asyncio.wait_for(reader.readline(), 10))
                    self.assertEqual(message["updateDataModel"]["value"]["index"], index)
                    for peer in transport._clients:
                        if peer.pending_bytes == 0:
                            self.assertEqual(peer.writer.transport.get_write_buffer_size(), 0)
                    self.assertLessEqual(slow_peer.pending_bytes, _MAX_PENDING_BYTES)
                    self.assertLessEqual(slow_peer.writer.transport.get_write_buffer_size(), _MAX_PENDING_BYTES)
                self.assertTrue(slow_peer.closed)
                await wait_until(lambda: slow_peer not in transport._clients)
                self.assertEqual(slow_peer.pending_bytes, 0)
                self.assertFalse(slow_peer._frames)
                self.assertTrue(slow_peer.task.done())
                late_reader, late_writer = await asyncio.open_unix_connection(path, limit=_MAX_PENDING_BYTES)
                writers.append(late_writer)
                await late_reader.readline()
                await late_reader.readline()
                latest = json.loads(await asyncio.wait_for(late_reader.readline(), 10))
                self.assertEqual(latest["updateDataModel"]["value"]["index"], 39)
                await transport.close()
                self.assertFalse(transport._clients)
                self.assertFalse(transport._client_tasks)
                successor = LocalServerTransport("demo", runtime_dir=root)
                try:
                    await successor.start()
                    await transport.close()
                    successor_reader, successor_writer = await asyncio.open_unix_connection(str(successor.endpoint["path"]))
                    writers.append(successor_writer)
                    await successor.send(create("successor"))
                    self.assertEqual(json.loads(await asyncio.wait_for(successor_reader.readline(), 5))["createSurface"]["surfaceId"], "successor")
                finally:
                    await successor.close()
            finally:
                for writer in writers:
                    writer.transport.abort()
                await transport.close()
                resources.close()

    async def test_real_large_bootstrap_keeps_captured_replay_before_updates_and_resources(self):
        with tempfile.TemporaryDirectory() as root:
            transport = LocalServerTransport("demo", runtime_dir=root)
            resources = ResourceRegistry()
            transport.resources = resources
            uri = resources.bytes(b"media", "image/png")
            entered, release = asyncio.Event(), asyncio.Event()
            original_write = _OutboundPeer._write

            async def pause_hello(peer, frame):
                if json.loads(frame).get("kind") == "hello":
                    entered.set()
                    await release.wait()
                await original_write(peer, frame)

            writer = None
            try:
                payload = "🎤" * (256 * 1024)
                for index in range(20):
                    await transport.send(create(str(index)))
                    await transport.send(update(str(index), payload))
                with patch.object(_OutboundPeer, "_write", pause_hello):
                    reader, writer = await asyncio.open_unix_connection(str(transport.endpoint["path"]), limit=_MAX_PENDING_BYTES)
                    await asyncio.wait_for(entered.wait(), 5)
                    peer = next(iter(transport._clients))
                    await transport.send(update("0", "live"))
                    writer.write(_encode_frame({"omochi": "resources/1", "kind": "request", "id": "stat", "op": "stat", "resource": uri.rsplit("/", 1)[1]}))
                    await writer.drain()
                    await wait_until(lambda: len(peer._frames) == 2)
                    release.set()
                    self.assertEqual(json.loads(await asyncio.wait_for(reader.readline(), 5))["kind"], "hello")
                    for index in range(20):
                        self.assertEqual(json.loads(await asyncio.wait_for(reader.readline(), 5)), create(str(index)))
                        self.assertEqual(json.loads(await asyncio.wait_for(reader.readline(), 5)), update(str(index), payload))
                        self.assertLessEqual(peer.pending_bytes, _MAX_PENDING_BYTES)
                    self.assertEqual(json.loads(await asyncio.wait_for(reader.readline(), 5)), update("0", "live"))
                    response = json.loads(await asyncio.wait_for(reader.readline(), 5))
                    self.assertEqual((response["id"], response["status"]), ("stat", 200))
                    self.assertFalse(peer.closed)
            finally:
                release.set()
                if writer is not None:
                    writer.transport.abort()
                await transport.close()
                resources.close()


if __name__ == "__main__":
    unittest.main()
