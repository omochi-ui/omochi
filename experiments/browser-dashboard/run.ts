import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const command = process.argv[2] ?? 'build';
const modes = ['session', 'profile', 'media', 'boards', 'hosted', 'faults', 'revocation', 'load', 'all'];
if (!['build', 'test:unit', 'test', ...modes.map(mode => `test:${mode}`)].includes(command)) {
  throw new Error(`Use build, test:unit, test, or test:<${modes.join('|')}>.`);
}

async function run(binary: string, args: string[]): Promise<void> {
  const child = spawn(binary, args, { cwd: root, stdio: 'inherit' });
  await new Promise<void>((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', code => code === 0 ? resolve() : reject(new Error(`${binary} failed (${code}).`)));
  });
}
const workspace = (name: string, script: string) => run('bun', ['run', '--filter', `@omochi-ui/${name}`, script]);

await run('bun', ['run', command.startsWith('test') && command !== 'test:unit' ? 'build:native' : 'build:web']);
await workspace('browser-transport', 'build');
await workspace('browser-hub', 'build');
if (command === 'test:unit' || command === 'test') {
  await workspace('browser-transport', 'test');
  await workspace('browser-hub', 'test');
  await workspace('browser-dashboard-experiment', 'test');
}
if (command !== 'test:unit') {
  await workspace('browser-transport', 'build:wasm');
  await workspace('browser-dashboard-experiment', 'build:browser-test');
  await workspace('browser-dashboard-experiment', 'build:hosted');
  if (command !== 'build') {
    await run('cargo', ['build', '--manifest-path', 'crates/Cargo.toml', '--locked', '-p', 'omochi-gateway', '--example', 'browser_fixture']);
    await run('bun', ['experiments/browser-dashboard/tests/browser-hub.mjs', '--mode', command === 'test' ? 'all' : command.slice(5), ...process.argv.slice(3)]);
  }
}
