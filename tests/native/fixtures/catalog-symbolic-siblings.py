from typing import Any, get_type_hints

from catalog import SymbolicProps
from omochi_ui.catalog import DynamicString

valid: SymbolicProps = {
    "label": "allowed", "plain": {"path": "/label"}, "typed": "any string",
    "binding": {"path": "/allowed"}, "items": ["allowed"], "alias": "allowed",
    "nested": [["allowed"]], "list": ["allowed"],
}
assert get_type_hints(SymbolicProps) == {
    "label": Any, "plain": DynamicString, "typed": Any, "binding": Any,
    "items": list[Any], "alias": Any, "nested": list[list[Any]], "list": Any,
}
assert SymbolicProps.__required_keys__ == set(valid)
