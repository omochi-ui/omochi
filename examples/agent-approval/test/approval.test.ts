import { expect, jest } from 'bun:test';
import { test, waitFor, type TestContext } from '@omochi-ui/core/testing';
import { mkdtemp, rm, stat, readFile } from 'node:fs/promises';
import { createConnection, createServer } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { startApprovalApp } from '../app.ts';
import {
  createOpenCodeHook,
  hookOutput,
  normalizeHook,
  type CommandAgent,
} from '../adapters.ts';
import { requestApproval } from '../client.ts';
import { ApprovalQueue } from '../queue.ts';
import {
  DEFAULT_REJECTION,
  MAX_FRAME_BYTES,
  parseRequest,
  type ApprovalRequest,
} from '../protocol.ts';
import { codeBlock } from '../ui.ts';
import openCodePlugin from '../opencode.ts';

const request: ApprovalRequest = {
  agent: 'codex',
  sessionId: 'session-1',
  toolCallId: 'call-1',
  cwd: '/project',
  tool: 'Bash',
  input: { command: 'git status --short' },
};

async function appFor(t: TestContext, waitMs?: number) {
  const root = await mkdtemp(path.join(tmpdir(), 'approval-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const app = await startApprovalApp({
    runtimeDir: root,
    serviceId: 'agent-approval',
    waitMs,
  });
  t.after(() => app.close());
  return app;
}

function observeQueue(t: TestContext, queue: ApprovalQueue) {
  const previous = queue.onChange;
  const listeners = new Set<() => void>();
  queue.onChange = () => {
    previous();
    for (const listener of [...listeners]) listener();
  };
  t.after(() => {
    queue.onChange = previous;
  });
  return (condition: () => boolean) =>
    new Promise<void>((resolve) => {
      const check = () => {
        if (!condition()) return;
        listeners.delete(check);
        resolve();
      };
      listeners.add(check);
      check();
    });
}

function useFakeClock(t: TestContext) {
  jest.useFakeTimers();
  t.after(() => {
    jest.useRealTimers();
  });
}

test('concurrent calls have independent IDs, one decision each, and bounded history', async (t) => {
  const queue = new ApprovalQueue();
  t.after(() => queue.close());
  const first = queue.submit(request);
  const second = queue.submit(request);
  expect(first.id).not.toBe(second.id);
  expect(queue.decide(second.id, 'reject', '  Use the feature branch.  ')).toBe(
    true,
  );
  expect(queue.decide(second.id, 'approve')).toBe(false);
  expect(queue.pending.has(first.id)).toBe(true);
  expect(await second.result).toEqual({
    requestId: second.id,
    outcome: 'reject',
    reason: 'Use the feature branch.',
  });
  queue.decide(first.id, 'approve', 'This note must not be sent.');
  expect(await first.result).toEqual({
    requestId: first.id,
    outcome: 'approve',
  });
  for (let index = 0; index < 105; index++) {
    const next = queue.submit(request);
    queue.decide(next.id, 'reject');
    await next.result;
  }
  expect(queue.history).toHaveLength(100);
  expect(queue.history[0]!.reason).toBe(DEFAULT_REJECTION);
});

test('timeout, cancellation, and shutdown abstain and disable late decisions', async (t) => {
  const queue = new ApprovalQueue(20);
  t.after(() => queue.close());
  useFakeClock(t);
  const timed = queue.submit(request);
  jest.advanceTimersByTime(19);
  expect(queue.pending.has(timed.id)).toBe(true);
  jest.advanceTimersByTime(1);
  expect((await timed.result).outcome).toBe('abstain');
  expect(queue.decide(timed.id, 'approve')).toBe(false);
  const cancelled = queue.submit(request);
  queue.cancel(cancelled.id);
  expect((await cancelled.result).reason).toBe('Hook disconnected.');
  expect(queue.decide(cancelled.id, 'reject')).toBe(false);
  const stopped = queue.submit(request);
  queue.close();
  expect((await stopped.result).reason).toBe('Service stopped.');
  expect((await queue.submit(request).result).outcome).toBe('abstain');
});

test('an expired request cannot be approved before its timer callback runs', async (t) => {
  const queue = new ApprovalQueue(0);
  t.after(() => queue.close());
  const pending = queue.submit(request);
  expect(queue.decide(pending.id, 'approve')).toBe(false);
  expect((await pending.result).outcome).toBe('abstain');
});

for (const agent of ['codex', 'claude-code', 'cursor'] as CommandAgent[]) {
  test(`${agent} normalizes tool input and emits approval, rejection, and abstention`, () => {
    const payload = {
      hook_event_name: agent === 'cursor' ? 'preToolUse' : 'PreToolUse',
      session_id: 'session-1',
      tool_use_id: 'call-1',
      cwd: '/project',
      tool_name: 'Bash',
      tool_input: request.input,
    };
    expect(normalizeHook(agent, payload)).toEqual({ ...request, agent });
    expect(hookOutput(agent, { outcome: 'abstain' })).toBe('');
    const approve = JSON.parse(hookOutput(agent, { outcome: 'approve' }));
    const reject = JSON.parse(
      hookOutput(agent, {
        outcome: 'reject',
        reason: '変更しないでください\nUse --dry-run.',
      }),
    );
    if (agent === 'cursor') {
      expect(approve).toEqual({ permission: 'allow' });
      expect(reject).toEqual({
        permission: 'deny',
        user_message: '変更しないでください\nUse --dry-run.',
        agent_message: '変更しないでください\nUse --dry-run.',
      });
    } else {
      expect(approve).toEqual({
        hookSpecificOutput: {
          hookEventName: 'PreToolUse',
          permissionDecision: 'allow',
        },
      });
      expect(reject).toEqual({
        hookSpecificOutput: {
          hookEventName: 'PreToolUse',
          permissionDecision: 'deny',
          permissionDecisionReason: '変更しないでください\nUse --dry-run.',
        },
      });
    }
    expect(() =>
      normalizeHook(agent, { ...payload, hook_event_name: 'PostToolUse' }),
    ).toThrow();
  });
}

test('Cursor falls back to conversation and workspace metadata', () => {
  expect(
    normalizeHook('cursor', {
      hook_event_name: 'preToolUse',
      conversation_id: 'conversation',
      workspace_roots: ['/workspace'],
      tool_name: 'Read',
      tool_input: { path: 'README.md' },
    }),
  ).toEqual({
    agent: 'cursor',
    sessionId: 'conversation',
    cwd: '/workspace',
    tool: 'Read',
    input: { path: 'README.md' },
  });
});

test('browser actions cross A2UI and resolve only the selected hook socket', async (t) => {
  const app = await appFor(t);
  expect((await stat(app.socketPath)).mode & 0o777).toBe(0o600);
  const first = requestApproval(request, { socketPath: app.socketPath });
  const second = requestApproval(
    { ...request, agent: 'cursor' },
    { socketPath: app.socketPath },
  );
  await waitFor(() => app.queue.pending.size === 2);
  const [firstId, secondId] = [...app.queue.pending.keys()];
  const browser = createConnection(app.transport.socketPath!);
  t.after(
    () =>
      new Promise<void>((resolve) => {
        browser.once('close', resolve);
        browser.end();
      }),
  );
  let snapshot = '';
  browser.on('data', (chunk) => {
    snapshot += chunk.toString();
  });
  browser.on('error', () => {});
  await waitFor(
    () => snapshot.includes('Approve') && snapshot.includes('git status'),
  );
  const invoke = (id: string, outcome: string, note?: string) =>
    browser.write(
      `${JSON.stringify({
        version: 'v0.9',
        action: {
          name: 'decide',
          surfaceId: 'main',
          sourceComponentId: 'root',
          timestamp: new Date().toISOString(),
          context: { input: { id, outcome, note } },
        },
      })}\n`,
    );
  invoke(secondId!, 'invalid');
  invoke(secondId!, 'reject', 'Please run tests first.');
  expect(await second).toEqual({
    requestId: secondId,
    outcome: 'reject',
    reason: 'Please run tests first.',
  });
  invoke(secondId!, 'approve');
  expect(app.queue.pending.has(firstId!)).toBe(true);
  invoke(firstId!, 'approve');
  expect(await first).toEqual({ requestId: firstId, outcome: 'approve' });
  await waitFor(() => snapshot.includes('Recent decisions'));
  expect(app.queue.history).toHaveLength(2);
});

test('client timeout removes the accepted request and disables late decisions', async (t) => {
  const app = await appFor(t);
  const changed = observeQueue(t, app.queue);
  useFakeClock(t);
  const accepted = changed(() => app.queue.pending.size === 1);
  const timed = requestApproval(request, {
    socketPath: app.socketPath,
    timeoutMs: 1000,
  });
  await accepted;
  const id = [...app.queue.pending.keys()][0]!;
  const removed = changed(() => app.queue.pending.size === 0);
  jest.advanceTimersByTime(999);
  expect(await Promise.race([timed, Promise.resolve('pending')])).toBe(
    'pending',
  );
  expect(app.queue.pending.has(id)).toBe(true);
  jest.advanceTimersByTime(1);
  expect((await timed).reason).toBe('Approval timed out.');
  await removed;
  expect(app.queue.history[0]!.reason).toBe('Hook disconnected.');
  await app.runtime.invoke('decide', { id, outcome: 'approve' });
  expect(app.queue.history[0]!.outcome).toBe('abstain');
});

test('client interruption removes pending actions; malformed actions do not kill the app', async (t) => {
  const app = await appFor(t);
  const changed = observeQueue(t, app.queue);
  await expect(app.runtime.invoke('decide', null)).rejects.toThrow(
    'expected object',
  );
  await expect(
    app.runtime.invoke('decide', { id: 'missing', outcome: 'invalid' }),
  ).rejects.toThrow('union');
  const controller = new AbortController();
  const accepted = changed(() => app.queue.pending.size === 1);
  const aborted = requestApproval(request, {
    socketPath: app.socketPath,
    signal: controller.signal,
  });
  await accepted;
  const removed = changed(() => app.queue.pending.size === 0);
  controller.abort();
  expect((await aborted).reason).toBe('Hook cancelled.');
  await removed;
  expect(app.queue.history.every((item) => item.outcome === 'abstain')).toBe(
    true,
  );
});

test('service deadline releases an accepted hook connection with abstention', async (t) => {
  const app = await appFor(t, 1000);
  const changed = observeQueue(t, app.queue);
  useFakeClock(t);
  const accepted = changed(() => app.queue.pending.size === 1);
  const pending = requestApproval(request, { socketPath: app.socketPath });
  await accepted;
  jest.advanceTimersByTime(999);
  expect(app.queue.pending.size).toBe(1);
  jest.advanceTimersByTime(1);
  expect((await pending).reason).toBe('Approval timed out.');
  expect(app.queue.pending.size).toBe(0);
  expect(app.queue.history[0]!.reason).toBe('Approval timed out.');
});

test('shutdown releases accepted hook connections with a service-stopped response', async (t) => {
  const app = await appFor(t);
  const changed = observeQueue(t, app.queue);
  const accepted = changed(() => app.queue.pending.size === 1);
  const pending = requestApproval(request, { socketPath: app.socketPath });
  await accepted;
  await app.close();
  expect((await pending).reason).toBe('Service stopped.');
  expect(app.queue.pending.size).toBe(0);
  expect(
    (await requestApproval(request, { socketPath: app.socketPath })).outcome,
  ).toBe('abstain');
});

test('malformed and oversized requests abstain without entering the queue', async (t) => {
  const app = await appFor(t);
  expect(() => parseRequest({ ...request, agent: 'unknown' })).toThrow();
  expect(
    (
      await requestApproval(
        { ...request, input: 'x'.repeat(MAX_FRAME_BYTES) },
        { socketPath: app.socketPath },
      )
    ).outcome,
  ).toBe('abstain');
  const response = await new Promise<string>((resolve) => {
    const socket = createConnection(app.socketPath);
    t.after(() => socket.destroy());
    socket.once('connect', () => socket.write('{broken}\n'));
    socket.once('data', (chunk) => resolve(chunk.toString()));
  });
  expect(JSON.parse(response).outcome).toBe('abstain');
  expect(app.queue.pending.size).toBe(0);
});

test('client treats malformed responses and disconnects as abstention', async (t) => {
  const directory = await mkdtemp(path.join(tmpdir(), 'approval-wire-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const socketPath = path.join(directory, 'test.sock');
  let response = '{"outcome":"allow"}\n';
  const server = createServer((socket) =>
    socket.once('data', () => socket.end(response)),
  );
  t.after(() => new Promise<void>((resolve) => server.close(() => resolve())));
  await new Promise<void>((resolve) => server.listen(socketPath, resolve));
  expect((await requestApproval(request, { socketPath })).reason).toBe(
    'Invalid approval response.',
  );
  response = '';
  expect((await requestApproval(request, { socketPath })).outcome).toBe(
    'abstain',
  );
});

test('OpenCode waits, preserves args, throws rejection notes, and abstains when unavailable', async (t) => {
  const app = await appFor(t);
  const hook = createOpenCodeHook('/project', { socketPath: app.socketPath });
  const input = { tool: 'bash', sessionID: 'session', callID: 'call' };
  const output = { args: { command: 'git status' } };
  const approved = hook(input, output);
  await waitFor(() => app.queue.pending.size === 1);
  app.queue.decide([...app.queue.pending.keys()][0]!, 'approve');
  await approved;
  expect(output).toEqual({ args: { command: 'git status' } });
  // Attach the rejection handler before the UI resolves the request.
  const rejected = hook(input, output).then(
    () => null,
    (error: Error) => error.message,
  );
  await waitFor(() => app.queue.pending.size === 1);
  app.queue.decide(
    [...app.queue.pending.keys()][0]!,
    'reject',
    'Use a temporary directory.',
  );
  expect(await rejected).toBe('Use a temporary directory.');
  await app.close();
  await hook(input, output);
  expect(Object.keys(await openCodePlugin({ directory: '/project' }))).toEqual([
    'tool.execute.before',
  ]);
});

test('real hook processes emit only agent JSON and fail open with empty stdout', async (t) => {
  const app = await appFor(t);
  const executable = fileURLToPath(new URL('../hook.ts', import.meta.url));
  for (const agent of ['codex', 'claude-code', 'cursor'] as CommandAgent[]) {
    const child = Bun.spawn([process.execPath, executable, agent], {
      env: { ...process.env, OMOCHI_APPROVAL_SOCKET: app.socketPath },
      stdin: new Blob([
        JSON.stringify({
          hook_event_name: agent === 'cursor' ? 'preToolUse' : 'PreToolUse',
          tool_name: 'Bash',
          tool_input: request.input,
          cwd: '/project',
          session_id: agent,
        }),
      ]),
      stdout: 'pipe',
      stderr: 'pipe',
    });
    t.after(() => {
      child.kill();
    });
    await waitFor(() => app.queue.pending.size === 1);
    const id = [...app.queue.pending.keys()][0]!;
    app.queue.decide(id, 'reject', 'Try --dry-run.');
    expect(await new Response(child.stdout).text()).toBe(
      hookOutput(agent, { outcome: 'reject', reason: 'Try --dry-run.' }),
    );
    expect(await child.exited).toBe(0);
    expect(await new Response(child.stderr).text()).toBe('');
  }
  await app.close();
  for (const agent of ['codex', 'claude-code', 'cursor']) {
    for (const payload of [
      'invalid json',
      JSON.stringify({
        hook_event_name: agent === 'cursor' ? 'preToolUse' : 'PreToolUse',
        tool_name: 'Read',
        tool_input: {},
      }),
    ]) {
      const child = Bun.spawn([process.execPath, executable, agent], {
        env: { ...process.env, OMOCHI_APPROVAL_SOCKET: app.socketPath },
        stdin: new Blob([payload]),
        stdout: 'pipe',
        stderr: 'pipe',
      });
      expect(await new Response(child.stdout).text()).toBe('');
      expect(await child.exited).toBe(0);
      expect(await new Response(child.stderr).text()).toContain(
        'normal permissions',
      );
    }
  }
});

test('code fences contain tool input even when it includes Markdown fences', () => {
  const input = { content: '```\n![untrusted](https://example.com/image)' };
  const rendered = codeBlock(input);
  expect(rendered.startsWith('````json\n')).toBe(true);
  expect(rendered.endsWith('\n````')).toBe(true);
});

test('settings use synchronous pre-execution hooks with a deadline longer than review', async () => {
  for (const agent of ['codex', 'claude-code', 'cursor']) {
    const config = JSON.parse(
      await readFile(
        new URL(`../config/${agent}.json`, import.meta.url),
        'utf8',
      ),
    );
    const entry =
      agent === 'cursor'
        ? config.hooks.preToolUse[0]
        : config.hooks.PreToolUse[0].hooks[0];
    expect(entry.timeout).toBe(360);
    expect(entry.async).not.toBe(true);
    expect(entry.command).toContain(`hook.ts\" ${agent}`);
    if (agent === 'cursor') expect(entry.failClosed).toBe(false);
  }
});
