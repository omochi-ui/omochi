# Goldfish Annotated Review Report

Status: historical repair proposals, superseded by the gateway-sharing plan on
2026-11-03. Do not apply these edits to the current target. Earlier manager
selection and unapproved personal CRUD expansion below are decision history,
not requirements of the browser-to-gateway architecture.
Target: [original reviewed proposal](../../dashboard-remote-access.server-hub-v1.md),
FeatureRange 1–239 before `## Detailed Implementation Plan`.
Source: [Stage 7 round 1 findings](critic_report.md).

The original round-1 assessment follows as a historical record. The parent
evaluates these findings without changing their reviewer-assigned
severity. Both are PROPOSE FIX. There are no CONTEST, ESCALATE, or DECLINE items.
The resource fix below chooses a publication model; the prior explanation was
not approval of this choice. At the time of the original report, the proposal
was unchanged. See the current status above for later author instructions.

## design-1-1 — egm-design — Major

Affected scope: Access model, lines 114–136.

Discrepancy: a valid handle for permitted Surface A does not establish resource
association. It can route another known resource ID from hidden Surface B in the
same service. Current resource authorization is service-scoped.

Rationale: handle lifetime checks and media restrictions do not enforce the
proposal's promised Surface-scoped read boundary. This independently repeats
ZO-001, which remains open.

AI suggestion: **PROPOSE FIX**.

Justification: source evidence supports the finding. A hub-enforced publication
rule can constrain reads while keeping SDK resource requests and gateway v4
unchanged. This is a permission contract to document explicitly, not proof of
resource ownership. Its cost is that publishing a complete URI in authoritative
Surface state also publishes read permission for the bytes. Apps must accept
this rule; explicit SDK resource declarations remain an alternative.

### Proposed edit A — insert before “Service generations, Surface replacement”

```text
For shared access, a Surface publishes read access to an immutable SDK resource
by including its complete canonical omochi://resources/<32-lowercase-hex> URI in
its current authoritative component properties or data model. This includes
values delivered to viewers even when those values are not currently rendered.
Publishing a URI authorizes reading its bytes; apps must not echo user-supplied
resource URIs into this state unless they intend that publication. Resource
publication does not assert exclusive ownership: several Surfaces may publish
the same resource. A resource is resolved only within its publishing service.

The hub derives publication from the merged upstream state, never client draft
data, action metadata, or a client-supplied reference list. A shared client uses
session-bound Surface and resource tokens. Reads require both the current grant
and current publication, including resource-backed image reads. A valid Surface
token alone does not authorize an arbitrary resource ID or external image URL.
External image URLs require publication too and retain the existing fetch policy.

The first version supports complete URI string values. Markdown destinations
require a defined parser and extraction fixtures before being supported;
constructed or unsupported destinations fail closed. Updating authoritative
state updates publication before the corresponding UI update is sent. Removing
a reference cancels dependent pending reads and tokens; re-adding it creates a
new publication lifetime. Already delivered bytes cannot be recalled. Resource
indexes and token tables have explicit bounds in the remote wire contract.
```

### Proposed edit B — add a resource publication alternative

Add this row to Alternatives and trade-offs:

```text
| Explicit SDK resource publication per Surface | Separates disclosure of a URI from permission to read bytes; supports custom computed destinations | Requires coordinated SDK, gateway, control-message, replay, and compatibility work; the initial proposal instead uses authoritative URI publication and rejects unsupported destinations |
```

### Proposed edit C — extend the implementation-input gate

Add a numbered open decision after the current item 7:

```text
8. Specify the publication extractor for canonical resource and HTTPS image URIs,
   supported Markdown forms, bounded indexes and token tables, and cancellation
   ordering for publication removal. Sharing cannot ship until fixtures cover
   known resource IDs from another Surface in the same service and stale tokens.
```

The implementation plan must subsequently reference and resolve this new gate
before sharing is enabled. That cross-range synchronization will be recorded
explicitly if these feature edits are approved; this report does not silently
change the implementation range.

### Required verification of the proposed resolution

A permitted Surface A and hidden Surface B share one service. Reading A's
published resource succeeds. Reading B's known resource through A's valid handle
fails, including through the image path. Client draft values cannot publish it.
Removal and re-addition cannot revive an old resource token. Copies of a token
cannot be used in another session. Unsupported reference forms fail closed.

## style-1-1 — egm-style — Major

Affected scope: Architecture lines 86–91 and Access model lines 109–119.

Discrepancy: the client interface includes dashboard CRUD, but personal access
only names interaction and layout editing. The document does not state whether
personal access includes create/delete, adding/removing views, or panel titles.

Rationale: this is an access-scope ambiguity. Implementers cannot map every
dashboard operation to a permission without choosing product behavior themselves.

AI suggestion: **PROPOSE FIX**.

Justification: the proposal intends personal use of the same dashboard interface,
and its HubClient includes dashboard management. The precise proposed choice is
the existing dashboard management operation set, while remote trust and grant
administration stay excluded. If a narrower policy is desired, the author should
choose that policy instead of accepting this replacement.

### Proposed edit — replace the Personal remote access table row

Current:

```text
| Personal remote access | Hub dashboards and app views | View, app interaction, and layout editing |
```

Replacement:

```text
| Personal remote access | Hub dashboards and app views | Create, update, and delete saved dashboards; add and remove app views; edit dashboard names, panel titles, and panel geometry; interact with apps |
```

Retain the separate statement that grants and trust are administered locally.

## Decision and loop status

Round 1 is not clean because it has two unsettled Major findings. No convergence
is claimed. The author may approve the proposed changes, continue without
changes, discuss individual findings, edit the document directly, or stop the
review. No further tools or edits are authorized merely by this report's
existence. A request to continue without changes leaves the findings open unless
the author explicitly accepts a risk or otherwise resolves them.
