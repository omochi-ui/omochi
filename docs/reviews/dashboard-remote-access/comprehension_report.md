## What this proposes

A hosted or separately hosted dashboard would connect from an ordinary browser
to the user's running Rust hub through iroh WASM and a public relay. The hub
would remain responsible for application state, dashboard configuration, and
authorization. Owners would enroll browser identities for either personal access
or one saved dashboard, with separate permissions for app interaction and layout
editing. Existing local connections would remain available, while browser
compatibility, wire details, storage behavior, and deployment choices remain
explicit release gates.

## How the existing system works, per this document

The Rust hub aggregates SDK applications through local or authenticated remote
gateways and serves the dashboard over HTTP, WebSocket, and SSE. The browser
currently selects these connections directly, and image delivery combines scoped
WebSocket handles with subsequent HTTP reads. Gateway enrollment authenticates
EndpointIds and records Gateway/Hub roles, but those roles do not authorize
dashboard clients. Dashboard storage already uses revision checks, and the CLI
currently starts the hub's iroh endpoint only after a gateway has been registered.

## Verdict

PASS — comprehension review complete.

Findings: None. No Blocker, Major, or Nit comprehension findings were identified.

Both reviewed ranges explain how the proposed browser connection, enrollment,
grants, and lifecycle fit together. The distinction between transport identity
and application authorization is clear, as is the dependency on trusted browser
software and an operating relay. The implementation plan connects its stages to
the numbered open decisions, without presenting those decisions as resolved. No
code-level completeness or implementation-readiness standard was applied.

## Ranges reviewed

- Feature specification: `docs/dashboard-remote-access.md:1–239`.
- Detailed implementation plan: `docs/dashboard-remote-access.md:240–330`.

## Read/search inventory

- Read the assigned comprehension persona and the complete target document.
- Read all 15 local files identified in the target's implementation reading list:
  `docs/architecture.md`; `crates/hub/src/http.rs`;
  `packages/hub-ui/src/App.tsx`; `packages/hub-ui/src/services.ts`;
  `packages/renderer/src/client.ts`; `packages/renderer/src/images.ts`;
  `spec/hub-browser-images.md`; `spec/gateway-v4.md`;
  `crates/transport/src/lib.rs`; `crates/transport/src/pairing.rs`;
  `crates/transport/src/trust.rs`; `crates/hub/src/remote.rs`;
  `crates/cli/src/main.rs`; `crates/hub/src/dashboards.rs`; and
  `deploy/relay/README.md`.
- Read the linked [official iroh browser documentation](https://docs.iroh.computer/languages/wasm-browser),
  which explains WASM integration and relay-dependent browser connections.
- Used file-length inspection and direct file reads. Re-read initially truncated
  material in smaller sections.
- Did not read sibling review reports, modify files, build, or run tests.

Required inputs unreadable: None.

## Review provenance and checkpoint

Stage 6: Comprehension Test. Reviewer: `egm-comprehension`, dispatched as
`dashboard_comprehension` in a fresh conversation context with `fork_turns: none`.
No additional document standard was applied.
Historical target: [original proposal](../../dashboard-remote-access.server-hub-v1.md).
The current plan has a different architecture; this report is not its review.
Target SHA-256: `bc7a539431495d8445ddee42110a8c8f1cd30f0f540b611db3a495f2935efa5f`.

This pass concerns comprehension only. It does not close Stage 5 finding ZO-001
or establish implementation readiness. The target has not been changed. Await
the author's checkpoint choice before Stage 7. No Stage 6 re-audit has been used.
