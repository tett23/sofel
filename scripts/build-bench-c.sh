#!/bin/sh
# ADR 0033: two independent output directories, at most two compilers.
# Wait for both builds, even when one fails, before returning failure.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
sh "$root/bench/build-c.sh" &
native=$!
sh "$root/bench/build-c.sh" wasm32-wasip1 &
wasm=$!
status=0
wait "$native" || status=1
wait "$wasm" || status=1
exit "$status"
