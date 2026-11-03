# Bounded browser snapshot v1

Status: future implementation contract, 2026-11-03. Read with
[browser hub v1](browser-hub-v1.md), whose budgets and source lifecycle apply.
This specifies browser-only code, not a change to the shared native/dev helper.

## Ownership and API

Create `packages/browser-hub/src/snapshot.ts` and the public pure helpers
`packages/core/src/data-patch.ts` and `json-size.ts`, exported by core/browser.
The renderer and browser snapshot use the same planner/sizer; neither imports
the other's package. Do not call
core A2uiSnapshot.apply/messages for admission: those clone before reporting size.
Reuse its replay/patch semantics through parity fixtures, not its allocation path.

```ts
interface Reservation { release(): void } // idempotent
interface SnapshotPool {
  readonly availableBytes: number;
  reserve(bytes: number): Reservation; // throws CapacityExceeded, no waiter
}
interface DeliveryPool {
  acquire(bytes: number): Reservation; // <=16 MiB and one live payload at most
}
class BoundedSnapshot {
  constructor(pools: { snapshots: SnapshotPool; delivery: DeliveryPool });
  readonly byteSize: number;
  apply(message: A2uiMessage): void;
  renderPayload(routePrefix: string): { messages: A2uiMessage[]; release(): void };
  dispose(): void;
}
```

The manager creates one 32 MiB SnapshotPool and one 16 MiB single-slot DeliveryPool
per profile and passes the same pools to every snapshot. New snapshots start at
zero charged bytes; reservations and availability updates are synchronous.

Messages have passed direct-mode schema/path/depth validation and are bounded by
the wire frame limit. apply does not perform another schema parse. JSON parsing
and schema validation of that bounded input frame may allocate; "pre-copy" here
means before constructing a candidate merged model, dense sparse expansion, or
detached replay. It is an encoded-retained-state bound, not a parser/RSS bound.
apply rejects after dispose. dispose releases its one current version reservation,
clears all retained state, and is idempotent. Only the manager calls these methods.
No async callback occurs inside apply; input frames are not retained by reference.

renderPayload requires a valid `<gatewayId>~<serviceId>/` prefix. It virtually
measures the routed replay first, acquires the profile's separate 16 MiB delivery
reservation for that size, then constructs a detached prefixed replay. Prefixing
changes only operation surfaceId fields, never app data. Only one payload may be live
in the profile; a competing call rejects CapacityExceeded without a waiter.
Its release is called in finally after synchronous renderer application and
drops the manager's payload references/reservation. Renderer model allocations
are not included in the encoded snapshot pool. The copy prevents renderer/form
mutation from changing source state. A retained committed BoundedSnapshot stays
charged until replaced/removed. No cached uncharged replay array is kept.

## Replay and byte definition

Store Surfaces in creation insertion order. Each has create message, a component
map in first-insertion order, a JSON data root, and hasData. Use the existing
A2uiSnapshot rules: duplicate create and update of unknown Surface reject;
deleting an unknown Surface is a no-op; a deleted ID can be created again.
Component updates replace the complete stored component by ID and preserve its
map position; sequential duplicate IDs in one update leave the last value.
Components are not deep-merged. create-only, components, and data replay order
is per Surface: create, at most one updateComponents if nonempty, then root
updateDataModel if hasData. No delete messages are retained.

byteSize is the sum of UTF-8 JSON.stringify lengths of these replay messages,
without array brackets, separators between messages, native envelope or length
header. Empty state costs zero. JSON.stringify emits compact JSON. Missing data
root replays updateDataModel with surfaceId/path:'/' and no value; JSON null is
distinct from missing. All replay wrappers and field/key strings are counted,
not just component/data values. Local merged replay messages may exceed 4 MiB;
they are internal and never sent on a gateway stream. A replay total is <=16 MiB.

Core json-size.ts exports jsonBytes(value: unknown): number. JSON input permits only null, booleans, finite numbers, strings,
arrays and plain objects; objects reject the prohibited keys from the hub contract.
No undefined/function/symbol/BigInt/nonfinite numbers enter source JSON values. Missing
data is tracked separately. Numbers use JSON.stringify(number) UTF-8 length
(-0 is 0). Count string quotes, backslash/quote escapes, control escapes
(backspace/formfeed/newline/return/tab two bytes; other U+0000–001F six), lone
surrogates as six-byte \\uXXXX and valid Unicode scalars by UTF-8 width. Count
object key strings, colon/comma/braces and array commas/brackets. Array holes
and explicit null each cost four bytes. No stringify/TextEncoder of a complete
model or complete incoming string is needed for measurement. Cache immutable
subtree byte sizes and maximum descendant depth in a WeakMap; do not cache
mutable incoming-frame or live renderer objects. Cache a subtree only after
proving all its containers immutable, not from shallow Object.isFrozen alone.
JSON-like client roots may have
undefined object properties (omit) and undefined array elements/holes (count and
materialize as null), matching JSON wire normalization. Missing whole root costs
zero. Other non-JSON values, cycles, accessors and prohibited keys reject before
materialization. Use a per-call traversal stack, not an unbounded retained cache.

## Virtual preflight, reserve, materialize, swap

Use immutable owned JSON state per stored version. Non-data snapshot subtrees
can be shared unchanged; changed data roots use the shared planner below and
materialize a complete detached data root after reservation. Accounting nevertheless
charges the full encoded size of each live version, not deduplicated heap bytes.
There is no persistent history. Incoming values are cloned only after admission.

For each apply:

1. Validate operation ordering, IDs, path and JSON tree restrictions without
   modifying current state. Decode JSON-pointer escapes as in core. Check
   depth<=128 and combined numeric path indices<=100000 before array expansion.
2. Build a virtual candidate descriptor, not a copied candidate. It references
   the old immutable state plus the bounded input and edit path. Traversal visits
   existing entries with a replacement/deletion overlay. Component overlays keep
   old positions and append new IDs; a set of the incoming IDs may be allocated
   (bounded by the 4 MiB frame). Delete walks the old map and omits the target.
   Surface wrappers and resulting component list use the replay definition above.
3. Measure the virtual replay arithmetically: sum create/component jsonBytes,
   component list commas/brackets, replay wrapper bytes, and the data plan's
   byteSize. A virtual component overlay adjusts sizes/counts without constructing
   its output array. jsonBytes only consumes ordinary values; the descriptor is
   never passed as if it were JSON. Reuse cached immutable subtree sizes.
   Stop as soon as the candidate exceeds 16 MiB or the remaining pool capacity.
   Reject merged replay nesting over 128 as well: a legal path plus a legal
   incoming subtree must not combine into an excessive resulting depth. Cached
   descendant depth lets the virtual visitor check this without copying.
   Do not build its full objects/arrays, concatenate JSON, or clone the old root.
4. Reserve the full measured candidate bytes while the old version remains
   charged. An exact-limit reservation succeeds, plus one byte fails. If reserve
   fails, current state/charge are unchanged and no candidate is materialized.
5. Materialize only after reservation: copy changed maps, clone incoming values,
   materialize the planned data root, and share other immutable owned subtrees. Freeze
   owned JSON containers. There are no getters/proxies in parsed input. A thrown
   validation/materialization error releases the candidate reservation and keeps
   the old state. Verify measured size equals the materialized replay size using
   the same traversal (no detached replay allocation).
6. Swap current state/reservation, then release the old reservation. Apply of a
   delete/no-op uses the same order; an unchanged byte-identical no-op can return
   without a new reservation. Release discarded candidate descriptors/input refs.

Do not reserve just a positive size delta: old and candidate copies can coexist.
If the pool lacks room to replace an existing version, fail admission even if the
candidate alone would fit; manager retracts the affected service and frees it.
This transient headroom rule is deliberate and testable. The retained-size bound
does not promise that allocator copies or renderer heaps equal these numbers.

For data paths, '' and '/' replace the root (missing value removes the root).
At intermediate positions, a primitive/missing node becomes array when the next
segment is a canonical decimal index, otherwise object. Array nonnumeric segments
reject. Setting an index past length virtually fills the gap with holes/nulls for
byte counting; only admitted candidates allocate the new array length. Removing
an array index shifts own later indices left and decrements length, including
holes. A missing/out-of-range final leaf deletion does nothing after traversal;
traversal still materializes missing/primitive parents even on deletion, as core
and Rust do (deleting /a/b from {} leaves {a:{}}). Object assignment uses own data
properties and preserves JS enumerable-key ordering; deletion omits that key.
These are the semantics of core's updateAtPointer, including replacement of a
primitive intermediate node and JSON serialization of holes. Count virtual array
length/gaps arithmetically rather than looping over a huge unadmitted index.

Core data-patch.ts exports this pure, synchronous plan. BoundedSnapshot uses it
for each updateDataModel and accounts for root byteSize plus replay wrappers.

```ts
interface DataPatchPlan {
  readonly byteSize: number; // candidate root only; undefined root is zero
  readonly maxDepth: number;
  materialize(): unknown; // detached JSON-normalized root, or undefined
}
planDataPatch(root: unknown, path: string, present: boolean, value?: unknown): DataPatchPlan;
```

Planning validates path/depth/index/own-key rules and constructs the virtual
overlay, measuring the candidate without cloning/expanding it. It leaves root and
value untouched. materialize traverses that overlay and clones the full result,
never sharing mutable objects with either input. It may be called only after the
caller reserves its measured output. Errors are fixed ProtocolError/CapacityExceeded
messages. No schema parse or renderer dependency belongs in this helper. Preserve
all pointer segments, including empty ones; canonical numeric indices match core,
so 01 is an object key but not an array index. Root '' and '/' are special. Both
consumers use the same operation and normalization semantics, not DataModel.set's
nested-path parser.

## Manager integration and failure ordering

Each admitted service owns at most one committed and one staging snapshot.
On snapshotStart allocate empty staging, then apply each native bare-ID message.
At snapshotEnd call renderPayload with the captured route prefix, allocate
provisional Surface leases, and call the authoritative sink once.
Detached delivery reservation measures the prefixed payload size as well: before
copying/prefixing, virtual sizing adds each surfaceId's route prefix and requires
the prefixed sum<=16 MiB. Oversize fails CapacityExceeded before delivery.
On success promote staging and dispose prior committed state; always release
the detached payload in finally. On failure abort provisional leases, remove
partial renderer state, dispose candidate and previous committed snapshots and
mark service terminal as specified by the hub contract.

In ready state apply the incoming increment to committed state with this same
preflight and charge the pending delivery frame until its synchronous apply
finishes. Before prefixing/cloning that increment, measure its routed delivery
size and reserve the delivery pool; release in finally. Attach a RenderBudget to
direct update events whose reserve adds scratch charges to this active delivery
slot; it does not acquire a second slot or wait. Snapshot failure, render
failure, unsubscribe, generation replacement and stop dispose every owned version.
Other services' reservations never release as a side effect. Renderer process
and user-form mutation operate on detached values, not snapshot-owned trees.

## Direct renderer patch boundary

Create `packages/renderer/src/data-patch.ts`, exporting
`applyDirectDataPatch(processor: Processor, message: A2uiMessage, budget: RenderBudget): void`.
It is called only for a direct update event containing updateDataModel; a wrong
operation or absent Surface rejects RenderError. Local/native messages bypass it.
Commit replay consists of already materialized root messages and bypasses it too.

1. Resolve the captured namespaced Surface and read surface.dataModel.get('/').
   This is the current live root, including unfinished local input. Never substitute
   the server-only BoundedSnapshot root here.
2. Call core/browser planDataPatch on that root with the incoming path (default
   '/'), value-presence flag and detached incoming value. Do not pass the original
   nested patch to MessageProcessor or call DataModel.set with that path.
3. Measure the resulting root update message using plan.byteSize plus exact
   compact JSON wrapper/surfaceId/path lengths. A missing root omits value. Check
   resulting message depth<=128 and call budget.reserve(normalizedMessageBytes)
   before materialize. The incoming routed payload charge remains live, so their
   sum must fit the 16 MiB delivery pool. This explicit transient headroom rule
   may reject a candidate that would fit alone. No clone occurs on rejection.
4. Materialize the detached candidate and synchronously call processMessages once
   with `{version:'v0.9',updateDataModel:{surfaceId,path:'/',value:candidate}}`,
   omitting value when candidate is undefined. The installed processor's root
   replacement API preserves its signals without using its differing patch parser.
   Release the scratch reservation in finally after that call; the resulting
   renderer model is ordinary renderer heap, outside encoded delivery accounting.

The source disallows actions during synchronous apply and enables them again only
after success. Failure returns the appropriate ApplyResult and retracts only this
service under the existing cleanup contract. Other direct message kinds pass
through normally. Do not change installed A2UI or native/local patch behavior.

A partial server patch preserves client edits outside its affected subtree. An
array removal shifts all subsequent values, including local edits. A server root
replacement is authoritative and replaces every draft in that Surface. A generation
change/reconnect creates fresh leases and server replay; drafts are not persisted
across that boundary. Test live/replay equivalence without client edits; with edits
test preservation against the shared patch applied to the edited live root, not
against a server-only replay that deliberately lacks those edits.

## Required tests and paths

Create `packages/browser-hub/test/snapshot.test.ts`,
`packages/core/test/data-patch.test.ts`, `packages/core/test/json-size.test.ts`, and
`packages/renderer/test/data-patch.test.ts`.
Compare jsonBytes against TextEncoder(JSON.stringify(...)) on bounded fixtures:
ASCII, escapes, BMP, astral and lone-surrogate strings, numeric edges, null/holes,
object keys and nested values. Test root removal/null, array hole expansion/delete
shift, primitive-path replacement, component replacement/insertion, duplicate
creates and delete/recreate. Replay parity uses the existing A2uiSnapshot with
identical legal messages; it is the oracle only for accepted semantics, not bounds.

Test exactly 16 MiB/+1 byte per snapshot and 32 MiB/+1 aggregate, old+candidate
transient charging, escaped-key size growth, many small patches accumulating to
the limit, and prefix-induced delivery overflow. Instrument the materializer so
rejected preflight/reservation performs zero candidate clones/array expansions;
verify rollback and all counters zero on stop. Mutate a detached payload/form
value and assert snapshot replay remains unchanged. Test thrown partial renderer
commit removes only its service and never makes it action-ready.

Installed-processor tests must apply live data updates and compare get('/') to a
fresh materialized replay for array delete/shift, primitive-parent replacement,
root removal/null, sparse writes, empty/escaped segments, canonical/leading-zero
indices and delete through missing parents. Also render ordinary bound fields,
perform a draft edit, patch a different field and assert the draft and action
metadata survive. Test root replacement clears the draft as specified, snapshot
ownership is isolated, exact/+1 scratch limits, no materialize on failed reserve,
all charges release on throw, another service remains ready, and native bypass
retains its previous path behavior. Use the actual installed MessageProcessor.
