---
title: "Connect machines"
description: "Connect machines · omochi"
sidebar:
  order: 5
---

Remote access has two boundaries: a Gateway admits a trusted Hub to its apps,
and a native Hub admits owner or shared-dashboard clients. Discovery and relay
admission do not grant either permission.

## Connect a Gateway to a native Hub

Install the native tools on both machines. On the machine hosting applications:

```sh
omochi-ui gateway --remote --pair --name workstation
```

On the Hub machine:

```sh
omochi-ui pair --name workstation
# Paste the invitation on stdin, then Ctrl+D.
omochi-ui hub
```

The Gateway prints a native `omochi://invite/<payload>` link, an HTTPS
`https://app.omochi.fyi/#g:<payload>` link and a QR. `pair` accepts either link.
Both carry the same unpadded base64url JSON and secret; legacy hex and `#connect:`
links are rejected. Stdin keeps the secret out of shell history.

The invitation admits one Hub and expires after 180 seconds. The first
redeeming authenticated Hub wins; that identity may retry within the lifetime.
Its authority covers all exposed services, actions and resources on that Gateway.
Registered Gateways reconnect by full iroh EndpointId. Each additional Hub needs
its own invitation.

## Connect from a hosted browser

On the Hub machine:

```sh
omochi-ui hub --remote
```

Open its local dashboard at `http://127.0.0.1:5555/`. In Connections, choose
**Register owner browser**, open the resulting link in the target browser and
accept explicitly. The link expires after three minutes; accepted owner access
lasts until revoked. The hosted browser connects to this native Hub, which must
keep running. `--web-url HTTPS_BASE` selects a custom static client.

Owner access permits all dashboards, panel/source changes, app interaction,
sharing and Gateway registration. Issue/revoke owner grants only from the local
Hub screen. For one-board access, use a [dashboard share](../dashboards/#share-a-saved-dashboard).

To add a Gateway from that browser, run `gateway --remote --pair` on the app
machine, open its printed HTTPS URL or scan the QR, select your owner Hub and
choose **Add to this hub**. This enrolls the native Hub at the Gateway using the
Hub's identity. Shared viewers cannot register Gateways.

Registration stays tied to the initiating owner grant and browser identity.
After disconnecting, reconnect with that authority to inspect the job. A
save-pending result can retry the Hub's local trust save without redeeming again.
Dismissing the invitation does not cancel work already started on the Hub.

## Flutter app

The [mobile and desktop app](https://github.com/omochi-ui/omochi/blob/main/apps/mobile/README.md)
can accept a Gateway invitation with **Add to this device** to enroll its embedded
Rust Hub. It can also open/paste owner/share links to an external Hub; selecting
one suspends the embedded Hub. Its identity is separate from the browser's, so
issue a new identity invitation for the app.

Invitation paste and custom `omochi://` handoff are supported. HTTPS links do not
currently launch the app directly; `mobile.omochi.fyi` serves a landing page only.
See the app guide for platform setup and validation limits.

## Network settings

Release builds use `https://relay.omochi.fyi` with built-in shared-token admission;
debug builds use n0 public relays. Both use n0 public address lookup when online.
`--relay` replaces the relay set; use `--relay https://relay.omochi.fyi` to test
production from a debug build. The built-in token is sent only to the exact
production relay URL. The browser follows its Hub link's relay hints.

`--offline` disables public lookup and relays while keeping direct connections
and LAN mDNS. An unavailable Gateway does not block other peers or local services.
`hub --remote` conflicts with `--offline`. Keep the HTTP/WS loopback bind and
Origin checks; hosted iroh access does not require exposing the HTTP server.

## Inspect and revoke

```sh
omochi-ui status
omochi-ui trust list
omochi-ui trust forget NAME_OR_ENDPOINT_ID
```

Gateway trust is distinct from Hub owner and share grants. Use `trust forget`
on the Gateway to revoke a Hub's access to its apps; manage owner/share grants
on the Hub. Removing a client's local connection record does not revoke the
server-side grant.

Revocation waits for live processes to acknowledge authorization updates and
destroy revoked connections. Acknowledgement failure remains an error after
durable removal. Already delivered data and executed actions cannot be recalled.
Legacy trust is not migrated: pair again after the native breaking upgrade.

See [sharing and recovery](https://github.com/omochi-ui/omochi/blob/main/docs/dashboard-sharing.md)
and the [current protocol index](https://github.com/omochi-ui/omochi/blob/main/spec/README.md).
