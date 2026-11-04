from __future__ import annotations

import asyncio
import json
from collections import deque
from typing import Any

# Pending actions exclude the action currently executing and resource RPC frames.
MAX_PENDING_MESSAGES = 256
MAX_PENDING_BYTES = 4 * 1024 * 1024


class ReceiveQueueOverflowError(RuntimeError):
    def __init__(self) -> None:
        super().__init__("pending action queue exceeded 256 messages or 4 MiB")


class ReceiveQueue:
    """Bound object count and compact UTF-8 JSON bytes without blocking resource RPC."""

    def __init__(self) -> None:
        self._entries: deque[tuple[dict[str, Any], int]] = deque()
        self._bytes = 0
        self._changed = asyncio.Event()
        self._done = False
        self._failure: Exception | None = None

    def put_nowait(self, message: dict[str, Any]) -> None:
        if self._done:
            return
        size = len(json.dumps(message, separators=(",", ":"), ensure_ascii=False).encode())
        if len(self._entries) >= MAX_PENDING_MESSAGES or self._bytes + size > MAX_PENDING_BYTES:
            raise ReceiveQueueOverflowError()
        self._entries.append((message, size))
        self._bytes += size
        self._changed.set()

    def empty(self) -> bool:
        return not self._entries

    async def get(self) -> dict[str, Any] | None:
        while True:
            if self._entries:
                message, size = self._entries.popleft()
                self._bytes -= size
                return message
            if self._failure is not None:
                raise self._failure
            if self._done:
                return None
            self._changed.clear()
            await self._changed.wait()

    def finish(self, error: Exception | None = None, *, discard: bool = False) -> None:
        self._done = True
        if error is not None:
            self._failure = error
        if discard:
            self._entries.clear()
            self._bytes = 0
        self._changed.set()
