import { spawn } from 'node:child_process';
import { createHash, X509Certificate } from 'node:crypto';
import { readFile, writeFile, chmod } from 'node:fs/promises';
import { once } from 'node:events';
import path from 'node:path';

async function run(command, args) {
  const child = spawn(command, args, { stdio: 'ignore' });
  const code = await new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', resolve);
  });
  if (code !== 0) throw new Error(`Fixture setup command failed: ${command}`);
}
export async function certificate(directory) {
  const cert = path.join(directory, 'cert.pem'),
    key = path.join(directory, 'key.pem');
  const ca = path.join(directory, 'ca.pem'),
    caKey = path.join(directory, 'ca-key.pem');
  const csr = path.join(directory, 'leaf.csr'),
    extensions = path.join(directory, 'leaf.ext');
  await run('openssl', [
    'req',
    '-x509',
    '-newkey',
    'rsa:2048',
    '-nodes',
    '-keyout',
    caKey,
    '-out',
    ca,
    '-days',
    '1',
    '-subj',
    '/CN=Omochi fixture CA',
    '-addext',
    'basicConstraints=critical,CA:TRUE',
    '-addext',
    'keyUsage=critical,keyCertSign,cRLSign',
  ]);
  await run('openssl', [
    'req',
    '-new',
    '-newkey',
    'rsa:2048',
    '-nodes',
    '-keyout',
    key,
    '-out',
    csr,
    '-subj',
    '/CN=localhost',
  ]);
  await writeFile(
    extensions,
    'basicConstraints=critical,CA:FALSE\nsubjectAltName=DNS:localhost,IP:127.0.0.1\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n',
  );
  await run('openssl', [
    'x509',
    '-req',
    '-in',
    csr,
    '-CA',
    ca,
    '-CAkey',
    caKey,
    '-CAcreateserial',
    '-out',
    cert,
    '-days',
    '1',
    '-extfile',
    extensions,
  ]);
  await chmod(key, 0o600);
  await chmod(caKey, 0o600);
  const certificate = new X509Certificate(await readFile(cert));
  const spki = createHash('sha256')
    .update(certificate.publicKey.export({ type: 'spki', format: 'der' }))
    .digest('base64');
  return { cert, key, ca, spki };
}
export async function gatewayFixture(
  directory,
  tls,
  publicRelay = false,
  media = false,
  load = false,
  faults = false,
  sdkGate = false,
) {
  const binary = path.resolve('crates/target/debug/examples/browser_fixture');
  if (!(await Bun.file(binary).exists()))
    throw new Error(
      'Missing browser_fixture. Run cargo build --manifest-path crates/Cargo.toml --locked -p omochi-gateway --example browser_fixture.',
    );
  const child = spawn(
    binary,
    [
      ...(publicRelay
        ? ['--public-relay']
        : ['--cert', tls.cert, '--key', tls.key, '--ca', tls.ca]),
      ...(media ? ['--media'] : []),
      ...(load ? ['--load'] : []),
      ...(faults ? ['--faults'] : []),
      ...(sdkGate ? ['--sdk-gate'] : []),
    ],
    {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, BROWSER_HUB_TMPDIR: directory },
    },
  );
  const messages = [];
  let ready;
  let failure = false;
  let buffer = '';
  child.once('error', () => {
    failure = true;
  });
  child.stderr.on('data', () => {
    /* Native details may contain addresses; only fixed diagnostics leave the harness. */
  });
  child.stdout.on('data', (data) => {
    buffer += data;
    if (buffer.length > 128 * 1024) {
      failure = true;
      return;
    }
    let end;
    while ((end = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, end);
      buffer = buffer.slice(end + 1);
      try {
        const message = JSON.parse(line);
        if (message.kind === 'ready') ready = message;
        else messages.push(message);
      } catch {
        failure = true;
      }
    }
  });
  async function until(predicate, label, timeout = 60000) {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      if (failure || child.exitCode !== null || child.signalCode !== null)
        throw new Error(`Gateway fixture stopped: ${label}`);
      if (await predicate()) return;
      await Bun.sleep(20);
    }
    throw new Error(`Gateway fixture timed out: ${label}`);
  }
  async function close() {
    if (child.exitCode !== null || child.signalCode !== null) return;
    child.stdin.write('{"op":"stop"}\n');
    const exit = once(child, 'exit').catch(() => {});
    const terminate = setTimeout(() => child.kill('SIGTERM'), 1500);
    const kill = setTimeout(() => child.kill('SIGKILL'), 3500);
    try {
      await exit;
    } finally {
      clearTimeout(terminate);
      clearTimeout(kill);
    }
  }
  try {
    await until(() => !!ready, 'TLS relay and two SDK gateways');
  } catch (error) {
    await close();
    throw error;
  }
  return {
    ready,
    messages,
    close,
    until,
    send: (value) => child.stdin.write(`${JSON.stringify(value)}\n`),
  };
}

export async function nativeBoardHub(directory) {
  const child = spawn(
    path.resolve('crates/target/debug/omochi-ui'),
    [
      '--config-dir',
      path.join(directory, 'native-state'),
      '--runtime-dir',
      path.join(directory, 'native-run'),
      'hub',
      '--offline',
      '--port',
      '0',
      '--assets',
      path.resolve('packages/hub-ui/dist'),
    ],
    { stdio: ['ignore', 'ignore', 'pipe'] },
  );
  let url = '',
    failed = false;
  child.once('error', () => {
    failed = true;
  });
  child.stderr.on('data', (value) => {
    const match = String(value).match(
      /dashboard (http:\/\/127\.0\.0\.1:\d+\/)/,
    );
    if (match) url = match[1];
  });
  const close = async () => {
    if (child.exitCode !== null || child.signalCode !== null || failed) return;
    const exit = once(child, 'exit').catch(() => {});
    child.kill('SIGTERM');
    const timer = setTimeout(() => child.kill('SIGKILL'), 3000);
    try {
      await exit;
    } finally {
      clearTimeout(timer);
    }
  };
  const deadline = Date.now() + 15000;
  while (!url && Date.now() < deadline && !failed && child.exitCode === null)
    await Bun.sleep(20);
  if (!url) {
    await close();
    throw new Error(
      'Native board fixture did not start. Run bun run build:native.',
    );
  }
  return { url, close };
}
