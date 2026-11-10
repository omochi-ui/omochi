import json
from typing import Any, get_type_hints

from catalog import DirectProps, NestedProps, OnlyProps

direct: DirectProps = {"label": "label", "metadata": {"enabled": True}}
nested: NestedProps = {"label": "label", "parentData": None, "nestedData": [1, "two"]}
only: OnlyProps = {"payload": False}
for props_type, hints, required, optional in [
    (DirectProps, {"label": str, "metadata": Any}, direct.keys(), set()),
    (
        NestedProps,
        {"label": str, "note": str, "parentData": Any, "nestedData": Any},
        nested.keys(),
        {"note"},
    ),
    (OnlyProps, {"payload": Any}, only.keys(), set()),
]:
    assert get_type_hints(props_type) == hints
    assert props_type.__required_keys__ == required
    assert props_type.__optional_keys__ == optional
print(json.dumps([direct, nested, only]))
