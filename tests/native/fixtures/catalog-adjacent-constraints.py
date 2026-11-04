import json
from typing import Any, get_type_hints

from catalog import AdjacentProps

valid: AdjacentProps = {
    "choice": "allowed", "details": {"label": "label", "count": 2}, "typed": "allowed", "both": "label",
    "items": ["allowed"], "objects": [{"label": "label", "count": 2}], "array": ["allowed"],
}
assert get_type_hints(AdjacentProps) == {
    "choice": Any, "details": dict[str, Any], "typed": Any, "both": Any,
    "items": list[Any], "objects": list[dict[str, Any]], "array": Any,
}
assert AdjacentProps.__required_keys__ == set(valid)
print(json.dumps([valid]))
