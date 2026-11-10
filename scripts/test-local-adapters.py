#!/usr/bin/env python3
"""Exercise installed Python, Node and Bun adapters against one runtime on any OS."""
import asyncio
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'packages/sdk/python/src'))
from omochi_ui import LocalServerTransport
from omochi_ui._native_local import open_connection

async def main():
    with tempfile.TemporaryDirectory(prefix='om-') as temp:
        root = Path(temp) / 'v2'
        owner = LocalServerTransport('demo', runtime_dir=root)
        await owner.start()
        child_source = '''
import {LocalServerTransport} from "@omochi-ui/sdk/node";
const t = new LocalServerTransport({serviceId:"demo",runtimeDir:process.argv[1]});
try { await t.ready; throw new Error("duplicate publisher accepted"); }
catch (error) { if (!/WouldBlock|locked|temporarily unavailable/i.test(String(error))) throw error; }
finally { await t.close(); }
'''
        for runtime in RUNTIMES:
            if not shutil.which(runtime): raise RuntimeError(f'{runtime} is required')
            command = [runtime, '--input-type=module', '-e', child_source, str(root)]
            result = await asyncio.to_thread(subprocess.run, command, cwd=ROOT / 'packages/sdk/typescript', capture_output=True, text=True, timeout=15)
            if result.returncode: raise AssertionError(result.stdout + result.stderr)
        reader, writer = await open_connection(owner.endpoint)
        message = {'version':'v0.9','createSurface':{'surfaceId':'main','catalogId':'test'}}
        await owner.send(message)
        assert json.loads(await asyncio.wait_for(reader.readline(), 5)) == message
        await owner.close()
        writer.close()
        successor = LocalServerTransport('demo', runtime_dir=root)
        await successor.start()
        await owner.close()
        assert (successor.directory / 'manifest.json').is_file()
        await successor.close()
        assert (root / '.svc-demo.lock~').is_file()
    print('Python/Node/Bun share publication ownership, byte streams and cleanup')

if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument('--runtime', action='append', choices=['node', 'bun'])
    RUNTIMES = parser.parse_args().runtime or ['node', 'bun']
    asyncio.run(main())
