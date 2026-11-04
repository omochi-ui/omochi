# Deployment

| Component | Configuration and instructions |
| --- | --- |
| `app.omochi.fyi` dashboard | [apps/dashboard](../apps/dashboard/README.md): Cloudflare Workers; run `bunx wrangler deploy` there |
| Docker | [Docker Compose](docker/README.md): local hub + Python example, native GHCR images |
| Production iroh relay | [relay](relay/README.md): iroh-relay, systemd and monitoring templates |

The hosted dashboard serves browser assets. Saved boards, app state and share
authorization live on the owner's running native hub:

```sh
omochi-ui hub --remote --web-url https://app.omochi.fyi/
```

Release builds use `https://relay.omochi.fyi`; debug builds use n0 defaults.
The hosted browser follows the sharing link’s relay URLs and authenticates only
to the exact production relay URL. Local HTTP/WS/SSE remains usable offline. See [dashboard sharing](../docs/dashboard-sharing.md)
and its [local acceptance record](../docs/dashboard-sharing-acceptance.md).
