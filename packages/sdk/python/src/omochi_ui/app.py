from __future__ import annotations

import asyncio
import inspect
from dataclasses import dataclass, field
from typing import Any, Callable, Generic, TypeVar, cast, get_type_hints, overload, Coroutine
from contextlib import asynccontextmanager, AbstractAsyncContextManager
from typing_extensions import TypeVar as DefaultTypeVar
from .schema import Schema, InputSchema, from_type
from .expression import Expr, encode_input, references
from .inputs import compose
from .references import compatible_type
from ._context import current_ui

from .resources import ResourceRegistry, hello
from .builder import UI
from .catalog import BASIC_CATALOG, Catalog
from .errors import DeclarationError, ProtocolError
from .reconcile import Reconciler
from .transport import StdioTransport, Transport

InputType = TypeVar("InputType")
StateType = TypeVar("StateType")
ExpressionType = DefaultTypeVar("ExpressionType", default=InputType)
ViewType = TypeVar("ViewType", bound=Callable[..., Any])
UIType = TypeVar("UIType", bound=UI)

@dataclass(frozen=True, slots=True)
class ActionCall:
    action: str
    input: Any
    _owner: object | None = field(default=None, kw_only=True, repr=False, compare=False)
    _references: tuple[Expr[Any], ...] = field(default=(), kw_only=True, repr=False, compare=False)


class InputValidationError(ProtocolError):
    """Only schema failures are recoverable at the Runtime receive boundary."""

class Action(Generic[InputType, ExpressionType]):
    def __init__(self, name: str, schema: Schema[Any] | None = None, owner: object | None = None):
        self.name, self._schema, self._owner = name, schema, owner

    def call(self, input: InputType | ExpressionType | Expr[InputType]) -> ActionCall:
        if self._schema is not None:
            input = compose(self._schema, input)
        return ActionCall(self.name, encode_input(input), _owner=self._owner, _references=references(input))

    def compose(self, input: object) -> Expr[InputType]:
        if self._schema is None:
            raise DeclarationError("compose() requires a registered Action schema")
        return compose(self._schema, input)

class ActionContext(Generic[StateType]):
    def __init__(self, runtime: "Runtime[StateType, Any]"):
        self._runtime = runtime
        self._messages: list[dict[str, Any]] = []
        self._update_lock = asyncio.Lock()
        self._active = True
    @overload
    def update(self) -> AbstractAsyncContextManager[StateType]: ...
    @overload
    def update(self, mutator: Callable[[StateType], Any]) -> Coroutine[Any, Any, None]: ...
    def update(self, mutator=None):
        return self._block() if mutator is None else self._mutate(mutator)

    @asynccontextmanager
    async def _block(self):
        task = asyncio.current_task()
        if getattr(self, "_owner", None) is task:
            raise DeclarationError("nested action updates are not supported")
        async with self._update_lock:
            if not self._active: raise DeclarationError("action has already completed")
            self._owner = task
            try:
                yield self._runtime.state
                self._runtime._revision += 1
                self._messages.extend(await self._runtime._render_all_and_send())
            finally:
                self._owner = None

    async def _mutate(self, mutator):
        async with self._block() as state:
            result = mutator(state)
            if inspect.isawaitable(result): await result

class App(Generic[StateType, UIType]):
    @overload
    def __init__(self: App[StateType, UI], state: StateType, *, resources: ResourceRegistry | None = None) -> None: ...
    @overload
    def __init__(self, state: StateType, *, catalog: Catalog[UIType], resources: ResourceRegistry | None = None) -> None: ...
    def __init__(self, state: StateType, *, catalog: Catalog[UIType] = cast(Catalog[UIType], BASIC_CATALOG), resources: ResourceRegistry | None = None):
        self.resources = resources or ResourceRegistry()
        self.state, self.catalog = state, catalog
        self._views: dict[str, Callable[[UIType, StateType], Any]] = {}
        self._actions: dict[str, tuple[Callable[..., Any], Schema[Any]]] = {}
        self._built = False
        self._owner = object()
    def surface(self, surface_id: str = "main") -> Callable[[ViewType], ViewType]:
        def register(view: ViewType) -> ViewType:
            if self._built: raise DeclarationError("app is already built")
            if surface_id in self._views: raise DeclarationError(f"surface already registered: {surface_id!r}")
            self._views[surface_id] = view
            return view
        return register

    def mount(self, view: Callable[[StateType], None], *, surface_id: str = "main") -> None:
        if inspect.iscoroutinefunction(view):
            raise DeclarationError("Surface render functions must be synchronous")
        try:
            inspect.signature(view).bind(object())
        except TypeError as error:
            raise DeclarationError("mount() render function must accept state only") from error
        def render(_ui: UIType, state: StateType) -> None:
            result = view(state)
            if inspect.isawaitable(result):
                if inspect.iscoroutine(result): result.close()
                raise DeclarationError("Surface render functions must be synchronous")
        self.surface(surface_id)(render)

    @overload
    def action(self, handler: Callable[[InputType, ActionContext[StateType]], Any], *, name: str | None = None) -> Action[InputType]: ...
    @overload
    def action(self, handler: Callable[[InputType, ActionContext[StateType]], Any], *, name: str | None = None, schema: Schema[InputType]) -> Action[InputType]: ...
    @overload
    def action(self, name: str | None = None, *, schema: InputSchema[InputType, ExpressionType]) -> Callable[[Callable[[InputType, ActionContext[StateType]], Any]], Action[InputType, ExpressionType]]: ...
    @overload
    def action(self, name: str | None = None, *, schema: Schema[InputType]) -> Callable[[Callable[[InputType, ActionContext[StateType]], Any]], Action[InputType, InputType]]: ...
    @overload
    def action(self, name: str | None = None) -> Callable[[Callable[[InputType, ActionContext[StateType]], Any]], Action[InputType]]: ...
    def action(self, handler=None, *, name=None, schema=None):
        if isinstance(handler, str):
            if name is not None:
                raise DeclarationError("Action name specified twice")
            name, handler = handler, None
        def register(handler):
            action_name = name or handler.__name__
            if self._built: raise DeclarationError("app is already built")
            if action_name in self._actions: raise DeclarationError(f"action already registered: {action_name!r}")
            try:
                inspect.signature(handler).bind(object(), object())
            except TypeError as error:
                raise DeclarationError("Action handler must accept input and context") from error
            declared = schema
            if declared is None:
                parameters = list(inspect.signature(handler).parameters.values())
                if not parameters:
                    raise DeclarationError("Action handler needs an input and context parameter")
                try:
                    annotation = get_type_hints(handler, include_extras=True).get(parameters[0].name)
                except (NameError, TypeError) as error:
                    raise DeclarationError("cannot resolve Action input annotation; specify schema=") from error
                if annotation is None:
                    # None is also a valid resolved annotation for a no-input action.
                    if parameters[0].annotation is inspect.Parameter.empty:
                        raise DeclarationError("Action input needs a type annotation or schema=")
                    annotation = type(None)
                declared = from_type(annotation)
            else:
                if not isinstance(declared, Schema):
                    raise DeclarationError("schema= must be an input Schema")
                try:
                    parameters = list(inspect.signature(handler).parameters.values())
                    annotation = get_type_hints(handler, include_extras=True).get(parameters[0].name)
                    expected = from_type(annotation) if annotation is not None else None
                except (NameError, TypeError, DeclarationError):
                    expected = None  # An explicit schema can support custom domain types.
                if expected is not None and not compatible_type(declared, expected):
                    raise DeclarationError("Action schema and handler input annotation have different types")
            self._actions[action_name] = (handler, declared)
            return Action(action_name, declared, self._owner)
        return register(handler) if handler is not None else register
    def build(self) -> "Runtime[StateType, UIType]":
        if self._built: raise DeclarationError("app is already built")
        self._built = True
        return Runtime(self.state, self.catalog, self._views, self._actions, self.resources, owner=self._owner)
    async def start(self, transport: Transport | None = None) -> None:
        await self.build().start(transport)

class Runtime(Generic[StateType, UIType]):
    def __init__(self, state, catalog, views, actions, resources=None, *, owner=None):
        self.resources = resources or ResourceRegistry()
        self.state, self.catalog = state, catalog
        self._views, self._actions = dict(views), dict(actions)
        self._reconciler = Reconciler()
        self._transport: Transport | None = None
        self._lock = asyncio.Lock()
        self._revision = 0
        self._started = False
        self._owner = owner
        self._ready = asyncio.Event()
        self._closed = asyncio.Event()
        self._tasks: set[asyncio.Task[Any]] = set()
        self._pending_updates: list[tuple[Callable[[StateType], Any], asyncio.Future[Any]]] = []
        self._update_scheduled = False
    @property
    def app_revision(self): return self._revision
    def declaration(self, surface_id: str):
        try: view = self._views[surface_id]
        except KeyError as error: raise DeclarationError(f"unknown surface: {surface_id!r}") from error
        ui_type = self.catalog.ui_type or cast(type[UIType], UI)
        ui = ui_type(surface_id, self.catalog, lambda _: "")
        ui._render_scope.app = self._owner
        token = current_ui.set(ui)
        try:
            result = view(ui, self.state)
            if inspect.isawaitable(result):
                if inspect.iscoroutine(result): result.close()
                raise DeclarationError("Surface render functions must be synchronous")
            return ui.build()
        finally:
            ui._render_scope.active = False
            current_ui.reset(token)
    def render(self, surface_id: str = "main") -> list[dict[str, Any]]:
        return self._reconciler.render(self.declaration(surface_id))
    def surface(self, surface_id: str = "main") -> list[dict[str, Any]]:
        return Reconciler(set(self._reconciler.capabilities)).render(self.declaration(surface_id))
    async def _render_all_and_send(self):
        messages = []
        for surface_id in self._views:
            current = self.render(surface_id); messages.extend(current)
            if self._transport:
                for message in current: await self._transport.send(message)
        return messages
    async def update(self, mutator):
        loop = asyncio.get_running_loop()
        future = loop.create_future()
        self._pending_updates.append((mutator, future))
        if not self._update_scheduled:
            self._update_scheduled = True
            loop.call_soon(lambda: asyncio.create_task(self._flush_updates()))
        return await future
    async def _flush_updates(self):
        async with self._lock:
            batch = self._pending_updates[:]
            self._pending_updates.clear(); self._update_scheduled = False
            try:
                for mutator, _ in batch:
                    result = mutator(self.state)
                    if inspect.isawaitable(result): await result
                    self._revision += 1
                messages = await self._render_all_and_send()
                for _, future in batch:
                    if not future.done(): future.set_result(messages)
            except Exception as error:
                for _, future in batch:
                    if not future.done(): future.set_exception(error)
    async def invoke(self, name: str, input: Any):
        try: handler, schema = self._actions[name]
        except KeyError as error: raise ProtocolError(f"unknown action: {name!r}") from error
        async with self._lock:
            context = ActionContext(self)
            try:
                value = schema.parse(input)
            except ProtocolError as error:
                raise InputValidationError(str(error)) from error
            try:
                result = handler(value, context)
                if inspect.isawaitable(result): await result
            finally:
                context._active = False
                # Finish an in-flight update before releasing the Runtime lock.
                async with context._update_lock:
                    pass
            return context._messages
    async def handle(self, message: dict[str, Any]):
        if message.get("type") == "getSurface":
            if not isinstance(message.get("surface"), str): raise ProtocolError("getSurface must contain string surface")
            return self.surface(message["surface"])
        if message.get("type") == "invoke":
            if not isinstance(message.get("action"), str): raise ProtocolError("invoke must contain string action")
            return await self.invoke(message["action"], message.get("input"))
        if "error" in message:
            error = message["error"]
            valid = (
                message.get("version") == "v0.9" and "action" not in message
                and isinstance(error, dict) and "code" in error
                and isinstance(error.get("surfaceId"), str)
                and isinstance(error.get("message"), str)
            )
            if valid and error["code"] == "VALIDATION_FAILED":
                valid = isinstance(error.get("path"), str) and set(error) == {"code", "surfaceId", "path", "message"}
            if not valid: raise ProtocolError("invalid client error report")
            # Client reports describe renderer failures; they do not invoke an action.
            return []
        payload = message.get("action", message)
        name = payload.get("name") if isinstance(payload, dict) else None
        if not isinstance(name, str): raise ProtocolError("action must contain string name")
        context = payload.get("context") or {}
        return await self.invoke(name, context.get("input", payload.get("input")))
    dispatch = handle
    async def ready(self): await self._ready.wait()
    def spawn(self, coroutine):
        task = asyncio.create_task(coroutine); self._tasks.add(task); task.add_done_callback(self._tasks.discard); return task
    async def shutdown(self):
        for task in self._tasks: task.cancel()
        if self._tasks: await asyncio.gather(*self._tasks, return_exceptions=True)
        self.resources.close()
        self._closed.set()
    async def wait_closed(self): await self._closed.wait()
    async def start(self, transport: Transport | None = None):
        if self._started: raise DeclarationError("runtime has already been started")
        self._started = True; self._transport = transport or StdioTransport()
        self._reconciler = Reconciler(set(self._transport.capabilities))
        self._transport.resources = self.resources
        try:
            await self._transport.send(hello())
            await self._render_all_and_send(); self._ready.set()
            async for message in self._transport.receive():
                try:
                    responses = await self.handle(message)
                except InputValidationError:
                    continue
                if message.get("type") == "getSurface":
                    for response in responses: await self._transport.send(response)
        finally: await self.shutdown()
