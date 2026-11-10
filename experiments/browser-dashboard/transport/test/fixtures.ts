import type {
  RawConnection,
  RawEndpoint,
  RawStream,
  WasmModule,
} from '../src/ffi.js';

export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
export class FakeStream implements RawStream {
  read: ReturnType<typeof deferred<string | null>> | null = null;
  write: ReturnType<typeof deferred<void>> | null = null;
  cancelGate: ReturnType<typeof deferred<void>> | null = null;
  active = 0;
  frees = 0;
  cancels = 0;
  finishes = 0;
  reads = 0;
  sends: string[] = [];
  blockWrites = false;
  send(json: string): Promise<void> {
    this.sends.push(json);
    if (!this.blockWrites) return Promise.resolve();
    this.active++;
    this.write = deferred<void>();
    return this.write.promise.finally(() => {
      this.active--;
    });
  }
  receive(): Promise<string | null> {
    this.reads++;
    this.active++;
    this.read = deferred<string | null>();
    return this.read.promise.finally(() => {
      this.active--;
    });
  }
  async finish() {
    this.finishes++;
  }
  cancel(): Promise<void> {
    this.cancels++;
    this.read?.reject('Cancelled');
    this.write?.reject('Cancelled');
    return this.cancelGate?.promise ?? Promise.resolve();
  }
  free() {
    if (this.active) throw new Error('FFI stream freed with live operations');
    this.frees++;
  }
}
export class FakeConnection implements RawConnection {
  readonly streams: FakeStream[] = [];
  readonly headers: string[] = [];
  readonly end = deferred<string>();
  openGate: ReturnType<typeof deferred<RawStream>> | null = null;
  readonly cancelled: number[] = [];
  closes = 0;
  frees = 0;
  remote_id() {
    return 'a'.repeat(64);
  }
  open(header: string) {
    this.headers.push(header);
    if (this.openGate) return this.openGate.promise;
    const stream = new FakeStream();
    this.streams.push(stream);
    return Promise.resolve(stream);
  }
  cancel_open(id: number) {
    this.cancelled.push(id);
    this.openGate?.reject('Cancelled');
  }
  closed() {
    return this.end.promise;
  }
  close() {
    this.closes++;
    this.end.resolve(
      JSON.stringify({ code: '0', reason: 'ignored raw reason' }),
    );
  }
  free() {
    this.frees++;
  }
}
export class FakeEndpoint implements RawEndpoint {
  readonly connections: FakeConnection[] = [];
  readonly cancelled: number[] = [];
  connectGate: ReturnType<typeof deferred<RawConnection>> | null = null;
  closes = 0;
  frees = 0;
  connect() {
    if (this.connectGate) return this.connectGate.promise;
    const connection = new FakeConnection();
    this.connections.push(connection);
    return Promise.resolve(connection);
  }
  cancel_connect(id: number) {
    this.cancelled.push(id);
    this.connectGate?.reject('Cancelled');
  }
  async close() {
    this.closes++;
  }
  frame_bytes() {
    return 0;
  }
  free() {
    this.frees++;
  }
}
export function fixture() {
  const endpoint = new FakeEndpoint();
  let endpointCalls = 0;
  const module: WasmModule = {
    default: async () => {},
    generate_identity: () =>
      JSON.stringify({ secretHex: '0'.repeat(64), endpointId: 'a'.repeat(64) }),
    identity_from_secret: (secretHex) =>
      JSON.stringify({ secretHex, endpointId: 'a'.repeat(64) }),
    RawEndpoint: {
      create: async () => {
        endpointCalls++;
        return endpoint;
      },
    },
  };
  return {
    module,
    endpoint,
    get endpointCalls() {
      return endpointCalls;
    },
  };
}
export const signal = () => new AbortController().signal;
