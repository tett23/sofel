#!/bin/sh
# Run every scripts/tests/test-*.sh and fail if any of them fails (ADR 0008).

set -u

dir=$(cd "$(dirname "$0")/tests" && pwd)
status=0
for t in "$dir"/test-*.sh; do
  if ! sh "$t"; then
    echo "failed: $t" >&2
    status=1
  fi
done
exit $status
