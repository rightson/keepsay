#!/bin/sh
# Usage: verify.sh [-R OWNER/REPO] [-t TAG] [--no-gh] SHA256SUMS FILE...
#
# Checks keepsay release downloads. See docs/release/VERIFYING.md.
#   1. Each FILE is listed in SHA256SUMS and its SHA-256 matches.
#   2. If the GitHub CLI (`gh`) is installed, each FILE and SHA256SUMS has a
#      build provenance attestation signed by this repo's release workflow,
#      built on a GitHub-hosted runner, from TAG when -t is given.
# Step 2 is optional but is the step that ties the bytes to the source.
# Without it you only know the files match the SHA256SUMS you downloaded.
# Exits non-zero if any check fails.
#
# Standalone: needs only a POSIX shell, awk, and sha256sum or shasum.
set -eu

repo=rightson/keepsay
tag=
use_gh=auto

die() { printf 'verify: %s\n' "$*" >&2; exit 1; }
usage() { die "usage: verify.sh [-R OWNER/REPO] [-t TAG] [--no-gh] SHA256SUMS FILE..."; }

while [ $# -gt 0 ]; do
  case $1 in
    -R | --repo) [ $# -ge 2 ] || usage; repo=$2; shift 2 ;;
    -t | --tag) [ $# -ge 2 ] || usage; tag=$2; shift 2 ;;
    --no-gh) use_gh=no; shift ;;
    -h | --help) sed -n '2,13p' "$0"; exit 0 ;;
    --) shift; break ;;
    -*) usage ;;
    *) break ;;
  esac
done
[ $# -ge 2 ] || usage
sums=$1
shift
[ -f "$sums" ] || die "no such file: $sums"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -- "$1" | awk '{print tolower($1)}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -- "$1" | awk '{print tolower($1)}'
  else
    die "need sha256sum or shasum"
  fi
}

# Provenance must come from this repo's release workflow on a GitHub-hosted
# runner and, with -t, from that exact tag.
gh_verify() {
  if [ -n "$tag" ]; then
    gh attestation verify "$1" --repo "$repo" \
      --signer-workflow "$repo/.github/workflows/release.yml" \
      --deny-self-hosted-runners --source-ref "refs/tags/$tag"
  else
    gh attestation verify "$1" --repo "$repo" \
      --signer-workflow "$repo/.github/workflows/release.yml" \
      --deny-self-hosted-runners
  fi
}

failed=0

printf '== SHA-256 against %s\n' "$sums"
for f in "$@"; do
  [ -f "$f" ] || { printf 'FAIL  %s: no such file\n' "$f"; failed=1; continue; }
  name=$(basename -- "$f")
  want=$(awk -v n="$name" '{ f = $2; sub(/^\*/, "", f); if (f == n) print tolower($1) }' "$sums")
  case $want in
    '') printf 'FAIL  %s: not listed in %s\n' "$name" "$sums"; failed=1; continue ;;
    *[!0-9a-f]*) printf 'FAIL  %s: listed more than once or malformed\n' "$name"; failed=1; continue ;;
  esac
  [ ${#want} -eq 64 ] || { printf 'FAIL  %s: malformed digest\n' "$name"; failed=1; continue; }
  got=$(sha256_of "$f")
  if [ "$got" = "$want" ]; then
    printf 'OK    %s\n' "$name"
  else
    printf 'FAIL  %s: sha256 %s, expected %s\n' "$name" "$got" "$want"
    failed=1
  fi
done

if [ "$use_gh" = auto ] && ! command -v gh >/dev/null 2>&1; then
  use_gh=missing
fi

case $use_gh in
  auto)
    printf '\n== Build provenance (gh attestation verify, repo %s)\n' "$repo"
    for f in "$sums" "$@"; do
      [ -f "$f" ] || continue
      if gh_verify "$f" >/dev/null; then
        printf 'OK    %s\n' "$(basename -- "$f")"
      else
        printf 'FAIL  %s: no valid attestation from %s release.yml\n' "$(basename -- "$f")" "$repo"
        failed=1
      fi
    done
    ;;
  missing)
    printf '\nNOTE  gh not found: provenance NOT checked. Install the GitHub CLI and\n'
    printf '      re-run, or see docs/release/VERIFYING.md for the manual command.\n'
    ;;
  no)
    printf '\nNOTE  --no-gh: provenance NOT checked.\n'
    ;;
esac

if [ "$failed" -ne 0 ]; then
  printf '\nVERIFICATION FAILED. Do not run these files.\n'
  exit 1
fi
printf '\nAll checks passed.\n'
