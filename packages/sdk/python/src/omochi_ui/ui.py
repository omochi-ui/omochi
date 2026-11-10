"""UI declarations for the Surface currently being rendered.

Use ordinary functions for components. The facade resolves the Builder for
every call, including custom Catalog UI subclasses; it never stores one globally.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import is_dataclass
from functools import wraps
from typing import Any, Concatenate, ParamSpec, TypeVar, overload

from ._context import get_ui
from .builder import UI
from .errors import DeclarationError
from .references import MISSING, Local

P = ParamSpec("P")
R = TypeVar("R")
T = TypeVar("T")
C = TypeVar("C", bound=UI)


def _method(method: Callable[Concatenate[UI, P], R]) -> Callable[P, R]:
    @wraps(method)
    def call(*args: P.args, **kwargs: P.kwargs) -> R:
        return getattr(get_ui(), method.__name__)(*args, **kwargs)

    return call


@overload
def local(
    key: str, initial: T, *, type: type[T], reset_key: Any = MISSING
) -> Local[T]: ...
@overload
def local(key: str, initial: T, *, reset_key: Any = MISSING) -> Local[T]: ...
def local(
    key: str, initial: T, *, type: Any = MISSING, reset_key: Any = MISSING
) -> Local[T]:
    import builtins

    builder = get_ui()
    options: dict[str, Any] = {}
    if type is not MISSING:
        options["type"] = type
    elif not is_dataclass(initial) and builtins.type(initial) not in (
        str,
        bool,
        int,
        float,
        builtins.type(None),
    ):
        raise DeclarationError("local() needs type= for dictionaries and containers")
    if reset_key is not MISSING:
        options["reset_key"] = reset_key
    return builder.local(key, initial, **options)


text = _method(UI.text)
image = _method(UI.image)
icon = _method(UI.icon)
video = _method(UI.video)
audio_player = _method(UI.audio_player)
divider = _method(UI.divider)
row = _method(UI.row)
column = _method(UI.column)
list = _method(UI.list)
card = _method(UI.card)
tabs = _method(UI.tabs)
modal = _method(UI.modal)
button = _method(UI.button)
text_field = _method(UI.text_field)
check_box = _method(UI.check_box)
choice_picker = _method(UI.choice_picker)
slider = _method(UI.slider)
date_time_input = _method(UI.date_time_input)
scope = _method(UI.scope)
component = _method(UI.component)


@overload
def catalog(type: type[C]) -> C: ...
@overload
def catalog() -> UI: ...
def catalog(type: Any = UI) -> UI:
    """Access custom Catalog UI methods on the active Builder explicitly."""
    builder = get_ui()
    if not isinstance(builder, type):
        raise DeclarationError("active Catalog has a different UI type")
    return builder


__all__ = [
    "audio_player",
    "button",
    "card",
    "catalog",
    "check_box",
    "choice_picker",
    "column",
    "component",
    "date_time_input",
    "divider",
    "icon",
    "image",
    "list",
    "local",
    "modal",
    "row",
    "scope",
    "slider",
    "tabs",
    "text",
    "text_field",
    "video",
]
