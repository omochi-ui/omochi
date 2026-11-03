# Dashboard sharing

The hosted dashboard at `https://app.omochi.fyi/` connects to a running native
hub over iroh. Deployment is defined in [apps/dashboard](../apps/dashboard/README.md).
The local HTTP/WS/SSE dashboard remains available on loopback, including offline
operation. Gateway v4, gateway trust and SDK transports are unchanged.

## Start the hub and create shares

```sh
omochi-ui hub --remote --web-url https://app.omochi.fyi/
```

For self-hosting, substitute the client's HTTPS base, including its trailing slash.
Remote access is opt-in and works without registered remote gateways. The hub
uses `https://relay.omochi.fyi` by default in release builds and n0 defaults in
debug builds; `--relay HTTPS_URL` overrides this selection. The browser follows
the sharing link’s relay URLs and sends the bundled shared admission token only
to the exact production relay URL. This relay admission never grants dashboard
access; the hub still validates each share credential or registered identity.
HTTP/WS keeps its existing loopback binding and Origin checks. `--remote`
conflicts with `--offline`, which preserves local operation without public
address lookup or relay access.

Open the local dashboard, save a board and select **Share dashboard**. Wait for
the hub's relay address, then choose:

- **Invite one browser**: optionally allow app interaction and/or editing names,
  positions and sizes. The first accepting browser identity is registered. Issue
  a separate invitation per recipient. Invitations expire after 3 minutes;
  accepted access lasts until revoked.
- **Anyone with the link**: reusable readonly access with no identity enrollment.
  Anyone receiving a forwarded URL can use it.

Copy the URL when created; only its credential digest is stored on the hub.
Both modes expose the permitted Surfaces' complete state and published images.
Shared editors cannot add/remove views, change source references, delete the
board or manage shares. App interaction can change application business state.

Shares follow the saved board: owner additions become visible, removals withdraw
views and deletion withdraws the board. Concurrent saves retain native revision
conflicts and unsaved drafts. Disconnected sources remain unavailable.

**Revoke access…** revokes one registered recipient or an entire public link.
Success waits for live listeners to stop old sessions and queued work. A timeout
remains an error while removal stays durable. Already delivered bytes and
dispatched actions cannot be recalled; refresh the list after an error and check
unresponsive hub processes.

## Browser identity and recovery

Invitation material is removed from history before UI/transport imports. Private
invitation secrets are not saved. Identity is committed to IndexedDB before
enrollment; pending connections keep the same identity after a lost reply.
**Retry existing access** checks authorization without redeeming another invite.
**Retry connection save** retries only the local transaction after a save failure.

One registered tab owns the endpoint through Web Locks. Another tab reports
`OwnerBusy`; close the active owner or retry after its exit. Corrupt identity
requires explicit reset. Reset/forget removes only local records, leaving boards
and hub grants intact. Ask the owner to revoke old access after losing identity.
Changing the hosted origin or clearing storage requires new enrollment.

Public reload keeps bearer context only in that tab's sessionStorage. If storage
is denied, reopen the original URL. Registered access needs IndexedDB and Web
Locks; initial support is desktop Linux Chrome/Chromium 156+ in a secure context.

Images use scoped native fetch/validation and bounded browser decoding. A
canonical complete URI in authoritative Surface state publishes an image;
arbitrary IDs, constructed URLs or embedded Markdown destinations alone do not.
Unavailable images and remote audio/video show explicit unavailable states;
local media behavior is preserved.

See the [protocol](../spec/dashboard-sharing-v1.md),
[local acceptance](dashboard-sharing-acceptance.md) and
[Workers deployment guide](../apps/dashboard/README.md).

## Verify relay access

Run the controlled TLS relay acceptance test, then reuse its built artifacts to
check browser authentication and sharing through the deployed production relay:

```sh
bun run test:dashboard-sharing
SHARING_PRODUCTION_RELAY=1 bun tests/browser/sharing.mjs
```

The production mode uses isolated local SDK/hub/browser state and a local HTTPS
static host. It checks the deployed relay without stopping or restarting it;
the controlled mode verifies relay interruption and recovery. Set
`SHARING_RESULTS` to retain sanitized reports and screenshots. This does not
deploy or verify the hosted Cloudflare application.

## Connect as an owner

Start the hub with `--remote --web-url https://app.omochi.fyi/`. In the local
Connections section, use **Register owner browser** and open the resulting
three-minute link in the target browser. Accept explicitly. Owner access permits
all dashboards, panel additions/removals, app interaction and share management.
Owner registration and revocation are available only on the local screen.

After enrollment, reloading uses the saved browser identity rather than the
invitation secret. Saved hub connections can be selected one at a time; unsaved
edits require confirmation before disconnecting. Forgetting a local connection
or resetting browser identity does not revoke its Hub grant. Revoke it from
**Owner browser access** on the local Hub. The local HTTP/WS dashboard continues
to work offline without owner enrollment.

The [owner protocol](../spec/owner-connection-v1.md) describes authority,
operations, persistence and revocation. `test:dashboard-sharing` includes owner
enrollment, dashboard creation, sharing, reload and local revocation; its
production relay mode exercises the same owner path.
