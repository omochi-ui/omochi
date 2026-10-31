"""Regression checks for the archive boundary before any npm package is created."""
import hashlib
import importlib.util
import io
import json
import tarfile
import tempfile
import unittest
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


if __name__ == '__main__':
    unittest.main()
