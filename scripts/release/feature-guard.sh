#!/bin/sh
# Usage: feature-guard.sh TARGET [extra cargo feature args...]
#
# Fails if the keepsay-cli release graph for TARGET enables keepsay-core's
# `test-endpoint` (INV-3: base URL override) or `test-support` (INV-1/2: mock
# vault). Same idea as the `release-feature-guard` job in ci.yml, but run
# against the exact target and feature set the release build uses.
# keepsay-core also refuses both features with compile_error! in release
# builds; this check catches the mistake before compiling anything.
set -eu

here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=scripts/release/common.sh
. "$here/common.sh"

[ $# -ge 1 ] || die "usage: feature-guard.sh TARGET [cargo feature args...]"
target=$1
shift

cd "$(repo_root)"

tree=$(cargo tree -p keepsay-cli --locked --target "$target" \
  -e normal,build,features -i keepsay-core "$@")
printf '%s\n' "$tree"

# A guard that sees nothing proves nothing.
printf '%s\n' "$tree" | grep -q '^keepsay-core v' \
  || die "cargo tree did not show keepsay-core; guard would be vacuous"

for f in test-endpoint test-support; do
  if printf '%s\n' "$tree" | grep -q "feature \"$f\""; then
    die "keepsay-core feature \"$f\" is enabled in the release graph for $target"
  fi
done
note "feature guard: no test-endpoint / test-support for $target"
