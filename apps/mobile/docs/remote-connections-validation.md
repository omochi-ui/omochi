# External Hub client validation

## Responsibility split

The refactor follows the actual consumers of the old `remote_boards.dart` and
`crates/native-bridge/src/downstream.rs`, including the hosted Web client.
The migration order is contracts and state, adapters, Flutter controllers, then
Widgets and localization; protocol and secure-profile formats stay compatible.

| Responsibility | Code | Contract |
| --- | --- | --- |
| Shared Rust rules | `crates/downstream-core/src` | Typed commands, access modes, permissions, errors, welcome/render frames; transactional snapshot/live/closed transitions, canonical sequence numbers, post-application ACKs, capacity limits and lease invalidation/authorization. No transport, clock or persistence dependencies. |
| Native adapter | `crates/native-bridge/src/downstream` | Invitation parsing/address conversion, secure-profile validation and CAS, enrollment, QUIC, deadlines and cancellation tokens. `mod.rs` dispatches typed commands; profile and connection implementations remain separate. |
| WASM adapter | `packages/hub-transport/src/lib.rs`, `ffi.ts`, `index.ts` | Raw transport and a WASM facade for the same Rust session rules. `packages/hub-ui/src/remote/session.ts` consumes the validated frame effects; the browser owns renderer application, timers, cancellation and IndexedDB. |
| C ABI | `crates/native-bridge/src/lib.rs` | Typed remote-command conversion, bounded queues and image buffers, storage callbacks and structured error codes. The existing textual error remains available to older callers. |
| Flutter repositories | `lib/src/remote/repository.dart`, `boards_repository.dart` | Bridge/identity-lease ownership, connection registration and selection persistence, typed board and sharing operations. Wire JSON and operation names remain inside these adapters. A2UI bodies stay JSON at the renderer seam. |
| Flutter controllers | `lib/src/native/client_remote.dart`, `lib/src/remote/*controller.dart` | `ChangeNotifier` state, serialized connection scheduling and navigation handoffs, polling and editable drafts. `HubClient` provides shared rendering/lifecycle operations; only `GatewayClient` exposes local gateway administration. |
| Widgets and OS integration | `lib/src/remote_boards.dart`, `lib/src/remote/{board_editor,board_view,hub_shares}.dart`, `lib/src/remote_connections.dart`, `lib/src/app.dart` | Typed presentation, dialogs, route presentation and OS lifecycle events. All remote interface text uses the existing ARB localization workflow. |

The shared Rust state tracks protocol lifetime; Flutter and browser controllers
still schedule reconnects and respond to their platform's lifecycle. A Widget
does not acquire a lease, call the bridge, persist a selection, or decide the
stop/suspend/resume sequence for switching connections.
Retry policy is defined by the Rust fault type and exposed through C ABI
`retryable` and WASM `downstream_retryable`; Dart keeps an exact-code fallback
for older bridge binaries. Renderer failures stop the session without an ACK.

Regression checks target the behavior that can cross these seams:

- Apply a render frame before acknowledging it; reject malformed or repeated
  sequences without partially changing valid state.
- Invalidate captured action/image leases on replacement, deletion and close.
- Await endpoint shutdown before releasing the identity lease; retain ownership
  after failed shutdown so a retry cannot overlap another endpoint.
- Serialize suspend/resume and connection selection, respect the unsaved draft
  guard, and keep a returning connection suspended while the app is backgrounded.
- Keep conflicting drafts and their revision; reload only after explicit user
  choice. Shared layout editing preserves panel membership.
- Keep invitation redemption separate from ordinary connection traffic, preserve
  pending registration before enrollment, and fail closed on unknown save outcomes.

## Refactor verification

2026-11-05, Linux x64, local checkout:

- `flutter analyze --no-pub`: no issues.
- Flutter full suite with the rebuilt `OMOCHI_HUB_LIBRARY`: **161 passed, no
  skips**, including native C ABI storage/shutdown checks.
- Shared Rust core: 6 passed; native bridge: 14 passed, including real QUIC
  control, ACK/lease handling and malformed peer JSON. The transport crate's
  Rust tests: 2 passed.
- Hosted Hub UI: 54 passed; browser transport: 12 passed. The hosted build and
  actual wasm-bindgen runtime checks for ACK, lease replacement, u64 precision,
  closure and common retry policy passed.
- Additional coverage includes typed conflict versus diagnostic substrings,
  post-render ACK, stale captured handles, failed-close lease retention, catalog
  serialization, shared-layout revision updates, ARB locale changes, and
  backgrounding while an unsaved-draft guard declines connection selection.
- The narrow-screen editor screenshot was rendered and inspected; no structural
  overflow. Flutter's test font is Ahem.

These checks exercise the refactor; they do not repeat the platform packaging or
external-relay acceptance work recorded below.

Relevant commands, from the repository root unless noted:

```sh
cargo test --manifest-path crates/Cargo.toml -p omochi-downstream-core -p omochi-native-bridge --offline
cargo build --manifest-path crates/Cargo.toml -p omochi-native-bridge --offline
bun run --filter @omochi-ui/hub-transport test
bun run --filter @omochi-ui/hub-ui test
bun run --filter @omochi-ui/hub-transport build:wasm
bun run --filter @omochi-ui/hub-ui build:hosted
```

In `apps/mobile`, run `flutter analyze --no-pub` and then
`OMOCHI_HUB_LIBRARY=/absolute/path/to/crates/target/debug/libomochi_native_bridge.so flutter test --no-pub`.

## Earlier platform acceptance baseline

2026-11-05, Linux x64, local checkout.

- Flutter analysis: no issues.
- Flutter suite with `OMOCHI_HUB_LIBRARY` pointing at the built native bridge:
  135 passed, including the C ABI tests (none skipped).
- Native bridge Rust suite: 12 passed. Includes real loopback QUIC owner control,
  public readonly denial, render ACK sequencing and deletion invalidating leases;
  corrupt secure documents and unknown enrollment-save outcomes fail closed.
- Hub UI suite: 54 passed; hosted TypeScript check passed. Shared invitation
  fixtures cover all three downstream modes and invalid payloads.
- Linux debug app bundle and Android x64 debug APK: built successfully.
- Android disposable production-host fixture on the connected x64 device: owner
  cold launch and invite/share/owner warm delivery passed, with Activity
  finish/reopen retaining the same Dart owner and identity lease. The fixture
  was uninstalled; the normal app identity was not used.

Commands (from the repository root, except Flutter commands in apps/mobile):

```sh
cargo test --manifest-path crates/Cargo.toml -p omochi-native-bridge --offline
cargo build --manifest-path crates/Cargo.toml -p omochi-native-bridge --offline
bun run --filter @omochi-ui/hub-ui test
bun check -p packages/hub-ui/tsconfig.hosted.json
flutter analyze --no-pub
OMOCHI_HUB_LIBRARY=/absolute/path/to/crates/target/debug/libomochi_native_bridge.so flutter test --no-pub
flutter build linux --debug --no-pub
```

The test editor can export a narrow-screen screenshot with
`flutter test --dart-define=REMOTE_BOARDS_SCREENSHOT=/tmp/mobile-boards.png test/remote_boards_test.dart`.

Not established by these checks: iOS device delivery, full Android enrollment, Apple provisioning,
Windows/macOS builds, public relay/separate-network enrollment, and end-to-end
remote owner/share revocation against a separately running production Hub.
These remain release acceptance work. HTTPS OS associations and domain deployment
are intentionally deferred; see ../deeplinks/README.md.
