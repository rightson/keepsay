# Shared helpers for scripts/release/*.sh. POSIX sh; sourced, not executed.
# shellcheck shell=sh

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

note() {
  printf '%s\n' "$*" >&2
}

# Prints the lowercase SHA-256 hex digest of file $1.
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -- "$1" | awk '{print tolower($1)}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -- "$1" | awk '{print tolower($1)}'
  else
    die "need sha256sum or shasum"
  fi
}

# Converts a Unix timestamp to touch -t format (CCYYMMDDhhmm.SS, UTC).
epoch_to_touch() {
  if date -u -d "@$1" +%Y%m%d%H%M.%S >/dev/null 2>&1; then
    date -u -d "@$1" +%Y%m%d%H%M.%S   # GNU date
  else
    date -u -r "$1" +%Y%m%d%H%M.%S    # BSD date (macOS)
  fi
}

# Prints the path as the compiler sees it. Under Git Bash on Windows, rustc
# and cl.exe see C:\..., not /c/..., so remap prefixes must use that form.
native_path() {
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -w "$1"
  else
    printf '%s\n' "$1"
  fi
}

repo_root() {
  git rev-parse --show-toplevel
}
