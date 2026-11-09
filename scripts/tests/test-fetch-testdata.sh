#!/bin/sh
# Tests for scripts/fetch-testdata.sh (ADR 0008).

set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
script=$root/scripts/fetch-testdata.sh
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

commit() {
  git -C "$tmp/origin" -c user.name=test -c user.email=test@example.com \
    commit -q --allow-empty -m "$1"
  git -C "$tmp/origin" rev-parse HEAD
}

git init -q "$tmp/origin"
git -C "$tmp/origin" config uploadpack.allowReachableSHA1InWant true
old=$(commit one)
commit two >/dev/null
printf '# comment\n\ndemo file://%s %s\n' "$tmp/origin" "$old" >"$tmp/lock"

sh "$script" "$tmp/lock" "$tmp/data" >/dev/null || fail "the first fetch failed"
[ "$(git -C "$tmp/data/demo" rev-parse HEAD)" = "$old" ] ||
  fail "HEAD is not the pinned commit"

rm -rf "$tmp/origin"
sh "$script" "$tmp/lock" "$tmp/data" >/dev/null ||
  fail "the second run fetched again"

SOFEL_TESTDATA_OFFLINE=1 sh "$script" "$tmp/lock" "$tmp/data" >/dev/null ||
  fail "offline failed although the data is present"

rm -rf "$tmp/data/demo"
if SOFEL_TESTDATA_OFFLINE=1 sh "$script" "$tmp/lock" "$tmp/data" >/dev/null 2>&1; then
  fail "offline succeeded although the data is missing"
fi

echo "ok: fetch-testdata.sh"
