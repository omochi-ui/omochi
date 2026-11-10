# Security Policy

## Reporting a vulnerability

Report suspected vulnerabilities through
[GitHub private vulnerability reporting](https://github.com/omochi-ui/omochi/security/advisories/new).
Please do not include vulnerability details in public issues, discussions or pull
requests before coordinating disclosure with the maintainers.

Include the affected package/application and version or commit, platform,
configuration, reproduction steps or a minimal proof of concept, and the expected
security impact. Describe the attacker's starting access and the boundary crossed.
Redact private keys, invitation secrets, share credentials and application data.

The maintainers use the private report to triage the issue and coordinate fixes,
advisories and disclosure. Reports are handled on a best-effort basis; there is no
guaranteed response or remediation deadline.

## Supported versions

Security fixes target the latest released version of each distributed package or
application. Older releases do not receive security backports; upgrade to the
latest release. Reports affecting development on the default branch are also
welcome.

SDK/browser packages and native distributions have separate release cycles. Use
the version of the affected component when reporting an issue. Cargo is the source
of truth for native versions; the private placeholder versions in
`packages/npm/cli` and `packages/npm/utils` are not public release versions.

## System and scope

Omochi connects application-defined UIs to local and remote dashboards. This
repository's runtime, clients and distribution/deployment code are covered:

| Component | Paths |
| --- | --- |
| Application SDKs and browser transport helpers | `packages/sdk/`, `packages/core/` |
| Native Gateway, Hub, discovery, transport, CLI and utilities | `crates/` |
| Shared browser renderer and local preview | `packages/renderer/`, `packages/dev-server/` |
| Dashboard, hosted transport and native/mobile clients | `packages/hub-ui/`, `packages/hub-transport/`, `apps/dashboard/`, `apps/mobile/` |
| Packaging, release automation and deployment | `packages/npm/`, `scripts/`, `.github/`, `deploy/` |

Security-relevant assets include application state, resources and action access;
dashboard configuration; identity keys; enrollment/share credentials; and durable
authorization records. Utility actions can operate processes, files or commands
with the application's OS privileges.

The local Hub and stdio preview use loopback HTTP/WebSocket endpoints by default.
Remote Gateway and Hub access is opt-in over iroh. The hosted dashboard serves
static client assets; the native Hub owns application data and authorization.
The mobile app can embed a Hub or connect to an external Hub.

## Threat model and trust boundaries

Consider unauthenticated remote peers, hostile websites, share recipients,
revoked peers, and malformed or malicious application messages and resources.
Discovery metadata, invitation/link payloads, client-supplied IDs, URLs, layouts
and protocol frames are inputs to validate, not proof of authority.

The OS account and its protected runtime/state storage are trusted. Local Unix
socket access relies on private, owned runtime directories, normally under
`XDG_RUNTIME_DIR` with mode 0700. Processes already controlling that account are
not isolated from each other by Omochi. Application code and explicitly configured
commands run with their existing privileges; the runtime is not a process sandbox.

Gateway trust, Hub owner grants and dashboard shares are distinct authority
domains. A registered Hub has Gateway-wide access to exposed services. A remote
owner can manage the Hub's dashboards, shares and Gateway registrations. A share
grants access only to its saved board and explicitly enabled permissions.

## Security invariants

- **Local browser access:** preserve loopback defaults and enforce the Hub's
  Origin policy on HTTP, SSE and WebSocket upgrades, including administrative and
  resource routes. `--allow-origin` explicitly extends the trusted browser origins.
- **Local discovery and ownership:** validate runtime ownership/permissions,
  service IDs, socket filenames and icons before use. Manifest paths must not
  escape their service directory. Publishers must retain the shared lifetime lock
  and must not remove another publisher's socket, manifest or lock inode.
- **Remote authentication:** use the exact iroh-authenticated EndpointId and
  registered role for ordinary Gateway traffic. Owner connections require their
  bound identity. Discovery records, display names, relay admission and
  application-supplied IDs never grant application access.
- **Enrollment and credentials:** keep Gateway and owner enrollment separate from
  ordinary traffic. Enrollment is expiring and binds the first authenticated
  redeeming identity; success requires durable authorization. Sharing v2 instead
  checks its reusable bearer credential on every connection. Do not advertise
  secrets in discovery, expose them in routine listings/logs or retain enrollment
  secrets in client connection records. Saved share credentials require an
  explicit save.
- **Permission and routing scope:** shares follow the saved board's current
  membership; `interact` and `editLayout` are independent. Layout editing cannot
  change membership, panel IDs or sources. Shares cannot administer owners or
  enroll Gateways; owner enrollment/revocation stays local. Actions, metadata and
  resources must remain scoped to their authorized service/Surface and current
  connection generation or lease. Do not replay actions on reconnect.
- **Persistence and revocation:** serialize store mutations across processes,
  re-read under the lock and replace records atomically. Stale caches must not
  restore removed authority. Gateway, owner and share revocation succeeds only
  after registered live processes acknowledge authorization reload and revoked
  session teardown. Acknowledgement failure remains an error after durable
  removal. Do not import legacy identities or trust into native authorization.
- **Content and resource safety:** application text/Markdown must not become
  executable browser content. Scoped images require current Surface authority;
  shared image requests also require authoritative publication. Keep public-HTTPS
  destination checks, DNS pinning and redirect revalidation, credential-free
  fetching, and raster MIME/size/pixel validation. A client-supplied resource ID or
  URL must not confer authority or select another service's resources.
- **Availability and stale work:** retain bounded frames, snapshots, queues,
  request concurrency and image budgets, with deadlines and cancellation. Late
  work from retired connections or Surfaces must not affect their successors.

## Reportable findings and severity context

Report a reproducible violation of these boundaries or another concrete security
impact. Examples include unauthorized state/resource access or actions, share to
owner escalation, effective access after successful revocation, credential
disclosure, cross-origin access to local administration, path traversal, script
execution, SSRF and attacker-triggered resource exhaustion.

Explain reachability, required permissions/user actions, deployment settings and
impact on confidentiality, integrity or availability. Loopback binding does not
by itself rule out a hostile-website attack. An authenticated or shared viewer
can still cross a meaningful permission boundary. A dependency advisory alone
does not establish exploitability; include the affected runtime/build path where
known. Reports with uncertain impact are welcome for private triage.

## Limitations and assessment context

Local HTTP/WS endpoints have no user login or TLS and accept requests without an
Origin header. Origin checks protect browser access, not against arbitrary local
programs. Widening the bind address or allowed origins changes this exposure.

Anyone holding a sharing v2 link, including a forwarded copy, has its granted
permissions. Revocation cannot recall already delivered data or dispatched app
actions. Forgetting a saved client connection does not revoke server-side access.

The scoped image policy does not cover every media or networking path: legacy
resource delivery, video/audio, custom catalogs and explicitly opened links have
separate behavior. Experimental configurations and custom components need their
own reachability analysis; they are not blanket exclusions. These limitations
do not excuse violations of the boundaries above.

See the current contracts for [Gateway v4](spec/gateway-v4.md),
[sharing v2](spec/dashboard-sharing-v2.md),
[owner connections](spec/owner-connection-v1.md),
[local service ownership](spec/local-service-ownership.md),
[resources](spec/resources.md) and [browser images](spec/hub-browser-images.md).
Historical plans and dated validation records describe their recorded scope;
they are not proof that a current security control works.
