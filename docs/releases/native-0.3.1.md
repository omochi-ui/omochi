# Native 0.3.1

Released 2026-11-06. Changes since `native-v0.3.0`.

## Changes

- Unified dashboard invitations and public links into reusable sharing links.
  App interaction and layout editing can be enabled independently; both are
  disabled by default. Anyone holding a forwarded link receives the same
  permissions, without registering a recipient identity.
- Updated browser and Flutter clients to use the same v2 sharing links. Native
  clients can open a link once or explicitly save its credential in protected
  storage.
- Fixed native profile loading when old `invite` or `public` records are present.
  Valid legacy sharing records are removed while retaining the existing client
  identity, owner connections and v2 shares. Corrupt data and storage failures
  continue to report errors.
- Raised the downstream listener limit from 16 to 128 concurrent connections,
  shared across owner/shared sessions and unauthenticated handshakes. The limit
  of eight in-flight request streams per connection is unchanged.
- Updated Japanese and English guides, architecture documentation and the
  dashboard sharing contract.

## Upgrade

**Existing v1 sharing links and registered shared-client grants stop working.**
The Hub replaces recognized v1 sharing storage with an empty v2 store. Reissue
all dashboard sharing links after upgrading; old identity grants are never
converted into link credentials. Owner links/enrollment and Gateway trust are
preserved.

The sharing protocol is now `omochi/dashboard/2`; the sharing enrollment protocol
has been removed. `POST /api/shares` and owner `shareCreate` accept a board ID and
permissions without the old `public` field. Update custom integrations and use
matching browser/native clients. Gateway v4 and SDK application protocols have
not changed.

Back up persisted configuration before upgrading. Restart native services with
0.3.1, update clients, then create and distribute replacement sharing links.
For Docker, use the checkout at `native-v0.3.1` and run:

```sh
export OMOCHI_VERSION=0.3.1
docker compose -f deploy/docker/compose.yaml pull hub
docker compose -f deploy/docker/compose.yaml up --build -d --wait
```

The versioned image is `ghcr.io/omochi-ui/omochi:0.3.1`, for `linux/amd64` and
`linux/arm64`. There is no `latest` image tag. npm CLI/utils and native platform
packages use version 0.3.1; the separate SDK/browser release is 0.2.1. npm
packages become installable after maintainer approval of their staged uploads.

See the [sharing guide](../dashboard-sharing.md),
[v2 protocol](../../spec/dashboard-sharing-v2.md),
[native release procedure](../native-releasing.md) and
[full comparison](https://github.com/omochi-ui/omochi/compare/native-v0.3.0...native-v0.3.1).
