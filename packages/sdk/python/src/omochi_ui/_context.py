"""Render lifetime and context; independent of the public UI facade."""

from __future__ import annotations

import asyncio
from contextvars import ContextVar
from threading import get_ident
from typing import TYPE_CHECKING

from .errors import DeclarationError

if TYPE_CHECKING:
    from .builder import UI

current_ui: ContextVar[UI | None] = ContextVar("omochi_ui_builder", default=None)


def _task() -> object:
    try:
        return asyncio.current_task()
    except RuntimeError:
        return None


class RenderScope:
    def __init__(self) -> None:
        self.active = True
        self.app: object | None = None
        self._thread = get_ident()
        self._task = _task()

    def check(self, target: RenderScope | None = None) -> None:
        if not self.active:
            raise DeclarationError("reference or Builder belongs to a completed render")
        if self._thread != get_ident() or self._task is not _task():
            raise DeclarationError("UI declarations must stay in their rendering task")
        if target is not None and target is not self:
            raise DeclarationError("reference belongs to a different Surface or render")


def get_ui() -> UI:
    builder = current_ui.get()
    if builder is None:
        raise DeclarationError("ui can only be used while rendering a Surface")
    builder._render_scope.check()
    return builder
