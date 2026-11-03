---
title: "Connect machines"
description: "Connect machines · omochi"
sidebar:
  order: 5
---

Remote access is opt-in. First build or install the native tools on both machines. Run on the machine hosting applications:

```sh
omochi-ui gateway --remote --pair --name workstation
```

On the hub machine:

```sh
omochi-ui pair --name workstation
# Paste the invitation on stdin, then Ctrl+D.
omochi-ui hub
```

The invitation admits one hub and expires after 180 seconds. Treat it as a secret. Once enrolled, registered gateways reconnect by their full iroh EndpointIds. Discovery locates identities; it never grants authorization.

Release builds use the production relay at `https://relay.omochi.fyi` with built-in shared-token authentication. Debug builds use n0 public relays for development. Both use n0 public address lookup when online. `--relay` overrides the relay set; use `--relay https://relay.omochi.fyi` to test production from a debug build. The built-in token is sent only to the production relay.

`--offline` disables public lookup and relays while keeping direct connections and LAN mDNS. An offline gateway does not block other peers or local services.

## Inspect and revoke

```sh
omochi-ui status
omochi-ui trust list
omochi-ui trust forget NAME_OR_ENDPOINT_ID
```

Revocation waits for live processes to acknowledge authorization updates and destroy revoked connections. Acknowledgement failure is an error even if the trust record was already removed. Do not assume it is safe to ignore the error. Legacy trust is not migrated: stop old services, pair again, and recreate saved dashboards after the native breaking upgrade.
