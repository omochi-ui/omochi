import json
from typing import Literal, get_args, get_type_hints

from catalog import ScalarProps

hints = get_type_hints(ScalarProps)
assert hints["enabled"] == Literal[True, False]
assert hints["count"] == Literal[1, 2]
assert hints["ratio"] is float
assert hints["empty"] == Literal[None]
assert hints["mixed"] == Literal[True, False, 1, 2, None, "true"] | float
assert hints["flags"] == list[Literal[True, False]]
assert hints["counts"] == list[list[Literal[1, 2]]]
assert hints["choices"] == list[Literal[True, False] | Literal[1, 2]]
assert hints["nullable"] == Literal[None] | Literal["text"]
assert hints["optional"] == Literal[True, False]
assert json.dumps(get_args(hints["enabled"])) == "[true, false]"
assert json.dumps(get_args(hints["count"])) == "[1, 2]"
assert json.dumps(get_args(hints["empty"])) == "[null]"
