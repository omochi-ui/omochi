# Local native migration benchmark

The native gateway/hub substantially reduces startup time and memory in this
local workload. Small-action latency also improves; the improvement for 64 KiB
payloads is more modest. These measurements compare the complete old and new
implementations, including their different process layouts and startup work.
They do not isolate the programming language.

Measured on 2026-10-31 with an AMD Ryzen AI 9 HX PRO 370 (24 logical CPUs), Linux
7.3.0-5-cachyos, Bun 1.4.3 and rustc 1.99.0. Native binaries used the normal release
build, not a test build with relay-server development features enabled.
[Raw observations and binary provenance](native-local-2026-10-31.json) record the
working-tree state and native executable hash. This was an interactive workstation,
not an isolated benchmarking host.

| Measurement (median across five runs) | TypeScript/Bun | Rust release |
| --- | ---: | ---: |
| Fresh-state launch to first surface | 615.82 ms | 5.17 ms |
| Launch range | 538.99–650.26 ms | 4.79–5.26 ms |
| PSS after the workload, entire process tree | 114.67 MiB | 13.34 MiB |
| RSS after the workload, entire process tree | 160.19 MiB | 15.89 MiB |
| Processes (hub plus gateway) | 2 | 1 |
| 64-byte action round trip, p50 | 0.128 ms | 0.062 ms |
| 64-byte action round trip, p95 | 0.301 ms | 0.094 ms |
| 64-KiB action round trip, p50 | 0.479 ms | 0.406 ms |
| 64-KiB action round trip, p95 | 1.541 ms | 0.853 ms |
| Idle CPU, percentage of one core | 1.49% | below sample resolution |

PSS apportions shared memory between processes; summed RSS can double-count shared
pages. The native PSS reduction is approximately 88% here. CPU is sampled from
process-tree ticks over two seconds, so zero means less than the approximately
0.5-percentage-point sampling resolution, not zero CPU work.

## Workload and reproduction

A synthetic SDK publishes one service over the unchanged Unix JSONL protocol.
Each test launches a fresh hub, which starts its local gateway automatically,
then connects one WebSocket client. A valid A2UI action is echoed back as a data
model update. Both implementations receive identical payloads. Each payload size
has 25 warmup and 200 measured sequential operations per run. Runs alternate
implementation order; each has fresh identity/dashboard state but a warm OS cache.
Memory is measured after both payload scenarios. The SDK fixture and measurement
client run outside the measured process tree for both implementations.

```sh
git worktree add --detach ../omochi-typescript b8a2f85
(cd ../omochi-typescript && bun install --frozen-lockfile && bun run build)
bun run build:native:release
OMOCHI_LEGACY_ROOT="$PWD/../omochi-typescript" bun scripts/bench-native.mjs
```

The script is Linux-only because it reads `/proc` for process-tree PSS/RSS and CPU.
It writes `artifacts/bench/native-local.json`, uses private temporary configuration
and runtime directories, and stops its own processes. `OMOCHI_BENCH_RUNS` can select
1–20 repetitions. No user identities, dashboards or trust records are used.

This does not measure browser rendering, simultaneous clients, resource streaming,
large catalogs, peak memory under overload, remote RTT, relay throughput or macOS.
Fresh-state startup also includes different identity creation and process-supervision
work, so the roughly 119-fold startup ratio should not be interpreted as a general
Rust execution-speed ratio. The practical benefit demonstrated here is primarily
lower local service overhead.
