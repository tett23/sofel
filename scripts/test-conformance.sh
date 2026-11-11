#!/bin/sh
# Build all conformance test artifacts, then run at most JOBS at a time
# (ADR 0033). Requires cargo, jq and xargs (as on both CI runners).
# Usage: sh scripts/test-conformance.sh [JOBS]; default: 2.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
jobs=${1-2}
case "$jobs" in
  ''|*[!0-9]*) echo 'test-conformance.sh: JOBS must be a positive integer' >&2; exit 2 ;;
esac
case "$jobs" in
  *[1-9]*) ;;
  *) echo 'test-conformance.sh: JOBS must be a positive integer' >&2; exit 2 ;;
esac
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel-conformance-run.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

# Cargo's artifact messages identify exactly this build's executables;
# globbing target/ would also pick up stale binaries from earlier builds.
cargo test --release -p sofel-conformance --no-run --message-format=json >"$tmp/build.json"
jq -j 'select(.reason == "compiler-artifact" and .profile.test and .executable != null)
  | .executable, "\u0000"' "$tmp/build.json" >"$tmp/tests"
if [ ! -s "$tmp/tests" ]; then
  echo 'test-conformance.sh: Cargo emitted no test executables' >&2
  exit 1
fi
# Each executable gets one libtest thread so the outer process limit
# does not multiply libtest's default parallelism. Tests that explicitly
# spawn threads keep that behaviour, as in cargo test.
# Cargo runs test binaries from their package directory.
cd "$root/crates/sofel-conformance"
xargs -0 -n 1 -P "$jobs" sh -c '"$1" --nocapture --test-threads=1' sh <"$tmp/tests"
# --no-run does not emit doctest executables; keep that check as well.
cargo test --release -p sofel-conformance --doc
