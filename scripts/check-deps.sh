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
#
# It also checks that every crate forbids `unsafe` (ADR 0003, ADR 0015):
# through the workspace lints (`[lints] workspace = true` with
# `unsafe_code = "forbid"` in the workspace) or `#![forbid(unsafe_code)]`
# at the root of each library and binary. A line `!unsafe <crate>` in
# DEPS exempts a crate.
# Needs cargo and jq.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
ws=${1:-$root}
deps=${2:-$root/scripts/deps.txt}
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

sed -e 's/#.*//' -e 's/[[:space:]]*$//' "$deps" | grep -v '^$' >"$tmp/lines" || true
grep -v '^!' "$tmp/lines" >"$tmp/allowed" || true
sed -n 's/^!unsafe //p' "$tmp/lines" >"$tmp/unsafe"

cargo metadata --format-version 1 --no-deps --offline --manifest-path "$ws/Cargo.toml" >"$tmp/meta"
jq -r '.packages[] | .name as $p | .dependencies[] | "\($p) \(.name)"' "$tmp/meta" |
  sort -u >"$tmp/actual"

status=0
while read -r crate dep; do
  if grep -qxF "$crate $dep" "$tmp/allowed" || grep -qxF "$crate *" "$tmp/allowed"; then
    continue
  fi
  echo "check-deps.sh: $crate depends on $dep, which $deps does not allow" >&2
  status=1
done <"$tmp/actual"

# `[lints]` followed by `workspace = true` in the manifest.
workspace_lints() {
  awk '/^\[/ { in_lints = ($0 == "[lints]") } in_lints && /^workspace[[:space:]]*=[[:space:]]*true/ { found = 1 } END { exit !found }' "$1"
}

ws_forbids=0
if awk '/^\[/ { in_rust = ($0 == "[workspace.lints.rust]") } in_rust && /^unsafe_code[[:space:]]*=[[:space:]]*"forbid"/ { found = 1 } END { exit !found }' "$ws/Cargo.toml"; then
  ws_forbids=1
fi

jq -r '.packages[] | "\(.name) \(.manifest_path)"' "$tmp/meta" >"$tmp/packages"
while read -r crate manifest; do
  grep -qxF "$crate" "$tmp/unsafe" && continue
  if [ $ws_forbids = 1 ] && workspace_lints "$manifest"; then
    continue
  fi
  roots=$(jq -r --arg p "$crate" '.packages[] | select(.name == $p) | .targets[]
    | select(.kind | any(. == "lib" or . == "bin" or . == "rlib" or . == "staticlib" or . == "cdylib"))
    | .src_path' "$tmp/meta")
  ok=1
  for r in $roots; do
    grep -qF '#![forbid(unsafe_code)]' "$r" || ok=0
  done
  if [ $ok = 0 ] || [ -z "$roots" ]; then
    echo "check-deps.sh: $crate does not forbid unsafe code; only crates marked \`!unsafe\` in $deps may use it" >&2
    status=1
  fi
done <"$tmp/packages"
exit $status
