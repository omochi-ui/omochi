"""Regression checks for native Cargo and npm packaging, without publishing."""
import hashlib
import importlib.util
import io
import json
import os
import tarfile
import tempfile
import unittest
import subprocess
import sys
from unittest.mock import patch
from pathlib import Path

spec = importlib.util.spec_from_file_location('native_npm', Path(__file__).with_name('package-native-npm.py'))
packaging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packaging)


class ArchiveTests(unittest.TestCase):
    def archive(self, root, *, version='0.2.0', target='x86_64-unknown-linux-gnu', corrupt_inner=False, duplicate=False, symlink=False):
        files = {name: name.encode() for name in ['bin/omochi-ui', 'bin/omochi-utils', 'LICENSE', 'README.md']}
        metadata = {'version': version, 'target': target, 'commit': 'test', 'dirty': False,
                    'sha256': {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}}
        if corrupt_inner:
            files['bin/omochi-ui'] = b'changed'
        files['manifest.json'] = json.dumps(metadata).encode()
        archive = Path(root) / 'test.tar.gz'
        with tarfile.open(archive, 'w:gz') as tar:
            for name, data in files.items():
                item = tarfile.TarInfo(name)
                item.size = len(data)
                tar.addfile(item, io.BytesIO(data))
            if duplicate:
                tar.addfile(tarfile.TarInfo('LICENSE'), io.BytesIO())
            if symlink:
                item = tarfile.TarInfo('link')
                item.type = tarfile.SYMTYPE
                item.linkname = '/etc/passwd'
                tar.addfile(item)
        archive.with_suffix('.gz.sha256').write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + '  test.tar.gz\n')
        return archive

    def test_accepts_matching_target_version_and_hashes(self):
        with tempfile.TemporaryDirectory() as root:
            target, metadata, files = packaging.validate_archive(self.archive(root), '0.2.0')
            self.assertEqual(target['cpu'], 'x64')
            self.assertEqual(metadata['version'], '0.2.0')
            self.assertIn('bin/omochi-utils', files)

    def test_rejects_wrong_versions_targets_inner_hashes_and_nonregular_members(self):
        for options in [{'version': '0.1.2'}, {'target': 'unknown'}, {'corrupt_inner': True}, {'duplicate': True}, {'symlink': True}]:
            with self.subTest(options=options), tempfile.TemporaryDirectory() as root:
                with self.assertRaises(ValueError):
                    packaging.validate_archive(self.archive(root, **options), '0.2.0')

    def test_rejects_corrupt_outer_archive_before_opening_tar(self):
        with tempfile.TemporaryDirectory() as root:
            archive = self.archive(root)
            archive.write_bytes(b'not the original archive')
            with self.assertRaisesRegex(ValueError, 'Archive checksum mismatch'):
                packaging.validate_archive(archive, '0.2.0')


class CrateSourceTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location('native_crates', Path(__file__).with_name('package-crates.py'))
        self.crates = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.crates)
        self.temporary = tempfile.TemporaryDirectory(prefix='omochi-crate-source-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.workspace = self.root / 'crates'
        (self.workspace / 'hub').mkdir(parents=True)
        (self.workspace / 'Cargo.toml').write_text('[workspace]\nmembers = ["hub"]\n[workspace.package]\nversion = "0.2.0"\n')
        (self.workspace / 'hub/Cargo.toml').write_text('[package]\nname = "omochi-hub"\n')
        (self.root / '.gitignore').write_text('/crates/hub/assets/\n/crates/target/\n/packages/hub-ui/dist/\n/artifacts/\n')
        source = self.root / 'packages/hub-ui/dist'
        source.mkdir(parents=True)
        (source / 'index.html').write_text('<html>generated</html>')
        for args in [['init', '-q'], ['add', '.'], ['-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture']]:
            subprocess.run(['git', *args], cwd=self.root, check=True)

    def run_pack(self):
        original_run = subprocess.run
        def fake_cargo(command, **kwargs):
            if command[0] != 'cargo':
                return original_run(command, **kwargs)
            self.assertIn('--allow-dirty', command)
            self.assertTrue((self.workspace / 'hub/assets/index.html').exists())
            package = self.workspace / 'target/package'
            package.mkdir(parents=True)
            (package / 'omochi-hub-0.2.0.crate').write_bytes(b'fixture archive')
            return subprocess.CompletedProcess(command, 0)
        with patch.object(self.crates, 'ROOT', self.root), patch.object(self.crates, 'WORKSPACE', self.workspace), patch.object(sys, 'argv', ['package-crates.py']), patch('subprocess.run', side_effect=fake_cargo):
            self.crates.main()

    def test_clean_sources_allow_generated_assets_without_marking_release_dirty(self):
        self.run_pack()
        manifest = json.loads((self.root / 'artifacts/crates/manifest.json').read_text())
        self.assertFalse(manifest['dirty'])
        self.assertFalse((self.workspace / 'hub/assets').exists())
        self.assertEqual(subprocess.check_output(['git', 'status', '--porcelain'], cwd=self.root), b'')

    def test_private_native_and_standalone_browser_inputs_are_not_distributed(self):
        (self.workspace / 'private').mkdir()
        (self.workspace / 'private/Cargo.toml').write_text(
            '[package]\nname = "private-native"\npublish = false\n')
        workspace = self.workspace / 'Cargo.toml'
        workspace.write_text(workspace.read_text().replace('["hub"]', '["hub", "private"]'))
        for directory, name in [('experiments/browser-dashboard/transport', 'browser-wasm'),
                                ('packages/hub-transport', 'hub-client-wasm')]:
            browser = self.root / directory
            browser.mkdir(parents=True)
            (browser / 'Cargo.toml').write_text(
                f'[workspace]\n[package]\nname = "{name}"\npublish = false\n')
        subprocess.run(['git', 'add', '.'], cwd=self.root, check=True)
        subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                        '-c', 'commit.gpgsign=false', 'commit', '-qm', 'private fixtures'], cwd=self.root, check=True)
        original_run = subprocess.run
        def fake_cargo(command, **kwargs):
            if command[0] != 'cargo':
                return original_run(command, **kwargs)
            self.assertIn('--exclude', command)
            self.assertEqual(command[command.index('--exclude') + 1], 'private-native')
            self.assertNotIn('browser-wasm', command)
            self.assertNotIn('hub-client-wasm', command)
            package = self.workspace / 'target/package'
            package.mkdir(parents=True)
            (package / 'omochi-hub-0.2.0.crate').write_bytes(b'public archive')
            return subprocess.CompletedProcess(command, 0)
        with patch.object(self.crates, 'ROOT', self.root), patch.object(self.crates, 'WORKSPACE', self.workspace), patch.object(sys, 'argv', ['package-crates.py']), patch('subprocess.run', side_effect=fake_cargo):
            self.crates.main()
        manifest = json.loads((self.root / 'artifacts/crates/manifest.json').read_text())
        self.assertEqual([package['name'] for package in manifest['packages']], ['omochi-hub'])

    def test_real_cargo_packages_and_verifies_gitignored_dashboard_assets(self):
        (self.workspace / 'hub/Cargo.toml').write_text(
            '[package]\nname = "omochi-hub"\nversion.workspace = true\n'
            'edition = "2024"\nlicense = "Apache-2.0"\n'
            'include = ["src/**", "assets/**", "Cargo.toml", "Cargo.lock"]\n')
        (self.workspace / 'hub/src').mkdir()
        (self.workspace / 'hub/src/lib.rs').write_text(
            'pub const DASHBOARD: &str = include_str!("../assets/index.html");\n')
        subprocess.run(['cargo', 'generate-lockfile', '--offline', '--manifest-path',
                        str(self.workspace / 'Cargo.toml')], cwd=self.root, check=True)
        subprocess.run(['git', 'add', '.'], cwd=self.root, check=True)
        subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                        '-c', 'commit.gpgsign=false', 'commit', '-qm', 'real crate'], cwd=self.root, check=True)
        with patch.object(self.crates, 'ROOT', self.root), patch.object(self.crates, 'WORKSPACE', self.workspace), patch.object(sys, 'argv', ['package-crates.py']), patch.dict(os.environ, {'CARGO_NET_OFFLINE': 'true'}):
            self.crates.main()
        manifest = json.loads((self.root / 'artifacts/crates/manifest.json').read_text())
        self.assertTrue(manifest['verified'])
        self.assertFalse(manifest['dirty'])
        with tarfile.open(self.root / 'artifacts/crates/omochi-hub-0.2.0.crate') as archive:
            self.assertEqual(archive.extractfile('omochi-hub-0.2.0/assets/index.html').read(),
                             b'<html>generated</html>')
        self.assertFalse((self.workspace / 'hub/assets').exists())
        self.assertEqual(subprocess.check_output(['git', 'status', '--porcelain'], cwd=self.root), b'')

    def test_source_edits_are_rejected_before_cargo_or_asset_staging(self):
        with (self.workspace / 'hub/Cargo.toml').open('a') as file:
            file.write('# uncommitted source edit\n')
        with patch.object(sys, 'stderr', io.StringIO()) as stderr, self.assertRaises(SystemExit) as error:
            self.run_pack()
        self.assertEqual(error.exception.code, 2)
        self.assertIn('Commit source changes before packaging', stderr.getvalue())
        self.assertFalse((self.workspace / 'hub/assets').exists())
        self.assertFalse((self.root / 'artifacts/crates/manifest.json').exists())


if __name__ == '__main__':
    unittest.main()
