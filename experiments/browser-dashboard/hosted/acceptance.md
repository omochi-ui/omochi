# Local implementation acceptance — 2026-11-03

Historical acceptance before relocation into `experiments/browser-dashboard`.
Commands below record that run; use [the experiment README](../README.md) for
current commands and paths. The asset hashes and measured results are preserved.

The browser runtime, shared dashboard editor and static distribution were tested
on Linux with Chromium 156.0.8078.73, Bun 1.4.3, rustc 1.99.0, iroh 1.4.1 and
wasm-bindgen 0.2.130. [acceptance.json](acceptance.json) records the asset SHA-256
hashes, uncompressed/gzip sizes, individual mode results and measured counters.
No host or relay was published. These are loopback TLS results, plus a public
relay smoke test from the same host; they do not establish separate-network
support or an uptime guarantee.

| Browser acceptance | Checks passed | Evidence |
| --- | ---: | --- |
| Session | 11 | Two authenticated gateways, eight real SDK services, full namespaces, form metadata, scoped Retry and isolated revocation |
| Profile | 27 | Durable identity, reload, explicit enrollment/refresh, lost acceptance/save recovery, OwnerBusy/crash takeover, denied/corrupt/versioned storage |
| Media | 27 | Actual RGB/RGBA pixels, PNG structure/inflation limits, stat/read/decode cancellation, revocation and zero external-media requests |
| Boards | 25 | Shared import/export UI, actual browser/native repositories, independent copies, CAS, quota/abort, advisory labels and no implicit enrollment |
| Hosted | 13 | Nested `/dashboard/`, prescribed CSP/MIME/cache with gzip, missing-file/API 404s, real actions and controlled relay recovery |
| Protocol faults | 18 | Real partial QUIC headers/bodies, snapshot deadlines/interruption, malformed roster/control/pong, partial processor failure, observer throws, recreation and max-u64 action/resource streams |
| Blocked SDK revocation | 8 | Actual native listener and stalled SDK Unix write, delayed ACK, preserved Local/in-progress work, rejected queued remote work and independently usable B |
| Load | 9 | Ten full minutes, 10000 ordered updates, repeated concurrent PNG readers, stable scopes and zero retained counters on stop |

`bun run test:browser-hub` passed session/profile/media/boards/hosted/load together.
The subsequently added faults and revocation modes passed independently; the
aggregate command now includes both. After adding the transparent SDK gate,
session/profile/media were rerun and passed. Profile/hosted were rerun after
persisted-pageshow recovery was added; it reboots the cached document and acquires
storage/endpoint ownership before restoring the selected sources. The separate public-relay session
smoke passed all 11 checks.

The protocol-fault mode uses an explicitly adversarial fixture-only peer that
forwards real SDK snapshots through `Gateway::serve_stream` and changes the wire
at bounded gates. It checks the browser bridge/session/installed renderer. The
session, profile, media, hosted and blocked-write authorization tests use the
production native remote listener. The blocked-write test changes only the Unix
byte flow, preserving the native authorization epoch and live ACK barrier.
No fault mode is exposed by the production CLI.

The controlled relay resumed at the same TLS URL and all eight surfaces recovered
in **14,557 ms**, within the 90-second acceptance bound. The hosted artifact
started under the exact prescribed CSP using only the isolated test leaf SPKI;
no wildcard certificate verification bypass was used. Every JS/CSS/WASM response
was also checked with gzip and its prescribed MIME/cache/security headers.

The ten-minute run lasted **600,438 ms** with 599 samples. Each of its 10000 Text
updates carried exactly 1024 ASCII payload bytes; the real SDK generated the
incremental A2UI messages, gateway framing and browser renderer updates. The
latest DOM values matched all eight services, with no failed source, rejected
promise or per-service ordering violation. Two admitted PNG readers performed
126 transfers. Eight renderer scopes and eight surface leases remained constant.
Reservation high-water marks were 18,720 snapshot bytes, 2,633 delivery bytes,
1,430 render-queue bytes and 145 encoded PNG bytes. All retained frame, snapshot,
delivery, queue, action, lease, resource and decoded-image counters were zero
after joined stop; historical high-water marks intentionally remain available.

Final JS heap used 11,747,300 bytes. Summed Linux Chromium process-tree RSS samples
ranged from 862,556,160 to 898,093,056 bytes. These are diagnostic observations,
including browser processes and decoder allocations; they are not protocol
encoded-budget assertions or a general memory ceiling. Current frame counters
were sampled between operations; synchronous reservation high-water marks and
admission tests cover transient TS pools, and Rust frame admission enforces its
own bound.

Additional passing verification:

- `bun run test`: workspace SDK/browser tests, Rust server tests, native client/
  renderer integration, approval composition and packaging.
- `bun run test:python` and `bun run test:rust`: cross-language SDK regression tests.
- `bun run test:renderer-package`: packed renderer/core declarations compile in an
  isolated browser consumer.
- `bun run release:check`, frontend lint and `git diff --check`.
- Native builds and packaging complete without invoking WASM generation; private
  browser inputs remain excluded from public SDK/native release selection.

Repeat the deployment-facing two-host checks in [README.md](README.md) before
claiming public-host/separate-network deployment support. The versioned protocol,
board schema and resource limits were not relaxed for these tests.
