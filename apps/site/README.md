# omochi.fyi

Astro + Starlight, served as static assets on Cloudflare Workers. Japanese is at `/` and `/docs/`; English is at `/en/` and `/en/docs/`. This app is private and does not participate in SDK/native releases.

## Develop

Run from the **repository root**, using the root Bun lockfile:

```sh
bun install --frozen-lockfile
bun run dev:site
bun run check:site
bun run build:site
bun run --filter @omochi-ui/site verify
bun run preview:site
```

Node.js 22.12+ (Node 24 recommended) is required by Astro; Bun 1.4.3+ manages the workspace. Site builds need neither Rust nor compiled SDK packages. Output and generated `.astro/` state are ignored. For a background development server, follow this directory's AGENTS.md.

## Content

Edit Japanese under `src/content/docs/docs/` and matching English pages under `src/content/docs/en/docs/`. Keep slugs and structure aligned. Markdown frontmatter controls titles, descriptions, and sidebar order. Starlight provides search, copy buttons, navigation, TOC, and locale switching. LP copy lives in `src/i18n/ja.ts` and `src/i18n/en.ts`. Japanese defines the dictionary shape; English uses `satisfies Messages` so missing or misspelled keys fail type checking. Keep both dictionaries aligned when adding a section.

`src/components/Landing.astro` selects the dictionary using `Astro.currentLocale` and assembles the sections in `src/components/landing/`. `src/layouts/Site.astro` owns metadata, navigation, and the footer. Astro's `i18n` config is the single routing configuration for both custom pages and Starlight Docs; do not add a second Starlight `locales` config. Use `getRelativeLocaleUrl` from `astro:i18n` for language-aware links.

User guides in the site's content collection are authoritative; `docs/` at the repository root retains design, architecture, release, and development materials. Link to protocol source files instead of copying normative contracts. Installation text targets the current source checkout and verified release archives; confirm registry publication before adding versioned install commands.

## GitHub Actions deployment

The [Site workflow](../../.github/workflows/site.yml) checks, builds, verifies and
dry-runs the site on relevant pull requests and pushes to `main`. After successful
checks on `main`, a separate deployment job downloads the `site` artifact and
publishes it to Worker **omochi-site**. To redeploy manually, run **Site** from
GitHub Actions with branch **main**; manual runs on other branches only validate.

Both web apps use the GitHub environment **cloudflare-workers**, with environment
secrets `CLOUDFLARE_ACCOUNT_ID` and `CLOUDFLARE_API_TOKEN`. See the shared
[setup and Workers Builds migration](../../deploy/README.md#github-actions-setup).
The build job does not use that environment or Cloudflare credentials.

Wrangler declares `omochi.fyi` as a Worker custom domain in the active zone;
Cloudflare provisions DNS/TLS. Keep Pages and other Workers from claiming the
same hostname. Publication is owned by GitHub Actions, so disconnect the old
Workers Builds repository connection before enabling this workflow's deployment.

## Local Worker verification / manual deployment

```sh
bun run build:site
bun run --cwd apps/site deploy --dry-run
bun run --cwd apps/site preview:workers
# After authenticating Wrangler to the intended account:
bun run deploy:site
```

The Worker has no server entry point or bindings. `dist/404.html` handles unknown URLs with HTTP 404. Trailing slashes are canonical. The production origin used by sitemap, alternate links, and canonical metadata is `https://omochi.fyi`.

After deploying, verify both LPs, a deep Docs URL, language switching, search in both languages, and an unknown path on the deployed domain. Previous successful deployments can be restored from the Worker deployment history.
