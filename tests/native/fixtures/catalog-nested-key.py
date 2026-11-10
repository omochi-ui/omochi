import json

from catalog import NESTEDKEYCATALOG, NestedKeyCatalogUI

ui = NestedKeyCatalogUI("main", NESTEDKEYCATALOG, lambda *args: "unused")
ui.component("Shortcut", key="shortcut", details={"key": "Enter"})
nodes = ui.build().root.slots[0].nodes
print(
    json.dumps(
        [
            {"component": node.component, "key": node.key, "props": node.props}
            for node in nodes
        ]
    )
)
