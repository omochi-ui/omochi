#!/usr/bin/env python3
"""Build/package host native services. Does not publish or install."""

import argparse
import hashlib
import io
import json
import platform
import subprocess
import tarfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKSPACE = ROOT / "crates"


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/native")
    args = parser.parse_args()
    if platform.system() not in ("Linux", "Darwin"):
        parser.error("supported hosts: Linux and macOS")
    target = next(
        line.split(": ", 1)[1]
        for line in run("rustc", "-vV").splitlines()
        if line.startswith("host: ")
    )
    version = tomllib.loads((WORKSPACE / "Cargo.toml").read_text())["workspace"][
        "package"
    ]["version"]
    if not args.skip_build:
        for package in ("core", "renderer", "hub-ui"):
            subprocess.run(
                ["bun", "run", "--filter", f"@omochi-ui/{package}", "build"],
                cwd=ROOT,
                check=True,
            )
        subprocess.run(
            [
                "cargo",
                "build",
                "--manifest-path",
                str(WORKSPACE / "Cargo.toml"),
                "--locked",
                "--release",
                "-p",
                "omochi-cli",
                "-p",
                "omochi-utils",
            ],
            cwd=ROOT,
            check=True,
        )
    files = {}
    for name in ("omochi-ui", "omochi-utils"):
        path = WORKSPACE / "target/release" / name
        if not path.is_file() or run(str(path), "--version") != f"{name} {version}":
            parser.error(f"missing or stale release binary: {path}")
        files[f"bin/{name}"] = (path.read_bytes(), 0o755)
    for name, path in [
        ("LICENSE", ROOT / "LICENSE"),
        ("README.md", WORKSPACE / "DISTRIBUTION.md"),
        ("install.sh", ROOT / "scripts/install-native.sh"),
    ]:
        files[name] = (path.read_bytes(), 0o755 if name.endswith(".sh") else 0o644)
    manifest = {
        "version": version,
        "target": target,
        "commit": run("git", "rev-parse", "HEAD"),
        "dirty": bool(run("git", "status", "--porcelain")),
        "sha256": {
            name: hashlib.sha256(data).hexdigest() for name, (data, _) in files.items()
        },
    }
    files["manifest.json"] = ((json.dumps(manifest, indent=2) + "\n").encode(), 0o644)
    args.output.mkdir(parents=True, exist_ok=True)
    archive = args.output / f"omochi-{version}-{target}.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        for name, (data, mode) in sorted(files.items()):
            info = tarfile.TarInfo(name)
            info.size, info.mode, info.mtime = len(data), mode, 0
            tar.addfile(info, io.BytesIO(data))
    archive.with_suffix(".gz.sha256").write_text(
        f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n"
    )
    print(archive)


if __name__ == "__main__":
    main()
