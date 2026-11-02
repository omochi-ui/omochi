from __future__ import annotations

import math
import types
from contextvars import ContextVar
from dataclasses import fields, is_dataclass
from typing import Any, Callable, Generic, TypeVar, get_args, get_origin, get_type_hints, is_typeddict, Literal, Union, Required, NotRequired, overload

from .errors import ProtocolError, DeclarationError
from .expression import Expr, SchemaValue, encode_input

T = TypeVar("T")
E = TypeVar("E")
_building: ContextVar[tuple[Any, ...]] = ContextVar("omochi_input_types", default=())

class Schema(Generic[T]):
    def __init__(self, decode: Callable[[Any, str], T], *, optional: bool = False):
        self.decode = decode
        self.optional = optional
        self.members: dict[str, Schema[Any]] | None = None
        self.kind: Any = None
        self.element: Schema[Any] | None = None
        self.items: tuple[Schema[Any], ...] | None = None
        self.variants: tuple[Schema[Any], ...] = ()
        self.unknown_keys = "reject"

    def refine(self, predicate: Callable[[T], bool], message: str = "input constraint failed") -> Schema[T]:
        """Add a server-side constraint without losing the declared wire shape."""
        def decode(value: Any, path: str) -> T:
            parsed = self.decode(value, path)
            if not predicate(parsed):
                raise ProtocolError(f"{path}: {message}")
            return parsed
        result = Schema(decode, optional=self.optional)
        result.__dict__.update(self.__dict__)
        result.decode = decode
        return result

    def input(self, value: T | Expr[T]) -> SchemaValue[T]:
        if not isinstance(value, Expr): self.parse(encode_input(value))
        return SchemaValue(self, value)

    def parse(self, value: Any) -> T:
        return self.decode(value, "$input")

    def field(self, name: str, value: Schema[E]) -> Field[T, E]:
        if self.members is None or name not in self.members:
            raise DeclarationError(f"unknown schema field: {name!r}")
        expected = self.members[name]
        if not _known_kind(expected.kind) or not _known_kind(value.kind) or expected.kind != value.kind:
            raise DeclarationError(f"field {name!r} has a different schema type")
        return Field(self, name, value)

class Field(Generic[T, E]):
    def __init__(self, owner: Schema[T], name: str, value: Schema[E]):
        self.owner, self.name, self.schema = owner, name, value

class InputSchema(Schema[T], Generic[T, E]):
    def __init__(self, concrete: type[T], expression: type[E]):
        concrete_hints = _input_hints(concrete)
        expression_hints = get_type_hints(expression, include_extras=True)
        if not is_typeddict(expression) or concrete_hints.keys() != expression_hints.keys():
            raise DeclarationError("expression TypedDict must have the same fields as the concrete input")
        for key, expected in concrete_hints.items():
            if not _expression_matches(expected, expression_hints[key]):
                raise DeclarationError(f"expression field {key!r} does not match its concrete type")
            expected_required = _required_fields(concrete)
            if (key in expected_required) != (key in _required_fields(expression)):
                raise DeclarationError(f"expression field {key!r} has different requiredness")
        inner = from_type(concrete)
        super().__init__(inner.decode)
        self.concrete, self.expression = concrete, expression
        self.members = inner.members
        self.kind = inner.kind
        self.element, self.items, self.variants = inner.element, inner.items, inner.variants


def _input_hints(kind: Any) -> dict[str, Any]:
    hints = get_type_hints(kind, include_extras=True)
    if is_dataclass(kind):
        return {f.name: hints[f.name] for f in fields(kind) if f.init}
    return hints


def _required_fields(kind: Any) -> set[str]:
    from dataclasses import MISSING
    if is_dataclass(kind):
        return {f.name for f in fields(kind) if f.init and f.default is MISSING and f.default_factory is MISSING}
    hints = get_type_hints(kind, include_extras=True)
    return {k for k, t in hints.items() if get_origin(t) is Required or (get_origin(t) is not NotRequired and k in kind.__required_keys__)}

def _expression_result_matches(expected: Any, actual: Any) -> bool:
    if actual == expected: return True
    if get_origin(actual) in (Union, types.UnionType):
        return all(_expression_result_matches(expected, t) for t in get_args(actual))
    if get_origin(expected) in (Union, types.UnionType):
        return any(_expression_result_matches(t, actual) for t in get_args(expected))
    return False


def _expression_matches(expected: Any, actual: Any) -> bool:
    from .expression import Expr
    if get_origin(expected) in (Required, NotRequired): expected = get_args(expected)[0]
    if get_origin(actual) in (Required, NotRequired): actual = get_args(actual)[0]
    if actual == expected: return True
    if get_origin(actual) is Expr: return _expression_result_matches(expected, get_args(actual)[0])
    if get_origin(actual) in (Union, types.UnionType):
        return all(_expression_matches(expected, t) for t in get_args(actual))
    if get_origin(expected) in (Union, types.UnionType):
        return any(_expression_matches(t, actual) for t in get_args(expected))
    if (is_dataclass(expected) or is_typeddict(expected)) and is_typeddict(actual):
        left, right = _input_hints(expected), _input_hints(actual)
        return left.keys() == right.keys() and _required_fields(expected) == _required_fields(actual) and all(_expression_matches(t, right[k]) for k, t in left.items())
    origin = get_origin(expected)
    if origin in (list, tuple, dict) and origin == get_origin(actual):
        left, right = get_args(expected), get_args(actual)
        return len(left) == len(right) and all(_expression_matches(a, b) for a, b in zip(left, right))
    return False


def _known_kind(kind: Any) -> bool:
    return kind is not None and (not isinstance(kind, tuple) or all(_known_kind(part) for part in kind))


def _fail(path: str, expected: str) -> Any:
    raise ProtocolError(f"{path}: expected {expected}")

def string() -> Schema[str]:
    result = Schema(lambda v, p: v if isinstance(v, str) else _fail(p, "string"))
    result.kind = str
    return result

def boolean() -> Schema[bool]:
    result = Schema(lambda v, p: v if type(v) is bool else _fail(p, "boolean"))
    result.kind = bool
    return result

def _finite(value: int | float) -> bool:
    try: return math.isfinite(value)
    except OverflowError: return False


def number() -> Schema[float]:
    result = Schema(lambda v, p: float(v) if type(v) in (int, float) and _finite(v) else _fail(p, "finite number"))
    result.kind = float
    return result

def integer() -> Schema[int]:
    result = Schema(lambda v, p: v if type(v) is int else _fail(p, "integer"))
    result.kind = int
    return result

def literal(value: T) -> Schema[T]:
    result = Schema(lambda v, p: v if type(v) is type(value) and v == value else _fail(p, repr(value)))
    result.kind = ("literal", type(value), repr(value))
    return result

def nullable(inner: Schema[T]) -> Schema[T | None]:
    result = Schema(lambda v, p: None if v is None else inner.decode(v, p), optional=inner.optional)
    result.kind = ("nullable", inner.kind)
    result.variants = (inner, literal(None))
    return result

def optional(inner: Schema[T]) -> Schema[T]:
    result = Schema(inner.decode, optional=True)
    result.kind = inner.kind
    result.members = inner.members
    result.element, result.items, result.variants = inner.element, inner.items, inner.variants
    result.unknown_keys = inner.unknown_keys
    return result

def array(inner: Schema[T]) -> Schema[list[T]]:
    result = Schema(lambda v, p: [inner.decode(x, f"{p}/{i}") for i, x in enumerate(v)] if isinstance(v, list) else _fail(p, "array"))
    result.kind = (list, inner.kind)
    result.element = inner
    return result

def tuple_(*items: Schema[Any]) -> Schema[tuple[Any, ...]]:
    result = Schema(lambda v, p: tuple(s.decode(x, f"{p}/{i}") for i, (s, x) in enumerate(zip(items, v))) if isinstance(v, (list, tuple)) and len(v) == len(items) else _fail(p, "tuple"))
    result.kind = (tuple, tuple(s.kind for s in items))
    result.items = items
    return result

def map_(inner: Schema[T]) -> Schema[dict[str, T]]:
    def decode(v: Any, p: str) -> dict[str, T]:
        if not isinstance(v, dict) or any(not isinstance(k, str) for k in v):
            return _fail(p, "object with string keys")
        return {k: inner.decode(x, f"{p}/{k}") for k, x in v.items()}
    result = Schema(decode)
    result.kind = (dict, inner.kind)
    result.element = inner
    return result

def object_(members: dict[str, Schema[Any]], *, unknown_keys: Literal["reject", "strip", "allow"] = "reject") -> Schema[dict[str, Any]]:
    def decode(v: Any, p: str) -> dict[str, Any]:
        if not isinstance(v, dict): return _fail(p, "object")
        out = {}
        for k, s in members.items():
            if k not in v:
                if not s.optional: return _fail(f"{p}/{k}", "required field")
            else: out[k] = s.decode(v[k], f"{p}/{k}")
        for k in v.keys() - members.keys():
            if unknown_keys == "reject": return _fail(f"{p}/{k}", "known field")
            if unknown_keys == "allow": out[k] = v[k]
        return out
    result = Schema(decode)
    result.members = members
    result.unknown_keys = unknown_keys
    result.kind = ("object", unknown_keys, tuple(sorted((k, s.kind, s.optional) for k, s in members.items())))
    return result

def union(*items: Schema[Any]) -> Schema[Any]:
    def decode(v: Any, p: str) -> Any:
        for s in items:
            try: return s.decode(v, p)
            except ProtocolError: pass
        return _fail(p, "matching union member")
    result = Schema(decode, optional=any(s.optional for s in items))
    result.kind = ("union", tuple(s.kind for s in items))
    result.variants = items
    return result

def custom(decode: Callable[[Any], T]) -> Schema[T]:
    def checked(v: Any, p: str) -> T:
        try: return decode(v)
        except Exception as error: raise ProtocolError(f"{p}: {error}") from error
    return Schema(checked)

@overload
def from_type(kind: type[T]) -> Schema[T]: ...
@overload
def from_type(kind: Any) -> Schema[Any]: ...
def from_type(kind: Any) -> Schema[Any]:
    stack = _building.get()
    if kind in stack:
        raise DeclarationError(f"recursive input type {kind!r} needs an explicit schema")
    token = _building.set((*stack, kind))
    try:
        return _from_type(kind)
    finally:
        _building.reset(token)


def _from_type(kind: Any) -> Schema[Any]:
    if kind is str: return string()
    if kind is bool: return boolean()
    if kind is int: return integer()
    if kind is float: return number()
    if kind in (None, type(None)): return literal(None)
    origin, args = get_origin(kind), get_args(kind)
    if origin in (Required, NotRequired):
        inner = from_type(args[0])
        return optional(inner) if origin is NotRequired else inner
    if origin in (types.UnionType, Union): return union(*(from_type(t) for t in args))
    if origin is Literal: return union(*(literal(v) for v in args))
    if origin is list: return array(from_type(args[0]))
    if origin is tuple:
        if len(args) == 2 and args[1] is Ellipsis:
            inner = from_type(args[0])
            result = Schema(lambda v, p: tuple(array(inner).decode(v, p)))
            result.kind = ("tuple_array", inner.kind)
            result.element = inner
            return result
        return tuple_(*(from_type(t) for t in args))
    if origin is dict and args[0] is str: return map_(from_type(args[1]))
    if is_dataclass(kind) or is_typeddict(kind):
        from dataclasses import MISSING
        hints = _input_hints(kind)
        members = {k: from_type(t) for k, t in hints.items()}
        if is_dataclass(kind):
            for f in fields(kind):
                if f.init and (f.default is not MISSING or f.default_factory is not MISSING):
                    members[f.name] = optional(members[f.name])
        else:
            for k in members.keys() - _required_fields(kind): members[k] = optional(members[k])
        obj = object_(members)
        def decode(v: Any, p: str) -> Any:
            values = obj.decode(v, p)
            try:
                return kind(**values)
            except (ValueError, TypeError) as error:
                raise ProtocolError(f"{p}: {error}") from error
        result = Schema(decode)
        result.members = members
        result.kind = kind
        return result
    raise DeclarationError(f"unsupported input type {kind!r}; supply an explicit schema")
