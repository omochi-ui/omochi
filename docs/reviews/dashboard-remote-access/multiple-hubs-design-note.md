# Multiple hubs and the authority boundary

Historical discussion, followed by the [browser gateway-client plan](../../dashboard-remote-access.md)
on 2026-11-03. The current plan uses gateway-wide enrollment and no downstream
Surface-sharing manager.

Status: architectural discussion following the author's question. This is a
recommendation, not an approved topology change or a completed review.

## Recommendation

Allow multiple independent hubs. Do not require one global hub, one hub per user,
or one shared catalog of dashboards and Surface grants. A hub aggregates a chosen
set of upstream gateways for its downstream clients. Holding snapshots and
enforcing permissions does not make it the owner of the source app's Surfaces.

Retain the selected Surface access manager in each hub for the initial design.
It enforces access through that hub: which client can receive which Surfaces,
send actions, and read published resources. It does not create/delete source
Surfaces, administer other hubs, or grant authority beyond its own upstream access.

## Responsibilities

| Component | Responsibility |
| --- | --- |
| SDK app | Create/delete source Surfaces, produce UI/data, execute business actions, register resource bytes |
| Source gateway | Authenticate consumers and expose source service connections |
| Hub | Aggregate authorized upstreams, retain bounded replay state, route traffic, enforce downstream access |
| Surface access manager | Filter subscriptions/actions/reads, bind handles and lifetimes, cancel revoked work |
| Dashboard application/repository | Compose/store layout documents and provide layout sharing workflows |

A bounded snapshot, connection table, cancellation tokens, and access checks are
part of delivering the hub's own service. Application business state, source
Surface ownership, global accounts, layout collaboration, and permissions for
unrelated hub instances are different concerns.

## Multiple independent hubs

```text
gateway A ----> hub X ----> local browser
     |            +------> remote browser
     +--------> hub Y ----> another dashboard
gateway B --------^
```

Each hub has its own identity, enrolls with selected gateways, and authorizes its
own clients. A grant through X does not enroll the recipient on Y or permit direct
gateway access. Revocation on X stops access through X, not access independently
granted through Y. Global revocation would need an expressly designed authority
and protocol; the proposal must not promise it by implication.

This topology does not require hub-to-hub forwarding, recursive aggregation,
permission replication, leader election, or a global coordinator. Several hubs
can connect to the same gateway. Hub-to-hub delegation is a separate feature with
routing-loop, amplification, identity, and revocation requirements.

## The trust limit of a hub-local manager

Today's source gateway authorizes registered Hub identities by role. Gateway v4
has service-scoped streams and no source-issued per-Surface grants. A paired hub
is trusted with the source data it receives. The gateway cannot independently
enforce recipient-specific Surface restrictions on that hub.

A local manager prevents a recipient from widening access through that hub. It
does not protect a gateway against an already trusted malicious hub, or prevent
redistribution of bytes already delivered. This boundary does not require a
single global hub process.

If untrusted hubs or permissions that survive arbitrary forwarding paths become
requirements, use source-issued verifiable grants enforced at the gateway/app
boundary. A hub would only narrow or forward that authority. This requires a new
source protocol, a delegation policy, and a specified revocation model. It is not
an existing capability and should not be hidden inside a browser-transport change
that promises to preserve gateway v4.

## Existing implementation evidence

- [Gateway admission](../../../crates/gateway/src/remote.rs) accepts registered
  Hub peers and spawns independently bounded sessions; it does not select one
  global hub. This establishes source structure, not a new multi-host test result.
- [Desktop identity storage](../../../crates/transport/src/store.rs) keeps one
  `hub_key` per configuration store. Different HTTP ports do not create distinct
  hub identities. Independent same-machine instances need an explicit identity
  and state policy.
- [CLI storage](../../cli.md) documents the default shared native-state directory
  and `--config-dir`. These need to be accounted for when supporting independent
  instances; a different port alone is insufficient.
- [Scoped subscriptions](../../../crates/hub/src/scoped.rs) already separate
  lifetime-bound Surface operations and independent browser subscriptions from
  source app ownership.
- [HTTP state](../../../crates/hub/src/http.rs) composes `Hub` and `DashboardStore`.
  Keeping existing persistence behind a dashboard repository does not require
  putting dashboard product permissions in the live Surface protocol.

## Effect on the ongoing review

Before the next architecture review, define grants as authority on a specific
hub's connection boundary, revise saved-dashboard revision/deletion coupling,
separate remote layout storage from Surface access, and document instance
identities and independent gateway enrollment. Source IDs identify data; the hub
identity identifies the access route and grant authority. A reference is never a
credential.

Keep the selected manager's publication checks. Defer source-issued delegation
unless untrusted hubs are a requirement. No main-proposal topology decision is
approved by this note, and no review finding is closed by it. No runtime code,
migrations, configuration, tests, or deployment have been changed.
