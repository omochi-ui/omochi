import { GlobalRegistrator } from '@happy-dom/global-registrator';

// Provide browser elements for A2UI while keeping Bun's network and timers.
const native = new Map([
  'fetch', 'Request', 'Response', 'Headers', 'WebSocket', 'AbortController',
  'AbortSignal', 'Blob', 'FormData', 'File', 'URL', 'URLSearchParams',
  'performance', 'setTimeout', 'clearTimeout', 'setInterval', 'clearInterval',
  'queueMicrotask',
].map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
GlobalRegistrator.register();
for (const [name, descriptor] of native) {
  if (descriptor) Object.defineProperty(globalThis, name, descriptor);
}
