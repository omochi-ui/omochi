"""Check accepted and rejected Python SDK usage with both supported checkers."""

import ast
import json
import os
import re
import subprocess
import sys
from pathlib import Path


def error_markers(source: str) -> dict[int, tuple[int, int]]:
    """Locate each marked statement, including multiline calls and decorators."""
    markers = {
        line for line, text in enumerate(source.splitlines(), 1) if "# error:" in text
    }
    statements = []
    for node in ast.walk(ast.parse(source)):
        if not isinstance(node, ast.stmt):
            continue
        start, end = node.lineno, node.end_lineno
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            start = min([start, *(item.lineno for item in node.decorator_list)])
            # A declaration marker covers its header, never unrelated body errors.
            end = max(node.lineno, node.body[0].lineno - 1)
        statements.append((start, end))
    return {
        marker: min(
            (span for span in statements if span[0] <= marker <= span[1]),
            key=lambda span: span[1] - span[0],
        )
        for marker in markers
    }


root = Path(__file__).resolve().parents[1]
sdk = root / "packages/sdk/python"
env = {**os.environ, "MYPYPATH": str(sdk / "src"), "PYTHONPATH": str(sdk / "src")}
for checker in ("mypy", "pyright"):
    base = [sys.executable, "-m", checker]
    if checker == "mypy":
        base += ["--follow-imports=silent"]
    for prefix, expected in [("", 8), ("interface_", 10)]:
        positive = str(sdk / f"tests/typing/{prefix}positive.py")
        negative = str(sdk / f"tests/typing/{prefix}negative.py")
        subprocess.run([*base, positive], cwd=root, env=env, check=True)
        args = [*base, negative]
        if checker == "pyright":
            args += ["--outputjson"]
        result = subprocess.run(
            args, check=False, cwd=root, env=env, text=True, capture_output=True
        )
        markers = error_markers(Path(negative).read_text())
        expected_lines = set(markers)
        if checker == "mypy":
            error_lines = {
                int(line) for line in re.findall(r":(\d+): error:", result.stdout)
            }
        else:
            diagnostics = json.loads(result.stdout)["generalDiagnostics"]
            errors = [item for item in diagnostics if item["severity"] == "error"]
            if any(
                Path(item["file"]).resolve() != Path(negative).resolve()
                for item in errors
            ):
                raise SystemExit(
                    f"{checker}: unexpected errors outside {negative}: {errors}"
                )
            error_lines = {item["range"]["start"]["line"] + 1 for item in errors}
        # Checkers can report a multiline call at its start or at an argument.
        # Match the marked statement while retaining errors outside that span.
        actual_lines = {
            next(
                (
                    marker
                    for marker, (start, end) in markers.items()
                    if start <= line <= end
                ),
                line,
            )
            for line in error_lines
        }
        if (
            result.returncode != 1
            or actual_lines != expected_lines
            or len(expected_lines) != expected
        ):
            print(result.stdout, result.stderr)
            raise SystemExit(
                f"{checker}: expected errors on {sorted(expected_lines)}, found {sorted(actual_lines)}"
            )
        print(f"{checker}: {prefix or 'legacy'} all {expected} invalid uses rejected")
