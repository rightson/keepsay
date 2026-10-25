#!/usr/bin/env bash
# Builds keepsay-core as static libraries for macOS (arm64 + x86_64), iOS
# device (arm64) and iOS simulator (arm64 + x86_64), generates the UniFFI
# Swift bindings, and packages KeepsayCoreFFI.xcframework.
#
# Output (all gitignored, see apple/.gitignore):
#   apple/build/core/KeepsayCoreFFI.xcframework
#   apple/build/core/bindings/keepsay_core.swift
#   apple/KeepsayKit/Frameworks/KeepsayCoreFFI.xcframework        (copy)
#   apple/KeepsayKit/Sources/KeepsayCoreBindings/keepsay_core.swift (copy)
# With the two copies in place, apple/KeepsayKit/Package.swift links the core.
#
# Environment:
#   KEEPSAY_CORE_PLATFORMS  space-separated subset of "macos ios ios-sim"
#                           (default: all three). The iOS app needs ios and
#                           ios-sim; the macOS app needs macos.
#   KEEPSAY_CORE_FEATURES   cargo features for keepsay-core (default:
#                           "whisper"). Must be identical for every slice:
#                           the bindings are generated once and UniFFI checks
#                           API checksums at load time.
#   KEEPSAY_CORE_PROFILE    release (default) or debug.
#   KEEPSAY_SKIP_BINDGEN=1  build and lipo the static libraries only (checks
#                           that core cross-compiles); no bindings, no
#                           XCFramework.
#
# The Swift bindings come from the workspace build tool
# `keepsay-uniffi-bindgen` (bin `uniffi-bindgen-swift`), the only crate that
# enables uniffi's `cli` feature (docs/changes/0018 D-12, 0019). It reads the
# UniFFI metadata from keepsay-core's static library (library mode), so no
# cdylib is built. keepsay-core is always built alone (`-p keepsay-core`) so
# feature unification cannot pull `cli` into the shipped library.
#
# Rules: never builds with `test-endpoint` or `test-support` (INV-1/2/3).
# Needs macOS with Xcode (lipo, xcodebuild).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="$ROOT/apple/build/core"
KIT="$ROOT/apple/KeepsayKit"
PLATFORMS="${KEEPSAY_CORE_PLATFORMS:-macos ios ios-sim}"
FEATURES="${KEEPSAY_CORE_FEATURES-whisper}"
PROFILE="${KEEPSAY_CORE_PROFILE:-release}"
MODULE="keepsay_coreFFI"

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}"
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"

die() { echo "build-core.sh: $*" >&2; exit 1; }

case ",$FEATURES," in
  *test-endpoint*|*test-support*) die "refusing test-only features in an app build: $FEATURES" ;;
esac
[ "$(uname -s)" = Darwin ] || die "needs macOS with Xcode"
SKIP_BINDGEN="${KEEPSAY_SKIP_BINDGEN:-0}"

targets_for() {
  case "$1" in
    macos) echo "aarch64-apple-darwin x86_64-apple-darwin" ;;
    ios) echo "aarch64-apple-ios" ;;
    ios-sim) echo "aarch64-apple-ios-sim x86_64-apple-ios" ;;
    *) die "unknown platform: $1" ;;
  esac
}

cargo_flags=(-p keepsay-core --lib --locked)
[ "$PROFILE" = release ] && cargo_flags+=(--release)
[ -n "$FEATURES" ] && cargo_flags+=(--features "$FEATURES")

rm -rf "$OUT"
mkdir -p "$OUT/libs" "$OUT/headers" "$OUT/bindings"

cd "$ROOT"

# 1. Static libraries per target (core/Cargo.toml: crate-type includes
#    staticlib), merged per platform with lipo.
xcf_args=()
for platform in $PLATFORMS; do
  slices=()
  for t in $(targets_for "$platform"); do
    rustup target add "$t" >/dev/null
    echo "==> keepsay-core staticlib for $t"
    cargo build "${cargo_flags[@]}" --target "$t"
    slices+=("target/$t/$PROFILE/libkeepsay_core.a")
  done
  mkdir -p "$OUT/libs/$platform"
  lipo -create "${slices[@]}" -output "$OUT/libs/$platform/libkeepsay_core.a"
  xcf_args+=(-library "$OUT/libs/$platform/libkeepsay_core.a" -headers "$OUT/headers")
done

if [ "$SKIP_BINDGEN" = 1 ]; then
  echo "==> KEEPSAY_SKIP_BINDGEN=1: static libraries in $OUT/libs; no bindings or XCFramework"
  exit 0
fi

# 2. Swift bindings, generated once in library mode from one of the static
#    libraries just built: every slice has the same features, hence the same
#    UniFFI metadata and checksums.
echo "==> uniffi-bindgen-swift (keepsay-uniffi-bindgen)"
cargo build -p keepsay-uniffi-bindgen --release --locked
bindgen="target/release/uniffi-bindgen-swift"
first_platform="${PLATFORMS%% *}"
first_target="$(targets_for "$first_platform" | cut -d' ' -f1)"
meta_lib="target/$first_target/$PROFILE/libkeepsay_core.a"
[ -f "$meta_lib" ] || die "missing $meta_lib"

"$bindgen" "$meta_lib" "$OUT/bindings" \
  --swift-sources --headers --modulemap \
  --module-name "$MODULE" \
  --modulemap-filename module.modulemap
mv "$OUT/bindings/$MODULE.h" "$OUT/headers/"
mv "$OUT/bindings/module.modulemap" "$OUT/headers/"
[ -f "$OUT/bindings/keepsay_core.swift" ] || die "bindgen produced no keepsay_core.swift"

# 3. XCFramework.
xcodebuild -create-xcframework "${xcf_args[@]}" -output "$OUT/KeepsayCoreFFI.xcframework"

# 4. Hand both to KeepsayKit.
rm -rf "$KIT/Frameworks/KeepsayCoreFFI.xcframework" "$KIT/Sources/KeepsayCoreBindings"
mkdir -p "$KIT/Frameworks" "$KIT/Sources/KeepsayCoreBindings"
cp -R "$OUT/KeepsayCoreFFI.xcframework" "$KIT/Frameworks/"
cp "$OUT/bindings/keepsay_core.swift" "$KIT/Sources/KeepsayCoreBindings/"

echo "==> done: $OUT/KeepsayCoreFFI.xcframework (platforms: $PLATFORMS; features: ${FEATURES:-none})"
