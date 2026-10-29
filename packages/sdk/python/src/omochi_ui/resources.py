"""Immutable, application-owned media served over the resources/1 protocol."""
from __future__ import annotations

import base64
import io
import os
import re
import secrets
import tempfile
from pathlib import Path
from threading import RLock

PROTOCOL = "resources/1"
CHUNK = 24 * 1024
MIME = {".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".gif": "image/gif", ".webp": "image/webp", ".svg": "image/svg+xml", ".avif": "image/avif", ".mp3": "audio/mpeg", ".wav": "audio/wav", ".ogg": "audio/ogg", ".m4a": "audio/mp4", ".mp4": "video/mp4", ".webm": "video/webm", ".mov": "video/quicktime"}


def is_resource(message):
    return isinstance(message, dict) and message.get("omochi") == PROTOCOL


def hello():
    return {"omochi": PROTOCOL, "kind": "hello"}


class ResourceRegistry:
    def __init__(self, max_file_bytes=1024**3, max_total_bytes=4 * 1024**3):
        self.max_file_bytes, self.max_total_bytes = max_file_bytes, max_total_bytes
        self._directory = None
        self._entries = {}
        self._total = 0
        self._closed = False
        self._lock = RLock()

    def file(self, path, mime_type=None):
        with open(path, "rb") as source:
            return self._register(source, mime_type or MIME.get(Path(path).suffix.lower()))

    def bytes(self, data, mime_type):
        return self._register(io.BytesIO(data), mime_type)

    def _register(self, source, mime):
        with self._lock:
            if self._closed:
                raise ValueError("resource registry closed")
            if not isinstance(mime, str) or not re.fullmatch(r"(image|audio|video)/[a-zA-Z0-9.+-]+", mime):
                raise ValueError("explicit media MIME type required")
            if self._directory is None:
                self._directory = tempfile.TemporaryDirectory(prefix="omochi-resources-")
            resource_id = secrets.token_hex(16)
            path = Path(self._directory.name) / resource_id
            size = 0
            try:
                with open(path, "xb") as target:
                    os.chmod(path, 0o600)
                    while chunk := source.read(65536):
                        size += len(chunk)
                        if size > self.max_file_bytes or self._total + size > self.max_total_bytes:
                            raise ValueError("resource storage limit exceeded")
                        target.write(chunk)
            except BaseException:
                path.unlink(missing_ok=True)
                raise
            self._entries[resource_id] = (path, size, mime)
            self._total += size
            return f"omochi://resources/{resource_id}"

    def release(self, uri):
        with self._lock:
            match = re.fullmatch(r"omochi://resources/([a-f0-9]{32})", uri)
            entry = self._entries.pop(match[1], None) if match else None
            if entry:
                entry[0].unlink(missing_ok=True)
                self._total -= entry[1]

    def close(self):
        with self._lock:
            self._entries.clear()
            self._total = 0
            self._closed = True
            if self._directory:
                self._directory.cleanup()
                self._directory = None

    def respond(self, message):
        result = {"omochi": PROTOCOL, "kind": "response", "id": message.get("id"), "status": 200}
        def error(status):
            return {**result, "status": status}
        with self._lock:
            resource = message.get("resource", "")
            if message.get("kind") != "request" or not isinstance(message.get("id"), str) or not re.fullmatch(r"[\w-]{1,80}", message["id"], re.ASCII) or not isinstance(resource, str) or not re.fullmatch(r"[a-f0-9]{32}", resource):
                return error(400)
            op = message.get("op")
            offset, length = message.get("offset"), message.get("length")
            if op not in ("stat", "read") or op == "read" and (type(offset) is not int or type(length) is not int or offset < 0 or offset > 2**53-1 or not 0 < length <= CHUNK):
                return error(400)
            entry = self._entries.get(resource)
            if not entry:
                return error(404)
            path, size, mime = entry
            if op == "stat":
                return {**result, "size": size, "mime": mime, "etag": f'"{resource}"'}
            if offset + length > size:
                return error(416)
            try:
                with open(path, "rb") as source:
                    source.seek(offset)
                    return {**result, "data": base64.b64encode(source.read(length)).decode("ascii")}
            except OSError:
                return error(500)
