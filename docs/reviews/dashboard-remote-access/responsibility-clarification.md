# Hub and dashboard responsibilities

Historical discussion, followed by the [revised plan](../../dashboard-remote-access.md)
on 2026-11-03. Board repositories and gateway access are now separate contracts.

Status: clarification following the author's question after Stage 7 round 1.
The Surface access manager is selected for the proposal. The earlier proposed
personal-access CRUD permission expansion is not approved. No runtime code or
stored dashboard data has been changed.

## What the code currently does

The dashboard UI owns the user operations to create, arrange, name, save, and
delete a dashboard. It calls `/api/dashboards` through its `api()` helper.

The hub's HTTP application exposes those routes. Its `HttpState` contains a
separate `DashboardStore`; handlers call its `list` and `mutate` methods.
`DashboardStore` validates layouts, checks revisions, serializes writes, and
persists `dashboards.json` in native state. Creation/deletion is therefore already
backed by the hub process, rather than browser-local storage. This does not imply
that the Surface transport or the core `Hub` aggregation object needs to own the
dashboard product's permission model.

The core `Hub` aggregates service state, routes actions, handles resource reads,
and maintains Surface lifetimes. `DashboardStore` is composed with it at the HTTP
boundary; the core object does not need a saved dashboard to render app Surfaces.
The existing local HTTP boundary trusts local access with its Origin policy; it
has no per-browser dashboard CRUD grant system today.

Implementation evidence:

- [Dashboard UI API calls and editing](../../../packages/hub-ui/src/App.tsx).
- [HTTP routes, HttpState, and dashboard handler](../../../crates/hub/src/http.rs).
- [DashboardStore persistence and revisions](../../../crates/hub/src/dashboards.rs).
- [Core Hub](../../../crates/hub/src/lib.rs).

## Where the proposal mixed responsibilities

The initial proposal put dashboard CRUD, live Surface access, and resources into
one `HubClient`. It then added a remote layout-editing permission and tied grants
to a saved dashboard's current revision. The style reviewer identified the
resulting ambiguity over personal CRUD permissions. Its proposed full-CRUD grant
would clarify that combined model, but the combined model itself is now being
reconsidered; it should not be accepted merely to clear a review finding.

Dashboard CRUD and sharing a layout are dashboard application concerns. Access
to live Surface data, actions, and resources requires hub enforcement. Keeping
these contracts distinct does not by itself require moving existing local
persistence out of the hub process.

## Proposed responsibility split for the next revision

| Layer | Responsibility |
| --- | --- |
| Dashboard application | Compose panels, edit layout, create/delete dashboard documents, and present sharing workflows |
| Dashboard repository | Load/save dashboard documents and resolve revision conflicts; the existing local HTTP store is one implementation |
| Hub client and remote protocol | Connect, list permitted views, subscribe to Surfaces, send allowed actions, and read authorized resources |
| Hub Surface access manager | Authenticate clients and enforce allowed Surface sets, operations, publication, generations, and revocation |

A dashboard may help its owner select which Surfaces to share and distribute a
layout containing their references. Sending or editing that layout cannot grant
access on its own: the hub's manager must independently authorize each requested
Surface. A recipient can arrange a layout without obtaining access to additional
Surfaces. A dashboard ID or revision must not become an implicit credential.

Under this split, creating or deleting a dashboard document does not inherently
require a Surface permission. Changing the manager's allowed Surface set does.
Deleting a layout and revoking access are distinct operations unless the sharing
workflow explicitly requests and successfully completes both. The owner must be
able to see and revoke actual grants independently of a layout's existence.

## Remaining decisions

The author has not yet selected where hosted/self-hosted dashboard documents are
stored or whether remote editing of the existing hub-backed documents is needed.
Choose a browser-local repository, a separately authorized remote repository, or
another explicit repository policy before claiming feature completeness. Do not
silently migrate existing dashboards or assume that a Surface grant authorizes
reads/writes of their saved layout documents.

If the split is adopted, the main proposal must remove layout CRUD from its
Surface grant table and `HubClient` contract, specify how shared layout documents
are delivered, and revise automatic grant-following-on-dashboard-revision and
deletion rules. Existing local routes can remain as a compatibility repository.
Review these changes against the original remote-access use case before the next
architecture review. The selected manager addresses the resource boundary, but
does not settle the dashboard repository decision.
