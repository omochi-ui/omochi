# Codebase details re-audit — browser hub v3, round 2

Reviewer: `browser_v3_codebase_r2`, fresh-context, read-only.
Standard: EGM Implementation Readiness Standard.
Target: `docs/dashboard-remote-access.md:291–468`, lines 1–290 as context and
the three linked specification artifacts as normative targets.

**Completion: complete. Verdict: not clean.** Four Major implementation-instruction
gaps remain. No Blocker finding establishes a false current-source claim or
invalidates the demonstrated browser-to-gateway direction.

The specification substantially resolves the earlier milestone-only plan.
Proposed files are clearly marked, and their absence is expected. Findings
concern missing APIs or changes outside the declared file map.

## codebase-v3-r2-1 — Major — Private browser package versions break the release selector

Affected scope: `spec/browser-hub-v1.md:23–35,611–616`;
`docs/dashboard-remote-access.md:311–324,337–338`.

The contract creates two private workspace packages at version `0.0.0` and keeps
SDK/browser releases separate from native releases. The map omits the required
release-selector adjustment.

Source evidence:

- `scripts/release.ts:126–138` loads every root workspace and validates version
  equality after filtering only `!manifest.omochiNative` (line 136), not private.
- Lines 62–68 reject selected versions differing from the SDK version. Publication
  ordering excludes private packages at line 94, after version validation.
- Root `package.json:16–20` exposes check/pack/staging commands using this script.
  Python release preparation also calls `loadRelease()`.

The prescribed private `0.0.0` manifests make those release paths reject them
before publication exclusion takes effect.

Repair: add `scripts/release.ts` and regression tests to M1. Specify that SDK
version coordination selects public SDK/browser packages and excludes private
build inputs, retaining native-stub exclusion and the prohibition on public
runtime dependencies on private workspaces. Verify both new private manifests.
Existing `test:packaging` does not invoke `loadRelease` and does not cover this.

## codebase-v3-r2-2 — Major — Renderer’s public core dependency is absent from packaging/native build instructions

Affected scope: `spec/browser-hub-v1.md:47–49,120–171,611–616`;
`docs/dashboard-remote-access.md:314–324,347–357,390–395`.

New public renderer APIs consume LiveSession/ServiceView types defined in
`@omochi-ui/core/browser`, creating a declaration dependency even for type-only
imports. The plan changes neither renderer's manifest nor every native build path.

Source evidence:

- `packages/renderer/package.json:20–35` lists core only in devDependencies;
  TypeScript emits declarations.
- `scripts/release.ts:98–110` removes devDependencies from published manifests.
- `packages/core/package.json:9–22` resolves public subpaths through generated dist.
- Root `package.json:21` builds renderer/hub-ui directly for build:native without
  building core first.
- `scripts/package-native.py:18–21` independently builds only renderer/hub-ui
  before Cargo and is missing from the modification map.

Existing core outputs can hide this in a workspace. Clean native builds need the
new declarations, and independently installed renderer needs a public dependency.

Repair: add renderer/package.json and scripts/package-native.py to M1; make core
a public renderer dependency and build it before renderer in both native paths.
Keep private transport/hub packages out of renderer's graph. Include clean-output
native build and isolated published-renderer type-consumer verification. This
requires the new public TS dependency, not WASM tooling for native builds.

## codebase-v3-r2-3 — Major — Stored identity verification needs an undeclared pure transport API

Affected scope: `spec/browser-hub-v1.md:208–216,406–407,423–428`;
`docs/dashboard-remote-access.md:312,325,399–409`.

openBrowserProfile must validate/create identity without sockets and verify the
stored secret derives the stored EndpointId before binding. The exhaustive facade
offers generateIdentity() for random identity creation and openEndpoint(secretHex,
relays) for endpoint binding; neither supplies pure existing-secret derivation.

Source: `experiments/browser-hub/wasm/src/lib.rs:16–39` derives the existing-key
identity through endpoint construction and bind().await; id() at 41–43 reads
that bound endpoint. The spike cannot meet the new no-socket verification order.

The implementer must add an unlisted operation, choose another crypto dependency,
or violate ordering. Specify a pure WASM identityFromSecret(secretHex) operation
with exact validation/output/errors, deriving the public ID without binding. Add
the handwritten FFI/facade operation and malformed-key/mismatch tests asserting
no endpoint opens.

## codebase-v3-r2-4 — Major — A2uiSnapshot lacks the prescribed pre-copy accounting API

Affected scope: `spec/browser-hub-v1.md:287–295,304–325`;
`docs/dashboard-remote-access.md:314–316,375–395`.

The contract chooses private A2uiSnapshot staging and merged-replay sizing during
apply, rejecting before copying/sparse expansion. The map neither modifies the
shared implementation nor specifies a bounded replacement.

Source evidence:

- `packages/core/src/a2ui-snapshot.ts:82–85,121` exposes only apply(value)/messages().
- updateAtPointer clones the existing model at lines 31–35.
- apply schema-parses/clones incoming values at lines 85–112.
- messages clones the full replay at lines 121–140; state is private at 83.
- No preflight/reservation/transactional apply/allocation-free replay sizing API.

Wrapping apply then serializing messages detects excess after the prohibited
copies. Incoming frame limits miss accumulation through small patches.

Choose a compatible shared-class budget/preflight API or a separate browser
snapshot implementation, with exact responsibilities. Define sizing for component
replacement, escaped strings, sparse arrays, deletes and retained versions,
including reservation rollback. Add the selected files and boundary tests to M1.
The numeric limits exist; their concrete enforcement API/algorithm remains missing.

## Prior-group resolution assessment

Historical severities are preserved; these are specification assessments only.

| Prior group | Original severity | Assessment |
| --- | --- | --- |
| V2-01 | Blocker | Substantially addressed: packages/ownership/APIs/map/checkpoints. Not fully resolved: findings 1–2, release/dependency/build omissions |
| V2-02 | Blocker | Substantially addressed: u64/generations/leases/cancel/failure/timers/budgets. Not fully resolved: finding 4, bounded snapshot API |
| V2-03 | Major | Adequately addressed: v4 bounds/bare channels/direct prefixing/metadata tests |
| V2-04 | Blocker | Substantially addressed: DB/ownership/enrollment recovery. Not fully resolved: finding 3, pure identity verification |
| V2-05 | Major | Adequately addressed: captured scopes/restricted PNG/budgets/abort/catalog overrides |
| V2-06 | Blocker | Adequately addressed: exchange schema/semantics/fixtures/both repositories/CAS/delete/import-as-copy |
| V2-07 | Major | Largely addressed: entries/headers/matrix/harness/acceptance; build completeness depends on findings 1–2 |
| V2-08 | Major | Adequately addressed: queued origins/cancellation/barrier/Local behavior/blocked-SDK regressions |

## Completion evidence

Read: persona and readiness standard; full main plan/contract/schema/fixtures;
v3 manifest/resolution; v2 readiness synthesis and codebase report; root README;
gateway v4; experiment README/RESULTS/recorded result; relevant experiment WASM,
client/build/harness/fixture/manifests; actual core exports/routing/snapshot/disposal;
renderer client/hook/images/catalogs/Markdown/manifests; hub-ui contracts/services/
layout/App/entries/build/native harness; Rust gateway queues/dispatch/resources/
revocation, pairing/CLI, hub scopes/capacity/images/dashboards/static; root/native/
package release and build graph.

All four hashes matched the freeze manifest. Required reads unavailable: none.
No edits, builds, tests, repeated experiments, or agent delegation.

Final verdict: not clean — four Major gaps before M1–M5 meet readiness standard.
