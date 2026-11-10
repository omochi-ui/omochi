import json
from typing import get_args, get_overloads, get_type_hints

from catalog import Card2Props, GeneratedCatalogUI, SelfProps, _BadgeProps, classProps

components = {
    "Card2": Card2Props(label="card"),
    "Self": SelfProps(label="self"),
    "_Badge": _BadgeProps(label="badge"),
    "class": classProps(label="keyword"),
}
for props in [Card2Props, SelfProps, _BadgeProps, classProps]:
    assert get_type_hints(props) == {"label": str}
    assert props.__required_keys__ == {"label"}
overloads = get_overloads(GeneratedCatalogUI.component)
assert [
    get_args(get_type_hints(method)["component"])[0] for method in overloads
] == list(components)
print(json.dumps(components))
