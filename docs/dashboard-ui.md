# Dashboard UI and board view

The dashboard follows [the design system](design-system.md). It uses the existing
React renderer and CSS; dashboard API and persisted panel geometry are unchanged.

## Screenshots

Same seeded dashboard before and after the change:

![Previous dashboard](assets/dashboard-ui/before.png)
![Updated dashboard](assets/dashboard-ui/dashboard.png)

[Editing](assets/dashboard-ui/editing.png) · [Board view](assets/dashboard-ui/board.png) ·
[Mobile board after reordering](assets/dashboard-ui/mobile-board.png)

## Operations

- **Create dashboard** starts a draft. **Add view** lists connected app views by
  device and app; search has a clear action when nothing matches.
- **Save dashboard** validates names next to the input and keeps the draft on
  failure. A revision conflict never overwrites another client's dashboard.
  **Discard and reload** explicitly discards local edits after confirmation.
- **Cancel** and switching dashboards confirm only when the draft differs from
  its starting state. Deleting a dashboard requires confirmation; source apps
  and their views are not deleted.
- Removing a view offers **Undo removal**, with no timer. The next layout edit,
  add/remove, save or cancel ends that undo operation.
- Drag the grip to move a panel; drag any edge or corner to resize it. Preview
  geometry is derived from the starting layout, not the previous pointer event.
  Escape or pointer cancellation restores the starting geometry. The grid opts
  out of scroll anchoring, and movement accounts for document scrolling.
- **Adjust layout** provides buttons for moving and changing width/height without
  dragging. Arrow keys on the grip move one cell. Changes use the existing
  collision rule: the edited panel takes priority and overlapping panels move down.
- At 760px and below, **Move up / Move down** changes order and repacks the common
  desktop grid left to right, preserving panel sizes. Resizing the browser alone
  never changes saved geometry.
- **Details** exposes full view and source names without relying on hover titles.

## Invitations and connection controls

[Owner connections](assets/dashboard-ui/connections-owner.png) ·
[Shared connection](assets/dashboard-ui/connections-share.png) ·
[Mobile connections](assets/dashboard-ui/connections-mobile.png) ·
[Mobile connection actions](assets/dashboard-ui/connection-actions-mobile.png) ·
[Connection manager](assets/dashboard-ui/connections-manage.png) ·
[Disconnected screen](assets/dashboard-ui/connections-idle.png)

These screenshots use controlled session responses and isolated browser data.

Owner registration and gateway invitations open in native modal dialogs. Receiving
an owner invitation preserves the current board and unsaved edits; accepting it
uses the existing unsaved-change confirmation before switching connections.
Dismissal returns to the current board. Public share links still open directly.

Dialogs use a shared header with a top-right close button that stays visible when
the content scrolls. Action rows wrap with consistent spacing, and button-shaped
links remain a single control at narrow widths. Sharing checkboxes have their own
layout instead of inheriting text-input dimensions.

The sidebar keeps owner and shared connections above the dashboards available
through the selected connection. Each row identifies authority and Hub; repeated
Hub grants also show a grant identifier. The current row shows connection state.
The **Switch connection** control appears only at widths of 760px or less, where
it opens the same sidebar. Wider screens keep the sidebar visible. Selection
restores the last dashboard/preset and board-view mode for that connection.

The sidebar is connection navigation: it does not repeat the shared-view banner,
connection details, or disconnect action. Per-row **Manage** opens IDs, a local
display name and removal of the remembered entry. The main content's connection
header shows the current name, authority and status beside **Remember connection**,
**Open in app** and **Disconnect**. Remember and app handoff appear for public
shares; remembering replaces the button with a **Remembered** confirmation for
five seconds, or until switching connections. Saved connections stay saved;
returning to one or reloading does not replay the confirmation. Keyboard focus
moves to **Open in app** when the save button disappears. The actions wrap on
narrow screens. Opening another record's manager never changes their target.
The gateway invitation also exposes
current-connection actions within its connection chooser, outside record menus.
Board view keeps its compact connection display. Recovery keeps **Retry** beside
the connection list. Public links opened in the tab remain available when switching
away; explicit save persists their bearer in a separate IndexedDB connection
catalog. Catalog patches read the current record inside an IDB transaction;
metadata updates never write cached bearer credentials. Cross-tab notifications
contain no secrets and cause a fresh read. The catalog admits at most 1,000
records; tab history retains at most 100 shares.

Shared views keep the owner list visible without acquiring its endpoint lock.
Returning to owner access requires an explicit selection, including when a share
belongs to the same Hub. Identity inspection and connection-list loading are
independent of endpoint ownership; disconnect stops the session and releases the
lease. Owner invitations retain their lease until completion or dismissal.

The sidebar's **Settings** opens language and browser identity preferences without
leaving the board. Reset confirms deletion of the identity, saved connections,
saved shared links and tab history, and preserves the unsaved-edit guard. Hub
boards and server grants remain. Local Hub administration is under its sidebar
connection details. The disconnected hosted screen centers connection navigation
and browser identity details in the main content, without a sidebar. Normal board actions keep Edit and Share visible, with Export, Board
view and Delete under **More actions**; Delete retains confirmation.

`bun tests/browser/connections.mjs` checks cold shared startup, multi-Hub board
restoration, explicit share saving, new-tab access, endpoint release, 320px/200%
text layouts and focus recovery with real React, IndexedDB and Web Locks and
controlled transport responses. It also verifies that header actions target the
current connection, record menus do not contain session actions, and native
handoff and focus remain available after remembering a connection.
Screenshots are written to
`/tmp/omochi-connections-screenshots` (or `SCREENSHOT_DIR`).

The connection layout was additionally checked at 320, 360, 390, 640, 760, 761,
1024, 1440 and 1920 CSS pixels in both directions, using English, Japanese,
German test strings and pseudo-localization. The checks include long names, 200%
text, simulated 200% browser-zoom geometry, and scrolling controls beneath the
measured sticky dialog header. Connection names wrap in the connection header;
management titles stay short while the full editable name remains in the form.
Real-device Safari and browser zoom shortcuts were not verified.

The pre-bootstrap native-app link remains available if transport initialization
fails. Once React loads, the invitation dialog owns that action, so registering
or dismissing the invitation removes it. Public-share native handoff is available
beside the active connection's name in the main content header. These labels
follow the selected interface language.

The controlled-session browser regression checks are run with
`bun tests/browser/gateway-reconnect.mjs`. They cover modal dismissal, owner
invitations above unsaved drafts, native handoff lifetime, 320px button layout,
and the existing reconnect and authorization-scope cases. These checks exercise
the real React UI with controlled session responses, not native enrollment.

## Board view

**Board view** hides navigation and editing controls, while preserving view
headers, connection information, live updates and app interactions. Long boards
scroll normally rather than shrinking. **Exit board view** is always available,
including when loading fails or the dashboard is empty.

Use `?dashboard=<id>&view=board` or `?preset=<id>&view=board` to bookmark this mode.
Exiting removes only `view`. It is a display mode, not an authorization boundary
or browser fullscreen mode. Switching modes preserves mounted Surface views.

## Reproduce the checks

```sh
bun install
bun run build:native
bun run --filter @omochi-ui/hub-ui test
bun run --filter @omochi-ui/hub-ui lint
bun run --filter @omochi-ui/renderer test
bun packages/hub-ui/test/browser.mjs
```

The browser script starts isolated native services and Chromium, uses real HTTP
and WebSocket traffic, and cleans up its temporary runtime and processes. It
requires local socket permissions. `CHROMIUM_BIN` selects Chromium and
`SCREENSHOT_DIR` selects the output directory (default
`/tmp/omochi-hub-screenshots`). `CAPTURE_ONLY=1` captures the initial seeded board
without assuming the new UI, allowing a baseline against an earlier binary.

Recorded verification: native/web build, hub-ui typecheck and lint passed;
hub-ui tests passed (10), renderer tests passed (4; opt-in native test skipped),
and the Chromium/native-service browser script passed. Vite retains its warning
about the renderer bundle exceeding 500kB. No server or wire-contract code changed.

The fixture is a dashboard named Daily overview with a Task runner view supplied
by a native button utility. The test also starts a second utility for mobile
reordering. Screenshots show the actual app rather than a mockup.

## Interface review

Scope: the dashboard shell, editing, picker, details and confirmation dialogs,
board view, and the basic embedded button fallback. Recon used `AGENTS.md`,
`CONTRIBUTING.md`, and `docs/design-system.md`. Custom application catalogs,
Flutter and the website are outside this review.

| Domain | Evidence inspected | Result |
| --- | --- | --- |
| Accessibility | App controls, native dialogs, browser accessibility tree, arrow/Escape interactions | Names and tested focus recovery verified; real screen-reader speech not verified |
| Layout | Desktop and 320px screenshots, mobile order, 200% text and RTL checks | No page overflow in tested states |
| Writing | App labels, errors, empty/search states, save conflict and discard flows | Recovery actions and destructive consequences made explicit |
| Typography | Host input sizes, wrapping, full values in Details | 16px input baseline; enlarged text inspected |
| Colors | Computed foreground/background pairs from Chromium | Measured body 15.46:1, secondary 6.13:1, primary 7.79:1, selected navigation 7.18:1 |
| UI | CSS state rules, reduced motion, forced colors, screenshots | Host transitions are 100ms, zero under reduced motion; static state cues retained |

Resolved root causes (locations are relative to `packages/hub-ui/src`):

| Severity | Domain | Location | Before | After | Why |
| --- | --- | --- | --- | --- | --- |
| HIGH | Writing | App.tsx:129 | Cancel immediately lost changes | Dirty-state confirmation | Prevent accidental loss |
| HIGH | Layout | App.tsx:286 | Full values depended on hover title | Keyboard/touch Details dialog | Make full names available |
| MEDIUM | Accessibility | App.tsx:142 | Empty names silently disabled saving | Inline error and invalid-input focus | Explain how to proceed |
| MEDIUM | Writing | App.tsx:325 | No filtered-empty explanation | Query-specific message and Clear search | Recover from an empty search |
| MEDIUM | Layout | App.tsx:305 | Coordinate inputs and bottom-right resize only | Edge/corner handles and non-drag controls | Direct and accessible adjustment |
| MEDIUM | UI | styles.css:26 | Generic button/input styles affected embedded apps | Scoped host selectors and renderer-owned neutral fallback | Keep shell and app styling independent |

Not verified: real assistive-technology speech, iOS/Safari touch behavior, browser
zoom distinct from text resizing, every custom app's styles, all hover/disabled
contrast combinations, and the exact original user-reported jumping sequence.
The regression suite covers pointer preview/cancellation and source-level layout
reversibility; it does not claim that the unspecified original sequence was reproduced.

Verdict: **Approve for the inspected states only**. The unverified checks above
remain explicit limitations, not claims of accessibility conformance.
