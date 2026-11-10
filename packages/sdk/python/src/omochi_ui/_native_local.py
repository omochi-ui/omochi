"""C ABI adapter for protocol-independent local IPC; loaded on first local use."""
from __future__ import annotations
import asyncio
import atexit
import ctypes
import json
import os
import sys
import threading
from pathlib import Path
from typing import Any

_CALLBACK = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_int32, ctypes.c_void_p, ctypes.c_size_t)
_library: Any = None
_context: Any = None
_pending: dict[int, tuple[asyncio.AbstractEventLoop, asyncio.Future[bytes]]] = {}
_next = 0
_state_lock = threading.RLock()
_owner_pid = os.getpid()
_stopping = False

def _complete(token: int, code: int, pointer: int, length: int) -> None:
    with _state_lock:
        pending = _pending.pop(token, None)
    if pending is None:
        return
    loop, future = pending
    data = ctypes.string_at(pointer, length)
    def deliver() -> None:
        if not future.done():
            if code:
                future.set_exception(OSError(data.decode("utf-8", errors="replace")))
            else:
                future.set_result(data)
    if not loop.is_closed():
        loop.call_soon_threadsafe(deliver)

_callback = _CALLBACK(_complete)

def _load() -> None:
    if os.getpid() != _owner_pid:
        raise RuntimeError("Local native transport cannot be inherited across fork; initialize it in the child or use spawn")
    with _state_lock:
        if _stopping:
            raise RuntimeError("Local native runtime is shutting down")
        _load_locked()

def _load_locked() -> None:
    global _library, _context
    if _library is not None:
        return
    name = "omochi_local_ffi.dll" if sys.platform == "win32" else "libomochi_local_ffi.dylib" if sys.platform == "darwin" else "libomochi_local_ffi.so"
    configured = os.environ.get("OMOCHI_LOCAL_LIBRARY")
    path = Path(configured) if configured else Path(__file__).parent / "_native" / name
    if not path.is_file():
        raise RuntimeError("Missing Omochi local native library; install the platform wheel or build the SDK native adapters")
    library = ctypes.CDLL(str(path))
    library.omochi_local_abi_version.restype = ctypes.c_uint32
    if library.omochi_local_abi_version() != 1:
        raise RuntimeError("Incompatible Omochi local native ABI")
    library.omochi_local_runtime_root.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    library.omochi_local_runtime_root.restype = ctypes.c_ssize_t
    library.omochi_local_create_private_directory.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    library.omochi_local_create_private_directory.restype = ctypes.c_int32
    library.omochi_local_create.restype = ctypes.c_void_p
    library.omochi_local_submit.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_size_t, _CALLBACK, ctypes.c_void_p]
    library.omochi_local_submit.restype = ctypes.c_int32
    library.omochi_local_destroy.argtypes = [ctypes.c_void_p]
    context = library.omochi_local_create()
    if not context:
        raise RuntimeError("Could not initialize Omochi local native runtime")
    _library, _context = library, context
    atexit.register(_shutdown)

def _shutdown() -> None:
    global _context, _stopping
    if os.getpid() != _owner_pid:
        return
    with _state_lock:
        _stopping = True
        context, _context = _context, None
    if context is not None:
        _library.omochi_local_destroy(context)

async def request(op: str, handle: int = 0, data: bytes = b"", **options: Any) -> bytes:
    global _next
    _load()
    loop = asyncio.get_running_loop()
    future: asyncio.Future[bytes] = loop.create_future()
    command = json.dumps({"op": op, "id": handle, **options}).encode()
    with _state_lock:
        if _stopping:
            raise RuntimeError("Local native runtime is shutting down")
        _next += 1
        token = _next
        _pending[token] = loop, future
        if _library.omochi_local_submit(_context, command, len(command), data, len(data), _callback, token):
            _pending.pop(token, None)
            raise RuntimeError("Invalid native request")
    try:
        return await future
    except asyncio.CancelledError:
        # Discard the stream after a potentially partial read/write.
        if op in ("read", "write"):
            asyncio.create_task(request("close", handle))
        raise

class Reader:
    def __init__(self, handle: int, limit: int):
        self.handle, self.limit = handle, limit
        self.buffer = bytearray()
    async def readline(self) -> bytes:
        while True:
            end = self.buffer.find(b"\n")
            if end >= 0:
                if end > self.limit:
                    raise ValueError("local frame exceeds limit")
                line = bytes(self.buffer[:end + 1])
                del self.buffer[:end + 1]
                return line
            if len(self.buffer) > self.limit:
                raise ValueError("local frame exceeds limit")
            chunk = await request("read", self.handle)
            if not chunk:
                line = bytes(self.buffer)
                self.buffer.clear()
                return line
            self.buffer.extend(chunk)

class Writer:
    def __init__(self, handle: int):
        self.handle = handle
        self.transport = self
        self.frame = b""
        self.closed = False
    def get_write_buffer_size(self) -> int:
        return len(self.frame)
    def set_write_buffer_limits(self, **kwargs: Any) -> None:
        pass
    def write(self, frame: bytes) -> None:
        if self.closed:
            raise ConnectionError("local stream closed")
        if self.frame:
            raise RuntimeError("drain the preceding frame before writing")
        self.frame = frame
    async def drain(self) -> None:
        frame, self.frame = self.frame, b""
        for offset in range(0, len(frame), 65536):
            await request("write", self.handle, frame[offset:offset + 65536])
    def abort(self) -> None:
        if not self.closed:
            self.closed = True
            self.frame = b""
            asyncio.create_task(request("close", self.handle))
    close = abort

async def open_connection(endpoint: dict[str, Any], limit: int = 4 * 1024 * 1024) -> tuple[Reader, Writer]:
    handle = json.loads(await request("connect", endpoint=endpoint))
    return Reader(handle, limit), Writer(handle)

def default_root() -> Path:
    _load()
    length = _library.omochi_local_runtime_root(None, 0)
    if length < 0:
        raise OSError("Could not resolve private runtime directory")
    buffer = ctypes.create_string_buffer(length)
    if _library.omochi_local_runtime_root(buffer, length) != length:
        raise OSError("Could not resolve private runtime directory")
    return Path(json.loads(buffer.raw))

def create_private_directory(path: Path) -> None:
    _load()
    encoded = json.dumps(str(path)).encode()
    status = _library.omochi_local_create_private_directory(encoded, len(encoded))
    if status:
        raise OSError(status, "Could not create private directory", str(path))

def _after_fork() -> None:
    global _state_lock, _owner_pid
    _state_lock = threading.RLock()
    _pending.clear()
    if _context is None:
        _owner_pid = os.getpid()

if hasattr(os, "register_at_fork"):
    os.register_at_fork(after_in_child=_after_fork)
