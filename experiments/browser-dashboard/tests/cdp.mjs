import { spawn } from 'node:child_process';
import { mkdir, readFile } from 'node:fs/promises';
import { once } from 'node:events';

/** Real Chromium, with an isolated private profile supplied by the caller. */
export async function chromium(directory, extraArgs = []) {
  await mkdir(directory, { recursive: true, mode: 0o700 });
  const child = spawn(process.env.CHROMIUM_BIN ?? 'chromium', [
    '--headless', '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage',
    '--no-first-run', '--no-default-browser-check', '--remote-debugging-port=0',
    `--user-data-dir=${directory}`, ...extraArgs, 'about:blank',
  ], { stdio: ['ignore', 'ignore', 'pipe'] });
  let endpoint = '';
  let startupError;
  const announced = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Chromium startup timed out; set CHROMIUM_BIN to Chromium 156+.')), 15000);
    child.once('error', error => { clearTimeout(timer); startupError = error; reject(new Error('Chromium unavailable; set CHROMIUM_BIN.')); });
    child.once('exit', () => { clearTimeout(timer); if (!endpoint) reject(new Error('Chromium exited before startup.')); });
    child.stderr.on('data', chunk => {
      const match = chunk.toString().match(/DevTools listening on (ws:\/\/\S+)/);
      if (match && !endpoint) { endpoint = match[1]; clearTimeout(timer); resolve(endpoint); }
    });
  });
  let socket;
  let sequence = 0;
  let ended = false;
  const pending = new Map();
  const observers = new Set();
  async function close() {
    if (ended) return;
    ended = true;
    socket?.close();
    for (const request of pending.values()) { clearTimeout(request.timer); request.reject(new Error('Browser closed')); }
    pending.clear();
    if (child.exitCode === null && child.signalCode === null && !startupError) {
      const exit = once(child, 'exit').catch(() => {});
      child.kill('SIGTERM');
      const kill = setTimeout(() => child.kill('SIGKILL'), 2000);
      try { await exit; } finally { clearTimeout(kill); }
    }
  }
  try {
    const address = new URL(await announced);
    const targets = await (await fetch(`http://${address.host}/json/list`)).json();
    socket = new WebSocket(targets.find(target => target.type === 'page').webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      socket.addEventListener('open', resolve, { once: true });
      socket.addEventListener('error', () => reject(new Error('Chromium control connection failed')), { once: true });
    });
    socket.addEventListener('message', event => {
      const message = JSON.parse(event.data);
      if (message.id) {
        const request = pending.get(message.id);
        if (!request) return;
        pending.delete(message.id); clearTimeout(request.timer);
        if (message.error) request.reject(new Error('Chromium rejected a control command'));
        else request.resolve(message.result);
      } else {
        for (const observer of observers) { try { observer(message); } catch { /* Independent observer. */ } }
      }
    });
    function command(method, params = {}, timeout = 30000, sessionId) {
      if (ended) return Promise.reject(new Error('Browser closed'));
      const id = ++sequence;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => { pending.delete(id); reject(new Error(`Chromium command timed out: ${method}`)); }, timeout);
        pending.set(id, { resolve, reject, timer });
        socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
      });
    }
    return {
      command, close,
      async memoryUsage() {
        // Diagnostic Linux RSS, separate from the protocol's encoded-byte accounting.
        let processes = 0, residentBytes = 0;
        async function visit(pid) {
          try {
            const status = await readFile(`/proc/${pid}/status`, 'utf8');
            residentBytes += Number(status.match(/^VmRSS:\s+(\d+) kB$/m)?.[1] ?? 0) * 1024;
            processes++;
            const children = await readFile(`/proc/${pid}/task/${pid}/children`, 'utf8');
            for (const value of children.trim().split(/\s+/).filter(Boolean)) await visit(Number(value));
          } catch { /* Processes can exit during a diagnostic sample. */ }
        }
        await visit(child.pid);
        return { processes, residentBytes };
      },
      async tab(url = 'about:blank') {
        const { targetId } = await command('Target.createTarget', { url });
        const { sessionId } = await command('Target.attachToTarget', { targetId, flatten: true });
        const tabCommand = (method, params, timeout) => command(method, params, timeout, sessionId);
        return { command: tabCommand,
          close: () => command('Target.closeTarget', { targetId }),
          async evaluate(expression, timeout = 30000) {
            const result = await tabCommand('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, timeout);
            if (result.exceptionDetails) throw new Error(`Browser evaluation failed at line ${result.exceptionDetails.lineNumber}`);
            return result.result.value;
          },
        };
      },
      onEvent(observer) { observers.add(observer); return () => observers.delete(observer); },
      async evaluate(expression, timeout = 30000) {
        const result = await command('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, timeout);
        // Don't copy arbitrary page exceptions (which may contain app data) into logs.
        if (result.exceptionDetails) throw new Error(`Browser evaluation failed at line ${result.exceptionDetails.lineNumber}`);
        return result.result.value;
      },
    };
  } catch (error) { await close(); throw error; }
}
