from __future__ import annotations

import math
from dataclasses import dataclass, fields, is_dataclass
from typing import Any, Generic, TypeVar
from .errors import DeclarationError

T = TypeVar("T", covariant=True)

@dataclass(frozen=True)
class Expr(Generic[T]):
    path: str

def encode_input(value: Any, ancestors: set[int] | None = None) -> Any:
    if isinstance(value, Expr): return {"path": value.path}
    if value is None or type(value) in (str, bool, int): return value
    if type(value) is float and math.isfinite(value): return value
    ancestors = ancestors if ancestors is not None else set()
    if id(value) in ancestors: raise DeclarationError("cyclic action input")
    ancestors.add(id(value))
    try:
        if is_dataclass(value) and not isinstance(value, type):
            return {f.name: encode_input(getattr(value, f.name), ancestors) for f in fields(value)}
        if isinstance(value, dict) and all(isinstance(k, str) for k in value):
            return {k: encode_input(v, ancestors) for k, v in value.items()}
        if isinstance(value, (list, tuple)): return [encode_input(v, ancestors) for v in value]
        raise DeclarationError("action input must be JSON, a dataclass, or a typed expression")
    finally: ancestors.remove(id(value))
