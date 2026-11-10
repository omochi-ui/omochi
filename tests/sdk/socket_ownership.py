"""Cross-language/process ownership, simultaneous startup and kernel crash recovery."""
from __future__ import annotations
import asyncio
import argparse
import itertools
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
ROOT = Path(__file__).resolve().parents[2]
ENV = {**os.environ, 'PYTHONPATH': str(ROOT / 'packages/sdk/python/src')}
EXE = '.exe' if os.name == 'nt' else ''
PROFILE = os.environ.get('OMOCHI_NATIVE_PROFILE', 'debug')
COMMANDS = {
    'python': [sys.executable, str(ROOT / 'tests/sdk/local_owner.py')],
    'node': ['node', str(ROOT / 'tests/sdk/local_owner.mjs')],
    'bun': ['bun', str(ROOT / 'tests/sdk/local_owner.mjs')],
    'rust': [str(ROOT / 'packages/sdk/rust/target' / PROFILE / 'examples' / ('local_fixture' + EXE))],
}

class Publisher:
    def __init__(self, kind, process): self.kind, self.process = kind, process
    @classmethod
    async def spawn(cls, kind, root):
        command = COMMANDS[kind] + [str(root)] if kind != 'utility' else [str(ROOT / 'crates/target' / PROFILE / ('omochi-utils' + EXE)), 'stream', '--service-id', 'shared', '--runtime-dir', str(root)]
        process = await asyncio.create_subprocess_exec(*command, cwd=ROOT, env=ENV, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, creationflags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name == 'nt' else 0)
        owner = cls(kind, process)
        if kind != 'utility':
            line = await owner.line()
            if line != 'waiting':
                await owner.kill()
                error = (await process.stderr.read()).decode(errors='replace')
                raise AssertionError(f'{kind} failed to start: {line!r}; {error}')
        return owner
    async def line(self):
        stream = self.process.stderr if self.kind == 'utility' else self.process.stdout
        return (await asyncio.wait_for(stream.readline(), 15)).decode().strip()
    async def start(self):
        if self.kind == 'utility':
            line = await self.line()
            return line.startswith('Published service')
        self.process.stdin.write(b'start\n'); await self.process.stdin.drain()
        return await self.line() == 'ready'
    async def close(self):
        if self.process.returncode is not None: return
        if self.kind == 'utility': self.process.send_signal(signal.CTRL_BREAK_EVENT if os.name == 'nt' else signal.SIGTERM)
        else:
            self.process.stdin.write(b'close\n'); await self.process.stdin.drain()
            assert await self.line() == 'closed'
        assert await asyncio.wait_for(self.process.wait(), 15) == 0
    async def kill(self):
        if self.process.returncode is None:
            self.process.kill(); await self.process.wait()

async def succession(root, first, second, crash=False):
    owner = await Publisher.spawn(first, root)
    successor = None
    try:
        assert await owner.start(), first
        manifest = root / 'svc-shared~/manifest.json'
        before = manifest.read_bytes()
        contender = await Publisher.spawn(second, root)
        try:
            assert not await contender.start(), f'{second} stole {first} ownership'
            await asyncio.wait_for(contender.process.wait(), 15)
            assert manifest.read_bytes() == before
        finally: await contender.kill()
        if crash:
            await owner.kill()
            assert manifest.exists(), 'crash fixture unexpectedly cleaned its manifest'
        else: await owner.close()
        successor = await Publisher.spawn(second, root)
        assert await successor.start(), f'{second} could not succeed {first}'
        assert manifest.is_file()
        await successor.close()
        assert not manifest.exists()
        assert (root / '.svc-shared.lock~').is_file()
    finally:
        await owner.kill()
        if successor: await successor.kill()

async def simultaneous(root):
    owners = [await Publisher.spawn(kind, root) for kind in COMMANDS]
    try:
        results = await asyncio.gather(*(owner.start() for owner in owners))
        assert sum(results) == 1, results
        for owner, won in zip(owners, results):
            if won: await owner.close()
    finally:
        for owner in owners: await owner.kill()

async def main():
    if os.name == 'nt':
        import ctypes
        if not ctypes.windll.kernel32.GetConsoleWindow():
            if not ctypes.windll.kernel32.AllocConsole(): raise OSError('Cannot allocate console for CTRL_BREAK tests')
    for first, second in itertools.product([*COMMANDS, 'utility'], repeat=2):
        with tempfile.TemporaryDirectory(prefix='om-') as root:
            await succession(Path(root) / 'v2', first, second)
    for first in [*COMMANDS, 'utility']:
        with tempfile.TemporaryDirectory(prefix='om-') as root:
            await succession(Path(root) / 'v2', first, 'python', crash=True)
    with tempfile.TemporaryDirectory(prefix='om-') as root: await simultaneous(Path(root) / 'v2')
    print('25 publisher pairs, 5 crash recoveries and simultaneous startup passed')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--skip-build', action='store_true', help='use existing binaries, including cross-compiled VM fixtures')
    options = parser.parse_args()
    if not options.skip_build:
        profile = ['--profile', PROFILE]
        subprocess.run(['cargo','build',*profile,'--manifest-path',str(ROOT/'packages/sdk/rust/Cargo.toml'),'--example','local_fixture'],check=True)
        subprocess.run(['cargo','build',*profile,'--manifest-path',str(ROOT/'crates/Cargo.toml'),'-p','omochi-utils'],check=True)
    asyncio.run(main())
