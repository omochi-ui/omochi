import json
from typing import Required, get_type_hints

from catalog import NamedProps

props: NamedProps = {
    "class": "button",
    "async": True,
    "data-label": "label",
    "1st": 1,
    "": "empty",
    "__private": "private",
    "K": "kelvin",
    'quote"line\n': "escaped",
}
optional: NamedProps = {**props, "None": "none", "optional key": "optional"}
assert NamedProps.__required_keys__ == props.keys()
assert NamedProps.__optional_keys__ == {"None", "optional key"}
hints = get_type_hints(NamedProps)
assert hints == {
    key: bool if key == "async" else float if key == "1st" else str for key in optional
}
annotated = get_type_hints(NamedProps, include_extras=True)
assert annotated == {
    key: Required[value] if key in props else value for key, value in hints.items()
}
assert "K" not in hints and "_NamedProps__private" not in hints
assert json.loads(json.dumps([props, optional])) == [props, optional]
print(json.dumps([props, optional]))
