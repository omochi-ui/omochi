"""Check accepted and rejected Python SDK usage with both supported checkers."""
import json
import os
import re
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
sdk = root / "packages/sdk/python"
env = {**os.environ, "MYPYPATH": str(sdk / "src"), "PYTHONPATH": str(sdk / "src")}
for checker in ("mypy", "pyright"):
    base = [sys.executable, "-m", checker]
    if checker == "mypy": base += ["--follow-imports=silent"]
    for prefix, expected in [("", 8), ("interface_", 10)]:
        positive = str(sdk / f"tests/typing/{prefix}positive.py")
        negative = str(sdk / f"tests/typing/{prefix}negative.py")
        subprocess.run([*base, positive], cwd=root, env=env, check=True)
        args = [*base, negative]
        if checker == "pyright": args += ["--outputjson"]
        result = subprocess.run(args, cwd=root, env=env, text=True, capture_output=True)
        source = Path(negative).read_text().splitlines()
        expected_lines = {i for i, line in enumerate(source, 1) if "# error:" in line}
        if checker == "mypy":
            error_lines = {int(line) for line in re.findall(r":(\d+): error:", result.stdout)}
        else:
            diagnostics = json.loads(result.stdout)["generalDiagnostics"]
            errors = [item for item in diagnostics if item["severity"] == "error"]
            if any(Path(item["file"]).resolve() != Path(negative).resolve() for item in errors):
                raise SystemExit(f"{checker}: unexpected errors outside {negative}: {errors}")
            error_lines = {item["range"]["start"]["line"] + 1 for item in errors}
        # Overload resolution may produce several diagnostics for one invalid
        # call. Check each marked use, not a checker/version-specific count.
        actual_lines = {
            line + 1 if source[line - 1].lstrip().startswith("@") and line + 1 in expected_lines else line
            for line in error_lines
        }
        if result.returncode != 1 or actual_lines != expected_lines or len(expected_lines) != expected:
            print(result.stdout, result.stderr)
            raise SystemExit(f"{checker}: expected errors on {sorted(expected_lines)}, found {sorted(actual_lines)}")
        print(f"{checker}: {prefix or 'legacy'} all {expected} invalid uses rejected")
