# Dashboard sharing

The hosted dashboard at `https://app.omochi.fyi/` connects to a running native
hub over iroh. Deployment is defined in [apps/dashboard](../apps/dashboard/README.md).
The local HTTP/WS/SSE dashboard remains available on loopback, including offline
operation. Gateway v4, gateway trust and SDK transports are unchanged.

## Start the hub and create shares

```sh
omochi-ui hub --remote
```

The web URL defaults to `https://app.omochi.fyi/`. For self-hosting, set
`--web-url https://your-host.example/dashboard/`, including its trailing slash.
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

Create a reusable sharing link. App interaction and layout editing can be allowed
independently; both are disabled by default. Anyone with the link, including
forwarded copies, receives the same permissions. To change permissions, revoke
the link and issue a new one. Links remain usable until revoked.

Copy the URL when created; only its credential digest is stored on the hub.
Links expose the permitted Surfaces' complete state and published images.
Shared editors cannot add/remove views, change source references, delete the
board or manage shares. App interaction can change application business state.

Shares follow the saved board: owner additions become visible, removals withdraw
views and deletion withdraws the board. Concurrent saves retain native revision
conflicts and unsaved drafts. Disconnected sources remain unavailable.

**Revoke access…** revokes an entire sharing link.
Success waits for live listeners to stop old sessions and queued work. A timeout
remains an error while removal stays durable. Already delivered bytes and
dispatched actions cannot be recalled; refresh the list after an error and check
unresponsive hub processes.

## Browser identity and recovery

Sharing secrets are removed from history before UI/transport imports. Warm link
navigation confirms discarding unsaved edits and closes the previous connection
before opening the next. Reload restores bearer context only from that tab's
sessionStorage. If storage is denied, reopen the original URL.

Owner enrollment still uses a durable browser identity in IndexedDB, owned by
one tab through Web Locks. Owner invitations require explicit acceptance and
expire after three minutes. Local forget/reset does not revoke Hub authority.

All v1 sharing links and identity registrations are retired on upgrade. The Hub
clears v1 sharing records under the lock and advances its generation. Reissue
links; no old identity grant is converted to bearer authority. Owner grants and
Gateway trust are retained.

Images use scoped native fetch/validation and bounded browser decoding. A
canonical complete URI in authoritative Surface state publishes an image;
arbitrary IDs, constructed URLs or embedded Markdown destinations alone do not.
Unavailable images and remote audio/video show explicit unavailable states;
local media behavior is preserved.

See the [protocol](../spec/dashboard-sharing-v2.md),
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

## Add a Gateway to the owner Hub

On the application machine, run `omochi-ui gateway --remote --pair`. Open its
printed HTTPS link or scan its QR, connect the target owner Hub and choose
**Add to this hub**. The invitation registers that native Hub at the Gateway;
the hosted browser remains a Hub client. Shared viewers cannot perform this
operation. Gateway access covers all its exposed services, actions and resources.

The native URI and `#g:` HTTPS link contain the same unpadded base64url payload
and expire after 180 seconds. CLI `omochi-ui pair` accepts either form. Legacy
hex and `#connect:` invitations are rejected. `--web-url HTTPS_BASE` selects a
custom hosted client.

If disconnected during registration, reconnect with the same owner grant and
browser identity to check status. **Retry saving registration** retries only the
Hub's local trust save after accepted enrollment; it does not redeem again.
Dismissing the invitation does not cancel work already started on the Hub.

## Flutter clients

The [Flutter app](../apps/mobile/README.md#外部hubへの接続) accepts owner/share
links as an external-Hub client with the same server permissions. It displays one
selected connection and suspends its embedded Hub while using an external one.
It stores external-client identity separately in OS-protected storage. Explicitly
saved public shares retain their bearer credential there; this differs from the
browser's tab-scoped sessionStorage behavior.

Use the same v2 sharing link in browsers and the app. Local forget does not revoke Hub access.
See the [remote connection validation record](../apps/mobile/docs/remote-connections-validation.md).
