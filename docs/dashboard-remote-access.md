# Remote access and dashboard clients

The normal hosted dashboard at `https://app.omochi.fyi/` connects to a running
native Hub over iroh. The Hub owns dashboards, aggregates Gateways and enforces
owner/share permissions. The static host serves UI assets; it does not proxy
application data. Local HTTP/WS/SSE remains on loopback by default.

```text
SDK apps -- Unix JSONL --> Gateway -- Unix / iroh --> Native Hub
                                                       |-- HTTP/WS/SSE --> Local browser
                                                       +-- iroh --> Hosted browser / external-Hub app

SDK apps -- Unix JSONL --> Gateway -- iroh --> Flutter app's embedded Rust Hub
```

## Choose a connection

| Goal | Setup | Authority |
| --- | --- | --- |
| Use apps on this machine | `omochi-ui hub` | Local HTTP owner; a local Gateway starts if needed |
| Add a remote machine's apps | `gateway --remote --pair`, then import on the Hub | That Hub is trusted for the Gateway's exposed services, actions and resources |
| Manage a Hub from a hosted browser | `hub --remote`, then **Register owner browser** locally | All dashboards, app interaction, layout/source changes, sharing and Gateway registration |
| Share one saved dashboard | **Share dashboard** on a remote-enabled Hub | Selected board; links may independently allow interaction and layout editing |
| Use apps directly from the Flutter app | Accept a Gateway invitation with **Add to this device** | Embedded Hub is trusted at that Gateway |
| Use an external Hub from the Flutter app | Open/paste an owner or share invitation | The external Hub's owner/share permissions |

## Gateway enrollment

On the application machine:

```sh
omochi-ui gateway --remote --pair --name workstation
```

The command prints a native `omochi://invite/<payload>` invitation, an HTTPS
`#g:<payload>` link and a QR. Both forms carry the same unpadded base64url JSON
and temporary secret. They expire after 180 seconds; the first authenticated Hub
to redeem one wins. Legacy hex and `#connect:` links are not accepted.

On a native Hub machine, use `omochi-ui pair --name workstation`, paste either
invitation form on stdin, then start `omochi-ui hub`. For a hosted browser,
connect as an owner first, open the gateway link and choose **Add to this hub**.
This registers the native Hub, using its identity. Shared viewers cannot register
Gateways. The Flutter app can instead enroll its own embedded Hub from the same
invitation flow; issue a separate invitation for each Hub identity.

Enrollment jobs remain tied to their initiating owner grant and browser identity.
Reconnect with that authority to inspect an uncertain result. A save-pending
result retries the local save without redeeming the invitation again. Dismissing
the browser prompt does not cancel enrollment already started by the Hub.

See the bilingual [remote guide](../apps/site/src/content/docs/en/docs/guides/remote.md)
and [Gateway v4 contract](../spec/gateway-v4.md).

## Owner and share connections

Start `omochi-ui hub --remote`, open its local dashboard and issue the required
link. `--web-url HTTPS_BASE` selects a self-hosted static client instead of
`https://app.omochi.fyi/`; it does not change the local HTTP bind or Origin policy.

Owner links use `#owner:` and are issued/revoked only through local owner
administration. Sharing v2 links use `#share:` and reusable bearer credentials, with independent
interaction and layout-editing permissions. Forwarded copies carry the same access. Share editing permits names,
titles, positions and sizes, while adding/removing views and changing sources
requires owner authority. Each share follows the saved board's current membership.

Browser identity is committed to IndexedDB before enrollment. Secrets from owner
invitations are not saved. One tab owns the endpoint through Web
Locks. Public-link reload context is kept in that tab's sessionStorage.
Forgetting a local record does not revoke server-side access. Revocation succeeds
only after live listener teardown is acknowledged; failure remains an error after
durable grant removal.

Use the [sharing guide](dashboard-sharing.md) for setup and recovery, and the
[sharing](../spec/dashboard-sharing-v2.md) and
[owner](../spec/owner-connection-v1.md) contracts for exact operations.

## Flutter client

The [Flutter app](../apps/mobile/README.md) has an embedded Rust Hub for direct
Gateway access and a separate external-Hub client for owner/share connections.
Only one connection is displayed; selecting an external Hub suspends the embedded
Hub. External-client identity and saved records have their own protected-storage
namespace. Explicitly saved public shares also store their bearer credential;
owner and identity enrollment secrets are never saved.

Custom `omochi://` links and pasted invitations are supported. HTTPS app links
are not enabled, and `mobile.omochi.fyi` currently serves a landing page only.
See the [mobile validation records](../apps/mobile/docs/validation.md) and
[external connection checks](../apps/mobile/docs/remote-connections-validation.md)
for platform coverage and remaining limits.

## Networking and validation

Remote access is opt-in. Release builds use `https://relay.omochi.fyi`; debug
builds use n0 relays. `--relay` overrides the selection. Relay admission is
separate from application authorization. `hub --remote` conflicts with
`--offline`; ordinary offline Hub/Gateway operation retains direct/LAN discovery.

[Deployment](../deploy/README.md), [hosted dashboard setup](../apps/dashboard/README.md)
and [relay operation](../deploy/relay/README.md) describe distribution and operation.
[Sharing acceptance](dashboard-sharing-acceptance.md) records local browser and
controlled-relay evidence; it does not establish public-host or separate-network
acceptance. Browser support and mobile platform coverage have their own limits.

## Direct-browser experiment and history

Direct browser-to-Gateway aggregation, browser-local boards and PNG resources
remain opt-in under [`experiments/browser-dashboard`](../experiments/browser-dashboard/README.md).
This uses gateway-wide trusted-Hub access rather than native Hub share grants.
Use its README for current commands and its dated acceptance for verified scope.

The [archived implementation plan](dashboard-remote-access.browser-experiment.md),
[reviewed proposal](dashboard-remote-access.browser-v5-reviewed.md),
[earlier native-Hub proposal](dashboard-remote-access.server-hub-v1.md) and
[review history](reviews/dashboard-remote-access/review-state.md) preserve design
history. The [specification index](../spec/README.md) separates current contracts
from experiment and review snapshots.
