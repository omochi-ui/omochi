from __future__ import annotations

import asyncio
import json
import os
import re
from collections import deque
from collections.abc import AsyncIterator
from pathlib import Path
from typing import Any

from .receive_queue import ReceiveQueue, ReceiveQueueOverflowError
from .resources import is_resource, hello
from .snapshot import A2uiSnapshot
from .transport import MAX_FRAME_BYTES, Transport

# Mirrors the id constraint the hub applies, so "<service_id>/<surface_id>" stays splittable.
SERVICE_ID_PATTERN = re.compile(r"^[a-z0-9._-]{1,64}$")

# Includes each frame's UTF-8 encoding and trailing LF, through drain completion.
_MAX_PENDING_BYTES = 16 * 1024 * 1024


def _encode_frame(message: dict[str, Any]) -> bytes:
    return (json.dumps(message, separators=(",", ":"), ensure_ascii=False) + "\n").encode()


class _OutboundPeer:
    def __init__(self, writer: asyncio.StreamWriter):
        self.writer = writer
        # drain() normally resumes below a nonzero low watermark. Resume only
        # when empty, so releasing a frame never leaves uncharged buffered bytes.
        writer.transport.set_write_buffer_limits(high=0, low=0)
        self.pending_bytes = 0
        self.closed = False
        self._frames: deque[bytes] = deque()
        self._ready = asyncio.Event()
        self.task: asyncio.Task[None] | None = None

    def _reserve(self, frame: bytes) -> bool:
        if self.closed:
            return False
        if len(frame) > _MAX_PENDING_BYTES - self.pending_bytes:
            self.abort()
            return False
        self.pending_bytes += len(frame)
        return True

    def enqueue(self, frame: bytes) -> None:
        if self._reserve(frame):
            self._frames.append(frame)
            self._ready.set()

    def abort(self) -> None:
        if self.closed:
            return
        self.closed = True
        self._frames.clear()
        self.pending_bytes = 0
        self._ready.set()
        self.writer.transport.abort()
        if self.task is not None and self.task is not asyncio.current_task():
            self.task.cancel()

    async def _write(self, frame: bytes) -> None:
        self.writer.write(frame)
        await self.writer.drain()
        if not self.closed:
            self.pending_bytes -= len(frame)

    async def _replay(self, bootstrap: list[dict[str, Any]]) -> None:
        # Replay is captured before joining broadcasts, but encoded one frame at
        # a time so a healthy peer can replay snapshots larger than the budget.
        for message in bootstrap:
            frame = _encode_frame(message)
            # Live frames cannot pass replay. If they leave insufficient room
            # for the next replay frame, disconnect instead of exceeding the
            # bound or deadlocking while waiting for that live queue to drain.
            if not self._reserve(frame):
                return
            await self._write(frame)
            del frame

    async def pump(self, bootstrap: list[dict[str, Any]]) -> None:
        try:
            await self._replay(bootstrap)
            bootstrap.clear()
            while not self.closed:
                await self._ready.wait()
                while self._frames and not self.closed:
                    await self._write(self._frames.popleft())
                self._ready.clear()
        except (ConnectionError, OSError):
            pass
        finally:
            bootstrap.clear()
            self.abort()


def _is_client_message(message: Any) -> bool:
    """Validate the v0.9 envelope before it can reach the shared Runtime."""
    if not isinstance(message, dict) or message.get("version") != "v0.9":
        return False
    if ("action" in message) == ("error" in message):
        return False
    if "action" in message:
        action = message["action"]
        return (
            isinstance(action, dict)
            and all(isinstance(action.get(key), str) for key in ("name", "surfaceId", "sourceComponentId", "timestamp"))
            and isinstance(action.get("context"), dict)
        )
    error = message["error"]
    return (
        isinstance(error, dict) and "code" in error
        and isinstance(error.get("surfaceId"), str)
        and isinstance(error.get("message"), str)
        and (error["code"] != "VALIDATION_FAILED" or (
            isinstance(error.get("path"), str)
            and set(error) == {"code", "surfaceId", "path", "message"}
        ))
    )


def validate_service_id(service_id: str) -> str:
    # "." and ".." match the character class but would collapse or escape the service directory.
    if service_id in (".", "..") or not SERVICE_ID_PATTERN.fullmatch(service_id):
        raise ValueError(f"service_id must match {SERVICE_ID_PATTERN.pattern} and cannot be '.' or '..': {service_id!r}")
    return service_id


def omochi_runtime_root() -> Path:
    from ._native_local import default_root
    return default_root()

class LocalServerTransport(Transport):
    """Publishes one application through user-private local IPC for discovery.

    The shared native adapter writes
    ``<runtime_dir>/svc-<service_id>~/manifest.json`` and serves a Unix domain
    socket or Windows named pipe restricted to the current OS user.

    Like the TypeScript ``WebSocketServerTransport`` it fans one application out to every
    connected renderer and merges their inbound messages into a single stream.
    """

    def __init__(
        self,
        service_id: str,
        *,
        name: str | None = None,
        icon: str | None = None,
        runtime_dir: str | os.PathLike[str] | None = None,
        capabilities: set[str] | None = None,
    ):
        self._service_id = validate_service_id(service_id)
        self._name = name or self._service_id
        self._icon = icon
        self._root = Path(runtime_dir) if runtime_dir is not None else None
        self.capabilities = capabilities or set()
        self._snapshot = A2uiSnapshot()
        self._clients: set[_OutboundPeer] = set()
        self._client_tasks: set[asyncio.Task[Any]] = set()
        self._queue = ReceiveQueue()
        self._native_id: int | None = None
        self._accept_task: asyncio.Task[None] | None = None
        self._endpoint: dict[str, Any] | None = None
        self._directory: Path | None = None
        self._start_lock = asyncio.Lock()
        self._closed = False
        self._close_task: asyncio.Task[None] | None = None

    @property
    def directory(self) -> Path | None:
        return self._directory

    @property
    def endpoint(self) -> dict[str, Any] | None:
        return self._endpoint

    async def start(self) -> None:
        from ._native_local import request
        async with self._start_lock:
            if self._native_id is not None or self._closed:
                return
            options: dict[str, Any] = {"serviceId": self._service_id, "name": self._name, "icon": self._icon}
            if self._root is not None:
                options["runtimeDir"] = str(self._root)
            # Shield startup so cancellation cannot orphan a successful publication.
            starting = asyncio.create_task(request("publish", **options))
            try:
                result = json.loads(await asyncio.shield(starting))
            except asyncio.CancelledError:
                async def cleanup() -> None:
                    result = json.loads(await starting)
                    await request("close", result["id"])

                # Keep both result delivery and teardown independent of further
                # caller cancellations. Retain the lock until ownership is gone.
                cleaning = asyncio.create_task(cleanup())
                while not cleaning.done():
                    try:
                        await asyncio.shield(cleaning)
                    except asyncio.CancelledError:
                        pass
                cleaning.result()
                raise
            self._native_id = result["id"]
            self._directory = Path(result["directory"])
            self._endpoint = result["endpoint"]
            self._accept_task = asyncio.create_task(self._accept_native())

    async def _accept_native(self) -> None:
        from ._native_local import request, Reader, Writer
        assert self._native_id is not None
        try:
            while not self._closed:
                handle = json.loads(await request("accept", self._native_id))
                task = asyncio.create_task(self._accept(Reader(handle, MAX_FRAME_BYTES), Writer(handle)))
                self._client_tasks.add(task)
                task.add_done_callback(self._client_tasks.discard)
        except OSError:
            if not self._closed:
                self._queue.finish(discard=True)

    async def send(self, message: dict[str, Any]) -> None:
        await self.start()
        if not is_resource(message):
            self._snapshot.apply(message)
        frame = _encode_frame(message)
        for client in list(self._clients):
            client.enqueue(frame)

    async def receive(self) -> AsyncIterator[dict[str, Any]]:
        await self.start()
        while True:
            message = await self._queue.get()
            if message is None:
                return
            yield message

    async def stop_receiving(self) -> None:
        await self.close()

    async def close(self) -> None:
        if self._close_task is None:
            # Stop inbound work before teardown can join the client tasks.
            self._closed = True
            self._close_task = asyncio.create_task(self._close(asyncio.current_task()))
        await asyncio.shield(self._close_task)

    async def _close(self, origin: asyncio.Task[Any] | None) -> None:
        from ._native_local import request
        async with self._start_lock:
            if self._native_id is not None:
                handle, self._native_id = self._native_id, None
                await asyncio.shield(request("close", handle))
            tasks = self._client_tasks | ({self._accept_task} if self._accept_task else set())
            tasks.discard(asyncio.current_task())
            tasks.discard(origin)
            if tasks:
                await asyncio.gather(*tasks, return_exceptions=True)
            self._directory = None
            self._endpoint = None
            self._queue.finish(discard=True)

    async def _accept(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        if self._closed:
            writer.close()
            return
        task = asyncio.current_task()
        assert task is not None
        self._client_tasks.add(task)
        client = _OutboundPeer(writer)
        bootstrap = ([hello()] if self.resources else []) + self._snapshot.messages()
        self._clients.add(client)
        client.task = asyncio.create_task(client.pump(bootstrap))
        try:
            while not self._closed and not client.closed and (line := await reader.readline()):
                if self._closed or client.closed:
                    break
                text = line.decode().strip()
                if not text:
                    continue
                try:
                    message = json.loads(text)
                except ValueError:
                    break
                if self.resources and is_resource(message):
                    if message.get("kind") == "request":
                        client.enqueue(_encode_frame(self.resources.respond(message)))
                    continue
                if not _is_client_message(message):
                    break
                # Valid client error reports carry no action; keep the connection open.
                if "error" in message:
                    continue
                # A2UI extensions must not select the Runtime's legacy command API.
                message.pop("type", None)
                self._queue.put_nowait(message)
        except ReceiveQueueOverflowError as error:
            # No queued action may execute after an action was lost. Abort peers
            # immediately so their resource RPCs observe a disconnect as well.
            self._queue.finish(error, discard=True)
            for peer in self._clients:
                peer.abort()
            await self.close()
        except (ConnectionError, OSError, ValueError):
            pass
        finally:
            self._clients.discard(client)
            client.abort()
            await asyncio.gather(client.task, return_exceptions=True)
            self._client_tasks.discard(task)
