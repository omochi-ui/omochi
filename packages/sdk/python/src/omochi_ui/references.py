"""Typed and dynamic projections of renderer-owned draft values."""
from __future__ import annotations

from typing import Any, Generic, TypeVar, overload

from ._context import RenderScope
from .errors import DeclarationError
from .expression import Expr
from .schema import Field, Schema, _known_kind, from_type

T = TypeVar("T")
F = TypeVar("F")
MISSING = object()


def escape_pointer(value: str) -> str:
    return value.replace("~", "~0").replace("/", "~1")


def _kind(kind: Any) -> Any:
    if isinstance(kind, tuple):
        if kind and kind[0] == "union":
            variants = frozenset(_kind(k) for k in kind[1])
            return next(iter(variants)) if len(variants) == 1 else ("union", variants)
        if kind and kind[0] == "nullable":
            return _kind(("union", (kind[1], ("literal", type(None), "None"))))
        return tuple(_kind(k) for k in kind)
    return kind


def same_type(left: Schema[Any], right: Schema[Any]) -> bool:
    return _known_kind(left.kind) and _known_kind(right.kind) and _kind(left.kind) == _kind(right.kind)


class ModelBinding(dict[str, str], Generic[T]):
    def __init__(self, path: str, *, scope: RenderScope | None = None, schema: Schema[Any] | None = None):
        dict.__init__(self, path=path)
        self._scope, self._schema = scope, schema

    @property
    def path(self) -> str:
        return self["path"]

    def __setitem__(self, _key: str, _value: str) -> None:
        raise TypeError("model bindings are read-only")

    def __delitem__(self, _key: str) -> None:
        raise TypeError("model bindings are read-only")

    def __deepcopy__(self, _memo: dict[int, Any]) -> dict[str, str]:
        return {"path": self.path}


class Local(Expr[T], Generic[T]):
    def __init__(self, path: str, scope: RenderScope, schema: Schema[Any] | None, initial: Any = MISSING):
        super().__init__(path, _schema=schema, _scope=scope)
        object.__setattr__(self, "_initial", initial)

    def _project(self, key: str | int) -> Local[Any]:
        assert self._scope is not None
        self._scope.check()
        schema = self._schema
        if schema is None:
            raise DeclarationError("field projection needs a declared type; pass type= to local()")
        if schema.members is not None and isinstance(key, str):
            child = schema.members.get(key)
            if child is None:
                raise DeclarationError(f"unknown schema field: {key!r}")
        elif schema.items is not None and isinstance(key, int) and 0 <= key < len(schema.items):
            child = schema.items[key]
        elif schema.element is not None:
            if schema.kind[0] is dict:
                if not isinstance(key, str):
                    raise DeclarationError("map keys must be strings")
            elif not isinstance(key, int) or key < 0:
                raise DeclarationError("array indices must be non-negative integers")
            child = schema.element
        else:
            raise DeclarationError("cannot project a field from this input type")
        initial = self._initial
        if isinstance(initial, dict):
            initial = initial.get(key, MISSING)
        elif isinstance(initial, (list, tuple)) and isinstance(key, int) and 0 <= key < len(initial):
            initial = initial[key]
        else:
            initial = MISSING
        return Local(f"{self.path}/{escape_pointer(str(key))}", self._scope, child, initial)

    @overload
    def field(self, field: Field[T, F]) -> Local[F]: ...
    @overload
    def field(self, field: str, type: type[F]) -> Local[F]: ...
    def field(self, field: Field[T, F] | str, type: Any = MISSING) -> Local[F]:
        if isinstance(field, Field):
            if type is not MISSING:
                raise DeclarationError("a Field descriptor already declares its type")
            if self._schema is None:
                # Legacy JSON locals can opt into a schema through their descriptor.
                field.owner.parse(self._initial)
                source = Local(self.path, self._scope, field.owner, self._initial)
            else:
                source = self
                if not same_type(source._schema, field.owner):
                    raise DeclarationError("field descriptor belongs to a different input type")
            child = source._project(field.name)
            expected = field.schema
        else:
            if type is MISSING:
                raise DeclarationError("typed field projection requires a type")
            child, expected = self._project(field), from_type(type)
        if not same_type(child._schema, expected):
            raise DeclarationError(f"field {field!r} has a different writable type")
        return child

    def __getattr__(self, name: str) -> DynamicLocal:
        if name.startswith("_"):
            raise AttributeError(name)
        return DynamicLocal(self._project(name))

    def __getitem__(self, key: str | int) -> DynamicLocal:
        return DynamicLocal(self._project(key))

    def bind(self) -> ModelBinding[T]:
        assert self._scope is not None
        self._scope.check()
        if self._initial is MISSING:
            raise DeclarationError("binding an absent field requires an explicit initial value")
        return ModelBinding(self.path, scope=self._scope, schema=self._schema)

    def value(self) -> Expr[T]:
        assert self._scope is not None
        self._scope.check()
        return Expr(self.path, _schema=self._schema, _scope=self._scope)


class DynamicLocal:
    """A checked-at-render reference, deliberately neither Any nor Local[Any]."""
    def __init__(self, local: Local[Any]):
        self._local = local

    def __getattr__(self, name: str) -> DynamicLocal:
        if name.startswith("_"):
            raise AttributeError(name)
        return DynamicLocal(self._local._project(name))

    def __getitem__(self, key: str | int) -> DynamicLocal:
        return DynamicLocal(self._local._project(key))

    def __bool__(self) -> bool:
        raise DeclarationError("a browser reference is not a Python value")
