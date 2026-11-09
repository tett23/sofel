#!/bin/sh
# Check that every crate depends only on what scripts/deps.txt allows,
# which mirrors the table in ADR 0003 (ADR 0008).
#
# Usage: scripts/check-deps.sh [WORKSPACE [DEPS]]
#   WORKSPACE  default: the repository root
#   DEPS       default: scripts/deps.txt
#
# Each line of DEPS is `<crate> <dependency>`; `<crate> *` allows any
# dependency; `#` starts a comment. All dependency kinds are checked.
# Needs cargo and jq.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
ws=${1:-$root}
deps=${2:-$root/scripts/deps.txt}
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

sed -e 's/#.*//' -e 's/[[:space:]]*$//' "$deps" | grep -v '^$' >"$tmp/allowed" || true

cargo metadata --format-version 1 --no-deps --offline --manifest-path "$ws/Cargo.toml" |
  jq -r '.packages[] | .name as $p | .dependencies[] | "\($p) \(.name)"' |
  sort -u >"$tmp/actual"

status=0
while read -r crate dep; do
  if grep -qxF "$crate $dep" "$tmp/allowed" || grep -qxF "$crate *" "$tmp/allowed"; then
    continue
  fi
  echo "check-deps.sh: $crate depends on $dep, which $deps does not allow" >&2
  status=1
done <"$tmp/actual"
exit $status
