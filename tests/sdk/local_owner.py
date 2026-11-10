import asyncio
import sys
from omochi_ui import LocalServerTransport
async def main():
    print('waiting', flush=True)
    if (await asyncio.to_thread(sys.stdin.readline)).strip() != 'start': return
    owner = LocalServerTransport('shared', runtime_dir=sys.argv[1])
    try:
        await owner.start()
        print('ready', flush=True)
        while line := await asyncio.to_thread(sys.stdin.readline):
            if line.strip() == 'close':
                await owner.close()
                print('closed', flush=True)
                break
    finally: await owner.close()
asyncio.run(main())
