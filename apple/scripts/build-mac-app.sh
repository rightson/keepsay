#!/usr/bin/env bash
# Builds apple/KeepsayMac with SwiftPM and wraps it into Keepsay.app
# (docs/changes/0015), signed with Hardened Runtime.
#
#   apple/scripts/build-mac-app.sh            # ad-hoc signature (CI, local)
#   SIGN_IDENTITY="Developer ID Application: …" TEAM_ID=XXXXXXXXXX \
#     apple/scripts/build-mac-app.sh          # release signing (M5, CI only)
#
# Run apple/scripts/build-core.sh first to link the Rust core; without it the
# app builds against UnavailableCore (no network, no STT).
#
# Output: apple/build/Keepsay.app and apple/build/Keepsay-macOS.zip
# Notarization is not done here (PROPOSAL §16 item 1; US-M2-17).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PKG="$ROOT/apple/KeepsayMac"
OUT="$ROOT/apple/build"
APP="$OUT/Keepsay.app"
VERSION="${KEEPSAY_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)}"
BUILD="${KEEPSAY_BUILD:-${GITHUB_RUN_NUMBER:-0}}"
IDENTITY="${SIGN_IDENTITY:--}"
ARCHS="${KEEPSAY_ARCHS:-$(uname -m)}"

[ "$(uname -s)" = Darwin ] || { echo "needs macOS" >&2; exit 1; }

arch_flags=()
for a in $ARCHS; do arch_flags+=(--arch "$a"); done

swift build --package-path "$PKG" -c release --product KeepsayMac "${arch_flags[@]}"
bin_dir="$(swift build --package-path "$PKG" -c release --product KeepsayMac "${arch_flags[@]}" --show-bin-path)"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$bin_dir/KeepsayMac" "$APP/Contents/MacOS/KeepsayMac"
sed -e "s/__VERSION__/${VERSION:-0.0.0}/" -e "s/__BUILD__/$BUILD/" \
  "$PKG/App/Info.plist" >"$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"

if [ "$IDENTITY" = "-" ]; then
  entitlements="$PKG/App/KeepsayMac.adhoc.entitlements"
else
  : "${TEAM_ID:?TEAM_ID is required for release signing}"
  entitlements="$OUT/KeepsayMac.entitlements"
  sed "s/__TEAM_ID__/$TEAM_ID/" "$PKG/App/KeepsayMac.entitlements" >"$entitlements"
fi

# --options runtime = Hardened Runtime (US-M2-03 ③).
codesign --force --sign "$IDENTITY" --options runtime --timestamp=none \
  --entitlements "$entitlements" "$APP"
codesign --verify --strict --verbose=2 "$APP"
codesign --display --entitlements - "$APP"

(cd "$OUT" && rm -f Keepsay-macOS.zip && ditto -c -k --keepParent Keepsay.app Keepsay-macOS.zip)
echo "==> $APP (archs: $ARCHS, signature: $IDENTITY)"
