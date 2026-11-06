# Native 0.3.2

Released 2026-11-07. Changes since `native-v0.3.1`.

- Bundled the updated dashboard with sidebar language settings, compact connection
  controls, consistent dialogs and cleaner board view. Hosted clients additionally
  expose browser identity settings with copy, reset and profile-loading recovery.
- Updated owner/gateway invitation flows to preserve the current board and unsaved
  edits, and fixed recovery after registration or local-save failures, duplicate
  invitations and browser profile contention.
- Improved keyboard focus, narrow-screen controls and Japanese/English native-app
  handoff labels, and removed stale invitation actions.
- Reorganized the English and Japanese user documentation and references.

The native server implementation and wire protocols have no changes in this
release. The native version advances to distribute the new embedded dashboard in
the CLI, Cargo packages, platform archives and Docker image. See the
[SDK/browser 0.2.2 notes](sdk-0.2.2.md) for the complete dashboard changes.

**Upgrade from 0.3.1:** no re-pairing, shared-link replacement or data migration is
required. Install 0.3.2 and restart the Hub to serve the updated dashboard.
Users upgrading from 0.3.0 or earlier must also follow the
[0.3.1 migration notes](native-0.3.1.md).

```sh
npm install -g @omochi-ui/cli@0.3.2 @omochi-ui/utils@0.3.2
# Or install from Rust sources:
cargo install omochi-cli --version 0.3.2 --locked
```

The Docker image is `ghcr.io/omochi-ui/omochi:0.3.2`, for `linux/amd64` and
`linux/arm64`. There is no `latest` image tag. npm packages become installable
after maintainer approval of their staged uploads.

See the [native release procedure](../native-releasing.md) and
[full comparison](https://github.com/omochi-ui/omochi/compare/native-v0.3.1...native-v0.3.2).
