import json
from typing import get_type_hints

from catalog import NamedProps, PROPERTYNAMESCATALOG, PropertyNamesCatalogUI

props: NamedProps = {
    "sourceURL": "source",
    "data_key": "data",
    "type": "type",
    "match": "match",
    "self": "self",
    "crate": "crate",
    "super": "super",
    "gen": "gen",
}
optional: NamedProps = {**props, "optionalURL": "optional"}
assert NamedProps.__required_keys__ == props.keys()
assert NamedProps.__optional_keys__ == {"optionalURL"}
assert get_type_hints(NamedProps).keys() == optional.keys()
ui = PropertyNamesCatalogUI("main", PROPERTYNAMESCATALOG, lambda *args: "unused")
ui.component("Named", key="required", **props)
ui.component(component="Named", key="optional", **optional)
print(json.dumps([node.props for node in ui.build().root.slots[0].nodes]))
