import { createInterface } from 'node:readline';
import { LocalServerTransport } from '@omochi-ui/sdk/node';
const lines = createInterface({ input: process.stdin });
console.log('waiting');
let owner;
try {
  for await (const line of lines) {
    if (line === 'start') {
      owner = new LocalServerTransport({
        serviceId: 'shared',
        runtimeDir: process.argv[2],
      });
      await owner.ready;
      console.log('ready');
    } else if (line === 'close') {
      await owner?.close();
      console.log('closed');
      break;
    }
  }
} finally {
  await owner?.close();
  lines.close();
  process.stdin.destroy();
}
