# Dashboard sharing acceptance — 2026-11-03

The downstream native-hub design is implemented. [acceptance.json](dashboard-sharing-acceptance.json)
records the tested static artifact hashes, versions, individual browser checks
and measured relay recovery. The artifact ran under the prescribed CSP on an
isolated HTTPS host at `/dashboard/`, with gzip, a controlled TLS relay, two real
SDK services/gateways, the production hub listener and the shared dashboard UI.
Only the isolated test leaf SPKI was trusted. No public host or relay was deployed.

Chromium **156.0.8078.73** passed **32 real-browser checks**:

- Both readonly public-link access and explicit identity invitation enrollment.
- Native local owner HTTP/WS UI creates links and manages independent grants.
- Nonshared same-service Surfaces remain absent; readonly controls are disabled.
- Actual validated SDK PNG pixels arrive through scoped native resource reads.
- Application actions carry form drafts, execute once and produce live updates.
- Shared editors can save names/layout but cannot add/remove source references;
  saved revisions propagate to other viewers and native CAS conflicts remain.
- Public and registered reloads, exclusive-tab ownership, local acceptance-save
  failure/retry without another redemption, corrupt identity and explicit reset.
- Owner membership removal retracts views, independent grant revocation works,
  invalid public credentials render no board and identity revocation ends access.
- Controlled relay loss/recovery restores fresh snapshots without replaying
  actions. The recorded recovery was **6,746 ms**, within the 90-second bound.
- Every JS/CSS/WASM response has the exact CSP/MIME/cache headers with gzip;
  missing assets and API paths return 404. No static-host app-data/API fallback
  or unhandled page exception occurred.

The [screenshot](assets/dashboard-sharing/shared-editor.png) shows the
registered editor after another source has been withdrawn. It contains only
fixture data and public IDs, with no credentials.

Native and unit evidence complements the browser run:

| Requirement | Evidence |
| --- | --- |
| Offline local mode and loopback defaults | CLI native tests exercise both ordinary and `hub --offline` HTTP/WS with no Bun on PATH |
| Opt-in iroh without registered upstreams | CLI native test starts `hub --remote --web-url` with no registered gateway; CLI argument tests keep offline mode separate |
| Gateway contract unchanged | Existing gateway/client/SDK integration suites pass; browser-client enrollment has separate ALPNs and storage |
| Server-side permissions | Native iroh test for public action/layout/resource denial, explicit enrollment and forged action destination narrowing |
| Lifetime, generation, queued work | Native scoped-handle tests and downstream test verify cancelled queued subscription work before successful revocation while preserving the source service |
| Durable grant storage | Share-store tests cover independent readers/writers, first-identity-wins contention, expiry, corruption, exact storage limit and no stale restoration |
| Revocation acknowledgement failure | Held live-lease test returns an error without restoring durable grant removal |
| Browser state and renderer application | Remote adapter tests cover snapshot apply-before-ACK, partial cleanup, recreated scopes, first-hello denial and image cancellation |
| Browser storage/recovery | Profile tests cover durable identity, lock ownership, denied storage, corruption/reset and acceptance-save retry |
| Packaging and compatibility | `bun run test`, Python/Rust SDK suites, isolated packed renderer consumer, release checks and native packaging tests |

Repeat with `bun run test:dashboard-sharing`; set `SHARING_RESULTS` to preserve
reports/screenshots and `CHROMIUM_BIN` if needed. See [the Workers app](../apps/dashboard/README.md) for build and deployment
instructions.

This acceptance establishes the local implementation and preview artifact, not
public-host/separate-network operation or a relay SLO. Repeat the documented
operator checks on the intended host and network before making deployment claims.
Remote audio/video adapters and implicit publication of constructed/embedded
Markdown resource destinations are not provided; unavailable states are explicit.
