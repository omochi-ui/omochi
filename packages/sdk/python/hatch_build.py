"""Build an ABI-independent native library, or bundle Rust sources in an sdist."""
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
        checkout = root.parents[2]
        repository_sdk = checkout / 'packages/sdk/rust'
        source_build = not (repository_sdk / 'local-ffi/Cargo.toml').is_file()
        native = root / 'native' if source_build else repository_sdk
        if self.target_name == 'sdist':
            if not source_build:
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
        command = ['cargo', 'build', '--release', '--manifest-path', str(native / 'Cargo.toml'), '-p', 'omochi-local-ffi']
        if target: command += ['--target', target]
        subprocess.run(command, check=True)
        output = Path(os.environ.get('CARGO_TARGET_DIR', str(native / 'target')))
        if not output.is_absolute(): output = Path.cwd() / output
        if target: output /= target
        windows, apple = sys.platform == 'win32', sys.platform == 'darwin'
        name = 'omochi_local_ffi.dll' if windows else 'libomochi_local_ffi.dylib' if apple else 'libomochi_local_ffi.so'
        build_data['force_include'][str(output / 'release' / name)] = f'omochi_ui/_native/{name}'
        build_data['pure_python'] = False
        tag = sysconfig.get_platform().replace('-', '_').replace('.', '_')
        if apple: tag = 'macosx_11_0_' + ('arm64' if platform.machine() == 'arm64' else 'x86_64')
        build_data['tag'] = 'py3-none-' + tag
