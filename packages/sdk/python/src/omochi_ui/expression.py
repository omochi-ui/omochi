from __future__ import annotations

import math
from dataclasses import dataclass, field, fields, is_dataclass
from typing import Any, Generic, TypeVar, TYPE_CHECKING

if TYPE_CHECKING:
    from .schema import Schema
    from ._context import RenderScope
from .errors import DeclarationError

T = TypeVar("T", covariant=True)


@dataclass(frozen=True)
class Expr(Generic[T]):
    path: str
    _schema: Schema[Any] | None = field(
        default=None, kw_only=True, repr=False, compare=False
    )
    _scope: RenderScope | None = field(
        default=None, kw_only=True, repr=False, compare=False
    )

    def __bool__(self) -> bool:
        raise DeclarationError("a browser reference is not a Python value")


Ref = Expr


@dataclass(frozen=True)
class Composed(Expr[T]):
    encoded: Any = None
    sources: tuple[Expr[Any], ...] = ()


def references(value: Any, ancestors: set[int] | None = None) -> tuple[Expr[Any], ...]:
    """Retain provenance until the declaration is attached to its Builder."""
    from .references import DynamicLocal

    if isinstance(value, DynamicLocal):
        value = value._local
    if isinstance(value, Composed):
        return value.sources
    if isinstance(value, Expr) and not isinstance(value, ObjectMerge):
        return (value,)
    ancestors = ancestors if ancestors is not None else set()
    if id(value) in ancestors:
        raise DeclarationError("cyclic action input")
    ancestors.add(id(value))
    try:
        if isinstance(value, ObjectMerge):
            children = [part.value for part in value.parts]
        elif is_dataclass(value) and not isinstance(value, type):
            children = [getattr(value, f.name) for f in fields(value) if f.init]
        elif isinstance(value, dict):
            children = list(value.values())
        elif isinstance(value, (list, tuple)):
            children = value
        else:
            children = ()
        return tuple(ref for child in children for ref in references(child, ancestors))
    finally:
        ancestors.remove(id(value))


def encode_input(value: Any, ancestors: set[int] | None = None) -> Any:
    from .references import DynamicLocal

    if isinstance(value, DynamicLocal):
        value = value._local
    if isinstance(value, Composed):
        for ref in value.sources:
            if ref._scope is not None:
                ref._scope.check()
        from copy import deepcopy

        return deepcopy(value.encoded)
    if isinstance(value, ObjectMerge):
        ancestors = ancestors if ancestors is not None else set()
        if id(value) in ancestors:
            raise DeclarationError("cyclic action input")
        ancestors.add(id(value))
        try:
            return {
                "call": "omochiMergeObjects",
                "args": {
                    "objects": [encode_input(p.value, ancestors) for p in value.parts]
                },
                "returnType": "object",
            }
        finally:
            ancestors.remove(id(value))
    if isinstance(value, Expr):
        if value._scope is not None:
            value._scope.check()
        return {"path": value.path}
    if value is None or type(value) in (str, bool, int):
        return value
    if type(value) is float and math.isfinite(value):
        return value
    ancestors = ancestors if ancestors is not None else set()
    if id(value) in ancestors:
        raise DeclarationError("cyclic action input")
    ancestors.add(id(value))
    try:
        if is_dataclass(value) and not isinstance(value, type):
            return {
                f.name: encode_input(getattr(value, f.name), ancestors)
                for f in fields(value)
                if f.init
            }
        if isinstance(value, dict) and all(isinstance(k, str) for k in value):
            return {k: encode_input(v, ancestors) for k, v in value.items()}
        if isinstance(value, (list, tuple)):
            return [encode_input(v, ancestors) for v in value]
        raise DeclarationError(
            "action input must be JSON, a dataclass, or a typed expression"
        )
    finally:
        ancestors.remove(id(value))


MERGE_OBJECTS_CAPABILITY = "omochi.mergeObjects/1"


@dataclass(frozen=True)
class SchemaValue(Generic[T]):
    schema: Any
    value: T | Expr[T]


@dataclass(frozen=True)
class ObjectMerge(Expr[T]):
    parts: tuple[SchemaValue[Any], ...] = ()


def merge_objects(result: Schema[T], *parts: SchemaValue[Any]) -> ObjectMerge[T]:
    from .schema import _known_kind

    if result.members is None and not (
        isinstance(result.kind, tuple) and result.kind[0] is dict
    ):
        raise DeclarationError("merge result must be an object schema")
    members = {}
    for part in parts:
        source = part.schema
        if source.members is None:
            if source.kind != result.kind or not _known_kind(source.kind):
                raise DeclarationError(
                    "dynamic maps require the same result map schema"
                )
        else:
            for name, field in source.members.items():
                if name in members:
                    raise DeclarationError(f"duplicate input key: {name}")
                members[name] = field
                expected = (
                    result.members.get(name) if result.members is not None else None
                )
                target = (
                    expected.kind
                    if expected
                    else result.kind[1]
                    if result.members is None
                    else None
                )
                if not _known_kind(target) or field.kind != target:
                    raise DeclarationError(f"incompatible merged field: {name}")
    if result.members is not None:
        for name, field in result.members.items():
            if not field.optional and (name not in members or members[name].optional):
                raise DeclarationError(f"missing merged field: {name}")
    return ObjectMerge("", tuple(parts), _schema=result)


def uses_object_merge(value: Any) -> bool:
    if isinstance(value, dict):
        return value.get("call") == "omochiMergeObjects" or any(
            uses_object_merge(v) for v in value.values()
        )
    return isinstance(value, list) and any(uses_object_merge(v) for v in value)
