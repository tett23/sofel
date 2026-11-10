#!/bin/sh
# Fetch wasi-sdk for this host as pinned in bench/wasi-sdk.lock, check its
# SHA-256, and unpack it (ADR 0013). Prints the wasi-sdk directory.
#
# Usage: bench/fetch-wasi-sdk.sh [LOCK [DEST]]
#   LOCK  default: bench/wasi-sdk.lock
#   DEST  default: target/wasi-sdk at the repository root
# The archive is unpacked into DEST/<archive name without .tar.gz>;
# an existing directory is reused.

set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
lock=${1:-$here/wasi-sdk.lock}
dest=${2:-$root/target/wasi-sdk}
host="$(uname -s)-$(uname -m)"

line=$(grep -v '^[[:space:]]*#' "$lock" | awk -v h="$host" '$1 == h { print; exit }')
if [ -z "$line" ]; then
  echo "fetch-wasi-sdk.sh: no entry for $host in $lock" >&2
  exit 1
fi
url=$(echo "$line" | awk '{ print $2 }')
sum=$(echo "$line" | awk '{ print $3 }')
archive=$(basename "$url")
dir=$dest/${archive%.tar.gz}

if [ -d "$dir" ]; then
  echo "$dir"
  exit 0
fi

mkdir -p "$dest"
curl -fsSL -o "$dest/$archive.part" "$url"
if command -v sha256sum >/dev/null 2>&1; then
  got=$(sha256sum "$dest/$archive.part" | awk '{ print $1 }')
else
  got=$(shasum -a 256 "$dest/$archive.part" | awk '{ print $1 }')
fi
if [ "$got" != "$sum" ]; then
  rm -f "$dest/$archive.part"
  echo "fetch-wasi-sdk.sh: SHA-256 mismatch for $archive: expected $sum, got $got" >&2
  exit 1
fi
mkdir -p "$dir.part"
tar -xzf "$dest/$archive.part" -C "$dir.part" --strip-components 1
mv "$dir.part" "$dir"
rm -f "$dest/$archive.part"
echo "$dir"
