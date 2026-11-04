import json
from typing import get_type_hints

from catalog import NamedProps

props: NamedProps = {
    "sourceURL": "source", "data_key": "data", "type": "type", "match": "match",
    "self": "self", "crate": "crate", "super": "super", "gen": "gen",
}
optional: NamedProps = {**props, "optionalURL": "optional"}
assert NamedProps.__required_keys__ == props.keys()
assert NamedProps.__optional_keys__ == {"optionalURL"}
assert get_type_hints(NamedProps).keys() == optional.keys()
print(json.dumps([props, optional]))
