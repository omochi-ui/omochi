# app.omochi.fyi

Cloudflare Workers deployment of the native-hub client dashboard. The Worker
serves static assets and applies the shared [header policy](headers.json).
Dashboard state and authorization remain on each owner's native hub.

## Deploy

From the repository root, install Bun 1.4.3+, Node.js and Rust, then prepare the
locked WASM toolchain:

```sh
bun install --frozen-lockfile
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
cd apps/dashboard
bunx wrangler login
bunx wrangler deploy
```

`wrangler deploy` runs the configured build (`bun run build:hosted` at the
repository root), then uploads the Worker and the complete
`packages/hub-ui/dist-hosted/` artifact together. `WASM_BINDGEN` may point to an
existing wasm-bindgen 0.2.130 binary. No hand-edited generated files are needed.

Deploy to the Cloudflare account managing the active `omochi.fyi` zone. The
[Wrangler config](wrangler.jsonc) declares `app.omochi.fyi` as a Custom Domain;
Cloudflare manages its DNS record and TLS certificate. If the account has multiple
choices, set `CLOUDFLARE_ACCOUNT_ID`. CI may authenticate with
`CLOUDFLARE_API_TOKEN` instead of interactive login; keep credentials outside git.
`workers.dev` and version preview URLs are disabled to keep one production origin.

## Local checks

```sh
bun run check
bunx wrangler deploy --dry-run
bun run test:preview
bunx wrangler dev --ip 127.0.0.1
```

The dry run builds and bundles without publishing. `test:preview` verifies the
actual local Workers runtime and asset binding. Local development serves the
same Worker and assets. CSP, MIME and caching are applied in `src/index.ts` from
`headers.json`; HTML and stable `iroh/` paths revalidate, while hashed assets have
immutable caching. Only `/` maps to the HTML entry; unknown paths, missing assets
and API/WS/resource routes return 404. There is no SPA fallback or app-data proxy.

After deploying, check `/`, a hashed JS/CSS asset and `/iroh/iroh_bg.wasm` for the
configured headers, and `/api/dashboards` and `/missing.js` for 404. Then exercise
sharing from a separate network; the [local acceptance record](../../docs/dashboard-sharing-acceptance.md)
does not establish public-host behavior.

## Connect a hub

```sh
omochi-ui hub --remote --web-url https://app.omochi.fyi/
```

Create links in the local dashboard's **Share dashboard** dialog. The hub uses
existing n0 relays by default; a project-owned relay is [optional](../../deploy/relay/README.md).
See [sharing and recovery](../../docs/dashboard-sharing.md) for permissions,
identity persistence and browser requirements.

Cloudflare references: [Static Assets](https://developers.cloudflare.com/workers/static-assets/),
[custom builds](https://developers.cloudflare.com/workers/wrangler/custom-builds/),
[Custom Domains](https://developers.cloudflare.com/workers/configuration/routing/custom-domains/).
