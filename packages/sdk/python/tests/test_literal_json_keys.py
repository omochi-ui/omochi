from __future__ import annotations

import json
import unittest
from pathlib import Path

from omochi_ui import UI
from omochi_ui.catalog import BASIC_CATALOG
from omochi_ui.errors import DeclarationError
from omochi_ui.ir import Declaration, Node, Slot
from omochi_ui.reconcile import Reconciler


FIXTURE = json.loads((Path(__file__).parents[4] / "spec/fixtures/literal-json-keys.json").read_text())


def node(value):
    return Node(value["key"], value["component"], value["props"], [
        Slot(slot["path"], slot["kind"], [node(child) for child in slot["nodes"]])
        for slot in value.get("slots", [])
    ])


def declaration(root, state):
    return Declaration("main", FIXTURE["catalogId"], node(root), state["data"], state["locals"])


class LiteralJsonKeyTests(unittest.TestCase):
    def test_component_preserves_self_property(self):
        ui = UI("main", BASIC_CATALOG, lambda *args: "unused")
        ui.component("Named", key="positional", **{"self": "positional"})
        ui.component(component="Named", key="keyword", **{"self": "keyword"})
        result = ui.build()
        self.assertEqual(Reconciler().render(result)[1]["updateComponents"]["components"], [
            {"id": "positional", "component": "Named", "self": "positional"},
            {"id": "keyword", "component": "Named", "self": "keyword"},
            {"id": "root", "component": "Column", "children": ["positional", "keyword"]},
        ])

    def test_shared_reconciliation(self):
        reconciler = Reconciler()
        for step in FIXTURE["steps"]:
            with self.subTest(step=step["name"]):
                value = declaration(FIXTURE["root"], step["state"])
                self.assertEqual(reconciler.render(value), step["expectedMessages"])
                self.assertEqual(reconciler.render(value), [])

    def test_public_builder(self):
        ui = UI("main", BASIC_CATALOG, lambda *args: "unused")
        for key in FIXTURE["keys"]:
            ui.local(key, FIXTURE["builder"]["initials"][key])
            with self.assertRaises(DeclarationError):
                ui.local(key, "duplicate")
            ui.text_field(key, label=key, default=FIXTURE["builder"]["defaults"][key])
        result = ui.build()
        self.assertEqual(result.data, {"inputs": FIXTURE["builder"]["defaults"]})
        self.assertEqual(result.locals, {
            key: {"initial": value} for key, value in FIXTURE["builder"]["initials"].items()
        })
        self.assertEqual(json.loads(json.dumps(result.to_dict())), result.to_dict())
        self.assertEqual(Reconciler().render(result)[-1]["updateDataModel"], {
            "surfaceId": "main", "path": "/", "value": {
                "inputs": FIXTURE["builder"]["defaults"], "$local": FIXTURE["builder"]["initials"],
            },
        })
        next_ui = UI("main", BASIC_CATALOG, lambda *args: "unused", result.data)
        for key in FIXTURE["keys"]:
            next_ui.text_field(key, label=key, default="replacement")
        self.assertEqual(next_ui.build().data, result.data)
        self.assertEqual(UI("main", BASIC_CATALOG, lambda *args: "unused", result.data).build().data, {"inputs": {}})

    def test_shared_slot_pointers(self):
        messages = Reconciler().render(declaration(FIXTURE["flatten"]["root"], {"data": {}, "locals": {}}))
        self.assertEqual(messages[1]["updateComponents"], {
            "surfaceId": "main", "components": FIXTURE["flatten"]["expectedComponents"],
        })


if __name__ == "__main__":
    unittest.main()
