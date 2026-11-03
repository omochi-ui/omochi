# Deployment

| Component | Configuration and instructions |
| --- | --- |
| `app.omochi.fyi` dashboard | [apps/dashboard](../apps/dashboard/README.md): Cloudflare Workers; run `bunx wrangler deploy` there |
| Optional project-owned iroh relay | [relay](relay/README.md): iroh-relay, systemd and monitoring templates |

The hosted dashboard serves browser assets. Saved boards, app state and share
authorization live on the owner's running native hub:

```sh
omochi-ui hub --remote --web-url https://app.omochi.fyi/
```

The existing n0 relay defaults work without a project-owned relay. Local
HTTP/WS/SSE remains usable offline. See [dashboard sharing](../docs/dashboard-sharing.md)
and its [local acceptance record](../docs/dashboard-sharing-acceptance.md).
