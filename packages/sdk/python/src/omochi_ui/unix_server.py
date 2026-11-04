from __future__ import annotations

import asyncio
import atexit
import errno
import json
import os
import re
import shutil
import socket as socket_module
import stat
import tempfile
from collections.abc import AsyncIterator
from contextlib import suppress
from pathlib import Path
from typing import Any

from .receive_queue import ReceiveQueue, ReceiveQueueOverflowError
from .resources import is_resource, hello
from .snapshot import A2uiSnapshot
from .transport import MAX_FRAME_BYTES, Transport

# Mirrors the id constraint the hub applies, so "<service_id>/<surface_id>" stays splittable.
SERVICE_ID_PATTERN = re.compile(r"^[A-Za-z0-9._-]{1,64}$")

# Linux caps sun_path at 108 bytes and macOS at 104; leave room for the null terminator.
_MAX_SOCKET_PATH_BYTES = 100

_SOCKET_FILE_NAME = "a2ui.sock"


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
    if service_id in (".", "..") or not SERVICE_ID_PATTERN.match(service_id):
        raise ValueError(f"service_id must match {SERVICE_ID_PATTERN.pattern} and cannot be '.' or '..': {service_id!r}")
    return service_id


def omochi_runtime_root() -> Path:
    """Resolves the directory holding one subdirectory per published service.

    ``$XDG_RUNTIME_DIR`` is per-user and mode 0700, and that permission is the entire
    local trust boundary. A world-writable ``/tmp/omochi`` is never used, because another
    local user could pre-create a service directory and squat the socket.
    """
    configured = os.environ.get("XDG_RUNTIME_DIR")
    if configured:
        return Path(configured) / "omochi"
    return Path(tempfile.gettempdir()) / f"omochi-{os.getuid()}" / "omochi"


def _is_socket_in_use(path: Path) -> bool:
    probe = socket_module.socket(socket_module.AF_UNIX, socket_module.SOCK_STREAM)
    probe.settimeout(0.5)
    try:
        probe.connect(str(path))
    except OSError as error:
        if error.errno not in (errno.ENOENT, errno.ECONNREFUSED):
            raise
        return False
    else:
        return True
    finally:
        probe.close()


class UnixSocketServerTransport(Transport):
    """Publishes one application on a unix domain socket and advertises it for discovery.

    The transport writes ``<runtime_dir>/<service_id>/manifest.json`` next to the socket
    so a hub can find it without configuration. Reaching the socket is the
    authentication: the runtime directory is mode 0700, so only the owning user can
    connect.

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
        self._root = Path(runtime_dir) if runtime_dir is not None else omochi_runtime_root()
        self.capabilities = capabilities or set()
        self._snapshot = A2uiSnapshot()
        self._writers: set[asyncio.StreamWriter] = set()
        self._queue = ReceiveQueue()
        self._server: asyncio.AbstractServer | None = None
        self._directory: Path | None = None
        self._lock_fd: int | None = None
        self._start_lock = asyncio.Lock()
        self._closed = False
        atexit.register(self._cleanup_sync)

    @property
    def directory(self) -> Path | None:
        return self._directory

    @property
    def socket_path(self) -> Path | None:
        return self._directory / _SOCKET_FILE_NAME if self._directory else None

    async def start(self) -> None:
        """Publishes the socket and manifest. Called automatically on first use."""
        async with self._start_lock:
            if self._server is not None or self._closed:
                return
            # Keep non-Unix transports importable on platforms without fcntl.
            import fcntl

            directory = self._root / self._service_id
            self._root.mkdir(parents=True, exist_ok=True, mode=0o700)
            self._root.chmod(0o700)
            # A permanent inode outside the removable service directory. Never unlink it.
            fd = os.open(self._root / f".{self._service_id}.lock~", os.O_CREAT | os.O_WRONLY, 0o600)
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BaseException as error:
                os.close(fd)
                if isinstance(error, BlockingIOError):
                    raise RuntimeError(f"another omochi service already owns {directory}") from error
                raise
            self._lock_fd = fd
            try:
                directory.mkdir(exist_ok=True, mode=0o700)
                await self._listen(directory)
            except BaseException:
                # Leave no empty directory behind for a discovery pass to walk into.
                # rmdir refuses a non-empty directory, so a live service is never touched.
                with suppress(OSError):
                    directory.rmdir()
                if self._directory is None:
                    self._release_lock()
                raise

    async def _listen(self, directory: Path) -> None:
        # mkdir's mode is masked by umask and leaves an existing directory untouched.
        self._root.chmod(0o700)
        directory.chmod(0o700)
        path = directory / _SOCKET_FILE_NAME
        if len(str(path).encode()) > _MAX_SOCKET_PATH_BYTES:
            raise ValueError(f"unix socket path is too long for this platform: {path}")
        if path.exists():
            # A path that still accepts connections means another instance owns this
            # service id. The lifetime flock serializes probe, bind and cleanup.
            if _is_socket_in_use(path):
                raise RuntimeError(f"another omochi service already owns {path}")
            path.unlink()
        self._server = await asyncio.start_unix_server(self._accept, path=str(path), limit=MAX_FRAME_BYTES)
        self._directory = directory
        # Linux enforces unix socket permissions on connect, and bind applies the umask.
        path.chmod(stat.S_IRUSR | stat.S_IWUSR)
        manifest: dict[str, Any] = {"name": self._name, "socket": _SOCKET_FILE_NAME}
        if self._icon is not None:
            manifest["icon"] = self._icon
        # Advertise only once the socket accepts connections, so a readable manifest
        # is always connectable.
        self._write_manifest(directory, manifest)

    async def send(self, message: dict[str, Any]) -> None:
        await self.start()
        if not is_resource(message):
            self._snapshot.apply(message)
        line = json.dumps(message, separators=(",", ":"), ensure_ascii=False) + "\n"
        for writer in list(self._writers):
            self._write(writer, line)

    async def receive(self) -> AsyncIterator[dict[str, Any]]:
        await self.start()
        while True:
            message = await self._queue.get()
            if message is None:
                return
            yield message

    async def close(self) -> None:
        if self._closed:
            return
        # A bind in progress must finish before its socket and manifest are removed.
        async with self._start_lock:
            if self._closed:
                return
            self._closed = True
            atexit.unregister(self._cleanup_sync)
            # Retract the advertisement before tearing the socket down.
            if self._directory is not None:
                shutil.rmtree(self._directory, ignore_errors=True)
            for writer in list(self._writers):
                writer.close()
            self._writers.clear()
            server = self._server
            self._server = None
            try:
                if server is not None:
                    server.close()
                    await server.wait_closed()
            finally:
                self._directory = None
                self._release_lock()
                # Cancellation must also release a consumer parked on receive().
                self._queue.finish(discard=True)

    @staticmethod
    def _write_manifest(directory: Path, manifest: dict[str, Any]) -> None:
        """Publishes atomically so a discovery pass never reads a half-written file."""
        temporary = directory / f".manifest.{os.getpid()}.tmp"
        temporary.write_text(json.dumps(manifest, ensure_ascii=False) + "\n")
        temporary.chmod(stat.S_IRUSR | stat.S_IWUSR)
        temporary.replace(directory / "manifest.json")

    @staticmethod
    def _write(writer: asyncio.StreamWriter, line: str) -> None:
        if writer.is_closing():
            return
        try:
            writer.write(line.encode())
        except OSError:
            pass

    async def _accept(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        if self._closed:
            writer.close()
            return
        self._writers.add(writer)
        try:
            if self.resources:
                self._write(writer, json.dumps(hello()) + "\n")
            for message in self._snapshot.messages():
                self._write(writer, json.dumps(message, separators=(",", ":"), ensure_ascii=False) + "\n")
            await writer.drain()
            while not self._closed and (line := await reader.readline()):
                text = line.decode().strip()
                if not text:
                    continue
                try:
                    message = json.loads(text)
                except ValueError:
                    break
                if self.resources and is_resource(message):
                    if message.get("kind") == "request":
                        self._write(writer, json.dumps(self.resources.respond(message)) + "\n")
                        await writer.drain()
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
            for client in self._writers:
                client.transport.abort()
            await self.close()
        except (ConnectionError, OSError, ValueError):
            pass
        finally:
            self._writers.discard(writer)
            writer.close()

    def _release_lock(self) -> None:
        if self._lock_fd is not None:
            import fcntl

            fd = self._lock_fd
            self._lock_fd = None
            try:
                # fork can retain this open file description until exec even with
                # CLOEXEC. Closing our fd alone must not delay a successor.
                fcntl.flock(fd, fcntl.LOCK_UN)
            finally:
                os.close(fd)

    def _cleanup_sync(self) -> None:
        """Last-resort cleanup: interpreter shutdown cannot await ``close()``."""
        if self._directory is not None:
            shutil.rmtree(self._directory, ignore_errors=True)
            self._directory = None
        self._release_lock()
