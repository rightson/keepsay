#!/bin/sh
# Usage: build.sh TARGET [extra cargo feature args...]
#
# Builds keepsay-cli for TARGET the way the release workflow does:
#   - pinned toolchain (rust-toolchain.toml), --locked, --release
#   - feature guard first (no test-endpoint / test-support)
#   - SOURCE_DATE_EPOCH from the commit, paths remapped for reproducibility
# Writes target/TARGET/release/keepsay-cli[.exe] and a .buildinfo file next
# to it that package.sh puts into the archive.
#
# Extra args go to both `cargo tree` and `cargo build`, e.g.
# `--no-default-features` for a quick local build without whisper.cpp.
set -eu

here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=scripts/release/common.sh
. "$here/common.sh"

[ $# -ge 1 ] || die "usage: build.sh TARGET [cargo feature args...]"
target=$1
shift

root=$(repo_root)
cd "$root"

if [ -z "${SOURCE_DATE_EPOCH:-}" ]; then
  SOURCE_DATE_EPOCH=$(git log -1 --format=%ct HEAD)
fi
export SOURCE_DATE_EPOCH

"$here/feature-guard.sh" "$target" "$@"

# Reproducibility: strip machine-specific paths from panic locations and
# debug info. CARGO_ENCODED_RUSTFLAGS (0x1f-separated) survives spaces in
# paths and overrides any RUSTFLAGS from the environment, so nothing else
# can sneak into a release build.
if [ -n "${RUSTFLAGS:-}" ]; then
  note "ignoring RUSTFLAGS from the environment for the release build"
fi
unset RUSTFLAGS
src=$(native_path "$root")
cargo_home=$(native_path "${CARGO_HOME:-$HOME/.cargo}")
us=$(printf '\037')
flags="--remap-path-prefix=$src=/keepsay${us}--remap-path-prefix=$cargo_home=/cargo"
case $target in
  *-windows-msvc)
    # Deterministic PE timestamp / debug directory.
    flags="$flags${us}-Clink-arg=/Brepro"
    exe=keepsay-cli.exe
    ;;
  *-windows-*)
    exe=keepsay-cli.exe
    ;;
  *)
    exe=keepsay-cli
    # whisper.cpp (C/C++) embeds __FILE__ in asserts; remap those too.
    case "$src$cargo_home" in
      *' '*) note "path contains a space; not setting -ffile-prefix-map" ;;
      *)
        cflags="-ffile-prefix-map=$src=/keepsay -ffile-prefix-map=$cargo_home=/cargo"
        CFLAGS="${CFLAGS:+$CFLAGS }$cflags"
        CXXFLAGS="${CXXFLAGS:+$CXXFLAGS }$cflags"
        export CFLAGS CXXFLAGS
        ;;
    esac
    ;;
esac
CARGO_ENCODED_RUSTFLAGS=$flags
export CARGO_ENCODED_RUSTFLAGS

cargo build --release --locked -p keepsay-cli --target "$target" "$@"

out="target/$target/release"
[ -f "$out/$exe" ] || die "expected binary $out/$exe"

# Everything here must be a function of (commit, toolchain, target, args) so
# two honest builds produce the same archive. Runner image details go to the
# workflow log instead.
{
  printf 'package: keepsay-cli\n'
  printf 'commit: %s\n' "$(git rev-parse HEAD)"
  printf 'target: %s\n' "$target"
  printf 'cargo-args: --release --locked -p keepsay-cli --target %s' "$target"
  for a in "$@"; do printf ' %s' "$a"; done
  printf '\n'
  printf 'source-date-epoch: %s\n' "$SOURCE_DATE_EPOCH"
  printf 'cargo: %s\n' "$(cargo -V)"
  printf '\n# rustc -Vv\n'
  rustc -Vv
} >"$out/keepsay-cli.buildinfo"

note "built $out/$exe"
note "sha256 $(sha256_of "$out/$exe")"
