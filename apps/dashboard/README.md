# app.omochi.fyi

Cloudflare Workers deployment of the native-hub client dashboard. The Worker
serves static assets and applies the shared [header policy](headers.json).
Dashboard state and authorization remain on each owner's native hub.

## GitHub Actions deployment

The [Dashboard workflow](../../.github/workflows/dashboard.yml) checks the Worker,
builds the hosted dashboard and WASM, dry-runs Wrangler, and verifies the local
Worker with the built assets. Relevant pull requests run those checks without
Cloudflare credentials. Pushes to `main` also deploy the verified `dashboard`
artifact to Worker **omochi-dashboard**, using the GitHub environment
**cloudflare-workers** and its `CLOUDFLARE_ACCOUNT_ID` / `CLOUDFLARE_API_TOKEN`
secrets. See the shared [setup instructions](../../deploy/README.md#github-actions-setup).

To redeploy manually, run **Dashboard** from GitHub Actions with branch **main**.
Other branches only validate. The build job installs Rust with the WASM target
and wasm-bindgen 0.2.130; the deploy job uploads the artifact without rebuilding.
Workers Builds is not used.

## Manual deployment

From the repository root, install Bun 1.4.3+, Node.js and Rust, then prepare the
locked WASM toolchain:

```sh
bun install --frozen-lockfile
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
cd apps/dashboard
bunx wrangler login
bun run deploy
```

`bun run deploy` runs the build (`bun run build:hosted` at the
repository root), then uploads the Worker and the complete
`packages/hub-ui/dist-hosted/` artifact together. `WASM_BINDGEN` may point to an
existing wasm-bindgen 0.2.130 binary. No hand-edited generated files are needed.
Direct `bunx wrangler deploy` only bundles the Worker and uploads existing assets;
it does not build the dashboard. CI uses this after downloading the checked artifact.

Deploy to the Cloudflare account managing the active `omochi.fyi` zone. The
[Wrangler config](wrangler.jsonc) declares `app.omochi.fyi` as a Custom Domain;
Cloudflare manages its DNS record and TLS certificate. If the account has multiple
choices, set `CLOUDFLARE_ACCOUNT_ID`. CI may authenticate with
`CLOUDFLARE_API_TOKEN` instead of interactive login; keep credentials outside git.
`workers.dev` and version preview URLs are disabled to keep one production origin.

## Local checks

```sh
bun run check
bun run dry-run
bun run test:preview
bun run dev
```

The dry run builds and bundles without publishing. `test:preview` verifies the
actual local Workers runtime and asset binding. Local development serves the
same Worker and assets. CSP, MIME and caching are applied in `src/index.ts` from
`headers.json`; HTML and stable `iroh/` paths revalidate, while hashed assets have
immutable caching. Only `/` maps to the HTML entry; unknown paths, missing assets
and API/WS/resource routes return 404. There is no SPA fallback or app-data proxy.
`dev` builds the dashboard once before starting Wrangler; rerun `bun run build`
after changing dashboard or transport sources to refresh the served assets.

After deploying, check `/`, a hashed JS/CSS asset and `/iroh/iroh_bg.wasm` for the
configured headers, and `/api/dashboards` and `/missing.js` for 404. Then exercise
sharing from a separate network; the [local acceptance record](../../docs/dashboard-sharing-acceptance.md)
does not establish public-host behavior.

## Connect a hub

```sh
omochi-ui hub --remote
```

The web URL defaults to `https://app.omochi.fyi/`; use `--web-url HTTPS_BASE`
to select a custom hosted client.

For full management, use **Register owner browser** in the local Connections
section and explicitly accept the resulting link in the hosted browser. Owner
registration and revocation remain local-only. Once connected as an owner, the
hosted browser can add Gateways using the link/QR from
`omochi-ui gateway --remote --pair`; choose **Add to this hub** to register the
native Hub, rather than the browser, at that Gateway.

For board-limited access, create links in the local dashboard's **Share dashboard** dialog. Release builds use `https://relay.omochi.fyi` by default; debug builds use n0
relays. The browser uses the relay URLs in the sharing link and sends the shared
admission token only to the exact production relay URL. Use
`--relay https://relay.omochi.fyi` to test production with a debug hub.
See [relay operation](../../deploy/relay/README.md).
See [sharing and recovery](../../docs/dashboard-sharing.md) for permissions,
identity persistence and browser requirements.

Cloudflare references: [Static Assets](https://developers.cloudflare.com/workers/static-assets/),
[custom builds](https://developers.cloudflare.com/workers/wrangler/custom-builds/),
[Custom Domains](https://developers.cloudflare.com/workers/configuration/routing/custom-domains/).
