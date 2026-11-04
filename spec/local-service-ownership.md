# Local SDK service ownership

The TypeScript, Python and Rust SDKs use the same lifetime lock before publishing
`<runtimeRoot>/<serviceId>/a2ui.sock` and `manifest.json`. The wire format and
manifest stay unchanged.

1. Create the private runtime root, then open
   `<runtimeRoot>/.<serviceId>.lock~` (mode 0600, without truncating it). `~` is not
   legal in a service id, so this file cannot collide with a service directory.
2. Acquire a nonblocking exclusive **flock(2)** lock on that descriptor. Reject
   contention, including another transport in the same process. Do not substitute
   POSIX record locks: their ownership and close semantics differ.
3. Only the lock holder creates the service directory, probes an existing socket,
   removes a refused/stale socket, binds, and publishes the manifest. A successful
   probe still rejects an existing older SDK service. Errors other than ENOENT or
   ECONNREFUSED are propagated without unlinking the socket.
4. Hold the descriptor until advertisement/socket cleanup and listener shutdown
   finish. A failed contender never removes the owner's directory. Completed
   transports forget their cleanup ownership before allowing a successor.
5. Close the descriptor to release the lock. **Never unlink or replace the lock
   file**, even after a crash or normal close: all contenders must lock the same
   inode. The kernel releases ownership when a process dies. The next owner can
   recover the stale socket while holding the same lock.

These guarantees apply to SDK versions implementing this agreement. Older SDKs
ignore the advisory lock; a live older socket is detected by the probe, but racing
an older SDK's startup or cleanup is not protected. Upgrade all publishers that
share a runtime root. The root must be a local filesystem that supports flock;
only remove its persistent lock files when every publisher has stopped.

## TypeScript distribution

Unix discovery lazily loads the `@deepseek-ai/node-addon-system/flock` entry
(pinned to 0.1.2). Its distributed C source calls `flock(fd, LOCK_EX | LOCK_NB)`
through Node-API v8; it neither opens nor closes the caller's descriptor. The
package supplies prebuilt binaries for Linux x64/arm64 (glibc and musl) and macOS
x64/arm64. Installation needs no compiler or CLI helper. Keep npm optional platform
dependencies enabled. Missing bindings fail acquisition before service files are
modified. Importing the SDK and using its other transports does not load the addon.
Python likewise loads its standard-library `fcntl` only when starting this
Unix-only transport. Rust uses libc's platform definitions for flock.

Dependency API and source: [package repository](https://github.com/deepseek-harness/deepseek-harness/tree/main/native/system/packages/entry).

## Regression checks

`bun run test:sdk-ownership` builds the TypeScript SDK and Rust test fixture, then
runs Node, Bun, Python and Rust processes against one directory. Stdin/stdout
barriers test pre-bind exclusion, a simultaneous start with exactly one winner,
failed-contender cleanup, obsolete close callbacks, and recovery after SIGKILL
leaves each SDK's real socket behind. Each SDK also has same-process lifecycle
regressions, including controlled bind and cleanup boundaries.
