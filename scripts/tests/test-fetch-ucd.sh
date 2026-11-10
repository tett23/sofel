#!/bin/sh
# Tests for scripts/fetch-ucd.sh (ADR 0019).

set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
script=$root/scripts/fetch-ucd.sh
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

mkdir -p "$tmp/origin/sub"
printf 'one\n' >"$tmp/origin/a.txt"
printf 'two\n' >"$tmp/origin/sub/b.txt"
good_a=$(sha256 "$tmp/origin/a.txt")
good_b=$(sha256 "$tmp/origin/sub/b.txt")
bad=0000000000000000000000000000000000000000000000000000000000000000

printf '# comment\n\na.txt file://%s/a.txt %s\nsub/b.txt file://%s/sub/b.txt %s\n' \
  "$tmp/origin" "$good_a" "$tmp/origin" "$good_b" >"$tmp/lock"
printf 'a.txt file://%s/a.txt %s\n' "$tmp/origin" "$bad" >"$tmp/bad.lock"

if sh "$script" "$tmp/bad.lock" "$tmp/bad" >/dev/null 2>&1; then
  fail "a wrong SHA-256 was accepted"
fi
[ ! -e "$tmp/bad/a.txt" ] || fail "a file with a wrong SHA-256 was kept"

sh "$script" "$tmp/lock" "$tmp/ucd" >/dev/null || fail "the first fetch failed"
cmp -s "$tmp/origin/a.txt" "$tmp/ucd/a.txt" || fail "a.txt differs"
cmp -s "$tmp/origin/sub/b.txt" "$tmp/ucd/sub/b.txt" || fail "sub/b.txt differs"

rm -rf "$tmp/origin"
sh "$script" "$tmp/lock" "$tmp/ucd" >/dev/null ||
  fail "the second run fetched again"

printf 'changed\n' >"$tmp/ucd/a.txt"
if sh "$script" "$tmp/lock" "$tmp/ucd" >/dev/null 2>&1; then
  fail "a changed file was accepted"
fi

echo "ok: fetch-ucd.sh"
