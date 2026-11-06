# SDK and browser packages 0.2.1

Released 2026-11-06. Changes since `v0.2.0`.

- Updated `@omochi-ui/hub-ui` to create and open reusable v2 sharing links.
  App interaction and layout editing can be enabled independently. Links work
  across browsers and native clients without shared-client identity enrollment.
- Simplified sharing controls and updated Japanese/English permission and
  invalid-link messages. Forwarded links carry the same permissions as the
  original link.
- Updated the browser transport to use `omochi/dashboard/2` and the unified
  `share` mode. Owner enrollment continues to use the existing v1 protocol.
- Updated dashboard, remote access, CLI and media documentation.

**Upgrade note:** use a matching native Hub 0.3.1 and current hosted/self-hosted
client. Existing v1 sharing links and registered shared-client grants are
retired; owners must issue replacement links. Owner access and Gateway trust
are preserved. See the [native upgrade notes](native-0.3.1.md).

TypeScript, Python and Rust SDK application behavior, core transport helpers
and renderer behavior have no source changes in this release. Their versions
advance together with hub-ui under the existing SDK/browser release policy.
Native CLI/utils are released separately as 0.3.1. npm packages become
installable after maintainer approval of their staged uploads.

See the [SDK/browser release procedure](../sdk-releasing.md) and
[full comparison](https://github.com/omochi-ui/omochi/compare/v0.2.0...v0.2.1).
