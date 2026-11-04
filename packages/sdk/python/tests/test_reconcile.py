from __future__ import annotations

import json
import unittest
from pathlib import Path

from omochi_ui.ir import Declaration, Node, Slot
from omochi_ui.reconcile import Reconciler, STANDARD_CATALOG


def declaration(value):
    return Declaration(
        surface_id="main",
        catalog_id=STANDARD_CATALOG,
        root=Node("root", "Button", {
            "action": {"event": {"name": "set", "context": {"input": value}}},
        }, [Slot("/child", "single", [Node("label", "Text", {"text": "Set"})])]),
        data={"value": value},
        locals={"form": {"initial": {"draft": value}, "resetKey": value}},
    )


class ReconcileTests(unittest.TestCase):
    def test_shared_json_type_changes(self):
        fixture = json.loads((Path(__file__).parents[4] / "spec/fixtures/reconciliation.json").read_text())
        for case in fixture["cases"]:
            with self.subTest(case=case["name"]):
                reconciler = Reconciler()
                reconciler.render(declaration(case["before"]))
                messages = reconciler.render(declaration(case["after"]))
                # JSON text makes the assertion distinguish false/0 and true/1 too.
                self.assertEqual(json.dumps(messages, sort_keys=True), json.dumps(case["expectedMessages"], sort_keys=True))
                self.assertEqual(reconciler.render(declaration(case["after"])), [])

    def test_equivalent_json_numbers_do_not_emit_updates(self):
        for before, after in [(0, 0.0), (1.0, 1), ({"nested": [1, 0.0]}, {"nested": [1.0, 0]})]:
            with self.subTest(before=before, after=after):
                reconciler = Reconciler()
                reconciler.render(declaration(before))
                self.assertEqual(reconciler.render(declaration(after)), [])


if __name__ == "__main__":
    unittest.main()
