# SDK and browser packages 0.2.2

Released 2026-11-07. Changes since `v0.2.1`.

- Added sidebar settings for language preferences. Hosted dashboards also show,
  copy and reset the browser identity there. Opening settings from a public share
  loads the registered profile without replacing the active share or changing its
  permissions; profile-loading failures expose a retry action.
- Simplified the dashboard header into a connection summary with app counts and
  a connection-management popup. Readable names take precedence over identifiers,
  which remain available in details. Board view hides editing and sharing notices.
- Moved owner and gateway invitations into dialogs that preserve the active board
  and unsaved edits. Accepting a connection change asks before discarding edits;
  dismissing an invitation keeps the current session.
- Fixed recovery after failed owner registration, local credential saves and
  profile locks held by another tab. Duplicate invitations no longer replace
  pending save recovery, and connection teardown ignores late results.
- Standardized dialog close controls, keyboard dismissal and focus restoration,
  and improved narrow-screen layouts. Storage failures during dismissal keep the
  invitation and recovery controls visible.
- Localized native-app handoff actions and removed stale invitation banners after
  React takes over. Active sharing links retain their own native-app handoff.
- Reorganized English and Japanese installation, CLI, SDK and mobile documentation,
  with dedicated typed-input, transport and catalog-generation references.

**Upgrade:** this is a compatible dashboard update. Sharing links, saved boards,
owner access and Gateway trust require no migration from 0.2.1 / native 0.3.1.
The sharing and Gateway protocols are unchanged.

TypeScript, Python and Rust SDK application behavior, core transport helpers and
renderer behavior have no implementation changes. Their versions advance together
with hub-ui under the existing SDK/browser release policy. Native CLI/utils ship
the updated dashboard separately as 0.3.2. npm packages become installable after
maintainer approval of their staged uploads.

See the [SDK/browser release procedure](../sdk-releasing.md) and
[full comparison](https://github.com/omochi-ui/omochi/compare/v0.2.1...v0.2.2).
