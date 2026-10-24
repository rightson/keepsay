#!/bin/sh
# Usage: meta.sh   (run by .github/workflows/release.yml, job `meta`)
#
# Decides what the release run is and writes it to $GITHUB_OUTPUT:
#   version            e.g. 1.2.3, or dryrun-<sha12> for workflow_dispatch
#   tag                e.g. v1.2.3 (empty for a dry run)
#   publish            true only for a v*.*.* tag push that passed all checks
#   prerelease         true when the tag has a -suffix
#   source_date_epoch  committer time of the commit being built
# For a tag push it also requires that:
#   - the tag is strict semver (vMAJOR.MINOR.PATCH[-PRE])
#   - it matches [workspace.package] version in Cargo.toml
#   - the tagged commit is reachable from origin/main (reviewed code only)
set -eu

here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
# shellcheck source=scripts/release/common.sh
. "$here/common.sh"

: "${GITHUB_EVENT_NAME:?}" "${GITHUB_REF:?}" "${GITHUB_SHA:?}" "${GITHUB_OUTPUT:?}"

cd "$(repo_root)"

sde=$(git log -1 --format=%ct "$GITHUB_SHA")

cargo_version=$(awk '
  /^\[/ { in_ws = ($0 == "[workspace.package]") }
  in_ws && /^version[[:space:]]*=/ { gsub(/.*=[[:space:]]*"|".*/, ""); print; exit }
' Cargo.toml)
[ -n "$cargo_version" ] || die "no [workspace.package] version in Cargo.toml"

publish=false
prerelease=false
tag=
case $GITHUB_EVENT_NAME in
  push)
    case $GITHUB_REF in
      refs/tags/v*) tag=${GITHUB_REF#refs/tags/} ;;
      *) die "release runs on tag pushes only, got $GITHUB_REF" ;;
    esac
    printf '%s\n' "$tag" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$' \
      || die "tag $tag is not vMAJOR.MINOR.PATCH[-PRE]"
    version=${tag#v}
    [ "$version" = "$cargo_version" ] \
      || die "tag $tag does not match Cargo.toml version $cargo_version"
    git merge-base --is-ancestor "$GITHUB_SHA" refs/remotes/origin/main \
      || die "tagged commit $GITHUB_SHA is not on origin/main"
    case $version in *-*) prerelease=true ;; esac
    publish=true
    ;;
  workflow_dispatch)
    version="dryrun-$(printf '%.12s' "$GITHUB_SHA")"
    ;;
  *)
    die "unexpected event $GITHUB_EVENT_NAME"
    ;;
esac

{
  printf 'version=%s\n' "$version"
  printf 'tag=%s\n' "$tag"
  printf 'publish=%s\n' "$publish"
  printf 'prerelease=%s\n' "$prerelease"
  printf 'source_date_epoch=%s\n' "$sde"
} >>"$GITHUB_OUTPUT"

note "version=$version tag=$tag publish=$publish prerelease=$prerelease sde=$sde"
