#!/bin/sh
# Fetch the Unicode Character Database files listed in ucd.lock and check
# their SHA-256 (ADR 0019).
#
# Usage: scripts/fetch-ucd.sh [LOCK [DEST]]
#   LOCK  default: ucd.lock at the repository root
#   DEST  default: target/ucd at the repository root
#
# Each line of LOCK is `<path> <url> <sha256>`; `#` starts a comment.
# A file already present with its SHA-256 is left alone. A file whose
# SHA-256 differs after fetching is removed, and the script fails.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
lock=${1:-$root/ucd.lock}
dest=${2:-$root/target/ucd}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

mkdir -p "$dest"

grep -v '^[[:space:]]*#' "$lock" | while read -r path url sum; do
  [ -n "${path:-}" ] || continue
  file=$dest/$path
  if [ -f "$file" ] && [ "$(sha256 "$file")" = "$sum" ]; then
    echo "$path: $sum (present)"
    continue
  fi
  mkdir -p "$(dirname "$file")"
  rm -f "$file"
  curl -fsSL -o "$file.tmp" "$url"
  got=$(sha256 "$file.tmp")
  if [ "$got" != "$sum" ]; then
    rm -f "$file.tmp"
    echo "fetch-ucd.sh: $path: SHA-256 $got, expected $sum" >&2
    exit 1
  fi
  mv "$file.tmp" "$file"
  echo "$path: $sum (fetched)"
done
