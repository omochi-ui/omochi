"""Build an ABI-independent native library, or bundle Rust sources in an sdist."""

import json
import os
import platform
import runpy
import struct
import subprocess
import sys
import sysconfig
from pathlib import Path

from hatchling.builders.hooks.plugin.interface import BuildHookInterface

native_build_environment = runpy.run_path(
    str(Path(__file__).with_name("cargo_build.py"))
)["native_build_environment"]


def validate_native_target(artifact, manifest):
    """Reject cross targets selected by Cargo flags, environment or configuration."""
    metadata = json.loads(
        subprocess.check_output(
            [
                "cargo",
                "metadata",
                "--manifest-path",
                str(manifest),
                "--format-version",
                "1",
                "--no-deps",
            ],
            text=True,
        )
    )
    host = next(
        line.split(": ", 1)[1]
        for line in subprocess.check_output(["rustc", "-vV"], text=True).splitlines()
        if line.startswith("host: ")
    )
    build = Path(artifact).parent
    layout = build.relative_to(Path(metadata["target_directory"])).parts
    if layout == ("release",):
        target = host
    elif len(layout) == 2 and layout[1] == "release":
        target = layout[0]
    else:
        raise ValueError(f"Unexpected Cargo artifact directory: {build}")
    if target != host:
        raise ValueError(
            f"Cargo target {target} does not match rustc host {host}; cross-platform wheels are unsupported"
        )


def macos_deployment_version(library, header):
    """Read the minimum macOS release from a thin, little-endian Mach-O file."""
    if len(header) < 32:
        raise ValueError("Truncated Mach-O header")
    commands, size = struct.unpack_from("<2I", header, 16)
    end = 32 + size
    if library.seek(0, os.SEEK_END) < end:
        raise ValueError("Truncated Mach-O load commands")
    offset, minimum = 32, None
    for _ in range(commands):
        if offset + 8 > end:
            raise ValueError("Invalid Mach-O load command")
        library.seek(offset)
        command, size = struct.unpack("<2I", library.read(8))
        if size < 8 or offset + size > end:
            raise ValueError("Invalid Mach-O load command size")
        version = None
        if command == 0x32:  # LC_BUILD_VERSION
            if size < 24:
                raise ValueError("Invalid Mach-O build version")
            target_platform, version = struct.unpack("<2I", library.read(8))
            if target_platform != 1:  # PLATFORM_MACOS
                raise ValueError("Native library does not target macOS")
        elif command == 0x24:  # LC_VERSION_MIN_MACOSX
            if size < 16:
                raise ValueError("Invalid Mach-O minimum version")
            version = struct.unpack("<I", library.read(4))[0]
        if version is not None:
            minimum = max(minimum or 0, version)
        offset += size
    if not minimum:
        raise ValueError("Native library has no macOS deployment target")
    major, minor, patch = minimum >> 16, (minimum >> 8) & 0xFF, minimum & 0xFF
    # Installers only recognize major_0 for macOS 11+. Rounding down would
    # advertise support for systems older than the library's deployment target.
    if patch or (major >= 11 and minor):
        raise ValueError(
            f"Native macOS deployment target {major}.{minor}.{patch} cannot be represented "
            "by a wheel platform tag; rebuild with MACOSX_DEPLOYMENT_TARGET set to "
            "a release version (10.x or 11.0 and later major.0 versions)"
        )
    return f"{major}_{minor}"


def validate_native_platform(artifact, tag):
    """Validate the binary's OS/CPU and return its deployment-aware wheel tag.

    The expected macOS platform is macosx_<cpu>; its version comes from the file.
    """
    actual = None
    macos_version = None
    with Path(artifact).open("rb") as library:
        header = library.read(64)
        if header[:6] == b"\x7fELF\x02\x01":  # ELF64, little endian
            architecture = {62: "x86_64", 183: "aarch64"}.get(
                int.from_bytes(header[18:20], "little")
            )
            if architecture:
                actual = f"linux_{architecture}"
        elif header[:4] == b"\xcf\xfa\xed\xfe":  # 64-bit Mach-O, little endian
            architecture = {0x1000007: "x86_64", 0x100000C: "arm64"}.get(
                int.from_bytes(header[4:8], "little")
            )
            if architecture:
                actual = f"macosx_{architecture}"
                macos_version = macos_deployment_version(library, header)
        elif header[:2] == b"MZ":
            library.seek(int.from_bytes(header[60:64], "little"))
            pe = library.read(6)
            if pe[:4] == b"PE\0\0":
                architecture = {0x8664: "amd64", 0xAA64: "arm64"}.get(
                    int.from_bytes(pe[4:6], "little")
                )
                if architecture:
                    actual = f"win_{architecture}"
    if actual != tag:
        raise ValueError(
            f"Native library platform {actual or 'unknown'} does not match wheel platform {tag}; "
            "build with Cargo configured for this Python platform (cross-platform wheels are unsupported)"
        )
    return f"macosx_{macos_version}_{architecture}" if macos_version else tag


class CustomBuildHook(BuildHookInterface):
    def initialize(self, version, build_data):
        root = Path(self.root)
        # A source distribution may be unpacked directly under /tmp or even /.
        # Only the checkout's known packages/sdk/python layout has Rust siblings.
        repository_sdk = root.with_name("rust")
        source_build = not (
            root.name == "python"
            and root.parent.name == "sdk"
            and root.parent.parent.name == "packages"
            and (repository_sdk / "local-ffi/Cargo.toml").is_file()
        )
        native = root / "native" if source_build else repository_sdk
        if self.target_name == "sdist":
            for name in ("local", "local-ffi"):
                for file in (native / name).rglob("*"):
                    if file.is_file() and "target" not in file.parts:
                        build_data["force_include"][str(file)] = (
                            f"native/{name}/{file.relative_to(native / name)}"
                        )
            return
        if self.target_name != "wheel":
            return
        if struct.calcsize("P") != 8:
            raise ValueError("Native wheels require a 64-bit Python interpreter")
        if sys.platform == "darwin":
            os.environ.setdefault("MACOSX_DEPLOYMENT_TARGET", "11.0")
        target = os.environ.get("CARGO_BUILD_TARGET")
        env = native_build_environment(Path.cwd(), target=target)
        command = [
            "cargo",
            "build",
            "--release",
            "--manifest-path",
            str(native / "Cargo.toml"),
            "-p",
            "omochi-local-ffi",
            "--message-format=json-render-diagnostics",
        ]
        if target:
            command += ["--target", target]
        result = subprocess.run(
            command, check=True, env=env, stdout=subprocess.PIPE, text=True
        )
        windows, apple = sys.platform == "win32", sys.platform == "darwin"
        name = (
            "omochi_local_ffi.dll"
            if windows
            else "libomochi_local_ffi.dylib"
            if apple
            else "libomochi_local_ffi.so"
        )
        artifacts = []
        for line in result.stdout.splitlines():
            if not line.startswith("{"):
                continue
            message = json.loads(line)
            if message.get("reason") != "compiler-artifact":
                continue
            artifact_target = message["target"]
            if (
                artifact_target["name"] != "omochi_local_ffi"
                or "cdylib" not in artifact_target["crate_types"]
            ):
                continue
            artifacts.extend(
                filename
                for filename in message["filenames"]
                if Path(filename).name == name
            )
        if len(artifacts) != 1:
            raise ValueError(
                f"Cargo must report exactly one {name} artifact; build one target at a time"
            )
        validate_native_target(artifacts[0], native / "Cargo.toml")
        tag = sysconfig.get_platform().replace("-", "_").replace(".", "_")
        if apple:
            tag = "macosx_" + ("arm64" if platform.machine() == "arm64" else "x86_64")
        tag = validate_native_platform(artifacts[0], tag)
        build_data["force_include"][artifacts[0]] = f"omochi_ui/_native/{name}"
        build_data["pure_python"] = False
        build_data["tag"] = "py3-none-" + tag
