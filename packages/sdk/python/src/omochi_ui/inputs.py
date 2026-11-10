"""Validate unresolved declarations without evaluating browser expressions."""

from __future__ import annotations

from dataclasses import fields, is_dataclass
from typing import Any

from .errors import DeclarationError, ProtocolError
from .expression import Expr, Composed, encode_input, references
from .references import DynamicLocal, same_type
from .schema import Schema


def accepts(target: Schema[Any], source: Schema[Any]) -> bool:
    if same_type(target, source):
        return True
    if source.variants:
        return all(accepts(target, variant) for variant in source.variants)
    if target.variants:
        return any(accepts(variant, source) for variant in target.variants)
    if isinstance(target.kind, tuple) and target.kind[0] == "nullable":
        return (
            source.kind == ("literal", type(None), "None")
            or source.kind == target.kind[1]
        )
    if isinstance(source.kind, tuple) and source.kind[0] == "literal":
        literal_type = source.kind[1]
        return (
            target.kind is literal_type or target.kind is float and literal_type is int
        )
    # Read-only integer expressions can be consumed as finite numbers. Writable
    # Local projections and controls use exact equality instead.
    return target.kind is float and source.kind is int


def validate(
    schema: Schema[Any],
    value: Any,
    path: str = "$input",
    ancestors: set[int] | None = None,
) -> None:
    if isinstance(value, DynamicLocal):
        value = value._local
    if isinstance(value, Expr):
        for ref in references(value):
            if ref._scope is not None:
                ref._scope.check()
        if value._schema is not None and not accepts(schema, value._schema):
            raise DeclarationError(f"{path}: reference has a different result type")
        return
    if not references(value):
        try:
            schema.decode(encode_input(value), path)
        except ProtocolError as error:
            raise DeclarationError(str(error)) from error
        return
    ancestors = ancestors if ancestors is not None else set()
    if id(value) in ancestors:
        raise DeclarationError("cyclic action input")
    identity = id(value)
    ancestors.add(identity)
    try:
        if schema.variants:
            for variant in schema.variants:
                try:
                    validate(variant, value, path, set(ancestors) - {id(value)})
                    return
                except DeclarationError:
                    pass
            raise DeclarationError(f"{path}: no matching input variant")
        if is_dataclass(value) and not isinstance(value, type):
            value = {f.name: getattr(value, f.name) for f in fields(value) if f.init}
        if schema.members is not None and isinstance(value, dict):
            for key, member in schema.members.items():
                if key not in value:
                    if not member.optional:
                        raise DeclarationError(f"{path}/{key}: missing required field")
                else:
                    validate(member, value[key], f"{path}/{key}", ancestors)
            for key in value.keys() - schema.members.keys():
                if schema.unknown_keys == "reject":
                    raise DeclarationError(f"{path}/{key}: unknown input field")
                if schema.unknown_keys == "allow":
                    encode_input(value[key])
        elif (
            schema.element is not None
            and isinstance(value, dict)
            and schema.kind[0] is dict
        ):
            for key, item in value.items():
                if not isinstance(key, str):
                    raise DeclarationError(f"{path}: expected string map keys")
                validate(schema.element, item, f"{path}/{key}", ancestors)
        elif isinstance(value, (list, tuple)) and (
            schema.element is not None or schema.items is not None
        ):
            if schema.items is not None and len(value) != len(schema.items):
                raise DeclarationError(f"{path}: incorrect tuple length")
            for index, item in enumerate(value):
                member = (
                    schema.items[index] if schema.items is not None else schema.element
                )
                validate(member, item, f"{path}/{index}", ancestors)
        else:
            raise DeclarationError(
                f"{path}: expressions do not match the declared input structure"
            )
    finally:
        ancestors.remove(identity)


def _normalize(schema: Schema[Any], value: Any) -> Any:
    """Apply structural strip policies without evaluating or coercing references."""
    if isinstance(value, (Expr, DynamicLocal)):
        return value
    if schema.variants:
        for variant in schema.variants:
            try:
                validate(variant, value)
            except DeclarationError:
                continue
            return _normalize(variant, value)
    if is_dataclass(value) and not isinstance(value, type):
        value = {f.name: getattr(value, f.name) for f in fields(value) if f.init}
    if schema.members is not None and isinstance(value, dict):
        return {
            key: _normalize(schema.members[key], item)
            if key in schema.members
            else item
            for key, item in value.items()
            if key in schema.members or schema.unknown_keys != "strip"
        }
    if schema.element is not None and isinstance(value, dict):
        return {key: _normalize(schema.element, item) for key, item in value.items()}
    if isinstance(value, (list, tuple)) and (
        schema.element is not None or schema.items is not None
    ):
        return [
            _normalize(
                schema.items[i] if schema.items is not None else schema.element, item
            )
            for i, item in enumerate(value)
        ]
    return value


def compose(schema: Schema[Any], value: Any) -> Composed[Any]:
    validate(schema, value)
    value = _normalize(schema, value)
    return Composed("", encode_input(value), references(value), _schema=schema)
