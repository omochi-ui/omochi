"""Build and use the Python distribution outside the checkout, at a shallow path."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def check_configured_cargo_output(stage):
    """Package the artifact Cargo reports, even when an inferred path is stale."""
    project = stage / 'configured-sdk'
    package = project / 'src/omochi_ui'
    package.mkdir(parents=True)
    (package / '__init__.py').write_text('')
    shutil.copy2(ROOT / 'packages/sdk/python/hatch_build.py', project / 'hatch_build.py')
    (project / 'pyproject.toml').write_text('''
[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
[project]
name = "omochi-ui-cargo-fixture"
version = "0.0.0"
[tool.hatch.build.targets.wheel]
packages = ["src/omochi_ui"]
[tool.hatch.build.hooks.custom]
path = "hatch_build.py"
''')
    native = project / 'native'
    crate = native / 'local-ffi'
    (crate / 'src').mkdir(parents=True)
    (native / 'Cargo.toml').write_text('[workspace]\nmembers = ["local-ffi"]\nresolver = "2"\n')
    (crate / 'Cargo.toml').write_text('''
[package]
name = "omochi-local-ffi"
version = "0.0.0"
edition = "2021"
[lib]
crate-type = ["cdylib"]
''')
    (crate / 'src/lib.rs').write_text('pub extern "C" fn fixture() -> u32 { 42 }\n')
    host = next(line.split(': ', 1)[1] for line in subprocess.check_output(
        ['rustc', '-vV'], text=True).splitlines() if line.startswith('host: '))
    (project / '.cargo').mkdir()
    (project / '.cargo/config.toml').write_text(
        f'[build]\ntarget-dir = "configured target"\ntarget = "{host}"\n')
    name = ('omochi_local_ffi.dll' if sys.platform == 'win32' else
            'libomochi_local_ffi.dylib' if sys.platform == 'darwin' else 'libomochi_local_ffi.so')
    stale = native / 'target/release' / name
    stale.parent.mkdir(parents=True)
    stale.write_text('stale library from an earlier build')
    env = dict(os.environ)
    for key in ('CARGO_BUILD_TARGET', 'CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET_DIR',
                'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS'):
        env.pop(key, None)
    subprocess.run([sys.executable, '-m', 'build', '--wheel', '--outdir',
                    str(project / 'dist')], cwd=project, env=env, check=True)
    artifact = project / 'configured target' / host / 'release' / name
    with zipfile.ZipFile(next((project / 'dist').glob('*.whl'))) as wheel:
        assert wheel.read(f'omochi_ui/_native/{name}') == artifact.read_bytes(), \
            'Python wheel must contain the configured Cargo artifact, not a stale library'


def main():
    with tempfile.TemporaryDirectory(prefix='omochi-python-dist-') as staging:
        stage = Path(staging)
        check_configured_cargo_output(stage)
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
    print('Configured Cargo artifact, standalone Python sdist, rebuilt sdist, wheel and installed native publication passed')


if __name__ == '__main__':
    main()
