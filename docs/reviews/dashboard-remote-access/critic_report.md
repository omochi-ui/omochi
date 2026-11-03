# Stage 7: Design Critique — round 1

Historical target: [original proposal](../../dashboard-remote-access.server-hub-v1.md), FeatureRange 1–239.
The current plan has a different architecture; this report is not its review.
The implementation plan begins at line 240. Reviewers read the full document for
definitions and release gates; findings below concern only the feature range.
Target SHA-256: `bc7a539431495d8445ddee42110a8c8f1cd30f0f540b611db3a495f2935efa5f`.

## Status and verdict

Changes requested. All three requested reviewers completed in separate fresh
conversation contexts using `fork_turns: none`. There are no failed reviewers.
Two Major findings, no Blockers, and no Nits were reported. The document has not
been changed. This round is not clean and the review loop has not converged.

| Reviewer | Standard | Completion | Findings |
| --- | --- | --- | --- |
| `egm-design` | Feature Specification Standard | completed | 1 Major |
| `egm-structural` | Feature Specification Standard | completed | None |
| `egm-style` | Persona only | completed | 1 Major |

The required EGM persona and standard files were read from the installed plugin.
No earlier reports, conversation history, or unlinked design notes were supplied
to these reviewers. None edited files, built code, or ran tests. All reported
that required inputs were readable.

## Findings list

### design-1-1 — Major — Resource authorization has no defined Surface ownership rule

Reviewer: `egm-design` (`dashboard_design_r1`).
Affected target: `docs/dashboard-remote-access.md:114–136`, especially the promises
that viewing includes resources required by permitted views and that
client-supplied resource IDs cannot widen access.

Evidence:

- `crates/hub/src/scoped.rs:170–197` validates a Surface handle's lifetime, then
  forwards the supplied resource request to its service. It does not establish
  that the resource belongs to that Surface.
- `crates/hub/src/images.rs:428–452` accepts any syntactically valid resource ID
  and reads it through that handle.
- `crates/hub/src/lib.rs:234–266` likewise routes generic resource requests by
  service and generation.
- The linked gateway contract identifies resources by service; it defines no
  Surface-to-resource ownership association.

Discrepancy and risk: A valid handle for permitted Surface A is insufficient to
distinguish its resources from resources belonging to hidden Surface B in the
same SDK service. A recipient who knows another resource ID could present it
with A's handle. Existing image lifetime and fetch restrictions do not establish
the promised dashboard isolation. This is a missing authorization policy, rather
than an unspecified frame format or queue limit covered by the open decisions.

Recommendation: Define how the hub determines that an image or generic resource
is authorized by a permitted Surface. For example, grant access only to resource
references discovered in the authorized Surface's current components/data, or
introduce an explicit resource capability association. State how association
changes invalidate pending reads. If service-wide resources are intended instead,
disclose that larger sharing boundary and adjust the isolation promise. Include
the case of two Surfaces in one service, with a recipient requesting the hidden
Surface's known resource ID through the permitted Surface.

The architecture otherwise addresses the feature standard's principal
requirements: motivation, scope, component boundaries, alternatives, relay
dependence, client-host trust, revocation, and bounded transport behavior.
Explicitly deferred browser compatibility, identity persistence, wire limits,
store compatibility, and revision-transition implementation choices were treated
as unresolved release gates, not implementation defects.

Traceability: this independently repeats Stage 5 finding ZO-001. It is the same
open issue, not a second distinct resource defect.

### style-1-1 — Major — Personal dashboard-management permission is ambiguous

Reviewer: `egm-style` (`dashboard_style_r1`).
Affected scope: Architecture lines 86–91 and Access model lines 109–119.

The client interface includes dashboard CRUD, but the personal grant's allowed
changes are described only as “app interaction, and layout editing.” The
following paragraph defines layout editing for a shared dashboard, leaving
personal permission to create or delete dashboards, add or remove views, and
change panel titles unclear. A reader cannot tell whether personal access
includes the existing dashboard management operations or only a subset. This is
an access-scope ambiguity, not a request to add functionality.

Recommendation: State the personal grant's operation set explicitly. If full
existing dashboard management is intended, replace its Allowed changes cell with:

> Create, update, and delete saved dashboards; add and remove app views; edit
> dashboard names, panel titles, and panel geometry; interact with apps.

Keep the existing statement that grants and trust are administered locally. If
a narrower personal grant is intended, enumerate that narrower set instead.

## Structural reviewer result

Reviewer: `egm-structural` (`dashboard_structural_r1`).
Verdict: PASS for structural review, complete. No findings or finding IDs.

The feature specification contains the applicable content required by the
feature-spec standard: problem/motivation; goals/non-goals; components, transport
boundaries, data flow and integration; security and permissions; failure modes,
scalability limits and mitigations; and evaluation of alternatives with specific
rejection reasons. The standard does not require exact heading names or ordering.

Open storage, protocol, numeric limits, compatibility, atomic enforcement,
browser support, deployment, and command choices are explicitly listed as
release gates with implementation-plan checkpoints. This structural pass does
not resolve those gates.

```text
H1    1  Dashboard remote access and sharing
 H2   6  Problem
 H2  18  Goals and scope
 H2  33  Current system and reading list
 H2  68  Architecture
 H2 103  Access model
 H2 138  Enrollment and connection links
 H2 179  Lifecycle, failures, and limits
 H2 207  Alternatives and trade-offs
 H2 222  Open decisions and release gates
```

No heading levels are skipped. Both tables have consistent columns and separator
rows. Lists are consistently formed, and the architecture code fence is closed.
All 15 local Markdown link targets exist. The external browser documentation
link was opened successfully.

## Read and search evidence

All reviewers read their assigned persona, the entire target document, root
repository instructions, every file in the target's 15-file baseline reading
list, and the linked official iroh browser documentation:

- `docs/architecture.md`.
- `crates/hub/src/http.rs`.
- `packages/hub-ui/src/App.tsx`, `packages/hub-ui/src/services.ts`.
- `packages/renderer/src/client.ts`, `packages/renderer/src/images.ts`.
- `spec/hub-browser-images.md`, `spec/gateway-v4.md`.
- `crates/transport/src/lib.rs`, `crates/transport/src/pairing.rs`,
  `crates/transport/src/trust.rs`.
- `crates/hub/src/remote.rs`, `crates/cli/src/main.rs`.
- `crates/hub/src/dashboards.rs`, `deploy/relay/README.md`.
- [Official iroh browser documentation](https://docs.iroh.computer/languages/wasm-browser).

Design additionally read its standard, verified the Cargo version pin, searched
resource/action definitions, and read relevant hub scoped/image/resource code
and `packages/renderer/src/resources.tsx`. Structural read its standard, Cargo
workspace manifest, and the browser integration script and checked links and
Markdown structure. Style also read the browser integration script. Searches for
additional relevant `AGENTS.md` files found none; missing optional
`docs/AGENTS.md` did not prevent access to the required root instructions.

## Resolution and next checkpoint

Both findings remain open. Precise proposed edits and their rationale are in
[the annotated resolution report](goldfish_review_report.md). No proposal edits
have been applied and no subsequent clean rounds have run. Approval of an edit
does not itself verify the edit: a later audit must check the resulting text.
