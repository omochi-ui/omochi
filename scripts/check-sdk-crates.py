"""Package both Rust SDK crates and compile examples from the actual archives."""
import argparse
from pathlib import Path
import subprocess
import tarfile
import tempfile
import tomllib

parser = argparse.ArgumentParser()
parser.add_argument("--allow-dirty", action="store_true")
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
sdk = root / "packages/sdk/rust"
version = tomllib.loads((sdk / "Cargo.toml").read_text())["package"]["version"]
command = ["cargo", "package", "--manifest-path", str(sdk / "Cargo.toml"), "--workspace", "--locked", "--no-verify"]
if args.allow_dirty: command.append("--allow-dirty")
subprocess.run(command, check=True)
with tempfile.TemporaryDirectory(prefix="omochi-sdk-crates-") as directory:
    extracted = Path(directory)
    for name in ("omochi-ui-macros", "omochi-ui"):
        with tarfile.open(sdk / "target/package" / f"{name}-{version}.crate") as archive:
            archive.extractall(extracted, filter="data")
    macro = extracted / f"omochi-ui-macros-{version}"
    package = extracted / f"omochi-ui-{version}"
    # Use the packaged macro, never the source-tree macro. This also works before
    # the dependency version exists on crates.io and avoids Cargo's tmp-registry bug.
    subprocess.run(["cargo", "check", "--manifest-path", str(package / "Cargo.toml"), "--examples", "--target-dir", str(sdk / "target/package-check"), "--config", f'patch.crates-io.omochi-ui-macros.path="{macro}"'], check=True)
