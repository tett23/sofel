#!/bin/sh
# Fetch the test data listed in testdata.lock, each at its pinned commit
# only (ADR 0005, ADR 0008).
#
# Usage: scripts/fetch-testdata.sh [LOCK [DEST]]
#   LOCK  default: testdata.lock at the repository root
#   DEST  default: target/testdata at the repository root
#
# Each line of LOCK is `<name> <url> <commit>`; `#` starts a comment.
# Data goes to DEST/<name>. A repository already at its commit is left
# alone. With SOFEL_TESTDATA_OFFLINE=1 nothing is fetched, and the script
# fails unless every repository is already at its commit.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
lock=${1:-$root/testdata.lock}
dest=${2:-$root/target/testdata}
offline=${SOFEL_TESTDATA_OFFLINE:-0}

mkdir -p "$dest"

grep -v '^[[:space:]]*#' "$lock" | while read -r name url commit; do
  [ -n "${name:-}" ] || continue
  dir=$dest/$name
  head=$(git -C "$dir" rev-parse -q --verify HEAD 2>/dev/null || true)
  if [ "$head" = "$commit" ]; then
    echo "$name: $commit (present)"
    continue
  fi
  if [ "$offline" = 1 ]; then
    echo "fetch-testdata.sh: $name is not at $commit in $dir (offline)" >&2
    exit 1
  fi
  rm -rf "$dir"
  mkdir -p "$dir"
  git -C "$dir" init -q
  git -C "$dir" fetch -q --depth 1 "$url" "$commit"
  git -C "$dir" -c advice.detachedHead=false checkout -q FETCH_HEAD
  echo "$name: $commit (fetched)"
done
