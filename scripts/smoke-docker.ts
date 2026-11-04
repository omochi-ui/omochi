import assert from 'node:assert/strict';

// Uses an isolated Compose project; only its own volumes are removed on exit.
const project = `omochi-smoke-${process.pid}`;
const env = { ...process.env, OMOCHI_VERSION: 'dev', OMOCHI_PORT: process.env.OMOCHI_PORT ?? '15555' };
const base = `http://127.0.0.1:${env.OMOCHI_PORT}`;
const files = ['-f', 'deploy/docker/compose.yaml', '-f', 'deploy/docker/compose.build.yaml'];
async function compose(...args: string[]) {
  const child = Bun.spawn(['docker', 'compose', '-p', project, ...files, ...args], { env, stdout: 'pipe', stderr: 'inherit' });
  const output = await new Response(child.stdout).text();
  assert.equal(await child.exited, 0, `compose ${args.join(' ')} failed`);
  return output.trim();
}
async function wait(check: () => Promise<boolean> | boolean) {
  const deadline = Date.now() + 60000;
  while (true) {
    try { if (await check()) return; } catch {}
    assert.ok(Date.now() < deadline, 'timed out waiting for Docker service');
    await Bun.sleep(200);
  }
}
let temperature = 70;
async function roundTrip() {
  temperature += 1;
  await wait(async () => (await fetch(`${base}/services`)).ok);
  const messages: any[] = [];
  const socket = new WebSocket(`${base.replace('http:', 'ws:')}/ws`);
  socket.onmessage = event => messages.push(JSON.parse(String(event.data)));
  try {
    await wait(() => messages.some(m => m.createSurface?.surfaceId.endsWith('~python-example/main')));
    const surfaceId = messages.find(m => m.createSurface).createSurface.surfaceId;
    await wait(() => JSON.stringify(messages).includes('Hello'));
    socket.send(JSON.stringify({ version: 'v0.9', action: { surfaceId, sourceComponentId: 'root', timestamp: new Date().toISOString(), name: 'run', context: { input: { temperature } } } }));
    await wait(() => JSON.stringify(messages).includes(`Ran at temperature ${temperature}`));
  } finally { socket.close(); }
}
try {
  await compose('up', '-d', '--no-build', '--pull', 'never', '--wait');
  const cargo = Bun.TOML.parse(await Bun.file('crates/Cargo.toml').text()) as { workspace: { package: { version: string } } };
  for (const binary of ['omochi-ui', 'omochi-utils']) {
    assert.equal(await compose('exec', '-T', 'hub', binary, '--version'), `${binary} ${cargo.workspace.package.version}`);
  }
  const page = await fetch(base);
  assert.ok((await page.text()).includes('<html'), 'embedded dashboard missing');
  await roundTrip();
  assert.equal((await fetch(`${base}/services`, { headers: { Origin: 'https://attacker.example' } })).status, 403);
  assert.equal((await fetch(`${base}/ws`, { headers: {
    Origin: 'https://attacker.example', Connection: 'Upgrade', Upgrade: 'websocket',
    'Sec-WebSocket-Version': '13', 'Sec-WebSocket-Key': 'dGhlIHNhbXBsZSBub25jZQ==',
  } })).status, 403);
  for (const service of ['hub', 'python']) {
    assert.equal(await compose('exec', '-T', service, 'id', '-u'), '10001');
    assert.equal(await compose('exec', '-T', service, 'stat', '-c', '%u:%g:%a', '/run/omochi', '/run/omochi/python-example'), '10001:10001:700\n10001:10001:700');
  }
  assert.ok((await compose('port', 'hub', '5555')).startsWith('127.0.0.1:'));
  const identity = await compose('exec', '-T', 'hub', 'omochi-ui', 'id', '--hub');
  const response = await fetch(`${base}/api/dashboards`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name: 'Docker persistence', panels: [] }) });
  assert.equal(response.status, 201);
  const board = await response.json() as { id: string };
  await compose('stop', 'python');
  await compose('start', 'python');
  await roundTrip();
  await compose('restart', 'hub');
  await roundTrip();
  await compose('kill', '-s', 'SIGKILL', 'hub', 'python');
  await compose('up', '-d', '--no-build', '--pull', 'never', '--wait');
  await roundTrip();
  assert.equal(await compose('exec', '-T', 'hub', 'omochi-ui', 'id', '--hub'), identity);
  const saved = await fetch(`${base}/api/dashboards/${board.id}`);
  assert.equal(saved.status, 200);
  assert.equal((await saved.json() as { name: string }).name, 'Docker persistence');
  console.log('Docker smoke passed: UI assets, actions, permissions, restart and persistence');
} finally {
  await compose('logs', '--no-color').then(console.log);
  await compose('down', '--volumes', '--remove-orphans');
}
