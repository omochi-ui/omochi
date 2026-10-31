import { test, expect } from 'bun:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawn, type ChildProcess } from 'node:child_process';
import { createInterface } from 'node:readline';
import { A2uiWebSocketClient, type Processor } from '../src/client.js';

// Exercises the actual renderer model/client API, not DOM layout or browser input.
test.skipIf(!process.env.OMOCHI_NATIVE_TEST)('renderer consumes native utility snapshots, dispatches actions and follows service restart', async () => {
  const profile = process.env.OMOCHI_NATIVE_PROFILE ?? 'debug';
  if (!['debug', 'release'].includes(profile)) throw new Error('invalid native profile');
  const binaries = resolve(import.meta.dirname, '../../server-rust/target', profile);
  const root = await mkdtemp(join(tmpdir(), 'omochi-renderer-'));
  const runtime = join(root, 'omochi');
  const processes: ChildProcess[] = [];
  const clients: A2uiWebSocketClient[] = [];
  const errors: string[] = [];
  const wait = async (ready: () => boolean) => {
    const deadline = Date.now() + 5000;
    while (!ready()) {
      if (Date.now() > deadline) throw new Error(`condition timed out; renderer errors: ${errors.join('; ')}`);
      await Bun.sleep(10);
    }
  };
  const start = async (binary: string, args: string[]) => {
    const child = spawn(join(binaries, binary), args, { env: { ...process.env, PATH: '/nonexistent' }, stdio: ['ignore', 'ignore', 'pipe'] });
    processes.push(child);
    const line = await new Promise<string>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`${binary} startup timed out`)), 5000);
      const lines = createInterface({ input: child.stderr! });
      child.once('error', reject);
      child.once('exit', code => reject(new Error(`${binary} exited ${code} before startup`)));
      lines.once('line', line => { clearTimeout(timer); lines.close(); resolve(line); });
    });
    return { child, line };
  };
  const stop = async (child: ChildProcess) => {
    if (child.exitCode !== null || child.signalCode !== null) return;
    const exited = new Promise<void>(resolve => child.once('exit', () => resolve()));
    child.kill('SIGTERM');
    const timer = setTimeout(() => child.kill('SIGKILL'), 2000);
    try { await exited; } finally { clearTimeout(timer); }
  };
  const utility = () => start('omochi-utils', ['button', '--service-id', 'button', '--runtime-dir', runtime, '--', '/bin/echo', 'renderer-ready']);
  const connect = (url: string) => {
    const state: { processor: Processor | null } = { processor: null };
    const client = new A2uiWebSocketClient(url, {
      onProcessor: processor => { state.processor = processor; },
      onError: error => { if (error) errors.push(error); },
      onStatus: () => {},
    });
    clients.push(client);
    client.connect();
    return state;
  };
  try {
    const { line } = await start('omochi-ui', ['--config-dir', join(root, 'config'), '--runtime-dir', runtime, 'hub', '--port', '0']);
    expect(line).toStartWith('[omochi-ui] dashboard http://127.0.0.1:');
    const url = line.replace('[omochi-ui] dashboard ', '').trim().replace('http:', 'ws:') + 'ws';
    const firstUtility = await utility();
    expect(firstUtility.line).toStartWith('Published service button');
    const first = connect(url);
    await wait(() => (first.processor?.model.surfacesMap.size ?? 0) === 1);
    const id = [...first.processor!.model.surfacesMap.keys()][0]!;
    expect(id).toMatch(/^[0-9a-f]{64}~button\/main$/);
    const content = (state: typeof first) => {
      const surface = state.processor?.model.surfacesMap.get(id);
      return surface ? JSON.stringify([...surface.componentsModel.entries].map(([, component]) => component.properties)) + JSON.stringify(surface.dataModel.get('/')) : '';
    };
    await wait(() => content(first).includes('Ready'));
    const surface = first.processor!.model.surfacesMap.get(id)!;
    const button = [...surface.componentsModel.entries].find(([, component]) => component.type === 'Button')!;
    expect(button).toBeDefined();
    await surface.dispatchAction(button[1].properties.action, button[0]);
    await wait(() => content(first).includes('renderer-ready'));
    const late = connect(url);
    await wait(() => content(late).includes('renderer-ready'));
    await stop(firstUtility.child);
    await wait(() => first.processor!.model.surfacesMap.size === 0 && late.processor!.model.surfacesMap.size === 0);
    const restarted = await utility();
    expect(restarted.line).toStartWith('Published service button');
    await wait(() => content(first).includes('Ready') && content(late).includes('Ready'));
    expect(content(first)).not.toContain('renderer-ready');
    expect(errors).toEqual([]);
  } finally {
    for (const client of clients) client.close();
    for (const child of processes.reverse()) await stop(child);
    await rm(root, { recursive: true, force: true });
  }
}, 30000);
