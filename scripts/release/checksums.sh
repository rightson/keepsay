#!/bin/sh
# Usage: checksums.sh DIR
#
# Writes DIR/SHA256SUMS for every regular file in DIR (except SHA256SUMS),
# sorted by name, in the `sha256sum` format: "<hex>  <name>".
# Verify with `sha256sum -c SHA256SUMS`, `shasum -a 256 -c SHA256SUMS`, or
# scripts/release/verify.sh.
set -eu

here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=scripts/release/common.sh
. "$here/common.sh"

[ $# -eq 1 ] || die "usage: checksums.sh DIR"
cd "$1"

LC_ALL=C
export LC_ALL

tmp=SHA256SUMS.tmp
: >"$tmp"
count=0
for f in *; do
  [ -f "$f" ] || continue
  case $f in SHA256SUMS | "$tmp") continue ;; esac
  case $f in *' '* | *'*'*) die "refusing file name with space or '*': $f" ;; esac
  printf '%s  %s\n' "$(sha256_of "$f")" "$f" >>"$tmp"
  count=$((count + 1))
done
[ "$count" -gt 0 ] || die "no files to checksum in $1"
mv "$tmp" SHA256SUMS
cat SHA256SUMS
