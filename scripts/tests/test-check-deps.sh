#!/bin/sh
# Tests for scripts/check-deps.sh (ADR 0008).

set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
script=$root/scripts/check-deps.sh
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

crate() {
  mkdir -p "$tmp/ws/$1/src"
  printf '[package]\nname = "%s"\nversion = "0.0.0"\nedition = "2024"\n\n[dependencies]\n%s\n' \
    "$1" "$2" >"$tmp/ws/$1/Cargo.toml"
  : >"$tmp/ws/$1/src/lib.rs"
}

mkdir -p "$tmp/ws"
printf '[workspace]\nresolver = "3"\nmembers = ["a", "b"]\n' >"$tmp/ws/Cargo.toml"
crate b ''
crate a 'b = { path = "../b" }'

printf '# allowed\na b\n' >"$tmp/allowed.txt"
sh "$script" "$tmp/ws" "$tmp/allowed.txt" 2>/dev/null || fail "an allowed dependency was rejected"

printf '# nothing allowed\n' >"$tmp/none.txt"
if sh "$script" "$tmp/ws" "$tmp/none.txt" 2>/dev/null; then
  fail "a dependency missing from deps.txt was accepted"
fi

printf 'a *\n' >"$tmp/any.txt"
sh "$script" "$tmp/ws" "$tmp/any.txt" 2>/dev/null || fail "'a *' did not allow every dependency"

echo "ok: check-deps.sh"
