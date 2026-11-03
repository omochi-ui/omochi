import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('../../', import.meta.url));
const bindgen = process.env.WASM_BINDGEN ?? 'wasm-bindgen';
async function run(binary, args) {
  const child = spawn(binary, args, { cwd: root, stdio: 'inherit' });
  await new Promise((resolve, reject) => {
    child.on('error', reject);
    child.on('exit', code => code === 0 ? resolve() : reject(new Error(`${binary} exited ${code}`)));
  });
}
await run('cargo', ['build', '--locked', '--manifest-path', 'experiments/browser-hub/Cargo.toml', '-p', 'omochi-browser-hub-fixture']);
await run('cargo', ['build', '--locked', '--manifest-path', 'experiments/browser-hub/Cargo.toml', '-p', 'omochi-browser-hub-spike', '--target', 'wasm32-unknown-unknown', '--release']);
await run(bindgen, ['--target', 'web', '--out-dir', 'experiments/browser-hub/web/pkg', 'experiments/browser-hub/target/wasm32-unknown-unknown/release/omochi_browser_hub_spike.wasm']);
await run('bun', ['packages/hub-ui/node_modules/vite/bin/vite.js', 'build', '--config', 'experiments/browser-hub/vite.config.mjs', '--configLoader', 'native']);
