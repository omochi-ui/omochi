"""Check accepted and rejected Python SDK usage with both supported checkers."""
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
sdk = root / "packages/sdk/python"
env = {**os.environ, "MYPYPATH": str(sdk / "src"), "PYTHONPATH": str(sdk / "src")}
positive = str(sdk / "tests/typing/positive.py")
negative = str(sdk / "tests/typing/negative.py")
for checker in ("mypy", "pyright"):
    base = [sys.executable, "-m", checker]
    if checker == "mypy": base += ["--follow-imports=silent"]
    subprocess.run([*base, positive], cwd=root, env=env, check=True)
    args = [*base, negative]
    if checker == "pyright": args += ["--outputjson"]
    result = subprocess.run(args, cwd=root, env=env, text=True, capture_output=True)
    if checker == "mypy":
        count = result.stdout.count(": error:")
    else:
        count = json.loads(result.stdout)["summary"]["errorCount"]
    if result.returncode != 1 or count != 6:
        print(result.stdout, result.stderr)
        raise SystemExit(f"{checker}: expected precisely six negative-case diagnostics, found {count}")
    print(f"{checker}: all six invalid uses rejected")
