"""Build and use the Python distribution outside the checkout, at a shallow path."""
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    with tempfile.TemporaryDirectory(prefix='omochi-python-dist-') as staging:
        stage = Path(staging)
        subprocess.run([sys.executable, '-m', 'build', '--sdist', '--outdir',
                        str(stage / 'dist'), str(ROOT / 'packages/sdk/python')], check=True)
        with tarfile.open(next((stage / 'dist').glob('*.tar.gz'))) as archive:
            archive.extractall(stage / 'unpacked', filter='data')
        source = next((stage / 'unpacked').iterdir())
        # /tmp/<name> has just two parents; checkout-only ancestor indexing fails.
        with tempfile.TemporaryDirectory(prefix='omochi-standalone-sdk-') as shallow:
            standalone = Path(shallow)
            for entry in source.iterdir():
                entry.rename(standalone / entry.name)
            env = dict(os.environ)
            env.setdefault('CARGO_TARGET_DIR', str(ROOT / 'packages/sdk/rust/target'))
            subprocess.run([sys.executable, '-m', 'build', '--outdir', str(stage / 'rebuilt'),
                            str(standalone)], env=env, cwd=stage, check=True)
            with tarfile.open(next((stage / 'rebuilt').glob('*.tar.gz'))) as archive:
                names = archive.getnames()
                for crate in ('local', 'local-ffi'):
                    assert any(name.endswith(f'/native/{crate}/src/lib.rs') for name in names)
        venv = stage / 'installed'
        subprocess.run([sys.executable, '-m', 'venv', str(venv)], check=True)
        python = venv / ('Scripts/python.exe' if sys.platform == 'win32' else 'bin/python')
        wheel = next((stage / 'rebuilt').glob('*.whl'))
        subprocess.run([str(python), '-m', 'pip', 'install', str(wheel)], check=True)
        subprocess.run([str(python), '-c', '''
import asyncio, json, tempfile
from pathlib import Path
from omochi_ui import LocalServerTransport
async def main():
    with tempfile.TemporaryDirectory() as root:
        transport = LocalServerTransport(service_id='standalone', runtime_dir=root)
        await transport.start()
        try:
            manifest = json.loads((Path(transport.directory) / 'manifest.json').read_text())
            assert manifest['version'] == 2
            assert manifest['serviceId'] == 'standalone'
        finally:
            await transport.close()
asyncio.run(main())
'''], cwd=stage, check=True)
    print('Standalone Python sdist, rebuilt sdist, wheel and installed native publication passed')


if __name__ == '__main__':
    main()
