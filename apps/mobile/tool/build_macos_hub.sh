#!/bin/bash
set -euo pipefail
APP_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO_ROOT="$(cd "$APP_ROOT/../.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/crates/target}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
PROFILE=debug
CARGO_PROFILE=dev
if [[ "${CONFIGURATION:-Debug}" != Debug ]]; then
  PROFILE=release
  CARGO_PROFILE=release
  # LLVM stripping can produce a LINKEDIT string pool rejected by macOS 27's
  # dyld, including host proc-macro dylibs. Strip the final bundle with Xcode.
  export CARGO_PROFILE_RELEASE_STRIP=none
fi
LIBRARIES=()
for ARCH in ${ARCHS:-$(uname -m)}; do
  case "$ARCH" in
  arm64) TARGET=aarch64-apple-darwin ;;
  x86_64) TARGET=x86_64-apple-darwin ;;
  *)
    echo "Unsupported macOS architecture: $ARCH" >&2
    exit 1
    ;;
  esac
  rustup target add "$TARGET"
  SDKROOT="$(xcrun --sdk macosx --show-sdk-path)" \
    cargo rustc --lib --crate-type cdylib --locked --manifest-path "$REPO_ROOT/crates/Cargo.toml" \
    -p omochi-native-bridge --target "$TARGET" --profile "$CARGO_PROFILE"
  LIBRARIES+=("$CARGO_TARGET_DIR/$TARGET/$PROFILE/libomochi_native_bridge.dylib")
done
OUT="$APP_ROOT/build/shared-hub/macos/${CONFIGURATION:-Debug}"
mkdir -p "$OUT"
STAGING="$(mktemp "$OUT/libomochi_native_bridge.XXXXXX")"
trap 'rm -f "$STAGING"' EXIT
xcrun lipo -create "${LIBRARIES[@]}" -output "$STAGING"
xcrun install_name_tool -id '@rpath/libomochi_native_bridge.dylib' "$STAGING"
if [[ "$PROFILE" == release ]]; then xcrun strip -S -x "$STAGING"; fi
# Xcode signs the app after its build phases. Sign this nested Mach-O first.
DESTINATION="${TARGET_BUILD_DIR:?}/${FRAMEWORKS_FOLDER_PATH:?}"
mkdir -p "$DESTINATION"
LIBRARY="$DESTINATION/libomochi_native_bridge.dylib"
mv -f "$STAGING" "$LIBRARY"
if [[ "${CODE_SIGNING_ALLOWED:-YES}" != NO ]]; then
  codesign --force --sign "${EXPANDED_CODE_SIGN_IDENTITY:--}" --timestamp=none "$LIBRARY"
fi
