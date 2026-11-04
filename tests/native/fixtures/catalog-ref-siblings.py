import json
from typing import Any, get_type_hints

from catalog import RefProps

valid: RefProps = {
    "choice": "b", "reverse": "b", "annotated": "label",
    "details": {"label": "label", "count": 2, "enabled": True},
    "requiredOnly": {"label": "label", "count": 2}, "array": ["b"],
    "arrayType": ["label"], "objectType": {"label": "label"}, "typed": "b", "union": "label",
    "combined": "b", "requiredInAllOf": {"label": "label", "count": 2},
}
assert get_type_hints(RefProps) == {
    "choice": Any, "reverse": Any, "annotated": str,
    "details": dict[str, Any], "requiredOnly": dict[str, Any], "array": Any,
    "arrayType": Any, "objectType": dict[str, Any], "typed": Any, "union": Any,
    "combined": Any, "requiredInAllOf": dict[str, Any],
}
assert RefProps.__required_keys__ == set(valid)
assert not RefProps.__optional_keys__
print(json.dumps([valid]))
