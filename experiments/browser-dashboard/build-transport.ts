import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { mkdir } from 'node:fs/promises';

const root = fileURLToPath(new URL('../../', import.meta.url));
const bindgen = process.env.WASM_BINDGEN ?? 'wasm-bindgen';
const setup = 'Run rustup target add wasm32-unknown-unknown and cargo install wasm-bindgen-cli --version 0.2.130 --locked. WASM_BINDGEN may select an existing 0.2.130 binary.';
async function inspect(command: string[]): Promise<string> {
  try {
    const child = Bun.spawn(command, { cwd: root, stdout: 'pipe', stderr: 'pipe' });
    const [stdout, stderr, code] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
    if (code !== 0) throw new Error(stderr);
    return stdout.trim();
  } catch { throw new Error(`Browser WASM tooling is unavailable. ${setup}`); }
}
async function run(command: string, args: string[]): Promise<void> {
  const child = spawn(command, args, { cwd: root, stdio: 'inherit' });
  await new Promise<void>((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', code => code === 0 ? resolve() : reject(new Error(`${command} failed (${code}). ${setup}`)));
  });
}
if (!(await inspect(['rustup', 'target', 'list', '--installed'])).split('\n').includes('wasm32-unknown-unknown')) throw new Error(setup);
if (await inspect([bindgen, '--version']) !== 'wasm-bindgen 0.2.130') throw new Error(`Exact wasm-bindgen 0.2.130 is required. ${setup}`);
await run('cargo', ['build', '--manifest-path', 'experiments/browser-dashboard/transport/Cargo.toml', '--locked', '--target', 'wasm32-unknown-unknown', '--release']);
await mkdir(new URL('./transport/generated/', import.meta.url), { recursive: true });
await run(bindgen, ['--target', 'web', '--out-name', 'iroh', '--out-dir', 'experiments/browser-dashboard/transport/generated',
  'experiments/browser-dashboard/transport/target/wasm32-unknown-unknown/release/omochi_browser_transport.wasm']);
