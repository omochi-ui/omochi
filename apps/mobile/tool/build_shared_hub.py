#!/usr/bin/env python3
"""Build the embedded Rust Hub; never starts a local HTTP service."""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

APP = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--platform", choices=["linux", "android"], required=True)
parser.add_argument("--profile", choices=["debug", "release"], default="debug")
parser.add_argument("--abi", action="append", choices=["arm64-v8a", "x86_64"])
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
root = Path(os.environ.get("OMOCHI_HUB_ROOT", APP.parents[1])).resolve()
manifest = root / "crates" / "Cargo.toml"
if not manifest.exists():
    parser.error(
        "Set OMOCHI_HUB_ROOT to the Omochi repository containing crates/Cargo.toml"
    )
# Share incremental Rust output across platform build systems and test commands.
target_dir = Path(
    os.environ.get("CARGO_TARGET_DIR", root / "crates" / "target")
).resolve()
if args.platform == "linux":
    targets = [(None, None)]
else:
    sdk = Path(
        os.environ.get(
            "ANDROID_HOME",
            os.environ.get("ANDROID_SDK_ROOT", Path.home() / "Android" / "Sdk"),
        )
    )
    ndk = Path(os.environ.get("ANDROID_NDK_HOME", sdk / "ndk" / "30.0.16248370"))
    hosts = {
        "linux": "linux-x86_64",
        "darwin": "darwin-x86_64",
        "win32": "windows-x86_64",
    }
    tools = ndk / "toolchains" / "llvm" / "prebuilt" / hosts[sys.platform] / "bin"
    if not tools.exists():
        parser.error("Install Android NDK 30.0.16248370 or set ANDROID_NDK_HOME")
    targets = [
        (
            abi,
            {"arm64-v8a": "aarch64-linux-android", "x86_64": "x86_64-linux-android"}[
                abi
            ],
        )
        for abi in args.abi or ["arm64-v8a", "x86_64"]
    ]
for abi, target in targets:
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target_dir)
    command = [
        "cargo",
        "build",
        "--locked",
        "--manifest-path",
        str(manifest),
        "-p",
        "omochi-native-bridge",
    ]
    if args.profile == "release":
        command.append("--release")
    source = target_dir
    if target:
        command += ["--target", target]
        linker = tools / f"{target}24-clang"
        env[f"CARGO_TARGET_{target.upper().replace('-', '_')}_LINKER"] = str(linker)
        env[f"CC_{target.replace('-', '_')}"] = str(linker)
        env[f"AR_{target.replace('-', '_')}"] = str(tools / "llvm-ar")
        source /= target
    subprocess.run(command, env=env, check=True)
    destination = args.output / abi if abi else args.output
    destination.mkdir(parents=True, exist_ok=True)
    # Never truncate a .so mapped by a running Flutter/test process: that causes
    # SIGBUS on its next page fault. Replacing a completed inode is safe.
    with tempfile.NamedTemporaryFile(
        dir=destination, prefix=".shared-hub-", delete=False
    ) as temporary:
        staged = Path(temporary.name)
    try:
        shutil.copy2(source / args.profile / "libomochi_native_bridge.so", staged)
        os.replace(staged, destination / "libomochi_native_bridge.so")
    finally:
        staged.unlink(missing_ok=True)
