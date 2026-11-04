# Local service ownership

All local service publishers, including the TypeScript, Python and Rust SDKs and
native utilities, use the same lifetime lock before publishing
`<runtimeRoot>/<serviceId>/a2ui.sock` and `manifest.json`. The wire format and
manifest stay unchanged. A service id has one owner across all publisher types.

1. Create the private runtime root, then open
   `<runtimeRoot>/.<serviceId>.lock~` (mode 0600, without truncating it). `~` is not
   legal in a service id, so this file cannot collide with a service directory.
2. Acquire a nonblocking exclusive **flock(2)** lock on that descriptor. Reject
   contention, including another transport in the same process. Do not substitute
   POSIX record locks: their ownership and close semantics differ.
3. Only the lock holder creates the service directory, probes an existing socket,
   removes a refused/stale socket, binds, and publishes the manifest. A successful
   probe still rejects an existing older publisher. Errors other than ENOENT or
   ECONNREFUSED are propagated without unlinking the socket.
4. Hold the descriptor until advertisement/socket cleanup and listener shutdown
   finish. A failed contender never removes the owner's directory. Completed
   transports forget their cleanup ownership before allowing a successor.
5. Release the lock and close the descriptor. Rust and Python explicitly call
   `flock(LOCK_UN)` before closing: a concurrent fork can retain the same open file
   description until exec, even with CLOEXEC, so close alone can delay a successor.
   TypeScript closes its descriptor through the Node/Bun transport. **Never unlink
   or replace the lock file**, even after a crash or normal close: all contenders
   must lock the same inode. After a crash the kernel releases ownership when the
   last reference to the open file description closes. The next owner can recover
   the stale socket while holding the same lock.

These guarantees apply to publisher versions implementing this agreement. Older
publishers ignore the advisory lock; a live older socket is detected by the probe,
but racing an older publisher's startup or cleanup is not protected. Upgrade all
publishers that share a runtime root. The root must be a local filesystem that
supports flock; only remove its persistent lock files when every publisher has
stopped.

## TypeScript distribution

Unix discovery lazily loads the `@deepseek-ai/node-addon-system/flock` entry
(pinned to 0.1.2). Its distributed C source calls `flock(fd, LOCK_EX | LOCK_NB)`
through Node-API v8; it neither opens nor closes the caller's descriptor. The
package supplies prebuilt binaries for Linux x64/arm64 (glibc and musl) and macOS
x64/arm64. Installation needs no compiler or CLI helper. Keep npm optional platform
dependencies enabled. Missing bindings fail acquisition before service files are
modified. Importing the SDK and using its other transports does not load the addon.
Python likewise loads its standard-library `fcntl` only when starting this
Unix-only transport. The Rust SDK uses libc's platform definitions for flock;
native utilities use fs2's Unix flock implementation with the same lock path.

Dependency API and source: [package repository](https://github.com/deepseek-harness/deepseek-harness/tree/main/native/system/packages/entry).

## Regression checks

`bun run test:sdk-ownership` builds the TypeScript SDK, Rust SDK fixture and native
utility fixture, then runs Node, Bun, Python, Rust SDK and native utility processes
against one directory. Stdin/stdout barriers test pre-bind exclusion, a simultaneous
start with exactly one winner,
failed-contender cleanup, obsolete close callbacks, and recovery after SIGKILL
leaves each publisher's real socket behind. Each SDK also has same-process
lifecycle regressions, including controlled bind and cleanup boundaries. Native
utility tests add same-process SDK contention, cancelled close, supervisor abort,
and successor ownership after close, while checking that the lock inode survives
cleanup. Rust and Python regressions retain a duplicated descriptor through owner
shutdown to reproduce fork inheritance deterministically, and verify that the
successor is immediately available while that descriptor remains open.
