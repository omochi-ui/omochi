#!/bin/bash
set -euo pipefail
APP_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO_ROOT="$(cd "$APP_ROOT/../.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
PROFILE=debug
CARGO_PROFILE=dev
if [[ "${CONFIGURATION:-Debug}" != Debug ]]; then
  PROFILE=release
  CARGO_PROFILE=release
fi
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/crates/target}"
OUT="$APP_ROOT/build/shared-hub/ios"
mkdir -p "$OUT/device" "$OUT/simulator"
for TARGET in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
  rustup target add "$TARGET"
  SDK=iphoneos
  if [[ "$TARGET" != aarch64-apple-ios ]]; then SDK=iphonesimulator; fi
  SDKROOT="$(xcrun --sdk "$SDK" --show-sdk-path)" IPHONEOS_DEPLOYMENT_TARGET=15.0 \
    cargo rustc --lib --crate-type staticlib --locked --manifest-path "$REPO_ROOT/crates/Cargo.toml" \
    -p omochi-native-bridge --target "$TARGET" --profile "$CARGO_PROFILE"
done
cp "$CARGO_TARGET_DIR/aarch64-apple-ios/$PROFILE/libomochi_native_bridge.a" "$OUT/device/"
xcrun lipo -create "$CARGO_TARGET_DIR/aarch64-apple-ios-sim/$PROFILE/libomochi_native_bridge.a" \
  "$CARGO_TARGET_DIR/x86_64-apple-ios/$PROFILE/libomochi_native_bridge.a" -output "$OUT/simulator/libomochi_native_bridge.a"
# xcodebuild needs an unused output path; replace the prior generated bundle only.
STAGING="$(mktemp -d "$OUT/framework.XXXXXX")"
xcodebuild -create-xcframework -library "$OUT/device/libomochi_native_bridge.a" \
  -headers "$REPO_ROOT/crates/native-bridge/include" -library "$OUT/simulator/libomochi_native_bridge.a" \
  -headers "$REPO_ROOT/crates/native-bridge/include" -output "$STAGING/OmochiHub.xcframework"
rm -rf "$OUT/OmochiHub.xcframework"
mv "$STAGING/OmochiHub.xcframework" "$OUT/"
rmdir "$STAGING"
