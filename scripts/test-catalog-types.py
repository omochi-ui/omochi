"""Check actual native-generated Python bindings with both supported checkers.

Build the native CLI first, then run with mypy and pyright installed:
    python scripts/test-catalog-types.py
"""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
SDK = ROOT / "packages/sdk/python/src"
CLI = ROOT / "crates/target/debug/omochi-ui"
ENV = {**os.environ, "MYPYPATH": str(SDK), "PYTHONPATH": str(SDK)}
BADGE = {
    "properties": {
        "label": {"type": "string"},
        "tone": {"enum": ["info", "warning"]},
        "self": {"type": "string"},
        "data-label": {"type": "string"},
        "details": {"type": "object", "properties": {"key": {"type": "string"}}},
    },
    "required": ["label"],
}
SETUP = '''from catalog import {constant}, {catalog_name}UI
ui = {catalog_name}UI("main", {constant}, lambda *args: "unused")
assert ui.catalog is {constant}
assert {constant}.id == "urn:test:typing"
assert {constant}.name == "{catalog_name}"
assert {constant}.ui_type is {catalog_name}UI
'''
POSITIVE = '''from catalog import BadgeProps
props: BadgeProps = {"label": "hello", "tone": "info", "self": "self", "data-label": "literal", "details": {"key": "Enter"}}
ui.component("Badge", key="badge", **props)
ui.component(component="Badge", key="keyword", label="hello", self="self")
nodes = ui.build().root.slots[0].nodes
assert [(node.component, node.key, node.props) for node in nodes] == [
    ("Badge", "badge", props),
    ("Badge", "keyword", {"label": "hello", "self": "self"}),
]
'''
NEGATIVE = '''from catalog import BadgeProps
ui.component("Badge", key="missing")  # error: required label
ui.component("Badge", key="wrong", label=1)  # error: label type
ui.component("Badge", key="enum", label="hello", tone="bad")  # error: enum
ui.component("Badge", key="extra", label="hello", typo=True)  # error: unknown prop
ui.component("Badge", key="self", label="hello", self=1)  # error: literal self prop
invalid: BadgeProps = {"label": "hello", "data-label": 1}  # error: literal-key type
ui.component("Missing", key="unknown")  # error: unknown component
ui.component("Text", key="builtin", text="hello")  # error: outside catalog
'''


def check(checker: str, directory: Path) -> None:
    base = [sys.executable, "-m", checker]
    if checker == "mypy":
        base += ["--follow-imports=silent"]
    # Include the generated declaration explicitly: import-only checking can
    # hide broken overloads/overrides when imported-module checking is silent.
    subprocess.run(
        [*base, "catalog.py", "positive.py"], cwd=directory, env=ENV, check=True,
    )
    args = [*base, "catalog.py", "negative.py"]
    if checker == "pyright":
        args += ["--outputjson"]
    result = subprocess.run(args, cwd=directory, env=ENV, text=True, capture_output=True)
    source = (directory / "negative.py").read_text().splitlines()
    expected = {i for i, line in enumerate(source, 1) if "# error:" in line}
    if checker == "mypy":
        errors = re.findall(r"^(.+?):(\d+): error:", result.stdout, re.MULTILINE)
        actual = {(Path(file).name, int(line)) for file, line in errors}
    else:
        errors = json.loads(result.stdout)["generalDiagnostics"]
        actual = {
            (Path(item["file"]).name, item["range"]["start"]["line"] + 1)
            for item in errors if item["severity"] == "error"
        }
    expected_errors = {("negative.py", line) for line in expected}
    if result.returncode != 1 or actual != expected_errors:
        raise AssertionError(
            f"{checker}: expected {sorted(expected_errors)}, got {sorted(actual)}\n"
            f"{result.stdout}\n{result.stderr}"
        )
    print(f"{checker}: {directory.name}: all {len(expected)} invalid uses rejected", flush=True)


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="omochi-catalog-typing-") as temporary:
        rejected = Path(temporary) / "reserved-key"
        rejected.mkdir()
        outputs = [rejected / f"catalog.{extension}" for extension in ("ts", "py", "rs")]
        for output in outputs:
            output.write_text("existing")
        result = subprocess.run([
            str(CLI), "generate", "catalog", "--catalog",
            str(ROOT / "spec/fixtures/catalog-builder-key.json"),
            "--python", str(outputs[1]), "--rust", str(outputs[2]),
        ], cwd=ROOT, text=True, capture_output=True)
        assert result.returncode != 0, "Python accepted a builder-reserved component property"
        assert 'component "Shortcut", property "key"' in result.stderr, result.stderr
        assert "reserved for the Python builder's node key" in result.stderr, result.stderr
        assert [output.read_text() for output in outputs] == ["existing"] * 3
        for name, components, title, name_override in [
            ("empty", {}, "TestCatalog", None),
            ("single", {"Badge": BADGE}, "TestCatalog", None),
            ("multiple", {"Badge": BADGE, "Empty": {"properties": {}}}, "TestCatalog", None),
            ("ui-title", {"Badge": BADGE}, "UI", None),
            ("ui-name-override", {"Badge": BADGE}, "TestCatalog", "UI"),
        ]:
            directory = Path(temporary) / name
            directory.mkdir()
            # Exercise Pyright's own targeted suppression independently of mypy's.
            (directory / "pyrightconfig.json").write_text(json.dumps({
                "enableTypeIgnoreComments": False,
            }))
            (directory / "input.json").write_text(json.dumps({
                "catalogId": "urn:test:typing", "title": title, "components": components,
            }))
            subprocess.run([
                str(CLI), "generate", "catalog", "--catalog", str(directory / "input.json"),
                "--python", str(directory / "catalog.py"),
                *(["--name", name_override] if name_override else []),
            ], cwd=ROOT, check=True)
            catalog_name = name_override or title
            positive = SETUP.format(constant=catalog_name.upper(), catalog_name=catalog_name)
            negative = positive
            if components:
                positive += POSITIVE
                negative += NEGATIVE
            else:
                positive += "assert ui.build().root.slots[0].nodes == []\n"
                negative += 'ui.component("Badge", key="unknown")  # error: empty catalog\n'
                negative += 'ui.component("Text", key="builtin", text="hello")  # error: empty catalog\n'
            if name == "multiple":
                positive += 'other = TestCatalogUI("other", TESTCATALOG, lambda *args: "unused")\n'
                positive += 'other.component("Empty", key="empty")\n'
                positive += 'assert other.build().root.slots[0].nodes[0].props == {}\n'
                negative += 'ui.component("Empty", key="extra", label="hello")  # error: wrong overload props\n'
            (directory / "positive.py").write_text(positive)
            (directory / "negative.py").write_text(negative)
            for checker in ("mypy", "pyright"):
                check(checker, directory)
            subprocess.run([sys.executable, "-B", "positive.py"], cwd=directory, env=ENV, check=True)


if __name__ == "__main__":
    main()
