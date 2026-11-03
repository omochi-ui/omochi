# Stage 5: Initial Zoom Out Review

Historical target: [original proposal](../../dashboard-remote-access.server-hub-v1.md).
The current plan has a different architecture; this report is not its review.
Scope: full file, lines 1–330. FeatureRange: 1–239. DetailedRange: 240–330.
Reviewer: `egm-zoom-out`, dispatched as `dashboard_zoom_out` in a fresh conversation
context with `fork_turns: none`.
Standard: `resources/document_standards/zoom-out-standard.md` from the EGM plugin.
Target SHA-256: `bc7a539431495d8445ddee42110a8c8f1cd30f0f540b611db3a495f2935efa5f`.

## Verdict

Changes requested. The proposal states a clear user problem and generally
chooses sensible boundaries. One Major architectural gap remains: resource
authorization is promised at Surface scope, while the available resource
mechanisms operate at service scope.

## ZO-001 — Major — Define the resource authority boundary for shared dashboards

Target: `docs/dashboard-remote-access.md:114–135`, `195–199`, `262–269`,
`281–290`, and `309–310`.

Facts:

- The proposal grants resources required by permitted views and says
  client-supplied resource IDs cannot widen access.
- `crates/hub/src/scoped.rs:171–197` validates a Surface handle and selects its
  owning service, then forwards the supplied resource request. It does not
  establish that the requested resource belongs to that Surface.
- `crates/hub/src/resource_http.rs:24–44` accepts a service route and resource ID.
- `packages/sdk/rust/src/resources.rs:199–227` resolves resources from one registry
  by resource ID, with no Surface association.
- `crates/hub/src/images.rs:428–470` uses the same service-scoped resource
  mechanism for image reads.
- Resource IDs are random and opaque, but the proposal explicitly requires
  authorization even when the client supplies an ID.

Inference: reusing a permitted Surface handle alone would still let a recipient
request another known resource ID from the same service. Two dashboards
containing different Surfaces from one service therefore require an additional
authorization rule. The document does not select that rule or explicitly
identify this boundary among its unresolved decisions.

Rationale: this affects the core sharing guarantee and the compatibility
commitment to unchanged SDK and gateway contracts. It cannot be resolved merely
by adding typed requests or transport framing. Discovering it during
implementation risks either broadening access to an entire service or adding an
unplanned SDK resource ownership mechanism.

Recommendation: choose and describe the smallest supported resource authority
model before approving dashboard-limited sharing. Explain how the hub establishes
that a resource is authorized for a permitted Surface, including resource-backed
image requests, and whether existing A2UI state supplies enough information.
Until that model exists, defer shared resource operations that cannot be
authorized. Add an adversarial acceptance case with two Surfaces in the same
service: the recipient can read the permitted Surface's resources but cannot
read a known resource ID belonging only to the other Surface. Preserve this
distinction when references or grants change.

No Blocker or Nit findings.

## Choices supported by this review

- A separate dashboard protocol avoids turning gateway v4 into a browser
  administration protocol.
- The WASM spike precedes substantial transport work and includes an explicit
  fallback decision.
- Local Origin checks and existing local routes remain independent of remote
  listening.
- Personal access can ship separately without claiming that sharing isolation
  is complete.
- The implementation plan's technical detail is appropriate for its stated
  scope; no anti-weeds violation was found.

## Verified reuse opportunities

`ScopedSubscription` already supplies snapshot/update sequencing and applied
acknowledgements; `SurfaceHandle` already supplies lifetime-bound actions, image
reads, resource routing, and cancellation. These are suitable foundations to
extend with dashboard grants. They do not currently implement dashboard-client
permission checks. The proposal's shared hub operations approach can accommodate
this reuse without another parallel subscription or action system.

## Reads completed

- Review persona and Zoom-Out Standard.
- Target document, all 330 lines.
- `README.md`, `docs/architecture.md`.
- Every file in the target's baseline reading list: hub HTTP, dashboard UI and
  service subscription, renderer client and images, browser image and gateway
  contracts, transport setup/pairing/trust, hub supervision, CLI startup,
  dashboard store, and relay deployment README.
- The explicitly named browser integration script and Cargo workspace manifest.
- Additional relevant baseline code: hub scoped operations, image resource
  reads, resource HTTP handling, authorization source, transport store/live
  acknowledgement mechanisms, SDK resource registry, resource protocol
  specification, and browser package manifests.
- [Official iroh browser documentation](https://docs.iroh.computer/languages/wasm-browser).

## Searches performed

- Existing `HubClient`, scoped subscription, Surface handle, action/resource,
  authorization, invitation, revocation, and identity mechanisms across hub,
  transport, core, renderer, and hub UI.
- Resource request fields and Surface ownership across the resource protocol
  and SDK registry.
- Browser package source inventory and dependency boundaries.

No required input was unreadable. An optional lookup for
`packages/renderer/src/resources.ts` found no file; source inventory identifies
`resources.tsx` instead. This review does not depend on that optional file.
No files were edited, builds performed, or tests run by the reviewer.

## Checkpoint

Stage 5 is complete. ZO-001 remains open. The proposal has not been changed in
response to this review. The author must choose whether to proceed to Stage 6,
update the document before a Stage 5 re-audit, or stop the review. Stage 5 permits
at most one re-audit.
