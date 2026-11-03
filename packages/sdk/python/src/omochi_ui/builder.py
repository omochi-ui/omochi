from __future__ import annotations

from contextlib import AbstractContextManager, contextmanager
import builtins
import json
from copy import deepcopy
from dataclasses import dataclass, is_dataclass
from .expression import Expr, encode_input, uses_object_merge, MERGE_OBJECTS_CAPABILITY
from .schema import from_type
from .references import Local, DynamicLocal, ModelBinding, same_type
from ._context import RenderScope
from typing import TypeVar, Any, Callable, Iterator, Literal, Sequence, Unpack, overload, TYPE_CHECKING, Mapping

from .catalog import (
    BASIC_CATALOG, ButtonOptions, ButtonProps, CardProps, Catalog, CheckableOptions, CommonProps,
    CheckBoxProps, ChoicePickerOptions, ChoicePickerProps, DateTimeInputOptions,
    DateTimeInputProps, DividerProps, DynamicString, IconProps, ImageOptions, ImageProps,
    LayoutOptions, LayoutProps, ListOptions, ListProps, MediaOptions, MediaProps, ModalProps,
    SliderOptions, SliderProps, TabsProps, TextFieldOptions, TextFieldProps, TextOptions,
    TextProps,
)
from .errors import DeclarationError, ProtocolError
from .ir import Declaration, Node, Slot

if TYPE_CHECKING:
    from .app import Action, ActionCall

I = TypeVar("I")
E = TypeVar("E")
EventInput = Mapping[str, object] | list[object] | tuple[object, ...] | DynamicLocal
STANDARD_CATALOG = BASIC_CATALOG.id
_MISSING = object()

def _escape_pointer(value: str) -> str:
    return value.replace("~", "~0").replace("/", "~1")

T = TypeVar("T")

@dataclass(frozen=True, slots=True)
class Tab:
    title: DynamicString
    content: Callable[["UI"], Any]

class UI:
    """Builds a typed normalized declaration for one A2UI surface."""

    def __init__(self, surface_id: str, catalog: Catalog, register_action: Callable[..., str], previous_data: dict[str, Any] | None = None):
        self.surface_id = surface_id
        self.catalog = catalog
        root_slot = Slot(path="/children", kind="list")
        self._root = Node(key="root", component="Column", slots=[root_slot])
        self._targets = [root_slot.nodes]
        self._register_action = register_action
        previous_inputs = (previous_data or {}).get("inputs", {})
        self._data: dict[str, Any] = {"inputs": deepcopy(previous_inputs) if isinstance(previous_inputs, dict) else {}}
        self._locals: dict[str, dict[str, Any]] = {}
        self._uncontrolled_inputs: set[str] = set()
        self._required_capabilities: set[str] = set()
        self._render_scope = RenderScope()
        self._scope_path: tuple[str, ...] = ()
        self._scopes: set[tuple[str, ...]] = set()
        self._auto_keys: dict[tuple[str, ...], int] = {}
        self._keys = {"root"}

    @overload
    def local(self, key: str, initial: T, *, type: type[T], reset_key: Any = _MISSING) -> Local[T]: ...
    @overload
    def local(self, key: str, initial: T, *, reset_key: Any = _MISSING) -> Local[T]: ...
    def local(self, key: str, initial: T, *, type: Any = _MISSING, reset_key: Any = _MISSING) -> Local[T]:
        self._render_scope.check()
        if not isinstance(key, str) or not key:
            raise DeclarationError("local key must be non-empty")
        key = self._scoped(key)
        if key in self._locals:
            raise DeclarationError(f"local key must be unique and non-empty: {key!r}")
        declared = None
        if type is not _MISSING:
            declared = from_type(type)
        elif builtins.type(initial) in (str, bool, int, float, builtins.type(None)) or is_dataclass(initial):
            declared = from_type(builtins.type(initial))
        encoded = encode_input(initial)
        if declared is not None:
            try:
                declared.parse(encoded)
            except ProtocolError as error:
                raise DeclarationError(str(error)) from error
        declaration = {"initial": encoded}
        if reset_key is not _MISSING:
            declaration["resetKey"] = deepcopy(reset_key)
        self._locals[key] = declaration
        return Local(_local_path(key, ""), self._render_scope, declared, encoded)

    def _scoped(self, key: str) -> str:
        if not self._scope_path:
            if key.startswith("$scope:"):
                raise DeclarationError("$scope: is reserved for namespaced keys")
            return key
        return "$scope:" + json.dumps([*self._scope_path, key], separators=(",", ":"), ensure_ascii=True)

    @contextmanager
    def scope(self, key: str) -> Iterator[None]:
        self._render_scope.check()
        if not isinstance(key, str) or not key:
            raise DeclarationError("scope key must be a non-empty string")
        previous = self._scope_path
        path = (*previous, key)
        if path in self._scopes:
            raise DeclarationError(f"duplicate scope: {path!r}")
        self._scopes.add(path)
        self._scope_path = path
        try:
            yield
        finally:
            self._scope_path = previous

    def _key(self, key: str | None) -> str:
        self._render_scope.check()
        if key is None:
            index = self._auto_keys.get(self._scope_path, 0)
            self._auto_keys[self._scope_path] = index + 1
            key = f"auto:{index}"
        resolved = self._scoped(key)
        if resolved in self._keys:
            raise DeclarationError(f"duplicate component key on {self.surface_id!r}: {resolved!r}")
        self._keys.add(resolved)
        return resolved

    def _append(self, component: str, props: dict[str, Any], key: str | None) -> Node:
        node = Node(key=self._key(key), component=component, props=props)
        self._targets[-1].append(node)
        return node

    def _slot(self, node: Node, path: str, kind: str, build: Callable[["UI"], Any]) -> Slot:
        slot = Slot(path=path, kind=kind)
        node.slots.append(slot)
        self._targets.append(slot.nodes)
        try:
            build(self)
        finally:
            self._targets.pop()
        return slot

    @overload
    def component(self, component: Literal["Text"], *, key: str, **props: Unpack[TextProps]) -> None: ...
    @overload
    def component(self, component: Literal["Image"], *, key: str, **props: Unpack[ImageProps]) -> None: ...
    @overload
    def component(self, component: Literal["Icon"], *, key: str, **props: Unpack[IconProps]) -> None: ...
    @overload
    def component(self, component: Literal["Video", "AudioPlayer"], *, key: str, **props: Unpack[MediaProps]) -> None: ...
    @overload
    def component(self, component: Literal["Row", "Column"], *, key: str, **props: Unpack[LayoutProps]) -> None: ...
    @overload
    def component(self, component: Literal["List"], *, key: str, **props: Unpack[ListProps]) -> None: ...
    @overload
    def component(self, component: Literal["Card"], *, key: str, **props: Unpack[CardProps]) -> None: ...
    @overload
    def component(self, component: Literal["Tabs"], *, key: str, **props: Unpack[TabsProps]) -> None: ...
    @overload
    def component(self, component: Literal["Modal"], *, key: str, **props: Unpack[ModalProps]) -> None: ...
    @overload
    def component(self, component: Literal["Divider"], *, key: str, **props: Unpack[DividerProps]) -> None: ...
    @overload
    def component(self, component: Literal["Button"], *, key: str, **props: Unpack[ButtonProps]) -> None: ...
    @overload
    def component(self, component: Literal["TextField"], *, key: str, **props: Unpack[TextFieldProps]) -> None: ...
    @overload
    def component(self, component: Literal["CheckBox"], *, key: str, **props: Unpack[CheckBoxProps]) -> None: ...
    @overload
    def component(self, component: Literal["ChoicePicker"], *, key: str, **props: Unpack[ChoicePickerProps]) -> None: ...
    @overload
    def component(self, component: Literal["Slider"], *, key: str, **props: Unpack[SliderProps]) -> None: ...
    @overload
    def component(self, component: Literal["DateTimeInput"], *, key: str, **props: Unpack[DateTimeInputProps]) -> None: ...
    def component(self, component: str, *, key: str, **props: Any) -> None:
        self._append(component, props, key)

    def text(self, value: DynamicString, *, key: str | None = None, **props: Unpack[TextOptions]) -> None:
        self._append("Text", {"text": value, **props}, key)

    def image(self, url: DynamicString, *, key: str | None = None, **props: Unpack[ImageOptions]) -> None:
        self._append("Image", {"url": url, **props}, key)

    def icon(self, name: str | dict[str, str], *, key: str | None = None, **props: Unpack[CommonProps]) -> None:
        self._append("Icon", {"name": name, **props}, key)

    def video(self, url: DynamicString, *, key: str | None = None, **props: Unpack[CommonProps]) -> None:
        self._append("Video", {"url": url, **props}, key)

    def audio_player(self, url: DynamicString, *, key: str | None = None, **props: Unpack[MediaOptions]) -> None:
        self._append("AudioPlayer", {"url": url, **props}, key)

    def divider(self, *, key: str | None = None, **props: Unpack[DividerProps]) -> None:
        self._append("Divider", props, key)

    @contextmanager
    def _container(self, component: str, *, key: str | None, props: dict[str, Any], path: str = "/children", kind: str = "list") -> Iterator[None]:
        node = self._append(component, props, key)
        slot = Slot(path=path, kind=kind)
        node.slots.append(slot)
        self._targets.append(slot.nodes)
        try:
            yield
        finally:
            self._targets.pop()
            if kind == "single" and len(slot.nodes) != 1:
                raise DeclarationError(f"{component} must contain exactly one child")

    def row(self, *, key: str | None = None, **props: Unpack[LayoutOptions]) -> AbstractContextManager[None]: return self._container("Row", key=key, props=props)
    def column(self, *, key: str | None = None, **props: Unpack[LayoutOptions]) -> AbstractContextManager[None]: return self._container("Column", key=key, props=props)
    def list(self, *, key: str | None = None, **props: Unpack[ListOptions]) -> AbstractContextManager[None]: return self._container("List", key=key, props=props)
    def card(self, *, key: str | None = None, **props: Unpack[CommonProps]) -> AbstractContextManager[None]: return self._container("Card", key=key, props=props, path="/child", kind="single")

    def tabs(self, tabs: Sequence[Tab], *, key: str | None = None, **props: Unpack[CommonProps]) -> None:
        node = self._append("Tabs", {**props, "tabs": [{"title": tab.title} for tab in tabs]}, key)
        for index, tab in enumerate(tabs):
            slot = self._slot(node, f"/tabs/{index}/child", "single", tab.content)
            if len(slot.nodes) != 1:
                raise DeclarationError("each tab must build exactly one child")

    def modal(self, *, trigger: Callable[["UI"], Any], content: Callable[["UI"], Any], key: str | None = None, **props: Unpack[CommonProps]) -> None:
        node = self._append("Modal", props, key)
        if len(self._slot(node, "/trigger", "single", trigger).nodes) != 1 or len(self._slot(node, "/content", "single", content).nodes) != 1:
            raise DeclarationError("modal trigger and content must each build exactly one child")

    @overload
    def button(self, label: DynamicString, call: ActionCall, *, key: str | None = None, **props: Unpack[ButtonOptions]) -> None: ...
    @overload
    def button(self, label: DynamicString, *, on_click: Action[I, E], input: I | E | Expr[I], key: str | None = None, **props: Unpack[ButtonOptions]) -> None: ...
    @overload
    def button(self, label: DynamicString, *, on_click: Action[Any, Any], input: EventInput, key: str | None = None, **props: Unpack[ButtonOptions]) -> None: ...
    @overload
    def button(self, label: DynamicString, *, on_click: Action[None, None], key: str | None = None, **props: Unpack[ButtonOptions]) -> None: ...
    def button(self, label: DynamicString, call: Any = _MISSING, *, on_click: Any = _MISSING, input: Any = _MISSING, key: str | None = None, **props: Unpack[ButtonOptions]) -> None:
        from .app import Action, ActionCall
        self._render_scope.check()
        if call is not _MISSING:
            if on_click is not _MISSING or input is not _MISSING:
                raise DeclarationError("cannot combine an ActionCall with on_click/input")
        else:
            if not isinstance(on_click, Action) or on_click._schema is None:
                raise DeclarationError("on_click must be a registered Action")
            if input is _MISSING:
                if not same_type(on_click._schema, from_type(builtins.type(None))):
                    raise DeclarationError("this Action requires input=")
                input = None
            call = on_click.call(on_click.compose(input))
        if not isinstance(call, ActionCall):
            raise DeclarationError("button requires an Action declaration")
        if call._owner is not None and call._owner is not self._render_scope.app:
            raise DeclarationError("Action belongs to a different App")
        for ref in call._references:
            if ref._scope is not None:
                ref._scope.check(self._render_scope)
        action_name = call.action
        button_key = key if key is not None else f"action:{action_name}"
        node = self._append("Button", {
            **props,
            "action": {"event": {"name": action_name, "context": {"input": deepcopy(call.input)}}},
        }, button_key)
        self._slot(node, "/child", "single", lambda ui: ui.text(label, key=f"{button_key}:label"))

    def _input(self, component: str, name: str, default: Any, bind: ModelBinding | Local | DynamicLocal | None, props: dict[str, Any], key: str | None) -> None:
        if "on_change" in props or "onChange" in props:
            raise DeclarationError(f"{component} does not define an input change action in the Basic Catalog")
        self._render_scope.check()
        input_key = key if key is not None else f"input:{name}"
        if bind is None:
            name = self._scoped(name)
            self._uncontrolled_inputs.add(name)
            self._data["inputs"].setdefault(name, default)
            path = f"/inputs/{_escape_pointer(name)}"
        else:
            expected_type = {"TextField": str, "CheckBox": bool, "Slider": float,
                             "ChoicePicker": list[str], "DateTimeInput": str}[component]
            if isinstance(bind, DynamicLocal): bind = bind._local
            if isinstance(bind, Local): bind = bind.bind()
            if not isinstance(bind, ModelBinding) or bind._scope is None:
                raise DeclarationError("bind must be a writable Local reference")
            bind._scope.check(self._render_scope)
            if bind._schema is not None and not same_type(bind._schema, from_type(expected_type)):
                raise DeclarationError(f"{component} binding has a different writable type")
            path = bind.path
        self._append(component, {**props, "value": {"path": path}}, input_key)

    def _input_control(self, component: str, name: str, fallback: Any, default: Any, bind: ModelBinding | Local | DynamicLocal | None, props: dict[str, Any], key: str | None) -> None:
        if bind is not None and default is not _MISSING:
            raise DeclarationError("default and bind cannot be used together")
        self._input(component, name, fallback if default is _MISSING else default, bind, props, key)

    def text_field(self, name: str, *, label: DynamicString, default: Any = _MISSING, bind: ModelBinding[str] | Local[str] | DynamicLocal | None = None, key: str | None = None, **props: Unpack[TextFieldOptions]) -> None: self._input_control("TextField", name, "", default, bind, {"label": label, **props}, key)
    def check_box(self, name: str, *, label: DynamicString, default: Any = _MISSING, bind: ModelBinding[bool] | Local[bool] | DynamicLocal | None = None, key: str | None = None, **props: Unpack[CheckableOptions]) -> None: self._input_control("CheckBox", name, False, default, bind, {"label": label, **props}, key)
    def choice_picker(self, name: str, *, options: list[dict[str, Any]], default: Any = _MISSING, bind: ModelBinding[list[str]] | Local[list[str]] | DynamicLocal | None = None, key: str | None = None, **props: Unpack[ChoicePickerOptions]) -> None: self._input_control("ChoicePicker", name, [], default, bind, {"options": options, **props}, key)
    def slider(self, name: str, minimum: float, maximum: float, *, default: Any = _MISSING, bind: ModelBinding[float] | Local[float] | DynamicLocal | None = None, key: str | None = None, **props: Unpack[SliderOptions]) -> None: self._input_control("Slider", name, minimum, default, bind, {"min": minimum, "max": maximum, **props}, key)
    def date_time_input(self, name: str, *, default: Any = _MISSING, bind: ModelBinding[str] | Local[str] | DynamicLocal | None = None, key: str | None = None, **props: Unpack[DateTimeInputOptions]) -> None: self._input_control("DateTimeInput", name, "", default, bind, props, key)

    def build(self) -> Declaration:
        self._render_scope.check()
        for name in list(self._data["inputs"]):
            if name not in self._uncontrolled_inputs:
                del self._data["inputs"][name]
        if uses_object_merge(self._root.to_dict()): self._required_capabilities.add(MERGE_OBJECTS_CAPABILITY)
        self._render_scope.active = False
        return Declaration(surface_id=self.surface_id, catalog_id=self.catalog.id, root=self._root, data=self._data, locals=self._locals, required_capabilities=self._required_capabilities)

def _local_path(key: str, path: str) -> str:
    return f"/$local/{_escape_pointer(key)}{path}"
