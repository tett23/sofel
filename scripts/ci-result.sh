#!/bin/bash
# ADR 0032 I06: one stable required check for both CI paths.
set -euo pipefail
if [[ ${CHANGES_RESULT:-} != success || ${ADR_RESULT:-} != success ]]; then
  echo 'ci-result: changes and adr must succeed' >&2; exit 1
fi
case ${DOCS_ONLY:-} in
  true) want=skipped ;;
  false) want=success ;;
  *) echo 'ci-result: docs_only is missing or invalid' >&2; exit 1 ;;
esac
for job in RUST GENERATORS CONFORMANCE WASM FUZZ BENCH CAPI_WASM; do
  key=${job}_RESULT
  if [[ ${!key:-} != "$want" ]]; then
    echo "ci-result: $job is ${!key:-missing}, expected $want" >&2
    exit 1
  fi
done
echo "ci-result: all required jobs have result $want"
