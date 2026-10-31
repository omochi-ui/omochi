#!/usr/bin/env python3
"""Package/verify all native crates, including the built dashboard, without publishing."""
import argparse
import hashlib
import json
import shutil
import subprocess
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKSPACE = ROOT / 'packages/server-rust'


def source_state():
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    dirty = bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip())
    return commit, dirty


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--allow-dirty', action='store_true')
    parser.add_argument('--no-verify', action='store_true', help='package inspection only; not a release gate')
    args = parser.parse_args()
    commit, dirty = source_state()
    if dirty and not args.allow_dirty:
        parser.error('Commit source changes before packaging; --allow-dirty is only for local verification')
    assets = WORKSPACE / 'hub/assets'
    if assets.exists():
        parser.error('hub/assets already exists; remove the stale generated directory before packaging')
    source = ROOT / 'packages/hub-ui/dist'
    if not (source / 'index.html').is_file():
        parser.error('build dashboard assets first with bun run build:native')
    output = ROOT / 'artifacts/crates'
    output.mkdir(parents=True, exist_ok=True)
    config = tomllib.loads((WORKSPACE / 'Cargo.toml').read_text())
    version = config['workspace']['package']['version']
    # Cargo treats explicitly included, gitignored assets as dirty. Check the
    # source tree ourselves; allow only this generated staging data in releases.
    command = ['cargo', 'package', '--manifest-path', str(WORKSPACE / 'Cargo.toml'), '--workspace', '--locked', '--allow-dirty']
    if args.no_verify:
        command.append('--no-verify')
    try:
        shutil.copytree(source, assets)
        subprocess.run(command, cwd=ROOT, check=True)
        final_commit, final_dirty = source_state()
        if final_commit != commit or (final_dirty and not args.allow_dirty):
            raise RuntimeError('Source tree changed while packaging')
        packages = []
        for member in config['workspace']['members']:
            name = tomllib.loads((WORKSPACE / member / 'Cargo.toml').read_text())['package']['name']
            filename = f'{name}-{version}.crate'
            archive = WORKSPACE / 'target/package' / filename
            shutil.copy2(archive, output / filename)
            packages.append({'name': name, 'version': version, 'filename': filename,
                             'sha256': hashlib.sha256(archive.read_bytes()).hexdigest()})
        (output / 'manifest.json').write_text(json.dumps({'version': version, 'verified': not args.no_verify,
                                                       'commit': commit,
                                                       'dirty': dirty or final_dirty,
                                                       'packages': packages}, indent=2) + '\n')
    finally:
        shutil.rmtree(assets)


if __name__ == '__main__':
    main()
