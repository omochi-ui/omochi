#!/usr/bin/env python3
"""Build npm wrappers and platform packages from checksum-verified native archives."""
import argparse
import hashlib
import json
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGETS = json.loads((ROOT / 'scripts/native/targets.json').read_text())


def platform_name(target):
    return f"@omochi-ui/native-{target['os']}-{target['cpu']}"


def validate_archive(archive, version):
    expected = archive.with_suffix('.gz.sha256').read_text().split()[0]
    if hashlib.sha256(archive.read_bytes()).hexdigest() != expected:
        raise ValueError(f'Archive checksum mismatch: {archive}')
    with tarfile.open(archive) as tar:
        members = tar.getmembers()
        if len({m.name for m in members}) != len(members) or any(not m.isfile() for m in members):
            raise ValueError('Archive must contain unique regular files')
        files = {m.name: tar.extractfile(m).read() for m in members}
    metadata = json.loads(files['manifest.json'])
    if metadata['version'] != version:
        raise ValueError('Native archive version mismatch')
    target = next((t for t in TARGETS if t['target'] == metadata['target']), None)
    if target is None:
        raise ValueError('Unsupported archive target')
    suffix = '.exe' if target['os'] == 'win32' else ''
    for name in ['bin/omochi-ui' + suffix, 'bin/omochi-utils' + suffix, 'LICENSE', 'README.md']:
        if hashlib.sha256(files[name]).hexdigest() != metadata['sha256'][name]:
            raise ValueError(f'File checksum mismatch: {name}')
    return target, metadata, files


def pack(directory, manifest, output):
    manifest.update({'license': 'Apache-2.0', 'repository': {'type': 'git', 'url': 'https://github.com/omochi-ui/omochi.git'}, 'publishConfig': {'access': 'public'}})
    (directory / 'package.json').write_text(json.dumps(manifest, indent=2) + '\n')
    shutil.copy2(ROOT / 'LICENSE', directory / 'LICENSE')
    if not (directory / 'README.md').exists():
        shutil.copy2(ROOT / 'crates/DISTRIBUTION.md', directory / 'README.md')
    result = subprocess.check_output(['npm', 'pack', '--ignore-scripts', '--json', '--pack-destination', str(output)], cwd=directory, text=True)
    artifact = json.loads(result)[0]
    return {k: artifact[k] for k in ['name', 'version', 'filename', 'integrity']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archives', nargs='+', type=Path)
    parser.add_argument('--partial', action='store_true', help='local smoke only; publication refuses an incomplete platform set')
    parser.add_argument('--output', type=Path, default=ROOT / 'artifacts/native-npm')
    args = parser.parse_args()
    version = tomllib.loads((ROOT / 'crates/Cargo.toml').read_text())['workspace']['package']['version']
    validated = [validate_archive(p, version) for p in args.archives]
    targets = [row[0]['target'] for row in validated]
    if len(set(targets)) != len(targets):
        parser.error('Duplicate targets')
    complete = set(targets) == {t['target'] for t in TARGETS}
    if not complete and not args.partial:
        parser.error('All supported target archives are required; --partial is for local smoke only')
    if len({m['commit'] for _, m, _ in validated}) != 1:
        parser.error('Archives must be built from the same commit')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    packages = []
    for target, metadata, files in sorted(validated, key=lambda row: row[0]['target']):
        with tempfile.TemporaryDirectory(prefix='omochi-native-npm-') as temporary:
            directory = Path(temporary)
            (directory / 'bin').mkdir()
            for name in ['omochi-ui', 'omochi-utils']:
                name += '.exe' if target['os'] == 'win32' else ''
                binary = directory / 'bin' / name
                binary.write_bytes(files['bin/' + name])
                binary.chmod(0o755)
            manifest = {'name': platform_name(target), 'version': version, 'description': 'Omochi native binaries',
                        'os': [target['os']], 'cpu': [target['cpu']], 'files': ['bin']}
            if 'libc' in target:
                manifest['libc'] = [target['libc']]
            packages.append(pack(directory, manifest, output))
    for name in ['cli', 'utils']:
        stub = json.loads((ROOT / 'packages/npm' / name / 'package.json').read_text())
        binary = stub['omochiNative']['binary']
        if stub['omochiNative']['crate'] != 'omochi-' + name or stub['name'] != '@omochi-ui/' + name:
            raise ValueError('Invalid native npm stub')
        with tempfile.TemporaryDirectory(prefix='omochi-native-wrapper-') as temporary:
            directory = Path(temporary)
            (directory / 'bin').mkdir()
            launcher = directory / 'bin' / (binary + '.cjs')
            shutil.copy2(ROOT / 'packages/npm/launcher.cjs', launcher)
            launcher.chmod(0o755)
            manifest = {'name': stub['name'], 'version': version, 'description': stub['description'],
                        'files': ['bin'], 'engines': stub['engines'], 'bin': {binary: './bin/' + launcher.name},
                        'nativeBinary': binary,
                        'nativePlatforms': {f"{t['os']}-{t['cpu']}": platform_name(t) for t in TARGETS},
                        'optionalDependencies': {platform_name(t): version for t in TARGETS}}
            packages.append(pack(directory, manifest, output))
    (output / 'manifest.json').write_text(json.dumps({'version': version, 'complete': complete,
        'commit': validated[0][1]['commit'], 'dirty': any(m['dirty'] for _, m, _ in validated),
        'packages': packages}, indent=2) + '\n')
    print(f'Packed {len(packages)} native npm packages at {output}')


if __name__ == '__main__':
    main()
