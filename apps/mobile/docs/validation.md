# Initial release validation records

2026-11-03. These results were obtained with the new app in this checkout,
separately from results for the experimental app.

## Environments and results

| Target | Result |
| --- | --- |
| Flutter | 3.47.9 / Dart 3.13.7; static analysis and 14 ordinary tests verified |
| Shared Rust Hub | 26 tests passed with `--no-default-features --lib`; one manual public-HTTPS test ignored |
| C ABI | Five Rust tests passed. Ordinary Flutter tests also cover real-library saving, shutdown, unknown outcomes, and corrupt documents |
| Web | hub-ui type checking, existing tests, normal build, and hosted WASM build verified. Real Chromium verified fragment removal, delivery to another tab, and mobile-link visibility/URI agreement without an owner-Hub connection |
| Android debug | Waydroid / Android 13 / x86_64. With two real Gateways and two TypeScript SDKs: enrollment, rendering, actions, suspend/resume, restart retaining identity, SDK/Gateway restart, and forgetting one registration passed |
| Android OS storage | Two tests passed with real secure storage, covering corrupt documents, save failure, unknown save outcome, shutdown during saving, forgetting failure, and re-enrollment |
| iOS simulator | iPhone 17 Pro / iOS 26.4 / ARM64. The same Gateway/SDK round-trip integration test as Android and two storage-failure tests using the real Keychain passed |
| Native builds | Android ARM64/x86_64 debug/release APKs, iOS simulator debug app, and unsigned iOS device ARM64 debug build verified |

The normal Android release APK uses development signing with the debug key.
This is not store signing or publication. ARM64 APK and iOS device results are
build verification, not physical-device runtime verification.

## Release capacity and memory

[Android release measurements](android-release-memory.json). Two Gateways /
eight SDKs, each with a 64 KiB model and a 1024×1024 image. A load-test APK using
the normal renderer/session/Rust Hub was built as an AOT release and launched
directly.

| State | RSS | Retained pixels |
| --- | --- | --- |
| After Hub startup | About 210.5 MiB | 0 |
| Eight services/eight images | About 238.0 MiB | 32 MiB |
| After suspension/stop | About 239.0 MiB | 0 |

Only the ninth service received a capacity error; all eight accepted views were
retained. Actions from existing views reached their SDKs and updates returned.
After suspension, the in-flight image count also reached zero.

To avoid Vulkan startup hangs in this Waydroid environment, the measurement APK
was launched with `--ez enable-impeller false` to select Skia. The normal release
manifest keeps Flutter's defaults. These results do not establish equivalent
performance on other devices or with default Vulkan.

RSS covers the entire process, including the VM, engine, Rust, and allocator
retained memory. It overlaps retained pixels and must not be added to them.
Releasing resources does not guarantee RSS returns to its startup value.
Conditions differ from the experimental app's debug/JIT measurements, so no
simple improvement percentage is reported.

HTTP control for the load measurement used localhost HTTPS verified with a
temporary fixture-only CA. Images and actions used iroh. The release app's
external-image policy and HTTP permissions were unchanged. Flutter Driver does
not support release mode, so result JSON and successful-test logs were collected
from Android logcat.

## Deep links

With the normal Android release app, passing `omochi://invite/...` through the OS
verified cold-start confirmation and that another invitation received while
running does not replace a pending one. The ID in the
[confirmation screenshot](android-invitation.png) is fictional; enrollment was
not performed.

iOS used XCTest's OS URL launch API to verify cold/warm confirmation, matching
Gateway IDs, cancellation, and a single confirmation button. SceneDelegate
explicitly forwards initial UIScene connection URLs and warm URLs to app_links.

Flutter widget tests cover startup-equivalent delivery, warm delivery, duplicate
notifications, cancellation, and calling pair only after confirmation.

## Validation limits

Physical iOS devices, physical Android ARM64 devices, devices with 16 KiB pages,
iOS release performance, and continuous communication across OS power-saving
or forced termination remain unverified. iOS network enumeration uses limited
compatibility patches in vendored dependencies; default-route, home-router,
and additional IPv6 flags have limitations.

## 2026-11-04 Column/Modal fixes and the preliminary Bonjour gate

Environment: Linux x86_64, Flutter 3.47.9 / Dart 3.13.7. Local `flutter devices`
listed only Waydroid / Android 13 / x86_64. The later investigation below found
SSH access to a Mac/Xcode, but no physical iOS device. Rendering verification
here used Flutter widget tests, not interaction on a device.

Before the fix, `flutter test test/native_layout_test.dart` reproduced a 784px
overflow in a 40-row Column and a bottom sheet remaining after Surface deletion.

| Command | Result |
| --- | --- |
| `cd apps/mobile && flutter analyze --no-pub` | No issues |
| `cd apps/mobile && flutter test --no-pub` | 56 passed, including 28 added in this change |
| `cargo test --manifest-path crates/Cargo.toml -p omochi-transport mdns_resolves_only_an_endpoint_id_without_internet -- --ignored --nocapture` | One passed. Discovery and connection between two Linux Endpoints using only EndpointId. No Rust changes; existing vendor/netwatch deprecation warnings |
| `git diff --check` | No issues |

Column tests verified scrolling to and acting on the last of 40 rows; weight
ratios after subtracting nonweighted children and spacing; a finite List viewport
when no free space remains; explicit/implicit flex; nesting; list/map templates
and data updates; 320px width and text scale 2; flex-height recalculation with
the keyboard; RTL/LTR alignment; and scrolling inside a Modal. The existing
screen-resize test also passed. IntrinsicHeight was not used.

Modal tests covered reopening after ordinary dismissal, Surface deletion and
replacement, renderer restart with the same render ID, Modal component removal,
disconnection retraction, forget, suspend, stop, navigation away, disposal during
build, and revocation before the route's first build. Old onPressed callbacks
could not reopen the sheet; another Surface's sheet and an overlaid dialog
remained. Forget/suspend and similar operations used a test GatewayClient and
did not revalidate Rust or OS behavior.

### Preliminary Bonjour gate: not performed

The plan required bidirectional discovery between Bonjour on a physical iOS
device and the existing Linux Gateway before expanding implementation. Production
iOS discovery and new internal Rust Hub interfaces were therefore deferred.
Linux-to-Linux mDNS success did not satisfy this gate. The `crates/Cargo.lock`
baseline was `swarm-discovery 0.6.5` / `iroh-mdns-address-lookup 0.6.1`.
`NSBonjourServices`, local-network usage descriptions, authorization, and the
Gateway wire contract were unchanged.

Access to Mac/Xcode was confirmed in the investigation below. The preliminary
gate still requires a signed physical-iOS Bonjour probe and a Linux Gateway
from the same checkout on the same LAN. The probe must use OS DNS-SD APIs
without a multicast entitlement. First record this evidence:

1. Use iOS Browse/Resolve/GetAddrInfo to discover Linux `_irohv1._udp`; compare
   the lowercase Base32 EndpointId instance name, all SRV ports, A/AAAA records,
   IPv6 scope IDs, and TXT.
2. Advertise the same format with iOS OS DNS-SD and verify that existing Linux
   mDNS lookup receives the same EndpointId and every connection candidate.
   Ensure automatic renaming does not change the EndpointId.
3. After both discovery directions pass, proceed to production implementation
   and verify existing iroh QUIC connections and peer role/trust authorization.
   Discovery data must not authorize access.

Required post-implementation checks include bidirectional iOS/Linux and
iOS/Android discovery/connections, IPv4/IPv6, multiple addresses/ports,
revocation, network changes, foreground resume, local-network permission denial
and regrant, and generation rejection of callbacks after stop. Include
rediscovery/reconnection using only a registered EndpointId with stale saved
addresses and no internet. These checks were not performed here, and iOS
discovery was not considered complete. If the gate fails, record the cause and
reconsider the approach rather than automatically switching to entitlements.

### Additional investigation: Mac Bonjour/Linux compatibility

On the same day, the user-specified `ssh macbook` worked with the existing SSH
configuration. The suggested backup private-key file was not used. The Mac ran
macOS 27.0.1 with Xcode-27.0.0 selected; `dns-sd` and iOS Simulator were available.
The physical-device section of `xcrun xctrace list devices` listed only the Mac
itself, and the user also confirmed no physical iOS device. Linux and Mac were
on the same LAN.

As a preliminary check separate from the physical-iOS gate, Linux started a
temporary Endpoint with the existing `omochi_transport::bind(key, true, None)`.
No real Gateway process or registered trust was used. Mac commands were
`dns-sd -B _irohv1._udp local`,
`dns-sd -L <instance> _irohv1._udp local`, and
`dns-sd -R <instance> _irohv1._udp local 41973 user-data=omochi-bonjour-probe`.
Each instance was the temporary EndpointId encoded as 52 lowercase Base32
characters. In reverse, a fresh Linux `MdnsAddressLookup::resolve` checked for
the Mac's IPv4 and port 41973. The probe stopped advertisements and child
processes on exit; the temporary Rust test was removed.

| Condition | Result |
| --- | --- |
| Normal offline Endpoint → Mac Browse | Passed; the Linux instance appeared |
| Same Endpoint → Mac Resolve | No callback for 15 seconds; existing advertisement had no TXT |
| Repeat with only Linux probe user data added | Mac Resolve passed; adding TXT alone changed the result |
| Mac Register → Linux Resolve | Failed twice; lookup ended after about 10 seconds without candidates |
| Multiple SRV records | Packets contained two ports for Linux IPv4/IPv6; normal Mac Resolve callback returned only one |

The TXT experiment did not change production advertisements. Linux multicast
packets were inspected only for the temporary instance; only TXT length was
recorded, not its contents. Mac SRV/TXT and A/AAAA reached Linux with CLASS
`0x8001`, including the cache-flush bit.

Comparing `cargo tree --manifest-path crates/Cargo.toml -p omochi-transport -e features -i hickory-proto`
with dependency sources showed that `hickory-proto 0.26.3` lacked its `mdns`
feature. Without it, `0x8001` was not normalized to `IN`, and
`swarm-discovery 0.6.5`'s `receiver.rs` excluded SRV/TXT/A/AAAA at
`dns_class != DNSClass::IN`. This specifically prevented reverse reception.
The receiver also collected SRV only from the answer section, missing Bonjour
Browse responses with SRV in the additional section, and did not aggregate
records split across packets.

At this investigation stage, bidirectional resolution between Mac Bonjour and
the current receiver had not passed. Compatibility fixes to existing peers
would revisit the initial assumption that Android/desktop discovery would stay
unchanged, so no implementation was made at that point and findings were
reported to the user.

### Bidirectional resolution fixes and revalidation: Mac/Linux passed

Later that day, after the user requested bidirectional resolution,
`swarm-discovery 0.6.5` was pinned in vendor. Fixes covered TXT advertisement for
empty metadata, cache-flush interpretation, and record matching across answer
and additional sections. Version, service type, EndpointId, per-port address
aggregation, and nonempty TXT were preserved. Authorization, trust, and the
Gateway wire contract were unchanged.

Five packet-level regression tests ran first; three failed for missing TXT,
cache-flush, and section differences. After fixes, all five passed, including
preservation of the existing advertisement format, multiple ports, IPv4/IPv6,
and nonempty TXT. Two existing vendor tests also passed. Native CI gained the
same library tests.

| Command (repository root) | Result |
| --- | --- |
| `CARGO_TARGET_DIR="$PWD/crates/target" cargo test --manifest-path vendor/swarm-discovery/Cargo.toml --lib` | Seven passed |
| `cargo test --locked --manifest-path crates/Cargo.toml -p omochi-transport --tests` | 15 passed, two ignored |
| `cargo test --locked --manifest-path crates/Cargo.toml -p omochi-transport mdns_resolves_only_an_endpoint_id_without_internet -- --ignored --nocapture` | One passed; actual QUIC connection after Linux-to-Linux mDNS discovery |
| `python3 apps/mobile/tool/bonjour_probe.py --ssh-host macbook` | `result: passed`; bidirectional address resolution between Mac/Linux on the same LAN |
| `cargo clippy --locked --manifest-path crates/Cargo.toml -p omochi-transport --all-targets -- -D warnings` | Passed; only existing vendor/netwatch deprecation warnings |

The reusable probe starts an offline Linux Endpoint with a temporary key and
runs Mac `dns-sd` over SSH. Mac Browse and Resolve passed; combining SRV queries
with GetAddrInfo retrieved both Linux SRV records, all six IPv4 addresses, and
one IPv6 address. In reverse, Linux resolved a temporary Mac instance registered
without TXT attributes to the correct EndpointId, obtaining Mac IPv4, global
IPv6, link-local IPv6, and the registered port. Endpoints, advertisements, and
child processes are stopped on exit.

The Mac advertisement used a fictional resolution-only EndpointId with no QUIC
listener. This probe verified address resolution, not bidirectional QUIC,
authorization, or offline reconnection. Link-local IPv6 scope-ID preservation
and connections, split-packet aggregation, revocation, network changes, and
foreground resume were outside this fix. The iOS Bonjour app implementation
and physical-device gate remained incomplete. No switch to multicast
entitlements was made.

## 2026-11-04 macOS app support and Bonjour integration tests

The macOS Runner gained Rust Hub build/bundle/signing and loads the C ABI
library from the app's Frameworks directory. Debug/release declare sandbox
network client/server access, Bonjour `_irohv1._udp`, local-network usage, and
the invitation URL scheme. macOS storage uses the login Keychain without
sharing, avoiding Keychain Sharing's provisioning-profile requirement. The
normal app has no memory-storage fallback.

Required sources from this checkout were synchronized to `/tmp/omochi-macos` on
the MacBook. Environment: Flutter 3.47.9 / Dart 3.13.7, macOS 27.0.1, Xcode 27.0,
Apple Silicon. Tests ran the macOS Flutter app, not iOS Simulator.

| Target | Result |
| --- | --- |
| `flutter analyze --no-pub` | No issues |
| `flutter test --no-pub` | 61 ordinary tests passed |
| `flutter build macos --debug --no-pub` | Passed, including Rust Hub bundling/signing |
| `flutter build macos --release --no-pub` | Passed; 79.8 MB. Confirmed arm64/x86_64 in app and Hub dylib, `codesign --verify --deep --strict`, release dylib loading, and C ABI version 1 |
| `flutter test --no-pub integration_test/bonjour_test.dart -d macos --dart-define=BONJOUR_MEMORY_STORE=true --dart-define=BONJOUR_HOLD_SECONDS=40` | One passed. Real Rust Hub inside the sandbox; OS DNSServiceBrowse/Resolve returned the instance matching its public EndpointId, host, port, and LAN interface. Hub stop/start retained the same ID |
| Linux `bonjour_probe --resolve <app's public Hub ID>` | Passed; obtained LAN IPv4, IPv6, and port from that macOS app's advertisement, not a fictional advertisement in another process |
| Above integration test with `--dart-define=BONJOUR_PEER_ID=<Linux Gateway ID>` | Passed. App OS Bonjour browsed/resolved a temporary offline Gateway from a newly built CLI; Linux simultaneously resolved the same app in reverse. No invitations or registered trust; temporary configuration/processes removed on exit |
| Bonjour integration with real Keychain | Storage preflight failed with `-25308: User interaction is not allowed`. OS query also reported login Keychain `unlocked=false`; revalidation after unlocking remains pending |

`BONJOUR_MEMORY_STORE=true` is an integration-test fixture for isolating
networking; it does not change normal app storage. A memory-store pass is not
evidence of real Keychain persistence or identity retention across processes.
In the locked SSH session, Flutter foregrounding also returned `open returned 1`,
but in-app Hub and OS DNS-SD tests ran successfully.

The first release build failed to load a Rust proc-macro with E0463. Direct
`dlopen` of the dylib returned `mis-aligned LINKEDIT string pool`; the debug
dylib loaded normally. Only the macOS build script was changed to disable
Cargo release stripping, then run Xcode `strip -S -x` on the final dylib before
signing. Both proc-macro and final release dylib loaded after the fix. Release
profiles on other platforms were unchanged.

The release app for ordinary launch is at
`/tmp/omochi-macos/apps/mobile/build/macos/Build/Products/Release/mobile.app`
on the MacBook. It uses development ad-hoc signing without notarization. Intel
was cross-built, not run on physical Intel hardware.

At this section's validation date, production discovery/advertisement still
used Rust mDNS. See the next section for the later production adapter and
results. OS DNS-SD calls added here were in integration tests. This did not
implement or validate the iOS OS Bonjour adapter. Physical iOS devices,
Mac↔Linux QUIC, registered-role authorization, and offline reconnection were
not verified by this macOS test.

## 2026-11-04 OS selection for production LAN discovery

Implemented in shared `crates/mdns` (`omochi-mdns`). Gateway, Hub, and mobile
native bridge use the same `bind()`. Apple selects OS Bonjour; Windows selects
DNSapi DNS-SD; Linux selects Avahi when an existing Avahi Server2 is available
on system D-Bus, otherwise Rust mDNS. Android retains Rust mDNS. Apple/Windows
do not automatically fall back to raw multicast. SDK, A2UI schema, Gateway wire
contract, and peer role/trust authorization were unchanged. No multicast
entitlement was added.

To match OS DNS-SD's one-service/one-SRV-port model, QUIC IPv4/IPv6 listeners
share a port. Receivers aggregate multiple SRV records and addresses, adding
the receiving interface's scope ID to link-local IPv6. Endpoints own
advertisements, browsers, and callbacks. Removal, stop, or backend
reinitialization invalidates generations, preventing stale callbacks from
recreating state. Avahi record queries use the same IPv4/IPv6 transport as the
service browser; deleting one cache entry does not remove valid records on
the other transport. Avahi reconnects on daemon-owner changes; Windows also
reinitializes on interface changes, including address changes.

| Check | Result |
| --- | --- |
| Linux production selection | `lan_probe` selected `avahi`; offline QUIC with only EndpointId passed |
| No Avahi | Used isolated D-Bus through `DBUS_SYSTEM_BUS_ADDRESS`; selected `rust-mdns`, and Avahi peer connected with only EndpointId. System daemon was not stopped |
| Existing Rust-peer compatibility | `lan_probe --rust` advertised the previous separate IPv4/IPv6 ports. Avahi obtained both ports and QUIC round-trip passed. Rust peer→Mac Bonjour QUIC also passed. Rust peer→Avahi passed using a probe that explicitly resolved the lookup registered on the actual endpoint |
| Mac Bonjour ↔ Linux Avahi | Both resolved LAN IPv4/IPv6/port from public EndpointId. Linux→Mac authenticated QUIC ping/pong passed |
| Mac→Linux incoming QUIC | Resolution passed but connection timed out. A separate plain UDP socket also failed to receive Mac→Linux traffic. Revalidation after resolving the inbound-path restriction remains pending. No root privileges or system-wide firewall changes used |
| Windows VM ↔ Linux Avahi | libvirt `win11` / Windows 11 build 26100 / NAT LAN. Confirmed `windows-dns-sd` selection. EndpointId-only resolution and authenticated QUIC ping/pong passed in both directions. Linux preserved virbr0 scope ID for Windows link-local IPv6 |
| Native advertisement update/stop/restart | `lan_probe lifecycle` passed on Linux Avahi, Mac Bonjour, and Windows DNS-SD: update 43123→43124, no stale advertisement after stop, readvertisement with the same EndpointId/43125, then stop again |
| macOS release | Rebuilt with the Bonjour adapter; 78.1 MB. Confirmed arm64/x86_64 app/Hub dylib, `codesign --verify --deep --strict`, and OS DNSService imports in the final dylib |
| macOS Flutter production adapter | Existing integration test with `BONJOUR_MEMORY_STORE=true` passed. Sandboxed production Rust Hub advertised through Bonjour; OS Browse/Resolve obtained itself and a Linux peer, and ID matched after stop/start |
| iOS device target | `cargo check -p omochi-transport --no-default-features --target aarch64-apple-ios` passed on Mac; not physical-device runtime evidence |
| Rust regression | Transport: 19 ordinary tests passed (16 library/3 integration); two public-relay/real-multicast tests ignored in normal runs. Explicit EndpointId-only offline multicast test passed. Hub: 26 passed, one public-HTTPS test ignored. Four LAN parser/generation/reset/cancel tests also passed on Mac |

`lan_probe` uses ephemeral identities and offline transport, compares the
authenticated EndpointId with the requested ID, and exchanges ping/pong. It
does not change saved trust or perform invitations or enrollment. Flutter analysis and all 61
ordinary tests passed. Clippy for changed transport passed on all Linux targets,
Windows library, and Mac library/examples with the mobile
`--no-default-features` configuration, with existing vendored netwatch
deprecation warnings. Mac Clippy including the desktop feature failed on an
existing `live.rs` nonminimal_bool; this is not a warning-free-workspace claim.

Windows validation used PowerShell inside the VM and temporary HTTP transfer
confined to its NAT network, without enabling SSH/RDP. Only the probe executable
was allowed inbound through an app-specific Windows Firewall rule; that rule
and temporary executable were removed after validation.

Windows GNU debug builds failed on the export-ordinal limit while generating
an unnecessary cdylib in dependency iroh 1.4.1. For this VM probe, a temporary
checkout linked only rlib outputs for iroh/iroh-relay; Rust implementation and
transport sources were unchanged. This did not complete or verify Windows
CLI/distribution packaging. The normal-source Windows transport passed cross
check/Clippy with `--no-default-features`.

Without a physical iOS device, local-network denial/regrant, foreground resume,
real address changes, and reconnection for registered IDs with stale saved
addresses remain unverified. Windows IPv6-only LANs and real network changes
also remain unverified. The Mac login Keychain remained locked, so the Flutter
networking test above used test-only memory storage. These do not satisfy
physical-device acceptance.

## 2026-11-04 Flutter Windows / Linux desktop

Windows/Linux Runners gained plugin registration, CMake build/bundling of the
Rust Hub, and executable-relative FFI loading. Windows uses MSVC and Linux GNU
targets. Existing shared transport selects Windows DNS-SD or Linux Avahi/Rust
mDNS. A2UI schema, SDK API, wire contract, and trust authorization were unchanged.

A Windows named mutex and Linux GApplication maintain one instance. URLs from
subsequent launches are forwarded to the existing app. OS URL-handler
registration is a separate script run by users after placing the bundle; builds
do not change handlers.

| Check | Result |
| --- | --- |
| Linux x64 build | Flutter 3.47.9 / Dart 3.13.7, CachyOS. Debug/Release passed. Release bundle about 47 MiB, including Hub `.so` and plugins |
| Linux real OS storage | `desktop_test.dart` passed with real Secret Service: identity/documents matched after Hub suspend/resume and restart through another bridge/store. Test-only namespace removed |
| Linux warm URL | Second launch of the same executable with a fictional test invitation exited 0; existing window showed the invitation dialog, and no Gateway was registered after cancellation |
| Linux display | Tests passed on Wayland and separate X11/Xvfb. Host Xwayland `:0` did not map the test window and waited; no pass claimed there. Experimental Runner display-order changes were not adopted |
| Linux two-Gateway integration | `mobile_test.dart -d linux` passed: real Gateway/SDK enrollment, two Surfaces, actions/SDK replies, suspend/resume, registration retained over Hub restart, service-generation updates, Gateway reconnection, and isolated forgetting. Used `TEST_ONLINE=false` and a dedicated loopback fixture |
| Linux bundle relocation | Archived/extracted the full bundle to another directory containing spaces; from working directory `/`, loaded the bundled Hub `.so` and confirmed ABI=1 |
| Linux URL registration | Separate `XDG_DATA_HOME` and test `xdg-mime` left existing system handlers unchanged. `desktop-file-validate` and real GLib `gio launch` confirmed exact URI arguments from paths containing spaces, quotes, `$`, and backticks. Paths with percent signs or newlines are explicitly rejected because GLib cannot handle percent signs in executable paths |
| Windows x64 build | Windows 11 build 26100, Flutter 3.47.9 / Dart 3.13.7, Visual Studio Build Tools 2022 17.14.44, Rust MSVC. Debug/Release passed. Release bundle 49,897,438 bytes; Hub DLL 15,731,712 bytes |
| Windows real OS storage/warm URL | `desktop_test.dart -d windows` passed with real DPAPI: identity/documents matched after restart, and invitation forwarding/cancellation from another launch worked. No memory-store substitution |
| Windows two-Gateway integration | `mobile_test.dart -d windows` passed against a Linux Gateway built from current code with fresh temporary data: two Surface actions/SDK replies, suspend/resume, retained registrations, service-generation updates, Gateway rediscovery/reconnection, and isolated forgetting. Test body 36 seconds; `TEST_ONLINE=false`, IPv4 on libvirt NAT |
| Windows bundle relocation | Copied the complete bundle to another directory containing spaces; from `C:\`, loaded the Release Hub DLL and confirmed ABI=1. Also confirmed Flutter DLL, MSVC release CRT, AOT data, and URL-registration script |
| Windows URL registration | Temporary real HKCU registration verified quoting of the executable path with spaces and `%1`, plus `URL Protocol`. Existing handler backed up and restored |
| Flutter regression | `flutter analyze --no-pub` reported no issues; 61 ordinary tests passed, including real C ABI tests |

[Linux app](screenshots/linux-desktop.png) and
[Windows app](screenshots/windows-desktop.png) screenshots were taken inside
real-OS-storage integration tests. They show test-only unenrolled screens and
contain no saved user information.

CI gained Linux/Windows x64 release builds and complete bundle artifacts. The
results above were run locally, not produced by CI. ARM64 target mapping exists
but remains unverified. Linux Video is unsupported because GenUI's video plugin
has no Linux implementation. General media playback, installers, and store
distribution were outside validation.

A real Windows build exposed SDK Unix-server/resource-file code being compiled
even though Hub only needs snapshots. The SDK's default `runtime` feature
preserves its existing API; Hub disables default features and uses the same
`A2uiSnapshot`/`SnapshotCursor` alone. Hub's own Unix connection path is also
restricted to `cfg(unix)` while retaining iroh authentication and role/trust
handling. Passed: 63 ordinary SDK tests including doc-tests, three existing
minimal Snapshot fixtures, Windows-target minimal SDK check, 51 Hub tests with
one public-HTTPS test ignored, and five native-bridge tests. Linux desktop
integration and release build also passed again after separation.

Windows required ATL to compile `flutter_secure_storage_windows`, so
`Microsoft.VisualStudio.Component.VC.ATL` was installed in VS Build Tools. This
MSVC build passed without modifying dependency sources or crate types and did
not use the earlier GNU-probe workaround. Flutter SDK initialization inside
the VM failed Dart HTTPS verification, so validation used the official SDK and
public pub cache fetched and verified on the host. TLS verification was not
disabled. Temporary offline SDK-bootstrap configuration was restored afterward.

The first Windows integration run used a pre-change Linux Gateway binary.
Initial connection/actions passed, but rediscovery after restart failed to
complete within 90 seconds. A fresh Windows DNS-SD probe also failed to obtain
that Gateway's address, and Linux browse results lacked its advertisement on
VM-side virbr0. A Linux run immediately after switching to the current Gateway
also timed out once on reconnection. A later Linux rerun with the same current
configuration passed all checks in five seconds; Windows then passed all checks
with a fresh temporary-identity fixture. Advertisement caching alone was not
established as the cause of the immediate-switch failure. Reconnection just
after switching advertisements from the old Gateway needs further investigation.

An additional minimal-Hub Clippy run failed on three existing findings:
collapsible_if in `scoped.rs`/`stdio.rs` and too_many_arguments in `lib.rs`.
Those implementations were unchanged. This is not a warning-free-workspace
claim; the Hub/bridge/SDK tests above passed.

Local distribution files were `build/omochi-linux-x64.tar.gz` (about 19 MiB)
and `build/omochi-windows-x64.zip` (20,348,049 bytes). Windows ZIP SHA-256
matched after transfer from the VM. The Windows VM was restored to its prior
stopped state. SSH/RDP were not enabled, and this Flutter validation added no
firewall rule.
