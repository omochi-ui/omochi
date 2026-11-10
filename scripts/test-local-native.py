#!/usr/bin/env python3
"""Real SDK -> local gateway -> hub -> WebSocket action roundtrip, including Windows."""
import asyncio
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import urllib.request
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'packages/sdk/python/src'))
from omochi_ui import LocalServerTransport

async def main():
    from websockets.asyncio.client import connect
    if os.name == 'nt':
        import ctypes
        if not ctypes.windll.kernel32.GetConsoleWindow():
            if not ctypes.windll.kernel32.AllocConsole(): raise OSError('Cannot allocate console for CTRL_BREAK tests')
    suffix = '.exe' if os.name == 'nt' else ''
    binary = ROOT / 'crates/target' / os.environ.get('OMOCHI_NATIVE_PROFILE', 'debug') / ('omochi-ui' + suffix)
    with tempfile.TemporaryDirectory(prefix='om-') as temporary:
        root = Path(temporary)
        owner = LocalServerTransport('demo', runtime_dir=root / 'runtime')
        await owner.start()
        await owner.send({'version':'v0.9','createSurface':{'surfaceId':'main','catalogId':'test'}})
        with socket.socket() as address:
            address.bind(('127.0.0.1', 0))
            port = address.getsockname()[1]
        command = [str(binary), '--config-dir', str(root / 'state'), '--runtime-dir', str(root / 'runtime'), 'hub', '--offline', '--port', str(port)]
        process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, creationflags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name == 'nt' else 0)
        try:
            url = f'http://127.0.0.1:{port}'
            async def ready():
                for _ in range(200):
                    if process.poll() is not None:
                        raise AssertionError(process.stderr.read().decode(errors='replace'))
                    try:
                        with await asyncio.to_thread(urllib.request.urlopen, url + '/services', timeout=1) as response:
                            if json.load(response): return
                    except OSError: pass
                    await asyncio.sleep(.05)
                raise TimeoutError('native hub did not discover SDK')
            await ready()
            async with connect(f'ws://127.0.0.1:{port}/ws') as websocket:
                async def surface():
                    async for text in websocket:
                        message = json.loads(text)
                        if 'createSurface' in message: return message['createSurface']['surfaceId']
                route = await asyncio.wait_for(surface(), 5)
                assert route.endswith('~demo/main'), route
                await websocket.send(json.dumps({'version':'v0.9','action':{'name':'probe','surfaceId':route,'sourceComponentId':'button','timestamp':'2026-11-10T00:00:00Z','context':{}}}))
                received = await asyncio.wait_for(anext(owner.receive()), 5)
                assert received['action']['surfaceId'] == 'main', received
                assert received['action']['name'] == 'probe', received
            await owner.close()
            process.send_signal(signal.CTRL_BREAK_EVENT if os.name == 'nt' else signal.SIGTERM)
            code = await asyncio.to_thread(process.wait, 10)
            assert code == 0, process.stderr.read().decode(errors='replace')
        finally:
            if process.poll() is None:
                process.kill()
                await asyncio.to_thread(process.wait)
            await owner.close()
            process.stderr.close()
    from omochi_ui._native_local import open_connection, request
    with tempfile.TemporaryDirectory(prefix='om-') as temporary:
        root = Path(temporary) / 'v2'
        utility = binary.with_name('omochi-utils' + suffix)
        process = subprocess.Popen([str(utility), 'stream', '--service-id', 'stream', '--runtime-dir', str(root)], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, creationflags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name == 'nt' else 0)
        try:
            ready = await asyncio.wait_for(asyncio.to_thread(process.stderr.readline), 10)
            assert ready.startswith(b'Published service'), ready
            directory = root / 'svc-stream~'
            endpoint = json.loads((directory / 'manifest.json').read_text())['endpoint']
            if endpoint['kind'] == 'unix': endpoint['path'] = str(directory / endpoint['path'])
            reader, writer = await open_connection(endpoint)
            process.stdin.write('hello 日本語\n'.encode()); process.stdin.flush()
            async def output():
                while line := await reader.readline():
                    if 'hello 日本語' in line.decode(): return
                raise AssertionError('utility closed before publishing input')
            await asyncio.wait_for(output(), 10)
            await request('close', writer.handle)
            process.send_signal(signal.CTRL_BREAK_EVENT if os.name == 'nt' else signal.SIGTERM)
            assert await asyncio.to_thread(process.wait, 10) == 0
        finally:
            if process.poll() is None: process.kill(); await asyncio.to_thread(process.wait)
            process.stdin.close(); process.stderr.close()
    print('SDK/gateway/hub roundtrip, native utility input and graceful shutdown passed')

if __name__ == '__main__': asyncio.run(main())
