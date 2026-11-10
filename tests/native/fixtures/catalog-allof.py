import json
from typing import Any, Required, get_type_hints

from catalog import CombinedProps, KeywordProps

expected = {
    "object": dict[str, Any],
    "single": str,
    "primitive": str,
    "mixed": Any,
    "conditional": str,
    "array": list[Any],
    "union": Any | bool,
    "nested": str,
}
assert get_type_hints(CombinedProps) == expected
assert get_type_hints(CombinedProps, include_extras=True) == {
    key: Required[value] for key, value in expected.items()
}
assert CombinedProps.__required_keys__ == expected.keys()
assert get_type_hints(KeywordProps) == {"class": str}
assert KeywordProps.__required_keys__ == {"class"}
props: CombinedProps = {
    "object": {"label": "label", "enabled": True},
    "single": "single",
    "primitive": "primitive",
    "mixed": "ready",
    "conditional": "ready",
    "array": ["ready"],
    "union": True,
    "nested": "nested",
}
assert json.loads(json.dumps(props)) == props
