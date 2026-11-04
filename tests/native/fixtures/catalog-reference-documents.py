from typing import get_type_hints

from catalog import ReferenceProps

assert get_type_hints(ReferenceProps) == {"value": str}
assert ReferenceProps.__required_keys__ == {"value"}
assert ReferenceProps.__optional_keys__ == set()
