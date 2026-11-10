"""Regression checks for local adapter staging and Cargo target selection."""

import contextlib
import importlib.util
import io
import json
import os
import runpy
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "build_local", Path(__file__).with_name("build-local.py")
)
build_local = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build_local)


class PythonWheelBuildTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="omochi-python-wheel-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        # Only the hook's initialize method is under test; the existing packaging
        # smoke test covers integration with the real Hatchling build backend.
        with patch.dict(
            sys.modules,
            {
                "hatchling.builders.hooks.plugin.interface": SimpleNamespace(
                    BuildHookInterface=object
                )
            },
        ):
            self.hook = runpy.run_path(
                str(build_local.ROOT / "packages/sdk/python/hatch_build.py")
            )["CustomBuildHook"]
        self.artifact = self.root / "target/release/libomochi_local_ffi.dylib"
        self.artifact.parent.mkdir(parents=True)

    def macho(self, version, *, legacy=False, architecture=0x1000007):
        encoded = (version[0] << 16) | (version[1] << 8) | version[2]
        command = (
            struct.pack("<4I", 0x24, 16, encoded, encoded)
            if legacy
            else struct.pack("<6I", 0x32, 24, 1, encoded, encoded, 0)
        )
        # Include an unrelated command so the parser must walk load commands.
        commands = struct.pack("<2I", 0x1B, 24) + bytes(16) + command
        return (
            struct.pack("<8I", 0xFEEDFACF, architecture, 0, 6, 2, len(commands), 0, 0)
            + commands
        )

    def build(self, deployment_target=None, architecture="x86_64"):
        output = json.dumps(
            {
                "reason": "compiler-artifact",
                "target": {"name": "omochi_local_ffi", "crate_types": ["cdylib"]},
                "filenames": [str(self.artifact)],
            }
        )
        environment = (
            {}
            if deployment_target is None
            else {"MACOSX_DEPLOYMENT_TARGET": deployment_target}
        )
        data = {"force_include": {}}
        with (
            patch.dict(os.environ, environment, clear=True),
            patch.object(sys, "platform", "darwin"),
            patch.object(
                self.hook.initialize.__globals__["platform"],
                "machine",
                return_value=architecture,
            ),
            patch.dict(
                self.hook.initialize.__globals__,
                {"native_build_environment": lambda *args, **kwargs: dict(os.environ)},
            ),
            patch.object(
                subprocess,
                "run",
                return_value=subprocess.CompletedProcess([], 0, stdout=output),
            ) as run,
            patch.object(
                subprocess,
                "check_output",
                side_effect=lambda command, **kwargs: (
                    f"host: {'aarch64' if architecture == 'arm64' else 'x86_64'}-apple-darwin\n"
                    if command[0] == "rustc"
                    else json.dumps({"target_directory": str(self.root / "target")})
                ),
            ),
        ):
            try:
                self.hook.initialize(
                    SimpleNamespace(root=str(self.root), target_name="wheel"),
                    "standard",
                    data,
                )
            finally:
                self.assertEqual(
                    os.environ["MACOSX_DEPLOYMENT_TARGET"], deployment_target or "11.0"
                )
                self.assertEqual(
                    run.call_args.kwargs["env"]["MACOSX_DEPLOYMENT_TARGET"],
                    deployment_target or "11.0",
                )
        return data

    def test_macos_wheel_tag_follows_artifact_deployment_target(self):
        for version, target, legacy, architecture, cpu, tag in (
            ((11, 0, 0), None, False, "x86_64", 0x1000007, "11_0_x86_64"),
            ((11, 0, 0), None, False, "arm64", 0x100000C, "11_0_arm64"),
            ((14, 0, 0), "14.0", False, "x86_64", 0x1000007, "14_0_x86_64"),
            ((14, 0, 0), "14.0", False, "arm64", 0x100000C, "14_0_arm64"),
            ((14, 0, 0), None, False, "x86_64", 0x1000007, "14_0_x86_64"),
            ((10, 13, 0), "10.13", True, "x86_64", 0x1000007, "10_13_x86_64"),
        ):
            with self.subTest(
                version=version, target=target, architecture=architecture
            ):
                self.artifact.write_bytes(
                    self.macho(version, legacy=legacy, architecture=cpu)
                )
                data = self.build(target, architecture)
                self.assertEqual(data["tag"], f"py3-none-macosx_{tag}")
                self.assertFalse(data["pure_python"])
                self.assertIn(str(self.artifact), data["force_include"])

    def test_macos_rejects_unrepresentable_or_missing_deployment_versions(self):
        for version in ((14, 4, 0), (14, 0, 1), (10, 13, 1)):
            with self.subTest(version=version):
                self.artifact.write_bytes(self.macho(version))
                with self.assertRaisesRegex(ValueError, "cannot be represented"):
                    self.build(".".join(map(str, version)))
        self.artifact.write_bytes(
            struct.pack("<8I", 0xFEEDFACF, 0x1000007, 0, 6, 0, 0, 0, 0)
        )
        with self.assertRaisesRegex(ValueError, "deployment target"):
            self.build()

    def test_macos_still_rejects_foreign_architectures(self):
        self.artifact.write_bytes(self.macho((14, 0, 0), architecture=0x100000C))
        with self.assertRaisesRegex(ValueError, "does not match wheel platform"):
            self.build("14.0")

    def test_macos_rejects_truncated_or_foreign_deployment_metadata(self):
        self.artifact.write_bytes(self.macho((14, 0, 0))[:-1])
        with self.assertRaisesRegex(ValueError, "Truncated Mach-O load commands"):
            self.build("14.0")
        foreign = bytearray(self.macho((14, 0, 0)))
        foreign[64:68] = (2).to_bytes(4, "little")  # PLATFORM_IOS
        self.artifact.write_bytes(foreign)
        with self.assertRaisesRegex(ValueError, "does not target macOS"):
            self.build("14.0")


class LocalBuildTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="omochi-local-build-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.sdk = self.root / "packages/sdk/rust"
        self.sdk.mkdir(parents=True)
        package = self.root / "packages/sdk/typescript/package.json"
        package.parent.mkdir(parents=True)
        package.write_text('{"version":"1.2.3"}')
        self.env = dict(os.environ)
        for key in (
            "CARGO_BUILD_TARGET",
            "CARGO_TARGET_DIR",
            "CARGO_BUILD_TARGET_DIR",
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_BUILD_RUSTFLAGS",
        ):
            self.env.pop(key, None)
        self.env["CARGO_HOME"] = str(self.root / "cargo-home")
        self.host = "x86_64-unknown-linux-gnu"

    def build(self, args=(), env=None):
        with (
            patch.object(build_local, "ROOT", self.root),
            patch.object(build_local, "SDK", self.sdk),
            patch.object(sys, "argv", ["build-local.py", *args]),
            patch.dict(os.environ, self.env | (env or {}), clear=True),
            contextlib.redirect_stdout(io.StringIO()) as output,
        ):
            build_local.main()
        return Path(output.getvalue().strip())

    def mock_cargo(self, target=None, *, release=False):
        target_dir = self.root / "configured target"
        build = target_dir / target if target else target_dir
        build /= "release" if release else "debug"
        build.mkdir(parents=True, exist_ok=True)
        prefix, suffix = (
            ("", ".dll") if target and "windows" in target else ("lib", ".so")
        )
        messages = []
        for name in ("omochi_local_node", "omochi_local_ffi"):
            artifact = build / f"{prefix}{name}{suffix}"
            artifact.write_text(f"{target or self.host} {name}")
            messages.append(
                json.dumps(
                    {
                        "reason": "compiler-artifact",
                        "target": {"name": name, "crate_types": ["cdylib"]},
                        "filenames": [str(artifact)],
                    }
                )
            )
        self.addCleanup(patch.stopall)
        run = patch.object(
            build_local.subprocess,
            "run",
            return_value=subprocess.CompletedProcess([], 0, stdout="\n".join(messages)),
        ).start()
        patch.object(
            build_local.subprocess,
            "check_output",
            side_effect=lambda command, **kwargs: (
                'unix\ntarget_os="linux"\ntarget_env="musl"\n'
                if command[:3] == ["rustc", "--print", "cfg"]
                else f"host: {self.host}\n"
                if command[0] == "rustc"
                else json.dumps({"target_directory": str(target_dir)})
            ),
        ).start()
        return run

    def test_default_host_build_stages_both_development_adapters(self):
        self.mock_cargo()
        output = self.build()
        self.assertEqual(output, self.root / "artifacts/local-dev" / self.host)
        self.assertEqual(
            (output / "omochi_local.node").read_bytes(),
            (
                self.root / "packages/sdk/typescript/native/omochi_local.node"
            ).read_bytes(),
        )
        self.assertTrue(
            (
                self.root
                / "packages/sdk/python/src/omochi_ui/_native/libomochi_local_ffi.so"
            ).is_file()
        )

    def test_environment_target_uses_target_artifacts_and_keeps_host_adapters(self):
        target = "aarch64-pc-windows-msvc"
        run = self.mock_cargo(target)
        native = self.root / "packages/sdk/typescript/native/omochi_local.node"
        native.parent.mkdir(parents=True)
        native.write_text("existing host adapter")
        output = self.build(
            env={"CARGO_BUILD_TARGET": target, "CARGO_TARGET_DIR": "configured target"}
        )
        self.assertEqual(output, self.root / "artifacts/local-dev" / target)
        self.assertEqual(
            (output / "omochi_local_ffi.dll").read_text(), f"{target} omochi_local_ffi"
        )
        manifest = json.loads((output / "package.json").read_text())
        self.assertEqual(manifest["name"], "@omochi-ui/local-win32-arm64")
        self.assertEqual(manifest["os"], ["win32"])
        self.assertEqual(manifest["cpu"], ["arm64"])
        self.assertEqual(native.read_text(), "existing host adapter")
        self.assertFalse(
            (self.root / "packages/sdk/python/src/omochi_ui/_native").exists()
        )
        self.assertEqual(run.call_args.kwargs["env"]["CARGO_BUILD_TARGET"], target)

    def test_explicit_target_takes_precedence_and_does_not_stage_development_adapters(
        self,
    ):
        target = "aarch64-unknown-linux-musl"
        run = self.mock_cargo(target, release=True)
        output = self.build(
            ["--target", target, "--release"], env={"CARGO_BUILD_TARGET": self.host}
        )
        self.assertEqual(output, self.root / "artifacts/local" / target)
        self.assertEqual(
            json.loads((output / "package.json").read_text())["libc"], ["musl"]
        )
        command = run.call_args.args[0]
        self.assertEqual(command[command.index("--target") + 1], target)
        self.assertIn("--release", command)
        self.assertFalse((self.root / "packages/sdk/typescript/native").exists())

    def test_configured_cross_target_does_not_stage_development_adapters(self):
        target = "aarch64-unknown-linux-gnu"
        self.mock_cargo(target)
        output = self.build()
        self.assertEqual(output.name, target)
        self.assertFalse((self.root / "packages/sdk/typescript/native").exists())
        self.assertFalse(
            (self.root / "packages/sdk/python/src/omochi_ui/_native").exists()
        )

    def test_musl_crt_flags_follow_cli_environment_config_and_host_targets(self):
        musl = "x86_64-unknown-linux-musl"
        config_dir = self.root / ".cargo"
        config_dir.mkdir()
        for selection in ("explicit", "environment", "config", "host"):
            for flags in (
                {},
                {"RUSTFLAGS": "--cfg existing -C target-feature=+crt-static"},
                {
                    "RUSTFLAGS": "--cfg ignored",
                    "CARGO_ENCODED_RUSTFLAGS": '--cfg\x1fexisting="with space"',
                },
            ):
                with self.subTest(selection=selection, flags=flags):
                    self.host = (
                        musl if selection == "host" else "x86_64-pc-windows-msvc"
                    )
                    run = self.mock_cargo(musl, release=True)
                    config = config_dir / "config.toml"
                    config.write_text(
                        f'[build]\ntarget = "{musl if selection == "config" else self.host}"\n'
                    )
                    if selection == "host":
                        config.unlink()
                    env = flags | {"CARGO_HOME": str(self.root / "cargo-home")}
                    if selection == "environment":
                        env["CARGO_BUILD_TARGET"] = musl
                    elif selection == "explicit":
                        env["CARGO_BUILD_TARGET"] = self.host
                    args = ["--release"]
                    if selection == "explicit":
                        args += ["--target", musl]
                    self.build(args, env)
                    actual = run.call_args.kwargs["env"]
                    if "CARGO_ENCODED_RUSTFLAGS" in flags:
                        self.assertEqual(actual["RUSTFLAGS"], flags["RUSTFLAGS"])
                        self.assertEqual(
                            actual["CARGO_ENCODED_RUSTFLAGS"],
                            flags["CARGO_ENCODED_RUSTFLAGS"]
                            + "\x1f-C\x1ftarget-feature=-crt-static",
                        )
                    elif "RUSTFLAGS" in flags:
                        self.assertEqual(
                            actual["RUSTFLAGS"],
                            flags.get("RUSTFLAGS", "")
                            + " -C target-feature=-crt-static",
                        )
                    else:
                        self.assertEqual(
                            actual["CARGO_ENCODED_RUSTFLAGS"],
                            "-C\x1ftarget-feature=-crt-static",
                        )
                    patch.stopall()

    def test_windows_crt_flags_follow_target_and_preserve_encoded_flags(self):
        windows = "aarch64-pc-windows-msvc"
        run = self.mock_cargo(windows, release=True)
        self.build(
            ["--target", windows, "--release"],
            {"CARGO_ENCODED_RUSTFLAGS": "--cfg\x1fexisting"},
        )
        self.assertEqual(
            run.call_args.kwargs["env"]["CARGO_ENCODED_RUSTFLAGS"],
            "--cfg\x1fexisting\x1f-C\x1ftarget-feature=+crt-static",
        )

    def test_configured_flags_keep_cargo_merging_and_target_precedence(self):
        target = "x86_64-unknown-linux-musl"
        config_dir = self.root / ".cargo"
        config_dir.mkdir()
        cargo_home = self.root / "cargo-home"
        cargo_home.mkdir()
        (cargo_home / "config.toml").write_text(
            '[build]\nrustflags = ["--cfg", "global_build"]\n'
            f'[target.{target}]\nrustflags = ["--cfg", "global_target"]\n'
        )
        config = config_dir / "config.toml"
        config.write_text(
            f'[build]\ntarget = "{target}"\nrustflags = ["--cfg", "local_build"]\n'
            f'[target.{target}]\nrustflags = ["--cfg", "local_target"]\n'
            '[target.\'cfg(all(unix, any(target_env = "musl", not(target_os = "linux"))))\']\n'
            'rustflags = ["--cfg", \'spaced="with space"\']\n'
            '[target.\'cfg(windows)\']\nrustflags = ["--cfg", "ignored"]\n'
        )
        run = self.mock_cargo(target)
        self.build(
            env={
                f"CARGO_TARGET_{target.upper().replace('-', '_')}_RUSTFLAGS": "--cfg env_target"
            }
        )
        self.assertEqual(
            run.call_args.kwargs["env"]["CARGO_ENCODED_RUSTFLAGS"].split("\x1f"),
            [
                "--cfg",
                "global_target",
                "--cfg",
                "local_target",
                "--cfg",
                "env_target",
                "--cfg",
                'spaced="with space"',
                "-C",
                "target-feature=-crt-static",
            ],
        )
        config.write_text(
            f'[build]\ntarget = "{target}"\nrustflags = ["--cfg", "local_build"]\n'
        )
        (cargo_home / "config.toml").write_text(
            '[build]\nrustflags = ["--cfg", "global_build"]\n'
        )
        self.build(env={"CARGO_BUILD_RUSTFLAGS": "--cfg env_build"})
        self.assertEqual(
            run.call_args.kwargs["env"]["CARGO_ENCODED_RUSTFLAGS"].split("\x1f"),
            [
                "--cfg",
                "global_build",
                "--cfg",
                "local_build",
                "--cfg",
                "env_build",
                "-C",
                "target-feature=-crt-static",
            ],
        )

    @unittest.skipUnless(
        shutil.which("cargo") and shutil.which("rustc"), "Cargo and rustc required"
    )
    def test_real_cargo_environment_config_and_explicit_target_layouts(self):
        host = next(
            line.split(": ", 1)[1]
            for line in subprocess.check_output(
                ["rustc", "-vV"], text=True
            ).splitlines()
            if line.startswith("host: ")
        )
        (self.sdk / "Cargo.toml").write_text(
            '[workspace]\nmembers = ["local-node", "local-ffi"]\nresolver = "2"\n'
        )
        for crate in ("local-node", "local-ffi"):
            directory = self.sdk / crate
            (directory / "src").mkdir(parents=True)
            (directory / "src/lib.rs").write_text("pub fn fixture() {}\n")
            (directory / "Cargo.toml").write_text(
                f'[package]\nname = "omochi-{crate}"\nversion = "0.0.0"\nedition = "2021"\n'
                '[lib]\ncrate-type = ["cdylib"]\n'
            )
        (self.root / ".cargo").mkdir()
        config = self.root / ".cargo/config.toml"
        for selection in ("environment", "config", "explicit"):
            with self.subTest(selection=selection):
                config.write_text(
                    f'[build]\ntarget-dir = "{selection} output"\n'
                    + (f'target = "{host}"\n' if selection == "config" else "")
                )
                env = {"CARGO_BUILD_TARGET": host} if selection == "environment" else {}
                args = ["--target", host] if selection == "explicit" else []
                if selection == "explicit":
                    env["CARGO_BUILD_TARGET"] = "invalid-overridden-target"
                output = self.build(args, env)
                self.assertTrue(
                    (self.root / f"{selection} output" / host / "debug").is_dir()
                )
                self.assertEqual(output.name, host)
                self.assertTrue((output / "omochi_local.node").is_file())


if __name__ == "__main__":
    unittest.main()
