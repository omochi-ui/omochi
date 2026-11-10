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


def native_artifacts(command: list[str], env: dict[str, str]) -> dict[str, Path]:
    """Use Cargo's output paths, including targets selected through configuration."""
    result = subprocess.run(command + ['--message-format=json-render-diagnostics'],
                            cwd=ROOT, check=True, env=env, stdout=subprocess.PIPE, text=True)
    artifacts = {}
    for line in result.stdout.splitlines():
        if not line.startswith('{'):
            continue
        message = json.loads(line)
        if message.get('reason') != 'compiler-artifact':
            continue
        name = message['target']['name']
        if name not in ('omochi_local_node', 'omochi_local_ffi') or 'cdylib' not in message['target']['crate_types']:
            continue
        for filename in message['filenames']:
            artifact = Path(filename)
            if artifact.suffix in ('.dll', '.dylib', '.so'):
                if name in artifacts:
                    raise ValueError('Build one target at a time with --target')
                artifacts[name] = artifact
    if len(artifacts) != 2:
        raise ValueError('Cargo did not report both local adapter libraries')
    return artifacts


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
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--manifest-path', str(SDK / 'Cargo.toml'), '--format-version', '1', '--no-deps'],
        cwd=ROOT, env=env, text=True))
    artifacts = native_artifacts(command, env)
    host = next(line.split(': ', 1)[1] for line in subprocess.check_output(['rustc', '-vV'], text=True).splitlines() if line.startswith('host: '))
    build = artifacts['omochi_local_node'].parent
    if artifacts['omochi_local_ffi'].parent != build:
        raise ValueError('Cargo reported local adapters for different targets')
    layout = build.relative_to(Path(metadata['target_directory'])).parts
    profile = 'release' if args.release else 'debug'
    if layout == (profile,):
        target = host
    elif len(layout) == 2 and layout[1] == profile:
        target = layout[0]
    else:
        raise ValueError(f'Unexpected Cargo artifact directory: {build}')
    windows, apple = 'windows' in target, 'apple' in target
    output = args.output or ROOT / ('artifacts/local' if args.release else 'artifacts/local-dev') / target
    output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(artifacts['omochi_local_node'], output / 'omochi_local.node')
    ffi = artifacts['omochi_local_ffi'].name
    shutil.copy2(artifacts['omochi_local_ffi'], output / ffi)
    version = json.loads((ROOT / 'packages/sdk/typescript/package.json').read_text())['version']
    os_name = 'win32' if windows else 'darwin' if apple else 'linux'
    cpu = 'arm64' if target.startswith('aarch64') else 'x64'
    libc = '-musl' if 'musl' in target else '-gnu' if os_name == 'linux' else ''
    manifest = {'name':f'@omochi-ui/local-{os_name}-{cpu}{libc}', 'version':version, 'main':'omochi_local.node', 'files':['omochi_local.node'], 'os':[os_name], 'cpu':[cpu], 'license':'Apache-2.0'}
    if os_name == 'linux': manifest['libc'] = ['musl' if 'musl' in target else 'glibc']
    (output / 'package.json').write_text(json.dumps(manifest, indent=2) + '\n')
    if not args.target and target == host:
        for directory, source in [(ROOT / 'packages/sdk/typescript/native', 'omochi_local.node'), (ROOT / 'packages/sdk/python/src/omochi_ui/_native', ffi)]:
            directory.mkdir(parents=True, exist_ok=True)
            shutil.copy2(output / source, directory / source)
    print(output)

if __name__ == '__main__': main()
