#!/usr/bin/env python3
"""Verify Linux mDNS <-> macOS Bonjour resolution on the same LAN over SSH.

No enrollment, stored identities, relays, app changes, or iOS device required.
The Mac advertises a synthetic EndpointId; this verifies discovery, not QUIC.
"""

import argparse
import base64
import ipaddress
import json
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading


# Run via stdin to python3 on the Mac. PTYs make dns-sd flush its callbacks.
MAC_PROBE = r'''
import errno, ipaddress, json, os, pty, re, select, signal, subprocess, time
settings = SETTINGS
children = {}
buffers = {}

def start(label, arguments):
    master, slave = pty.openpty()
    try:
        process = subprocess.Popen(['dns-sd', *arguments], stdin=subprocess.DEVNULL,
                                   stdout=slave, stderr=slave, start_new_session=True)
    finally:
        os.close(slave)
    children[master] = (label, process)
    buffers[label] = b''

def read_for(seconds):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        ready, _, _ = select.select(list(children), [], [], .1)
        for fd in ready:
            try:
                chunk = os.read(fd, 65536)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                chunk = b''
            if chunk:
                label, _ = children[fd]
                buffers[label] += chunk
                if len(buffers[label]) > 256 * 1024:
                    raise RuntimeError('dns-sd probe output exceeded its limit')

def output(label):
    return buffers[label].decode(errors='replace')

try:
    service = '_irohv1._udp'
    start('browse', ['-B', service, 'local'])
    start('resolve', ['-L', settings['linux'], service, 'local'])
    # Resolve reports one SRV; QueryRecord enumerates the full multi-port RRset.
    start('srv', ['-Q', settings['linux'] + '.' + service + '.local.', 'SRV'])
    start('register', ['-R', settings['mac'], service, 'local', str(settings['port'])])
    read_for(4)
    records = sorted(set(re.findall(r'\bSRV\s+IN\s+\d+\s+\d+\s+(\d+)\s+(\S+)', output('srv'))))
    if not records:
        raise RuntimeError('no SRV records: ' + output('srv'))
    targets = sorted({target for _, target in records})
    for index, target in enumerate(targets):
        start('host' + str(index), ['-G', 'v4v6', target])
    read_for(4)
    addresses = {}
    for index, target in enumerate(targets):
        values = set()
        for line in output('host' + str(index)).splitlines():
            fields = line.split()
            if len(fields) < 3 or fields[1] != 'Add':
                continue
            try:
                values.add(str(ipaddress.ip_address(fields[-2].split('%')[0])))
            except ValueError:
                pass
        addresses[target] = sorted(values)
    registered = settings['mac'] + '.' + service + '.local.'
    if not (registered in output('register') and 'Name now registered and active' in output('register')):
        raise RuntimeError('Bonjour registration failed: ' + output('register'))
    if settings['linux'] not in output('browse') or 'can be reached at' not in output('resolve'):
        raise RuntimeError('Bonjour browse/resolve failed: ' + output('resolve'))
    print(json.dumps({'browse': True, 'resolve': True, 'srv': [
        {'port': int(port), 'host': host, 'addresses': addresses[host]}
        for port, host in records
    ]}))
finally:
    for fd, (_, process) in children.items():
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGINT)
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        os.close(fd)
'''


def endpoint_name(value):
    return base64.b32encode(bytes.fromhex(value)).decode().lower().rstrip("=")


def socket_address(value):
    host, port = value.rsplit(":", 1)
    return str(ipaddress.ip_address(host.strip("[]").split("%", 1)[0])), int(port)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ssh-host", default="macbook")
    parser.add_argument("--port", type=int, default=41973)
    args = parser.parse_args()
    if not 1 <= args.port <= 65535:
        parser.error("--port must be between 1 and 65535")
    root = Path(__file__).resolve().parents[3]
    command = [
        "cargo", "run", "--locked", "--quiet", "--manifest-path",
        str(root / "crates/Cargo.toml"), "-p", "omochi-transport",
        "--example", "bonjour_probe",
    ]
    with tempfile.TemporaryFile(mode="w+") as log:
        process = subprocess.Popen(
            command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=log, text=True, cwd=root,
        )
        events = queue.Queue(maxsize=16)

        def read_events():
            try:
                for line in process.stdout:
                    events.put(json.loads(line), timeout=1)
                events.put({"event": "eof"}, timeout=1)
            except Exception as error:
                events.put({"event": "error", "message": str(error)}, timeout=1)

        reader = threading.Thread(target=read_events, daemon=True)
        reader.start()
        try:
            ready = events.get(timeout=120)
            if ready.get("event") != "ready":
                raise RuntimeError("Linux endpoint did not start: " + repr(ready))
            settings = {
                "linux": endpoint_name(ready["linux"]),
                "mac": endpoint_name(ready["mac"]), "port": args.port,
            }
            script = MAC_PROBE.replace("SETTINGS", repr(settings), 1)
            remote = subprocess.run(
                ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10",
                 "--", args.ssh_host, "python3 -"],
                input=script, capture_output=True, text=True, timeout=30,
            )
            if remote.returncode:
                raise RuntimeError("Mac probe failed: " + remote.stderr + remote.stdout)
            mac = json.loads(remote.stdout)
            resolved = events.get(timeout=12)
            if resolved.get("event") != "resolved" or resolved["endpoint"] != ready["mac"]:
                raise RuntimeError("Linux did not resolve the Mac's EndpointId: " + repr(resolved))
            expected = {socket_address(value) for value in ready["addresses"]}
            observed = {
                (address, record["port"])
                for record in mac["srv"] for address in record["addresses"]
            }
            if not expected or expected - observed:
                raise RuntimeError("Bonjour missed Linux addresses: " + repr(sorted(expected - observed)))
            reverse = [socket_address(value) for value in resolved["addresses"]]
            if not any(port == args.port and not ipaddress.ip_address(ip).is_loopback
                       for ip, port in reverse):
                raise RuntimeError("Linux missed the registered Bonjour address/port")
            process.stdin.write("stop\n")
            process.stdin.flush()
            process.wait(timeout=5)
            if process.returncode:
                raise RuntimeError("Linux probe failed")
            print(json.dumps({
                "result": "passed", "linux_to_bonjour": mac,
                "bonjour_to_linux": {"addresses": resolved["addresses"]},
                "scope": "macOS/Linux address resolution; no QUIC or iOS device",
            }, indent=2))
        except Exception:
            log.seek(0)
            print(log.read(), end="", file=sys.stderr)
            raise
        finally:
            if process.poll() is None:
                try:
                    process.stdin.write("stop\n")
                    process.stdin.flush()
                    process.wait(timeout=5)
                except (BrokenPipeError, subprocess.TimeoutExpired):
                    process.terminate()
                    process.wait(timeout=5)
            reader.join(timeout=2)


if __name__ == "__main__":
    main()
