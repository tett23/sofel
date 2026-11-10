#!/bin/sh
# Tests for bench/fetch-wasi-sdk.sh (ADR 0013), with a local archive.

set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
script=$root/bench/fetch-wasi-sdk.sh
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

mkdir -p "$tmp/src/fake-sdk/bin"
echo hello >"$tmp/src/fake-sdk/bin/clang"
tar -czf "$tmp/fake-sdk.tar.gz" -C "$tmp/src" fake-sdk
if command -v sha256sum >/dev/null 2>&1; then
  sum=$(sha256sum "$tmp/fake-sdk.tar.gz" | awk '{ print $1 }')
else
  sum=$(shasum -a 256 "$tmp/fake-sdk.tar.gz" | awk '{ print $1 }')
fi
host="$(uname -s)-$(uname -m)"

printf '%s file://%s %s\n' "$host" "$tmp/fake-sdk.tar.gz" 0000 >"$tmp/bad.lock"
if sh "$script" "$tmp/bad.lock" "$tmp/out-bad" >/dev/null 2>&1; then
  fail "a wrong SHA-256 was accepted"
fi
[ ! -d "$tmp/out-bad/fake-sdk" ] || fail "a rejected archive was unpacked"

printf '# comment\n%s file://%s %s\n' "$host" "$tmp/fake-sdk.tar.gz" "$sum" >"$tmp/good.lock"
dir=$(sh "$script" "$tmp/good.lock" "$tmp/out") || fail "the right SHA-256 was rejected"
[ "$(cat "$dir/bin/clang")" = hello ] || fail "the archive was not unpacked into $dir"

printf 'Nowhere-none file://x 0\n' >"$tmp/none.lock"
if sh "$script" "$tmp/none.lock" "$tmp/out-none" >/dev/null 2>&1; then
  fail "a lock without this host was accepted"
fi

echo "ok: fetch-wasi-sdk.sh"
