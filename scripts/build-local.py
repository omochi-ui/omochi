#!/usr/bin/env python3
"""Build SDK adapters and stage host binaries for development or distribution."""
import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
SDK = ROOT / 'packages/sdk/rust'

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--release', action='store_true')
    parser.add_argument('--target')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    command = ['cargo', 'build', '--manifest-path', str(SDK / 'Cargo.toml'), '-p', 'omochi-local-node', '-p', 'omochi-local-ffi']
    if args.release: command.append('--release')
    if args.target: command += ['--target', args.target]
    env = dict(os.environ)
    if args.release and platform.system() == 'Windows': env['RUSTFLAGS'] = env.get('RUSTFLAGS', '') + ' -C target-feature=+crt-static'
    subprocess.run(command, cwd=ROOT, check=True, env=env)
    target = args.target or next(line.split(': ', 1)[1] for line in subprocess.check_output(['rustc', '-vV'], text=True).splitlines() if line.startswith('host: '))
    windows, apple = 'windows' in target, 'apple' in target
    prefix, suffix = ('', '.dll') if windows else ('lib', '.dylib' if apple else '.so')
    build = Path(os.environ.get('CARGO_TARGET_DIR', str(SDK / 'target')))
    if not build.is_absolute(): build = ROOT / build
    if args.target: build /= args.target
    build /= 'release' if args.release else 'debug'
    output = args.output or ROOT / ('artifacts/local' if args.release else 'artifacts/local-dev') / target
    output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(build / f'{prefix}omochi_local_node{suffix}', output / 'omochi_local.node')
    ffi = f'{prefix}omochi_local_ffi{suffix}'
    shutil.copy2(build / ffi, output / ffi)
    version = json.loads((ROOT / 'packages/sdk/typescript/package.json').read_text())['version']
    os_name = 'win32' if windows else 'darwin' if apple else 'linux'
    cpu = 'arm64' if target.startswith('aarch64') else 'x64'
    libc = '-musl' if 'musl' in target else '-gnu' if os_name == 'linux' else ''
    manifest = {'name':f'@omochi-ui/local-{os_name}-{cpu}{libc}', 'version':version, 'main':'omochi_local.node', 'files':['omochi_local.node'], 'os':[os_name], 'cpu':[cpu], 'license':'Apache-2.0'}
    if os_name == 'linux': manifest['libc'] = ['musl' if 'musl' in target else 'glibc']
    (output / 'package.json').write_text(json.dumps(manifest, indent=2) + '\n')
    if not args.target:
        for directory, source in [(ROOT / 'packages/sdk/typescript/native', 'omochi_local.node'), (ROOT / 'packages/sdk/python/src/omochi_ui/_native', ffi)]:
            directory.mkdir(parents=True, exist_ok=True)
            shutil.copy2(output / source, directory / source)
    print(output)

if __name__ == '__main__': main()
