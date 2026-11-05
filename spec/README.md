# Specification index

These contracts describe separate boundaries. Gateway enrollment does not grant
dashboard owner access, and a dashboard share does not enroll its recipient at a
Gateway. See the [remote access overview](../docs/dashboard-remote-access.md) for
the connection paths and the [documentation map](../docs/README.md) for guides.

## Active contracts

| Contract | Boundary / implementation |
| --- | --- |
| [IR schema](ir.schema.json), [basic fixture](fixtures/basic.json) | Shared declarative runtime behavior across TypeScript, Python and Rust SDKs |
| [Action input](action-input.md) | Client input expressions, object composition and Rust Serde behavior |
| [Resources v1](resources.md) | SDK resource registration and request/reply transport |
| [Local service ownership](local-service-ownership.md) | Private runtime directories and cross-process Unix publisher exclusion |
| [Gateway v4](gateway-v4.md) | Gateway-to-Hub framing, trust, enrollment and web invitation handoff |
| [Browser images](hub-browser-images.md) | Local Hub image subscription and requests |
| [Dashboard sharing v1](dashboard-sharing-v1.md) | Native Hub-to-client shares, permissions, leases and revocation |
| [Owner connection v1](owner-connection-v1.md) | External owner enrollment and Hub control operations |
| [Board export schema](board-export.schema.json), [fixture](fixtures/board-export.json) | Configuration exchange; no credentials or live application state |

The browser and mobile external-Hub adapters share the downstream contracts.
The mobile C ABI is defined in
[`omochi_hub.h`](../crates/native-bridge/include/omochi_hub.h), currently version 2;
rebuild Flutter and the Rust library together. Adapter-specific capacity limits
are documented in the [mobile guide](../apps/mobile/README.md) and
[owner contract](owner-connection-v1.md).

## Experimental and historical material

[Browser hub v1](browser-hub-v1.md) and
[bounded browser snapshots](browser-snapshot-v1.md) preserve the direct-Gateway
browser experiment's implementation contracts. Use
[`experiments/browser-dashboard`](../experiments/browser-dashboard/README.md)
for its current locations, commands and acceptance limits. They do not define
the normal hosted dashboard or the mobile embedded Hub.

Files named `*.review-v*.md` are immutable review baselines, not current
implementation instructions. The [review history](../docs/reviews/dashboard-remote-access/review-state.md)
records their context. Existing paths are retained for review links.
