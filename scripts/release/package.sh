#!/bin/sh
# Usage: package.sh TARGET VERSION OUTDIR
#
# Packs the binary from build.sh plus LICENSE, README.md, SECURITY.md and
# BUILDINFO.txt into OUTDIR/keepsay-cli-VERSION-TARGET.{tar.gz,zip}.
# Windows targets get .zip, everything else .tar.gz. The archive is made as
# deterministic as the local tools allow: fixed file order, mtimes from
# SOURCE_DATE_EPOCH, fixed modes, owner 0, gzip without name or timestamp.
# Prints the archive path on stdout.
set -eu

here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=scripts/release/common.sh
. "$here/common.sh"

[ $# -eq 3 ] || die "usage: package.sh TARGET VERSION OUTDIR"
target=$1
version=$2
outdir=$3

case $version in
  *[!0-9A-Za-z.+-]* | '') die "bad version: $version" ;;
esac

root=$(repo_root)
mkdir -p "$outdir"
outdir=$(CDPATH='' cd -- "$outdir" && pwd)
cd "$root"

if [ -z "${SOURCE_DATE_EPOCH:-}" ]; then
  SOURCE_DATE_EPOCH=$(git log -1 --format=%ct HEAD)
fi

case $target in
  *-windows-*) exe=keepsay-cli.exe; kind=zip ;;
  *) exe=keepsay-cli; kind=tar.gz ;;
esac

bin="target/$target/release/$exe"
info="target/$target/release/keepsay-cli.buildinfo"
[ -f "$bin" ] || die "missing $bin (run build.sh first)"
[ -f "$info" ] || die "missing $info (run build.sh first)"

name="keepsay-cli-$version-$target"
base="$root/target/package"
stage="$base/$name"
rm -rf "$stage"
mkdir -p "$stage"

cp "$bin" "$stage/$exe"
cp LICENSE README.md SECURITY.md "$stage/"
{ printf 'version: %s\n' "$version"; cat "$info"; } >"$stage/BUILDINFO.txt"

chmod 0755 "$stage" "$stage/$exe"
chmod 0644 "$stage/LICENSE" "$stage/README.md" "$stage/SECURITY.md" "$stage/BUILDINFO.txt"

stamp=$(epoch_to_touch "$SOURCE_DATE_EPOCH")
cd "$base"
for f in "$name" "$name"/*; do
  TZ=UTC0 touch -t "$stamp" "$f"
done

list="$base/$name.list"
{
  printf '%s\n' "$name"
  for f in "$name"/*; do printf '%s\n' "$f"; done | LC_ALL=C sort
} >"$list"

case $kind in
  tar.gz)
    out="$outdir/$name.tar.gz"
    if tar --version 2>/dev/null | grep -q 'GNU tar'; then
      tar --format=ustar --no-recursion --owner=0 --group=0 --numeric-owner \
        --mtime="@$SOURCE_DATE_EPOCH" -cf - -T "$list" | gzip -n -9 >"$out"
    else
      # bsdtar (macOS)
      tar --format=ustar --no-recursion --uid 0 --gid 0 --uname '' --gname '' \
        -cf - -T "$list" | gzip -n -9 >"$out"
    fi
    ;;
  zip)
    out="$outdir/$name.zip"
    rm -f "$out"
    files=$(grep '/' "$list")
    if command -v zip >/dev/null 2>&1; then
      # -X: no extra attributes (uid/gid, extended timestamps); -D: no dir entries.
      printf '%s\n' "$files" | TZ=UTC0 zip -X -D -9 -q "$out" -@
    elif command -v 7z >/dev/null 2>&1; then
      printf '%s\n' "$files" >"$list.files"
      # -mtc=off: no NTFS timestamps; only the DOS mtime set above.
      7z a -tzip -mx=9 -mtc=off -bso0 "$(native_path "$out")" "@$name.list.files"
      rm -f "$list.files"
    else
      die "need zip or 7z to build $out"
    fi
    ;;
esac

rm -f "$list"
note "packaged $out"
printf '%s\n' "$out"
