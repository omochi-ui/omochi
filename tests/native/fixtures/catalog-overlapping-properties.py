import json
from typing import Any, get_type_hints

from catalog import ForwardProps, ReverseProps, ParentProps, ParentReverseProps

valid = {"value": "b", "details": {"label": "label", "enabled": True, "count": 2}}
forward: ForwardProps = valid
reverse: ReverseProps = valid
parent: ParentProps = valid
parent_reverse: ParentReverseProps = valid
for props_type in [ForwardProps, ReverseProps, ParentProps, ParentReverseProps]:
    assert get_type_hints(props_type) == {
        "value": Any,
        "details": dict[str, Any],
        "same": str,
    }
    assert props_type.__required_keys__ == {"value", "details"}
    assert props_type.__optional_keys__ == {"same"}
print(json.dumps([forward, reverse, parent, parent_reverse]))
