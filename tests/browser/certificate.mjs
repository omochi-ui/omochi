import { spawn } from 'node:child_process';
import { X509Certificate, createHash } from 'node:crypto';
import { readFile, writeFile, chmod } from 'node:fs/promises';
import path from 'node:path';
async function run(command, args) {
  const child = spawn(command, args, { stdio: 'ignore' });
  const code = await new Promise((resolve, reject) => { child.once('error', reject); child.once('exit', resolve); });
  if (code !== 0) throw new Error(`Fixture setup command failed: ${command}`);
}
export async function certificate(directory) {
  const cert = path.join(directory, 'cert.pem'), key = path.join(directory, 'key.pem');
  const ca = path.join(directory, 'ca.pem'), caKey = path.join(directory, 'ca-key.pem');
  const csr = path.join(directory, 'leaf.csr'), extensions = path.join(directory, 'leaf.ext');
  await run('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', caKey, '-out', ca,
    '-days', '1', '-subj', '/CN=Omochi fixture CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign']);
  await run('openssl', ['req', '-new', '-newkey', 'rsa:2048', '-nodes', '-keyout', key, '-out', csr, '-subj', '/CN=localhost']);
  await writeFile(extensions, 'basicConstraints=critical,CA:FALSE\nsubjectAltName=DNS:localhost,IP:127.0.0.1\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n');
  await run('openssl', ['x509', '-req', '-in', csr, '-CA', ca, '-CAkey', caKey, '-CAcreateserial', '-out', cert, '-days', '1', '-extfile', extensions]);
  await chmod(key, 0o600); await chmod(caKey, 0o600);
  const certificate = new X509Certificate(await readFile(cert));
  const spki = createHash('sha256').update(certificate.publicKey.export({ type: 'spki', format: 'der' })).digest('base64');
  return { cert, key, ca, spki };
}
