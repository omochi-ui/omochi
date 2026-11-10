"""Build an ABI-independent native library, or bundle Rust sources in an sdist."""
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import sysconfig
from hatchling.builders.hooks.plugin.interface import BuildHookInterface

class CustomBuildHook(BuildHookInterface):
    def initialize(self, version, build_data):
        root = Path(self.root)
        # A source distribution may be unpacked directly under /tmp or even /.
        # Only the checkout's known packages/sdk/python layout has Rust siblings.
        repository_sdk = root.with_name('rust')
        source_build = not (
            root.name == 'python' and root.parent.name == 'sdk'
            and root.parent.parent.name == 'packages'
            and (repository_sdk / 'local-ffi/Cargo.toml').is_file()
        )
        native = root / 'native' if source_build else repository_sdk
        if self.target_name == 'sdist':
            for name in ('local', 'local-ffi'):
                for file in (native / name).rglob('*'):
                    if file.is_file() and 'target' not in file.parts:
                        build_data['force_include'][str(file)] = f'native/{name}/{file.relative_to(native / name)}'
            return
        if self.target_name != 'wheel':
            return
        if sys.platform == 'win32': os.environ.setdefault('RUSTFLAGS', '-C target-feature=+crt-static')
        if sys.platform == 'darwin': os.environ.setdefault('MACOSX_DEPLOYMENT_TARGET', '11.0')
        target = os.environ.get('CARGO_BUILD_TARGET')
        command = ['cargo', 'build', '--release', '--manifest-path', str(native / 'Cargo.toml'), '-p', 'omochi-local-ffi', '--message-format=json-render-diagnostics']
        if target: command += ['--target', target]
        result = subprocess.run(command, check=True, stdout=subprocess.PIPE, text=True)
        windows, apple = sys.platform == 'win32', sys.platform == 'darwin'
        name = 'omochi_local_ffi.dll' if windows else 'libomochi_local_ffi.dylib' if apple else 'libomochi_local_ffi.so'
        artifacts = []
        for line in result.stdout.splitlines():
            if not line.startswith('{'):
                continue
            message = json.loads(line)
            if message.get('reason') != 'compiler-artifact':
                continue
            artifact_target = message['target']
            if artifact_target['name'] != 'omochi_local_ffi' or 'cdylib' not in artifact_target['crate_types']:
                continue
            artifacts.extend(filename for filename in message['filenames'] if Path(filename).name == name)
        if len(artifacts) != 1:
            raise ValueError(f'Cargo must report exactly one {name} artifact; build one target at a time')
        build_data['force_include'][artifacts[0]] = f'omochi_ui/_native/{name}'
        build_data['pure_python'] = False
        tag = sysconfig.get_platform().replace('-', '_').replace('.', '_')
        if apple: tag = 'macosx_11_0_' + ('arm64' if platform.machine() == 'arm64' else 'x86_64')
        build_data['tag'] = 'py3-none-' + tag
