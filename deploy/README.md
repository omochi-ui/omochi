# Deployment

| Component | Configuration and instructions |
| --- | --- |
| `omochi.fyi` site | [apps/site](../apps/site/README.md): Cloudflare Workers, published by the Site workflow |
| `app.omochi.fyi` dashboard | [apps/dashboard](../apps/dashboard/README.md): Cloudflare Workers, published by the Dashboard workflow |
| Docker | [Docker Compose](docker/README.md): local hub + Python example, native GHCR images |
| Production iroh relay | [relay](relay/README.md): iroh-relay, systemd and monitoring templates |

## GitHub Actions setup

Both web workflows build and validate on relevant pull requests and `main`
pushes. Only a successful `main` run can deploy, including a manual
`workflow_dispatch` run selecting `main`. Each deploy job downloads its own
workflow's build artifact and uses the **cloudflare-workers** GitHub environment.
Production runs are serialized per workflow; a newer push does not cancel an
in-progress deployment. PR checks can cancel older checks for the same PR.

1. In the GitHub repository, open **Settings → Environments → cloudflare-workers**
   (create the environment if needed).
2. Add these **environment secrets**:
   - `CLOUDFLARE_ACCOUNT_ID`: the account managing the `omochi.fyi` zone and both Workers.
   - `CLOUDFLARE_API_TOKEN`: a token with the **Edit Cloudflare Workers** permissions,
     scoped to that account and zone. See Cloudflare's
     [GitHub Actions authentication guide](https://developers.cloudflare.com/workers/ci-cd/external-cicd/github-actions/).
3. Set the environment's deployment branch rule to allow `main`. Any configured
   environment approval rules apply to the deploy jobs, not the build jobs.
4. For **omochi-site**, disconnect the existing Workers Builds repository connection
   in Cloudflare under **Workers & Pages → omochi-site → Settings → Builds → Disconnect**.
   Do the same for **omochi-dashboard** if it has a connection. This prevents two
   systems publishing the same Worker; keep the Workers and their custom domains.
   See [disconnecting builds](https://developers.cloudflare.com/workers/ci-cd/builds/#disconnecting-builds).
5. Merge the workflows to `main`, then run **Site** and **Dashboard** from the
   Actions tab on `main` to verify the first automated deployments. Later pushes
   affecting each app trigger its workflow automatically.

The existing Wrangler deployments and domains are reused; no initial deployment
or `wrangler login` is needed in CI. Credentials are exposed only to each deploy
step, and are not stored in source or artifacts.

## Native hub and relay

The hosted dashboard serves browser assets. Saved boards, app state and share
authorization live on the owner's running native hub:

```sh
omochi-ui hub --remote
```

The web URL defaults to `https://app.omochi.fyi/`; use `--web-url HTTPS_BASE`
to select a custom hosted client.

Release builds use `https://relay.omochi.fyi`; debug builds use n0 defaults.
The hosted browser follows the sharing link’s relay URLs and authenticates only
to the exact production relay URL. Local HTTP/WS/SSE remains usable offline. See [dashboard sharing](../docs/dashboard-sharing.md)
and its [local acceptance record](../docs/dashboard-sharing-acceptance.md).
