import { createInterface } from 'node:readline';
import { UnixSocketServerTransport } from '../../packages/sdk/typescript/dist/node/index.js';
const commands = createInterface({ input: process.stdin })[
  Symbol.asyncIterator
]();
console.log('OWNER WAITING');
await commands.next();
const transport = new UnixSocketServerTransport({
  serviceId: 'demo',
  runtimeDir: process.env.OMOCHI_OWNER_ROOT,
});
try {
  await transport.ready;
  console.log('OWNER READY');
} catch {
  await transport.close();
  console.log('OWNER REJECTED');
  process.exit(0);
}
await commands.next();
await transport.close();
console.log('OWNER CLOSED');
await commands.next();
await transport.close();
process.exit(0);
