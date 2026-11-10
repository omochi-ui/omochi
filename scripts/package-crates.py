#!/usr/bin/env python3
"""Package/verify all native crates, including the built dashboard, without publishing."""

import argparse
import hashlib
import json
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
from contextlib import contextmanager
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKSPACE = ROOT / "crates"


def patch_arguments(packages):
    arguments = []
    for name, directory in packages.items():
        arguments.extend(
            ["--config", f"patch.crates-io.{name}.path={json.dumps(str(directory))}"]
        )
    return arguments


def package_native(command, sdk):
    if not sdk:
        subprocess.run(command, cwd=ROOT, check=True)
        return
    # Adding unpublished patches records unused patches in the checkout's lock
    # before Cargo normalizes path dependencies for the archives. Keep that
    # temporary bookkeeping out of the source lock and release artifacts.
    lock = WORKSPACE / "Cargo.lock"
    original = lock.read_bytes()
    try:
        subprocess.run(
            [argument for argument in command if argument != "--locked"]
            + patch_arguments(sdk),
            cwd=ROOT,
            check=True,
        )
    finally:
        lock.write_bytes(original)


@contextmanager
def local_sdk(enabled, allow_dirty):
    if not enabled:
        yield {}
        return
    sdk = ROOT / "packages/sdk/rust"
    version = tomllib.loads((sdk / "Cargo.toml").read_text())["package"]["version"]
    command = [
        "cargo",
        "package",
        "--manifest-path",
        str(sdk / "Cargo.toml"),
        "--workspace",
        "--exclude",
        "omochi-local-node",
        "--exclude",
        "omochi-local-ffi",
        "--locked",
        "--no-verify",
    ]
    if allow_dirty:
        command.append("--allow-dirty")
    subprocess.run(command, cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="omochi-native-sdk-") as temporary:
        packages = {}
        for name in ["omochi-local", "omochi-ui-macros", "omochi-ui"]:
            with tarfile.open(
                sdk / "target/package" / f"{name}-{version}.crate"
            ) as archive:
                archive.extractall(temporary, filter="data")
            packages[name] = Path(temporary) / f"{name}-{version}"
        yield packages


def verify_archives(members, version, sdk):
    # Cargo's publication check normally resolves the SDK from crates.io. CI also
    # needs to exercise unpublished SDK APIs, using only the distributable files.
    with tempfile.TemporaryDirectory(prefix="omochi-native-crates-") as temporary:
        packages = dict(sdk)
        native = []
        for member in members:
            if member.get("publish") is False:
                continue
            name = member["name"]
            with tarfile.open(
                WORKSPACE / "target/package" / f"{name}-{version}.crate"
            ) as archive:
                archive.extractall(temporary, filter="data")
            directory = Path(temporary) / f"{name}-{version}"
            packages[name] = directory
            native.append(directory)
        root = Path(temporary)
        (root / "Cargo.toml").write_text(
            '[workspace]\nresolver = "3"\nmembers = '
            + json.dumps([directory.name for directory in native])
            + "\n"
        )
        shutil.copy2(WORKSPACE / "Cargo.lock", root / "Cargo.lock")
        subprocess.run(
            [
                "cargo",
                "build",
                "--workspace",
                "--manifest-path",
                str(root / "Cargo.toml"),
                "--target-dir",
                str(WORKSPACE / "target/package-check"),
                *patch_arguments(packages),
            ],
            cwd=ROOT,
            check=True,
        )


def source_state():
    commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
    ).strip()
    dirty = bool(
        subprocess.check_output(
            ["git", "status", "--porcelain"], cwd=ROOT, text=True
        ).strip()
    )
    return commit, dirty


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow-dirty", action="store_true")
    parser.add_argument(
        "--no-verify",
        action="store_true",
        help="package inspection only; not a release gate",
    )
    parser.add_argument(
        "--local-sdk",
        action="store_true",
        help="CI only: verify archives against the packaged local SDK; not a release gate",
    )
    args = parser.parse_args()
    commit, dirty = source_state()
    if dirty and not args.allow_dirty:
        parser.error(
            "Commit source changes before packaging; --allow-dirty is only for local verification"
        )
    assets = WORKSPACE / "hub/assets"
    if assets.exists():
        parser.error(
            "hub/assets already exists; remove the stale generated directory before packaging"
        )
    source = ROOT / "packages/hub-ui/dist"
    if not (source / "index.html").is_file():
        parser.error("build dashboard assets first with bun run build:native")
    output = ROOT / "artifacts/crates"
    output.mkdir(parents=True, exist_ok=True)
    config = tomllib.loads((WORKSPACE / "Cargo.toml").read_text())
    version = config["workspace"]["package"]["version"]
    members = [
        tomllib.loads((WORKSPACE / member / "Cargo.toml").read_text())["package"]
        for member in config["workspace"]["members"]
    ]
    # Cargo treats explicitly included, gitignored assets as dirty. Check the
    # source tree ourselves; allow only this generated staging data in releases.
    command = [
        "cargo",
        "package",
        "--manifest-path",
        str(WORKSPACE / "Cargo.toml"),
        "--workspace",
        "--locked",
        "--allow-dirty",
    ]
    for member in members:
        if member.get("publish") is False:
            command.extend(["--exclude", member["name"]])
    if args.no_verify or args.local_sdk:
        command.append("--no-verify")
    try:
        shutil.copytree(source, assets)
        with local_sdk(args.local_sdk, args.allow_dirty) as sdk:
            package_native(command, sdk)
            if args.local_sdk and not args.no_verify:
                verify_archives(members, version, sdk)
        final_commit, final_dirty = source_state()
        if final_commit != commit or (final_dirty and not args.allow_dirty):
            raise RuntimeError("Source tree changed while packaging")
        packages = []
        for member in members:
            if member.get("publish") is False:
                continue
            name = member["name"]
            filename = f"{name}-{version}.crate"
            archive = WORKSPACE / "target/package" / filename
            shutil.copy2(archive, output / filename)
            packages.append(
                {
                    "name": name,
                    "version": version,
                    "filename": filename,
                    "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
                }
            )
        (output / "manifest.json").write_text(
            json.dumps(
                {
                    "version": version,
                    "verified": not args.no_verify,
                    "localSdk": args.local_sdk,
                    "commit": commit,
                    "dirty": dirty or final_dirty,
                    "packages": packages,
                },
                indent=2,
            )
            + "\n"
        )
    finally:
        shutil.rmtree(assets)


if __name__ == "__main__":
    main()
