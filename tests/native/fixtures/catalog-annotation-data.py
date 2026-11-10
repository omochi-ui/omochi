from typing import Any, get_type_hints

from catalog import AnnotationProps

assert get_type_hints(AnnotationProps) == {
    "value": dict[str, Any],
    "default": str,
    "examples": list[str],
}
assert AnnotationProps.__required_keys__ == {"value", "default", "examples"}
assert AnnotationProps.__optional_keys__ == set()
