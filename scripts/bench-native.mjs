// Linux-only, local Unix SDK -> gateway -> hub -> WebSocket comparison.
// Run after building both the legacy TypeScript CLI and native release binaries.
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:net';
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  readdirSync,
  rmSync,
} from 'node:fs';
import { tmpdir, cpus, release } from 'node:os';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';

if (process.platform !== 'linux')
  throw new Error('This benchmark reads Linux /proc');
const legacyRoot = process.env.OMOCHI_LEGACY_ROOT;
if (!legacyRoot)
  throw new Error(
    'Set OMOCHI_LEGACY_ROOT to a built pre-Rust-cleanup checkout (for example b8a2f85).',
  );
const repetitions = Number(process.env.OMOCHI_BENCH_RUNS ?? 5);
if (!Number.isInteger(repetitions) || repetitions < 1 || repetitions > 20)
  throw new Error('invalid run count');
const hz = Number(
  execFileSync('getconf', ['CLK_TCK'], { encoding: 'utf8' }).trim(),
);
const root = resolve(import.meta.dirname, '..');
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function within(promise, milliseconds, label) {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`${label} timed out`)),
          milliseconds,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}
function treeStats(pid) {
  const rows = new Map();
  for (const name of readdirSync('/proc').filter((name) =>
    /^\d+$/.test(name),
  )) {
    try {
      const line = readFileSync(`/proc/${name}/stat`, 'utf8');
      const fields = line.slice(line.lastIndexOf(')') + 2).split(' ');
      rows.set(Number(name), {
        parent: Number(fields[1]),
        ticks: Number(fields[11]) + Number(fields[12]),
      });
    } catch {}
  }
  const included = new Set([pid]);
  for (let changed = true; changed;) {
    changed = false;
    for (const [id, row] of rows)
      if (included.has(row.parent) && !included.has(id)) {
        included.add(id);
        changed = true;
      }
  }
  let rssKiB = 0,
    pssKiB = 0,
    ticks = 0;
  for (const id of included) {
    if (!rows.has(id)) continue;
    ticks += rows.get(id).ticks;
    const memory = readFileSync(`/proc/${id}/smaps_rollup`, 'utf8');
    rssKiB += Number(memory.match(/^Rss:\s+(\d+)/m)[1]);
    pssKiB += Number(memory.match(/^Pss:\s+(\d+)/m)[1]);
  }
  return { processes: included.size, rssKiB, pssKiB, ticks };
}
function quantile(values, q) {
  return [...values].sort((a, b) => a - b)[
    Math.min(values.length - 1, Math.ceil(q * values.length) - 1)
  ];
}
async function sample(mode) {
  const directory = mkdtempSync(join(tmpdir(), 'omochi-bench-'));
  const runtime = join(directory, 'run', 'omochi', mode === 'rust' ? 'v2' : '');
  const service = join(runtime, mode === 'rust' ? 'svc-echo~' : 'echo');
  mkdirSync(service, { recursive: true, mode: 0o700 });
  // mkdir's recursive mode also makes the runtime private.
  if (mode === 'rust')
    writeFileSync(
      join(runtime, 'runtime.json'),
      JSON.stringify({ version: 2 }),
      {
        mode: 0o600,
      },
    );
  writeFileSync(
    join(service, 'manifest.json'),
    JSON.stringify(
      mode === 'rust'
        ? {
            version: 2,
            serviceId: 'echo',
            name: 'Benchmark echo',
            endpoint: { kind: 'unix', path: 'a2ui.sock' },
          }
        : { name: 'Benchmark echo', socket: 'a2ui.sock' },
    ),
    { mode: 0o600 },
  );
  const sockets = new Set();
  const fixture = createServer((socket) => {
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
    socket.on('error', () => {});
    socket.write(
      JSON.stringify({
        version: 'v0.9',
        createSurface: { surfaceId: 'main', catalogId: 'urn:test:benchmark' },
      }) + '\n',
    );
    socket.write(
      JSON.stringify({
        version: 'v0.9',
        updateDataModel: { surfaceId: 'main', value: { sequence: 0 } },
      }) + '\n',
    );
    let pending = '';
    socket.on('data', (bytes) => {
      pending += bytes.toString();
      for (let newline; (newline = pending.indexOf('\n')) >= 0;) {
        const line = pending.slice(0, newline);
        pending = pending.slice(newline + 1);
        if (!line.trim()) continue;
        const message = JSON.parse(line);
        if (message.action)
          socket.write(
            JSON.stringify({
              version: 'v0.9',
              updateDataModel: {
                surfaceId: 'main',
                value: message.action.context,
              },
            }) + '\n',
          );
      }
    });
  });
  await new Promise((resolve, reject) => {
    fixture.once('error', reject);
    fixture.listen(join(service, 'a2ui.sock'), resolve);
  });
  const argv =
    mode === 'rust'
      ? [
          join(root, 'crates/target/release/omochi-ui'),
          '--config-dir',
          join(directory, 'config', 'native'),
          '--runtime-dir',
          runtime,
          'hub',
          '--port',
          '0',
        ]
      : [
          process.execPath,
          join(legacyRoot, 'packages/cli/dist/cli.js'),
          'hub',
          '--port',
          '0',
        ];
  let child, ws;
  const started = performance.now();
  try {
    child = spawn(argv[0], argv.slice(1), {
      cwd: root,
      detached: true,
      env: {
        ...process.env,
        XDG_RUNTIME_DIR: join(directory, 'run'),
        XDG_CONFIG_HOME: join(directory, 'config'),
      },
      stdio: ['ignore', 'ignore', 'pipe'],
    });
    const exited = new Promise((resolve) => child.once('exit', resolve));
    let logs = '';
    const address = await within(
      new Promise((resolve, reject) => {
        child.once('error', reject);
        child.stderr.on('data', (bytes) => {
          logs += bytes.toString();
          const match = logs.match(/http:\/\/127\.0\.0\.1:\d+/);
          if (match) resolve(match[0]);
        });
        child.once('exit', () =>
          reject(new Error(`hub exited during startup: ${logs}`)),
        );
      }),
      10000,
      'hub startup',
    );
    let surface,
      sequence = 0;
    const pending = new Map();
    let ready;
    const first = new Promise((resolve) => {
      ready = resolve;
    });
    ws = new WebSocket(address.replace('http:', 'ws:') + '/ws');
    ws.addEventListener('message', (event) => {
      const message = JSON.parse(event.data);
      if (message.createSurface) {
        surface = message.createSurface.surfaceId;
        ready();
      }
      const token = message.updateDataModel?.value?.sequence;
      if (pending.has(token)) {
        pending.get(token)(performance.now());
        pending.delete(token);
      }
    });
    await within(first, 10000, 'initial snapshot');
    const startupMs = performance.now() - started;
    const scenarios = [];
    for (const payloadBytes of [64, 65536]) {
      const payload = 'x'.repeat(payloadBytes),
        times = [];
      for (let index = 0; index < 225; index++) {
        const token = ++sequence;
        const response = new Promise((resolve) => pending.set(token, resolve));
        const sent = performance.now();
        ws.send(
          JSON.stringify({
            version: 'v0.9',
            action: {
              surfaceId: surface,
              name: 'echo',
              sourceComponentId: 'root',
              timestamp: '2026-10-31T00:00:00.000Z',
              context: { sequence: token, payload },
            },
          }),
        );
        const received = await within(response, 5000, 'action echo');
        if (index >= 25) times.push(received - sent);
      }
      scenarios.push({
        payloadBytes,
        samples: times.length,
        p50Ms: quantile(times, 0.5),
        p95Ms: quantile(times, 0.95),
        meanMs: times.reduce((a, b) => a + b, 0) / times.length,
      });
    }
    const before = treeStats(child.pid),
      idleStart = performance.now();
    await pause(2000);
    const after = treeStats(child.pid),
      idleSeconds = (performance.now() - idleStart) / 1000;
    const result = {
      mode,
      startupMs,
      ...after,
      idleCpuPercent: (100 * (after.ticks - before.ticks)) / hz / idleSeconds,
      scenarios,
    };
    delete result.ticks;
    ws.close();
    process.kill(-child.pid, 'SIGTERM');
    await within(exited, 5000, 'hub shutdown');
    return result;
  } finally {
    ws?.close();
    if (child?.pid) {
      try {
        process.kill(-child.pid, 'SIGKILL');
      } catch {}
    }
    for (const socket of sockets) socket.destroy();
    await new Promise((resolve) => fixture.close(resolve));
    rmSync(directory, { recursive: true, force: true });
  }
}
const results = [];
for (let index = 0; index < repetitions; index++) {
  // Alternate order to reduce warm-cache/order bias; every run has fresh state.
  for (const mode of index % 2
    ? ['rust', 'typescript']
    : ['typescript', 'rust']) {
    const result = await sample(mode);
    results.push(result);
    console.error(
      `${mode}: startup ${result.startupMs.toFixed(1)} ms, PSS ${(result.pssKiB / 1024).toFixed(1)} MiB`,
    );
  }
}
const provenance = {
  commit: execFileSync('git', ['rev-parse', 'HEAD'], {
    cwd: root,
    encoding: 'utf8',
  }).trim(),
  dirty: !!execFileSync('git', ['status', '--porcelain'], {
    cwd: root,
    encoding: 'utf8',
  }).trim(),
  nativeSha256: createHash('sha256')
    .update(readFileSync(join(root, 'crates/target/release/omochi-ui')))
    .digest('hex'),
};
const report = {
  ...provenance,
  timestamp: new Date().toISOString(),
  platform: `Linux ${release()}`,
  cpu: cpus()[0].model,
  logicalCpus: cpus().length,
  bun: process.versions.bun,
  rust: execFileSync('rustc', ['--version'], { encoding: 'utf8' }).trim(),
  repetitions,
  method:
    'Fresh config; warm OS cache; local synthetic Unix SDK echo; 25 warmup + 200 measured actions per payload; 2s idle; process-tree RSS/PSS incl. gateway; no browser rendering or remote transport',
  results,
};
const output = join(root, 'artifacts/bench/native-local.json');
mkdirSync(join(root, 'artifacts/bench'), { recursive: true });
writeFileSync(output, JSON.stringify(report, null, 2) + '\n');
console.log(output);
