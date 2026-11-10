# Local service ownership and runtime format 2

Rust, Python, Node and Bun use `omochi-local`, the Rust SDK workspace's shared
local-publication module. It owns filesystem policy, locks, manifests, listeners
and byte streams. SDK JSONL, snapshot materialization and fanout remain SDK
responsibilities; the native gateway uses the same IPC with gateway v4 framing.

## Runtime root and naming

Resolution order is an explicit SDK/CLI runtime directory, nonempty
`OMOCHI_RUNTIME_DIR`, then the platform default:

- Unix with XDG: `$XDG_RUNTIME_DIR/omochi/v2`.
- Unix without XDG: `<tempdir>/omochi-<effective uid>/omochi/v2`.
- Windows: the LocalAppData known folder, `Omochi\Runtime\v2`.

Overrides specify the complete runtime root. `up` exports `OMOCHI_RUNTIME_DIR` to
its children and does not require a particular final path component. Windows
native configuration defaults to LocalAppData `Omochi\rust-v1`; existing Unix
configuration and native identity/trust are retained.

Local service IDs match `[a-z0-9._-]{1,64}`, excluding `.` and `..`. Uppercase is
rejected, never normalized. Logical IDs remain unchanged in messages and routes.
All platforms use `svc-<serviceId>~` directories and `.svc-<serviceId>.lock~` lock
files. The prefix/suffix avoid Windows device names and trailing periods without
encoding the ID. The existing gateway v4 wire ID grammar remains accepted for
remote peers; the new local publisher rule does not reinterpret remote routes.

A runtime contains `runtime.json` with `{"version":2}`. Initialization is serialized
by `.runtime.lock~`. A nonempty directory without the marker is rejected as legacy
state. Discovery opens and validates an existing runtime without repairing it.

## Access control

The private runtime is local authentication. Unix managed directories are owned
by the effective user, real directories and mode 0700; files are mode 0600.
Default runtime ancestors are validated before traversing their children. Safe
owned directories without group/world write permission may be tightened; foreign,
writable and symlink directories are rejected before accessing their contents.

Windows objects have the current process TokenUser SID as owner. New directories,
files and pipe servers receive a protected current-user-only DACL at creation;
directory permissions inherit to children. Existing insecure ACLs are rejected.
Administrators/SYSTEM are not added automatically. This does not promise to
prevent a privileged administrator from taking ownership through OS privileges.
Managed reparse points, remote drives, UNC runtime roots and filesystems without
persistent ACLs are rejected.

Manifests are untrusted input. Discovery checks the service ID against its
containing directory, version, name, bounded size, icon and endpoint. Invalid
records are omitted until repaired; an unsafe runtime root fails the scan.
Filesystem notifications are hints, with periodic rescans retained.

## Manifest and endpoints

`local-service.schema.json` describes manifest v2. A Unix example is:

```json
{"version":2,"serviceId":"demo","name":"Demo","endpoint":{"kind":"unix","path":"a2ui.sock"}}
```

`icon` is optional. A Unix endpoint is a plain filename inside that service's
private directory. Windows uses `{"kind":"namedPipe","name":"\\\\.\\pipe\\omochi-local-v2-<32 lowercase hex digits>"}`.
The 128-bit pipe name is random for each publication and is not derived from user
paths. `gateway.json` contains `version: 2` and the gateway endpoint; its permanent
ownership lock is `.gateway.lock~`. Gateway wire protocol version 4 is independent
of runtime/manifest version 2.

Windows pipes are duplex byte streams with overlapped I/O, remote-client rejection
and first-instance exclusivity. The accept loop creates the next waiting instance
before handing off the connected instance. A client verifies the server object's
owner SID before sending protocol data, including when following an old manifest.
Busy pipe instances have bounded connection retries. Unix socket paths are capped
at 100 bytes; use a shorter runtime override when the path cannot fit.

## Publication ownership and teardown

1. Validate the ID and private runtime, then open the permanent service lock file
   without truncating/replacing it. Unix uses flock; Windows uses LockFileEx through
   the shared file-lock module. Acquisition is exclusive and nonblocking, including
   independent transports in the same process.
2. Only the owner prepares the service directory and binds. On Unix a live old
   socket rejects startup. Only NotFound/ConnectionRefused permits stale recovery;
   non-sockets and other errors are preserved. Windows pipe instances disappear
   with their final handle, so the next owner replaces the stale advertisement
   with a new pipe name.
3. Advertise atomically only after listening. A manifest is at most 128 KiB; names,
   IDs, icons and endpoint kinds are checked before advertisement.
4. On close, retract the manifest, stop accepting, cancel/join connections and
   pending I/O, then remove owned files and release the lock. Stream guards retain
   ownership after a cancelled/aborted supervisor until the streams are dropped.
5. Cleanup compares filesystem identities and preserves replaced manifests and
   sockets. A failed contender does not clean up the owner. Old callbacks cannot
   remove a successor. Never unlink service lock files. Unix unlocks explicitly
   before close so a duplicated open file description does not delay a successor.

Native store, dashboard and acknowledgement writes use a private same-directory
temporary file, flush and replacement while retaining their domain lock. Windows
replacement uses POSIX rename semantics, flushes the renamed handle and has bounded
sharing-violation retries. Existing readers retain the old file. A failure
following replacement is not a rollback guarantee. Trust mutations reread under
lock and revocation waits for all live-process acknowledgements; failure remains
an error and must not restore removed records.

## Language adapters and distributions

Rust SDKs/native services directly use the common crate. Node/Bun use Node-API v8;
Python uses a C ABI with asynchronous completion callbacks. These adapters do not
implement A2UI framing or snapshot policy. Reads/writes are single-flight per
direction, with at most 64 KiB per binding operation. Cancelling a potentially
partial operation discards its stream. Native context destruction cancels work and
joins callbacks before releasing language state.

SDK binaries are prebuilt for Linux glibc/musl, macOS and Windows x64/ARM64. npm
uses exact-version optional platform packages; Python uses platform wheels with
no CPython ABI dependency and an sdist containing the Rust source. Local source
builds use `python3 scripts/build-local.py`; source wheel builds require Cargo.
The SDK native release is independent of native CLI releases. Importing the SDK
does not load the native library; local transport or private media storage does.
Python native contexts must be initialized after fork, or multiprocessing must use
spawn/forkserver; inherited native contexts report an error instead of reusing a
Rust runtime whose worker threads no longer exist.

## Migration and verification

Stop old publishers, gateway and hub before upgrading. Replace
`UnixSocketServerTransport` with `LocalServerTransport`; Rust options become
`LocalServerOptions`, TypeScript options `LocalServerTransportOptions`. Replace
socket-path accessors with typed endpoints. Explicit old runtime roots require a
new empty directory. There is no automatic runtime migration or legacy manifest
reader. Rename uppercase service IDs explicitly and update composition/dashboard
references. Do not reset or migrate native identity/trust for this change.

`bun run test:sdk-ownership` exercises all 25 SDK/native publisher pairs, one-winner
simultaneous startup and crash recovery across all five publisher types. Core tests
cover cancellation, inherited locks, traversal, private roots, byte streams and
owned cleanup. `scripts/test-local-native.py` exercises the real SDK/gateway/hub
roundtrip, utility stdin and graceful shutdown. Windows CI additionally verifies
foreign-token ACL denial, Job Object teardown and x64/ARM64 execution. SDK CI builds
and tests all eight native distribution configurations.
