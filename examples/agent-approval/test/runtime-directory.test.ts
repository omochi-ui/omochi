import { expect } from 'bun:test';
import { test, type TestContext } from '@omochi-ui/core/testing';
import {
  chmod,
  lstat,
  mkdir,
  mkdtemp,
  readdir,
  rm,
  symlink,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { approvalSocketPath } from '../protocol.ts';

const childScript = `
  import { stat } from "node:fs/promises";
  import { startApprovalApp } from ${JSON.stringify(new URL('../app.ts', import.meta.url).href)};
  import { approvalSocketPath } from ${JSON.stringify(new URL('../protocol.ts', import.meta.url).href)};
  try {
    const app = await startApprovalApp({ runtimeDir: process.env.APP_CUSTOM_RUNTIME, serviceId: "agent-approval" });
    try {
      if (process.env.APP_CUSTOM_RUNTIME === undefined && approvalSocketPath() !== app.socketPath) {
        throw new Error("Hook socket does not match app socket: " + approvalSocketPath() + " != " + app.socketPath);
      }
      console.log(JSON.stringify({
        directory: app.transport.directory,
        approvalSocket: (await stat(app.socketPath)).isSocket(),
        transportSocket: (await stat(app.transport.directory + "/a2ui.sock")).isSocket(),
      }));
    } finally { await app.close(); }
  } catch (error) {
    console.error(String(error));
    process.exitCode = 1;
  }
`;

async function sandboxFor(t: TestContext) {
  const sandbox = await mkdtemp(path.join(tmpdir(), 'aa-'));
  t.after(() => rm(sandbox, { recursive: true, force: true }));
  return sandbox;
}

async function startInChild(
  t: TestContext,
  sandbox: string,
  runtimeDir?: string,
  runtimeOverride?: string,
) {
  // Only the subprocess resolves this default: do not mutate the test runner's
  // environment or touch the user's real predictable fallback directory.
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    TMPDIR: sandbox,
    TMP: sandbox,
    TEMP: sandbox,
    XDG_RUNTIME_DIR: '',
  };
  delete env.APP_CUSTOM_RUNTIME;
  delete env.OMOCHI_APPROVAL_SOCKET;
  delete env.OMOCHI_RUNTIME_DIR;
  if (runtimeDir !== undefined) env.APP_CUSTOM_RUNTIME = runtimeDir;
  if (runtimeOverride !== undefined) env.OMOCHI_RUNTIME_DIR = runtimeOverride;
  const child = Bun.spawn([process.execPath, '-e', childScript], {
    env,
    stdout: 'pipe',
    stderr: 'pipe',
  });
  t.after(async () => {
    child.kill();
    await child.exited;
  });
  const [exitCode, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  return { exitCode, stdout, stderr };
}

for (const mode of [0o770, 0o777]) {
  test(`default approval runtime rejects a fallback parent with mode ${mode.toString(8)}`, async (t) => {
    const sandbox = await sandboxFor(t);
    const parent = path.join(sandbox, `omochi-${process.geteuid!()}`);
    await mkdir(parent);
    await chmod(parent, mode);
    const result = await startInChild(t, sandbox);
    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain('fallback runtime directory is unsafe');
    expect((await lstat(parent)).mode & 0o777).toBe(mode);
    expect(await readdir(parent)).toEqual([]);
  });
}

test('default approval runtime rejects a symlink fallback parent without touching its target', async (t) => {
  const sandbox = await sandboxFor(t);
  const target = path.join(sandbox, 'outside');
  await mkdir(target);
  await chmod(target, 0o755);
  const parent = path.join(sandbox, `omochi-${process.geteuid!()}`);
  await symlink(target, parent);
  const result = await startInChild(t, sandbox);
  expect(result.exitCode).toBe(1);
  expect(result.stderr).toContain('fallback runtime directory is unsafe');
  expect((await lstat(parent)).isSymbolicLink()).toBe(true);
  expect((await lstat(target)).mode & 0o777).toBe(0o755);
  expect(await readdir(target)).toEqual([]);
});

for (const runtimeOverride of [undefined, '']) {
  test(`default approval runtime with ${runtimeOverride === '' ? 'empty' : 'unset'} override creates a private fallback and matching hook socket`, async (t) => {
    const sandbox = await sandboxFor(t);
    const parent = path.join(sandbox, `omochi-${process.geteuid!()}`);
    const root = path.join(parent, 'v2');
    const result = await startInChild(t, sandbox, undefined, runtimeOverride);
    expect(result.stderr).toBe('');
    expect(result.exitCode).toBe(0);
    expect(JSON.parse(result.stdout)).toEqual({
      directory: path.join(root, 'svc-agent-approval~'),
      approvalSocket: true,
      transportSocket: true,
    });
    for (const directory of [parent, root]) {
      const metadata = await lstat(directory);
      expect(metadata.uid).toBe(process.geteuid!());
      expect(metadata.mode & 0o777).toBe(0o700);
    }
  });
}

test('approval hook preserves nonempty runtime and explicit socket overrides', () => {
  expect(approvalSocketPath({ OMOCHI_RUNTIME_DIR: 'custom' })).toBe(
    path.join('custom', 'svc-agent-approval~', 'approval.sock'),
  );
  expect(
    approvalSocketPath({
      OMOCHI_RUNTIME_DIR: '',
      OMOCHI_APPROVAL_SOCKET: 'explicit.sock',
    }),
  ).toBe('explicit.sock');
  expect(approvalSocketPath({ OMOCHI_APPROVAL_SOCKET: '' })).toBe('');
});

test('explicit approval runtime still creates and secures a custom directory', async (t) => {
  const sandbox = await sandboxFor(t);
  const root = path.join(sandbox, 'custom');
  const result = await startInChild(t, sandbox, root);
  expect(result.stderr).toBe('');
  expect(result.exitCode).toBe(0);
  expect(JSON.parse(result.stdout)).toEqual({
    directory: path.join(root, 'svc-agent-approval~'),
    approvalSocket: true,
    transportSocket: true,
  });
  expect((await lstat(root)).mode & 0o777).toBe(0o700);
  expect(await readdir(sandbox)).toEqual(['custom']);
});
