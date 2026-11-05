# Sharing v2 acceptance

Validated on 2026-11-06 (JST) using Chrome 156 and the controlled loopback TLS
relay. The [machine-readable record](dashboard-sharing-acceptance.json) contains
35 successful browser checks and no errors. No production host was deployed.

## Verified behavior

- Readonly links disable application controls; enabled links independently grant
  application interaction and layout editing. Forwarded copies work with a new endpoint.
- Bearer fragments are scrubbed, reload uses tab-scoped storage, snapshots exclude
  unshared Surfaces, and published PNGs pass native image validation.
- Layout saves preserve native revision conflicts. Owner membership removals
  withdraw views, revocation closes every holder of a link, and other links remain usable.
- Relay interruption recovers fresh snapshots without replaying application actions.
- Layout-only access is described correctly in English and Japanese.
- Owner enrollment, creation, share management, reload and revocation remain functional.
- Static assets retain CSP/MIME/cache behavior, without an application-data fallback.

## Automated checks

- `cargo test --manifest-path crates/Cargo.toml --workspace --locked`: passed.
- Hub share-store regression tests: permission combinations, credential secrecy,
  v1 credential retirement, membership/layout limits, revision conflicts, stale
  readers, malformed storage and exact byte limits passed.
- Hub UI: 54 tests passed; hosted TypeScript checking and builds passed.
- Browser transport: 12 JavaScript tests and 5 Rust tests passed.
- Flutter sharing, client, connection and navigation tests: 33 passed.
- `tests/browser/sharing.mjs`: 35 checks passed; screenshots were inspected.

Use `bun run test:dashboard-sharing` to build and rerun the browser suite.
The screenshots are emitted to `SHARING_RESULTS` when specified. This record
covers local controlled-relay validation; it does not establish a production
relay SLO or separate-network deployment validation.
