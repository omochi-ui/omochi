# Omochi Mobile landing page

Minimal Hono + Vite landing page at `https://mobile.omochi.fyi/`, hosted on
Cloudflare Worker **mobile-site**. The logo matches `apps/site/public/assets/mark.svg`.
This private app is separate from SDK/native releases.

Run from the repository root with the shared Bun lockfile:

```sh
bun install --frozen-lockfile
bun run dev:mobile-site
bun run check:mobile-site
bun run build:mobile-site
bun run --cwd apps/mobile-site verify
bun run --cwd apps/mobile-site deploy --dry-run
bun run preview:mobile-site
```

The [Mobile Site workflow](../../.github/workflows/mobile-site.yml) validates
relevant pull requests and pushes to `main`. Successful `main` runs deploy the
verified `dist/` artifact using the shared **cloudflare-workers** environment
and its `CLOUDFLARE_ACCOUNT_ID` / `CLOUDFLARE_API_TOKEN` secrets. Manual runs on
`main` also deploy; other branches only validate.

Wrangler declares `mobile.omochi.fyi` as a custom domain and provisions DNS/TLS.
Disconnect any Workers Builds repository connection for **mobile-site** so
GitHub Actions is its only publisher. See [deployment setup](../../deploy/README.md).
For a manual deployment after building and authenticating, run
`bun run deploy:mobile-site`.

The landing page can be deployed now. Store support is currently blocked, so
HTTPS links do not launch the native app. Only `/` is implemented; `/open`,
`apple-app-site-association` and `assetlinks.json` are not served. The remaining
handoff setup is described in [the mobile deep-link guide](../mobile/deeplinks/README.md).
