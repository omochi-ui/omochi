from __future__ import annotations

import asyncio
import hashlib
import json
import sys
import tempfile
import unittest

from omochi_ui import UnixSocketServerTransport


MAX_FRAME_BYTES = 4 * 1024 * 1024


def action(value: str) -> dict:
    return {
        "version": "v0.9",
        "action": {
            "name": "submit",
            "surfaceId": "main",
            "sourceComponentId": "submit",
            "timestamp": "2026-11-04T00:00:00Z",
            "context": {"input": value},
        },
    }


def encode(message: dict) -> bytes:
    return json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode()


def sized_action(size: int) -> dict:
    # Non-ASCII input makes the byte limit observably different from a character limit.
    available = size - len(encode(action("")))
    return action("あ" * (available // 3) + "x" * (available % 3))


def accepted_frames():
    yield "small", action("x" * 100), b"\n"
    yield "large ASCII", action("x" * 70_000), b"\n"
    yield "large UTF-8", action("あ" * 70_000), b"\n"
    yield "below limit", sized_action(MAX_FRAME_BYTES - 1), b"\n"
    yield "exact limit", sized_action(MAX_FRAME_BYTES), b"\n"
    yield "CRLF at limit", sized_action(MAX_FRAME_BYTES - 1), b"\r\n"
    yield "EOF at limit", sized_action(MAX_FRAME_BYTES), b""


def rejected_frames():
    yield "over limit", encode(sized_action(MAX_FRAME_BYTES + 1)) + b"\n"
    yield "CR counts toward limit", encode(sized_action(MAX_FRAME_BYTES)) + b"\r\n"
    yield "unterminated over limit", encode(sized_action(MAX_FRAME_BYTES + 1))


STDIO_RECEIVER = """
import asyncio
import hashlib
from omochi_ui import StdioTransport

async def main():
    try:
        async for message in StdioTransport().receive():
            value = message['action']['context']['input']
            print(hashlib.sha256(value.encode()).hexdigest(), flush=True)
    except ValueError:
        print('REJECTED', flush=True)

asyncio.run(main())
"""


class JsonlLimitTests(unittest.IsolatedAsyncioTestCase):
    async def start_stdio(self):
        process = await asyncio.create_subprocess_exec(
            sys.executable,
            "-c",
            STDIO_RECEIVER,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )

        async def cleanup():
            if process.returncode is None:
                process.kill()
            await process.wait()

        self.addAsyncCleanup(cleanup)
        return process

    async def test_stdio_accepts_large_and_boundary_frames(self):
        for name, message, ending in accepted_frames():
            with self.subTest(name=name):
                process = await self.start_stdio()
                stdout, stderr = await asyncio.wait_for(
                    process.communicate(encode(message) + ending),
                    timeout=10,
                )
                expected = hashlib.sha256(
                    message["action"]["context"]["input"].encode()
                ).hexdigest()
                self.assertEqual(stdout.decode().splitlines(), [expected])
                self.assertEqual(process.returncode, 0, stderr.decode())
                self.assertEqual(stderr, b"")

    async def test_stdio_rejects_oversized_frames_without_waiting_for_eof(self):
        for name, frame in rejected_frames():
            with self.subTest(name=name):
                process = await self.start_stdio()
                process.stdin.write(frame)
                await asyncio.wait_for(process.stdin.drain(), timeout=10)
                # Keep stdin open: rejection must happen even without a delimiter or EOF.
                self.assertEqual(
                    await asyncio.wait_for(process.stdout.readline(), timeout=10),
                    b"REJECTED\n",
                )
                await asyncio.wait_for(process.wait(), timeout=10)
                self.assertEqual(await process.stdout.read(), b"")
                self.assertEqual(await process.stderr.read(), b"")
                self.assertEqual(process.returncode, 0)

    async def test_unix_accepts_large_and_boundary_frames(self):
        with tempfile.TemporaryDirectory() as root:
            transport = UnixSocketServerTransport("demo", runtime_dir=root)
            try:
                await transport.start()
                for name, message, ending in accepted_frames():
                    with self.subTest(name=name):
                        reader, writer = await asyncio.open_unix_connection(
                            str(transport.socket_path)
                        )
                        try:
                            writer.write(encode(message) + ending)
                            await asyncio.wait_for(writer.drain(), timeout=10)
                            if not ending:
                                writer.write_eof()
                            self.assertEqual(
                                await asyncio.wait_for(
                                    anext(transport.receive()), timeout=10
                                ),
                                message,
                            )
                        finally:
                            writer.close()
                            await writer.wait_closed()
            finally:
                await transport.close()

    async def test_unix_rejects_only_the_oversized_connection(self):
        for name, frame in rejected_frames():
            with self.subTest(name=name), tempfile.TemporaryDirectory() as root:
                transport = UnixSocketServerTransport("demo", runtime_dir=root)
                disconnected = asyncio.Event()
                errors = []
                accept = transport._accept

                async def observe_accept(reader, writer):
                    try:
                        await accept(reader, writer)
                    except Exception as error:
                        errors.append(error)
                    finally:
                        disconnected.set()

                transport._accept = observe_accept
                try:
                    await transport.start()
                    reader, writer = await asyncio.open_unix_connection(
                        str(transport.socket_path)
                    )
                    try:
                        writer.write(frame)
                        await asyncio.wait_for(writer.drain(), timeout=10)
                        await asyncio.wait_for(disconnected.wait(), timeout=10)
                        self.assertEqual(
                            await asyncio.wait_for(reader.read(), timeout=10), b""
                        )
                        self.assertEqual(errors, [])
                        self.assertTrue(transport._queue.empty())
                    finally:
                        writer.close()
                        await writer.wait_closed()

                    # A real message received on another connection is the liveness barrier.
                    reader, writer = await asyncio.open_unix_connection(
                        str(transport.socket_path)
                    )
                    try:
                        message = action("still alive")
                        writer.write(encode(message) + b"\n")
                        await writer.drain()
                        self.assertEqual(
                            await asyncio.wait_for(
                                anext(transport.receive()), timeout=10
                            ),
                            message,
                        )
                    finally:
                        writer.close()
                        await writer.wait_closed()
                finally:
                    await transport.close()
