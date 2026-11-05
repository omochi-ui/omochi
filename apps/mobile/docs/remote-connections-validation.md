# External Hub client validation

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
