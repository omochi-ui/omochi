import json
from typing import get_type_hints

from catalog import EmptyProps, NestedProps, SiblingProps

sibling: SiblingProps = {"label": "label", "count": 2}
nested: NestedProps = {"label": "label", "count": 2, "isValid": True}
empty: EmptyProps = {"label": "label"}
optional: NestedProps = {
    **nested,
    "note": "note",
    "accessibility": {"label": "accessible"},
    "checks": [],
    "validationErrors": ["error"],
}
assert get_type_hints(SiblingProps) == {"label": str, "count": float, "note": str}
assert get_type_hints(EmptyProps) == {"label": str}
hints = get_type_hints(NestedProps)
assert hints.keys() == optional.keys()
assert hints["label"] is str and hints["count"] is float
assert hints["note"] is str and hints["isValid"] is bool
for props_type, required, optional_keys in [
    (SiblingProps, sibling.keys(), {"note"}),
    (
        NestedProps,
        nested.keys(),
        {"note", "accessibility", "checks", "validationErrors"},
    ),
    (EmptyProps, empty.keys(), set()),
]:
    assert props_type.__required_keys__ == required
    assert props_type.__optional_keys__ == optional_keys
print(json.dumps([sibling, nested, empty, optional]))
