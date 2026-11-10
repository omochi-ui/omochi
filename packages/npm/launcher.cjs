#!/usr/bin/env node
'use strict';
const path = require('node:path');
const { spawn } = require('node:child_process');
const manifest = require('../package.json');
const platform = `${process.platform}-${process.arch}`;
const dependency = manifest.nativePlatforms[platform];
function fail(message) { console.error(`[${manifest.name}] ${message}`); process.exit(1); }
if (!dependency) fail(`Unsupported platform ${platform}. Use a supported Linux/glibc, macOS or Windows build.`);
if (process.platform === 'linux' && !process.report?.getReport().header.glibcVersionRuntime) {
  fail('The prebuilt Linux binary requires glibc. On musl, build with cargo install instead.');
}
let binary;
try {
  const installed = require.resolve(`${dependency}/package.json`);
  const native = require(installed);
  if (native.version !== manifest.version) fail(`Binary version mismatch: expected ${manifest.version}, found ${native.version}. Reinstall the package.`);
  binary = path.join(path.dirname(installed), 'bin', manifest.nativeBinary + (process.platform === 'win32' ? '.exe' : ''));
} catch {
  fail(`Missing ${dependency}@${manifest.version}. Reinstall with optional dependencies enabled (npm install --include=optional).`);
}
const child = spawn(binary, process.argv.slice(2), { stdio: 'inherit', env: { ...process.env, ...(process.platform === 'win32' ? { OMOCHI_LAUNCHER_PID: String(process.pid) } : {}) } });
const handlers = new Map();
// Console events already reach the native child on Windows. Keep the launcher
// alive while the child handles them, rather than turning them into TerminateProcess.
if (process.platform === 'win32') {
  for (const signal of ['SIGINT', 'SIGBREAK']) {
    const handler = () => {};
    handlers.set(signal, handler);
    process.on(signal, handler);
  }
}
for (const signal of (process.platform === 'win32' ? [] : ['SIGINT', 'SIGTERM', 'SIGHUP'])) {
  const handler = () => child.kill(signal);
  handlers.set(signal, handler);
  process.on(signal, handler);
}
child.once('error', error => fail(`Cannot start ${binary}: ${error.message}`));
child.once('exit', (code, signal) => {
  for (const [name, handler] of handlers) process.removeListener(name, handler);
  if (signal && process.platform !== 'win32') process.kill(process.pid, signal);
  else process.exitCode = code ?? 1;
});
