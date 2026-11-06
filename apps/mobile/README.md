# Omochi Mobile

English · [Japanese](README.ja.md)

A Flutter app for Android, iOS, macOS, Windows, and Linux. The shared Rust Hub
inside the app connects to Gateways over iroh and uses GenUI to display and
interact with A2UI v0.9 surfaces. No external HTTP Hub, LLM, or API key is needed.

The app selects Japanese or English from the system's preferred languages and
falls back to English for unsupported languages. It also follows language changes
while running. Content and status strings sent by Gateway applications are
displayed as received.

## Getting started

Use Flutter 3.47.9 / Dart 3.13.7, Rust, and sources from the same checkout.

```sh
cd apps/mobile
flutter pub get
flutter run -d <device>
```

Android requires minSdk 24, NDK 30.0.16248370, and the Rust targets
`aarch64-linux-android` and `x86_64-linux-android`. Gradle calls
`tool/build_shared_hub.py` and includes the shared library in the APK. A small
host adapter handles Android DNS/TLS initialization and the multicast lock.
Debug builds select Impeller's OpenGL ES backend to avoid Vulkan startup hangs
in the Waydroid environment used for validation.

Android uses `singleTask` to deliver browser invitations to the existing
Activity. The process retains the Flutter engine, so a destroyed and recreated
Activity reconnects to the same Dart runtime and Hub. A process-held file lock
for each namespace also prevents another Flutter engine or process from opening
a Hub with the same identity. Activity destruction does not release the lock;
it is explicitly released after native Hub shutdown and storage completion.
The OS releases it when the process exits. The lock file is kept to preserve
the same inode.

iOS requires version 15.0 or later, Xcode, and the Rust targets
`aarch64-apple-ios`, `aarch64-apple-ios-sim`, and `x86_64-apple-ios`. An Xcode
build phase calls `tool/build_ios_hub.sh` to generate a static XCFramework for
devices and simulators. FFI symbols are force-loaded and Dart resolves them
within the process. Configure signing locally when running on a physical device.

```sh
flutter build apk --debug --target-platform android-arm64,android-x64
flutter build apk --release --target-platform android-arm64,android-x64
# macOS
flutter build ios --simulator --debug
flutter build ios --debug --no-codesign
# macOS desktop
flutter run -d macos
flutter build macos --release
```

macOS requires version 12.0 or later. An Xcode build phase calls
`tool/build_macos_hub.sh` to bundle and sign the arm64/x86_64 Rust Hub in
`mobile.app/Contents/Frameworks` for the requested architectures. The app runs
with development ad-hoc signing and needs neither Rust nor an external Hub at
runtime. Debug and release builds enable sandbox network client/server access
and declare the Bonjour service type and local-network usage description.

macOS stores its identity in the login Keychain without sharing. Only macOS
avoids the data protection keychain so that Keychain Sharing and a provisioning
profile are unnecessary. Saving may fail with `-25308` in an SSH session when
the Mac screen or login Keychain is locked. Unlock the Keychain before starting.
A save failure does not regenerate keys or fall back to a plaintext file.

Rust builds can be large. Use `CARGO_TARGET_DIR` to select a volume with enough
space. If necessary, stop the app, move Flutter's `build` directory to another
volume, and leave a symlink at the original location. Do not commit
machine-specific paths.

## Windows / Linux desktop

```sh
flutter config --enable-linux-desktop --enable-windows-desktop
flutter pub get --enforce-lockfile
# Linux host
flutter run -d linux
flutter build linux --release
# Windows host
flutter run -d windows
flutter build windows --release
```

CMake runs `tool/build_desktop_hub.dart` to build and bundle the same Rust Hub.
Windows uses MSVC targets and Linux uses GNU targets, with x64/ARM64 selection.
Debug uses Cargo dev; Profile and Release use Cargo release. Validation and CI
cover x64; ARM64 remains unverified. The entire bundle is required at runtime,
but users do not need Rust or Flutter installed.

Windows development requires Flutter's supported Visual Studio Desktop
development with C++ workload, Windows SDK, MSVC ATL
(`Microsoft.VisualStudio.Component.VC.ATL`), and a Rust MSVC target. Enable
Windows Developer Mode for plugin symlinks. Distribute all of
`build/windows/x64/runner/Release`, including the Hub DLL and redistributable
release C runtime. The app loads the Hub DLL beside the executable. Identities
use `flutter_secure_storage` with per-user DPAPI protection. A second launch
forwards invitation URLs to the running copy of the same executable rather than
opening another Hub with the same identity.

Linux development requires clang, CMake, Ninja, pkg-config, and development
packages for GTK 3, libsecret, GStreamer, and its base plugins. On Debian/Ubuntu,
these include `libgtk-3-dev libsecret-1-dev libgstreamer1.0-dev
libgstreamer-plugins-base1.0-dev`. Distribute all of
`build/linux/x64/release/bundle`. The Hub loads from
`lib/libomochi_native_bridge.so`. Runtime also requires GTK 3, libsecret,
GStreamer, and an unlocked Secret Service keyring such as GNOME Keyring or
KWallet. There is no plaintext fallback when protected storage is unavailable.
GApplication delivers invitation URLs to the existing window through a single
instance. Linux Video playback is unsupported because GenUI's video plugin
has no Linux implementation.

After extracting the bundle to a permanent location, run the bundled registration
script if you want browsers to open `omochi://` links. This changes the current
user's existing `omochi://` handler to the Omochi app.

```sh
# Linux: run inside the bundle
./register_linux.sh "$PWD"
# Windows PowerShell: run inside the bundle
powershell -ExecutionPolicy Bypass -File .\register_windows.ps1 -BundlePath .
```

Register again after moving the bundle. Pasting invitations works without handler
registration. Incoming QUIC on Windows may require an app-specific Windows
Firewall allowance. Linux uses Avahi when available and Rust mDNS otherwise.

## From invitation to interaction

Run `omochi-ui gateway --remote --pair` on the computer hosting the Gateway.
Open the printed HTTPS invitation page on your phone and choose
**Open in Omochi app**. Check the Gateway ID and expiry in the app, then choose
**Add to this device** to enroll. A browser owner-Hub connection is unnecessary.

Choose a service's Surface from Apps to interact with it. Connections supports
pasting invitations, viewing connection status, reconnecting, forgetting, and
toggling Internet relays. Pasting accepts both native URIs and HTTPS invitations.
Current invitations use base64url; the old hex format is rejected.

Receiving a link does not enroll automatically. Cold and warm deliveries open
the same confirmation screen. Duplicate notifications are ignored while
confirmation is pending, and a different invitation does not replace it.
Invitations are not persisted, and their secrets are removed from logs. The app
has no custom QR scanner; use the existing CLI QR → web → app flow.

## Responsibilities and persistence

- The Rust Hub owns identity, enrollment, authorization, revocation, reconnection,
  subscriptions, Surface generations, and image-fetch policy. Dart has no second Hub.
- Flutter handles OS storage, lifecycle, GenUI, input compatibility, and image
  decoding. The canonical C ABI is `../../crates/native-bridge/include/omochi_hub.h`.
- `omochi.mobile.v1.identity` is stored in Android protected storage or iOS
  Keychain. The new app has its own identity and does not migrate the experiment's
  identity. Corrupt documents and read failures do not regenerate keys.
- Save failures and unknown outcomes are not enrollment success. Forgetting removes
  the device's registration; revoke access at the Gateway with its `trust forget`.
- Suspension stops communication and subscriptions. Resume obtains fresh snapshots
  and action handles. Actions with unknown delivery outcomes are not replayed.
- Standard Image and Markdown raster images use the shared resolver. It supports
  public HTTPS:443 and authorized Omochi resources, with no direct-network fallback.
- Limits are 2 Gateways / 8 services, 4 megapixels per image, 32 MiB of retained
  pixels, and 2 concurrent image loads. These are not a total RSS limit.

Compatibility handling for GenUI nested action contexts, UTC times, and
per-service input models carries over from the experiment with regression tests.
The old Dart Hub, HTTP Hub mode, Lab, and dashboard editing in the embedded Hub
are excluded. The web scaffold is unsupported.

The native catalog's Column scrolls content beyond the viewport and assigns
finite heights to weighted children and implicit flex. Nested Columns and Modals
use the same implementation. A Modal bottom sheet belongs to its Surface's
renderer generation. Deletion, replacement, disconnection, forgetting, suspension,
and navigation away close only that Surface's route.

The netdev/netwatch dependencies in `../../vendor` have limited iOS compatibility
patches. They avoid macOS-only libc types and use the iOS address list with
foreground polling every 5 seconds. Default-route, home-router, and additional
IPv6 flag detection have limitations. See the vendor README for details.

Production LAN discovery is selected in the shared Rust transport layer: OS
Bonjour on iOS/macOS, OS DNS-SD on Windows, Avahi over D-Bus on Linux when
available with Rust mDNS otherwise, and existing Rust mDNS on Android. No
multicast entitlement was added. iOS has build verification only; physical-device
permission dialogs, resume, and rediscovery acceptance remain incomplete. See
[validation records](docs/validation.md).

Bidirectional Mac Bonjour/Linux address resolution was verified after compatibility
fixes to vendored `swarm-discovery 0.6.5`. To repeat it from Linux with SSH access
to a Mac on the same LAN, run
`python3 apps/mobile/tool/bonjour_probe.py --ssh-host macbook` at the repository
root. The Mac needs `dns-sd` and Python 3. The probe uses no registered identity
and does not verify physical iOS devices or QUIC connections.

## Verification

The [Mobile workflow](../../.github/workflows/mobile.yml) runs for PRs, pushes to
main, and manual dispatch, excluding site-only changes. It runs analysis and
ordinary tests with Flutter 3.47.9, builds the Linux Rust shared library, and runs
C ABI tests. After those pass, it builds Android ARM64/x86_64 release APKs,
a macOS release app, Windows/Linux release bundles, and an iOS release archive.
It retains APKs, macOS/Windows ZIPs, Linux tar.gz files, and an unsigned IPA as
Actions artifacts for 14 days. macOS uses development ad-hoc signing without
notarization.

APKs currently use the development debug key. iOS builds without signing
credentials; the app from `flutter build ipa --no-codesign` is packaged as
`Payload/Runner.app` in the IPA. Physical-device installation and distribution
require separate signing and provisioning. Device integration tests are not
part of this workflow.

```sh
flutter analyze
flutter test
# Android: test the production lease implementation on the JVM (requires JDK)
python3 test_driver/android/identity_lease_test.py
# Android device/emulator: test browser tasks and leases in a temporary package
# Does not launch or change storage in the normal app; uninstalls the test package
python3 test_driver/android/launch_test.py --device <adb-device>
# Test Activity finish/restart and invitation delivery with the production Flutter host
# Only native DNS/TLS initialization is stubbed; no Rust Hub or secure storage is used
python3 test_driver/android/engine_lifecycle_test.py --device <adb-device>
# Windows/Linux: real OS storage, Hub restart, and URLs forwarded to the running app
flutter test integration_test/desktop_test.dart -d windows
flutter test integration_test/desktop_test.dart -d linux
# macOS: OS Bonjour Browse/Resolve of the sandboxed Hub and Keychain persistence
flutter test integration_test/bonjour_test.dart -d macos
# Optional: Linux storage-failure tests with the real C ABI
python3 tool/build_shared_hub.py --platform linux --output build/shared-hub/linux
LD_LIBRARY_PATH="$PWD/build/shared-hub/linux" flutter test test/shared_hub_bridge_test.dart
```

Close the normal app before Windows/Linux integration tests. `desktop_test.dart`
uses and then deletes a test-only storage namespace. Set
`--dart-define=DESKTOP_SCREENSHOT=/absolute/path/desktop.png` to capture the app
after canceling the invitation.

To isolate networking on a test Mac with a locked Keychain, add
`--dart-define=BONJOUR_MEMORY_STORE=true` to the Bonjour integration test.
This replaces only the test's storage fixture. With
`--dart-define=BONJOUR_HOLD_SECONDS=40`, use the public Hub ID from the log on
another LAN host to resolve the app's advertisement:
`cargo run --locked --manifest-path crates/Cargo.toml -p omochi-transport --example bonjour_probe -- --resolve <Hub ID>`.

Add `--dart-define=BONJOUR_PEER_ID=<public EndpointId of a LAN peer>` to test
Browse/Resolve of that peer through the app's OS Bonjour. This flag selects
the test peer; production always uses OS Bonjour in the shared transport layer.

Device integration tests require `../../crates/target/debug/omochi-ui` built
from the same checkout, Bun dependencies, and a built `@omochi-ui/core`.

```sh
# repository root
bun install --frozen-lockfile
bun run --filter @omochi-ui/core build
bun run --filter @omochi-ui/renderer build
bun run --filter @omochi-ui/hub-ui build
cargo build --manifest-path crates/Cargo.toml -p omochi-cli

# terminal 1, apps/mobile
python3 tool/native_fixture.py --online
# terminal 2, Android
adb reverse tcp:5559 tcp:5559
flutter test integration_test/mobile_test.dart -d <device> --dart-define=TEST_ONLINE=true
flutter test integration_test/storage_test.dart -d <device>
flutter test integration_test/capacity_test.dart -d <device>
adb reverse --remove tcp:5559
# iOS simulator: run the fixture on the same Mac or create an SSH tunnel to localhost
flutter test integration_test/mobile_test.dart -d <simulator> --dart-define=TEST_ONLINE=true
```

The fixture starts two Gateway/SDK pairs with isolated temporary configuration.
Its HTTP API handles only invitations and process control; A2UI, actions, and
images travel over iroh. For parallel platform validation, use separate fixtures
with `--port` and `--dart-define=FIXTURE_URL=...`. Ctrl+C cleans up temporary data
and child processes.

`storage_test.dart` injects faults into real OS storage to verify unknown save
outcomes, forgetting failures, and shutdown during saving. `capacity_test.dart`
checks 2 Gateways / 8 SDKs, each with a 64 KiB model and a 1024×1024 image,
rejection of a ninth service, and image release after suspension.

Release load measurements launch a test APK directly rather than using Flutter
Driver. Release builds disallow HTTP, so use `native_fixture.py --tls --port 5561`
and `--dart-define-from-file` with the printed CA encoded as `FIXTURE_CA_BASE64`.
Test TLS trust applies only to the test code's HTTP client. Rebuild with the
normal `lib/main.dart` after measurement.

Record environments and results in [validation records](docs/validation.md).
Physical iOS behavior/performance, Android ARM64 device performance, continuous
communication beyond power-saving restrictions, and store distribution remain
unverified.

## iOS OS URL delivery tests

With the normal app installed in a simulator, run the separate XCTest UI tests.
A simulator running version 16.4 or later is required. Tests use fake invitations
and do not enroll at a Gateway.

```sh
flutter build ios --simulator --debug
xcrun simctl install <simulator-id> build/ios/iphonesimulator/Runner.app
xcodebuild test -project test_driver/ios/LinkTests.xcodeproj -scheme LinkTests \
  -destination 'platform=iOS Simulator,id=<simulator-id>' \
  -derivedDataPath build/ios-links
```

Tests launch the app through OS URLs and verify cold/warm confirmation screens,
IDs, a single confirmation button, and cancellation.

Example release load test, with the TLS fixture running in another terminal:

```sh
python3 - /path/to/fixture-cert.pem /tmp/mobile-fixture.json <<'PY'
import base64
import json
import pathlib
import sys
certificate = pathlib.Path(sys.argv[1]).read_bytes()
pathlib.Path(sys.argv[2]).write_text(json.dumps({
    'FIXTURE_URL': 'https://127.0.0.1:5561',
    'FIXTURE_CA_BASE64': base64.b64encode(certificate).decode(),
}))
PY
adb reverse tcp:5561 tcp:5561
flutter build apk --release --target=integration_test/capacity_test.dart \
  --target-platform android-arm64,android-x64 \
  --dart-define-from-file=/tmp/mobile-fixture.json
adb install -r build/app/outputs/flutter-apk/app-release.apk
# This Waydroid environment only: use Skia. Normal devices need no extra arguments.
adb shell am start -n fyi.omochi.mobile/.MainActivity --ez enable-impeller false
adb logcat -d -s flutter | rg 'Capacity memory:|All tests passed'
# Restore the normal app after measurement
flutter build apk --release --target=lib/main.dart --target-platform android-arm64,android-x64
adb install -r build/app/outputs/flutter-apk/app-release.apk
adb reverse --remove tcp:5561
```

## Connecting to an external Hub

Use Hub connections in the toolbar to register shared dashboards and owner
connections. Paste HTTPS links containing `#share:` / `#owner:`,
`omochi://share/…`, or `omochi://owner/…`, or use the web page's **Open in app**.
Shared connections follow the destination's action and layout-editing permissions.
Owners can edit boards, use presets, create/revoke shares, and inspect connection
information. Manage owner enrollments in the destination Hub's local admin UI.

Only one connection is displayed at a time; the embedded Hub is suspended while
an external connection is active. Saved connections remain available after restart,
and the last selected connection is restored. Public shares support temporary
viewing or explicit saving. Their bearer secret is stored in protected storage
only when the public share is saved. Invitation and owner-enrollment secrets are
not persisted; unknown enrollment outcomes recover with the pending grant and
the same identity.

External identity and records are isolated under `omochi.mobile.remote.v1.identity`.
The embedded Hub's existing identity is neither migrated nor overwritten.
Deleting a connection in the app does not revoke authorization at its destination.
A used web invitation cannot transfer to the app identity; issue a fresh
invitation at the destination.

The C ABI is version 2. Rebuild Flutter and the Rust library together. HTTPS
launch templates and publication instructions are in [deeplinks](deeplinks/README.md).
The `mobile.omochi.fyi` landing page deploys from [mobile-site](../mobile-site/README.md).
Direct app launch from HTTPS links is currently disabled and unavailable because
store support is not ready. `/open` and OS association JSON are not served yet.
Pasting invitations and custom URI handoffs remain available.

Dashboard sharing v2 permits app actions and layout editing per link; both are
disabled by default. Forwarded links have the same permissions. Individual browser
enrollment is unnecessary. Reissue v1 public links and individual shares. Owner
invitations and Gateway trust settings are unchanged.
