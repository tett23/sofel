#!/bin/sh
# ADR 0032 I04/I06 and ADR 0034 I04.
[ -n "${BASH_VERSION:-}" ] || exec bash "$0" "$@"
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
run() { bash "$root/scripts/ci-result.sh"; }
export CHANGES_RESULT=success ADR_RESULT=success DOCS_ONLY=true
export RUST_RESULT=skipped CONFORMANCE_RESULT=skipped WASM_RESULT=skipped
export FUZZ_RESULT=skipped BENCH_RESULT=skipped CAPI_WASM_RESULT=skipped
run
DOCS_ONLY=false bash "$root/scripts/ci-result.sh" >/dev/null 2>&1 && exit 1
export DOCS_ONLY=false
export RUST_RESULT=success CONFORMANCE_RESULT=success WASM_RESULT=success
export FUZZ_RESULT=success BENCH_RESULT=success CAPI_WASM_RESULT=success
run
for name in CHANGES_RESULT ADR_RESULT RUST_RESULT CONFORMANCE_RESULT WASM_RESULT FUZZ_RESULT BENCH_RESULT CAPI_WASM_RESULT; do
  old=${!name}
  printf -v "$name" failed
  export "$name"
  if run >/dev/null 2>&1; then echo "accepted failed $name" >&2; exit 1; fi
  printf -v "$name" "$old"
  export "$name"
done
for value in cancelled skipped ''; do
  RUST_RESULT=$value bash "$root/scripts/ci-result.sh" >/dev/null 2>&1 && exit 1
done
DOCS_ONLY= bash "$root/scripts/ci-result.sh" >/dev/null 2>&1 && exit 1
echo 'ok: ADR 0032 I04/I06 and ADR 0034 I04'
