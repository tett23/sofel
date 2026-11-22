#!/bin/sh
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
git init -q "$tmp"
check() {
  expected=$1
  name=$2
  status=0
  sh "$root/scripts/check-fixtures.sh" "$tmp" >"$tmp/output" 2>&1 || status=$?
  if [ "$status" -ne "$expected" ]; then
    cat "$tmp/output"
    echo "FAIL: $name" >&2
    exit 1
  fi
}
check 0 'ADR 0088 I1, I2: empty repository'
mkdir -p "$tmp/crates/x/tests/data"
printf x >"$tmp/crates/x/tests/data/a.ttf"
git -C "$tmp" add .
check 1 'ADR 0088 I2: missing readme'
grep -F 'crates/x/tests/data/a.ttf' "$tmp/output"
printf '## b.ttf\n' >"$tmp/crates/x/tests/data/README.md"
check 1 'ADR 0088 I2: wrong heading'
printf '## a.ttf\n' >"$tmp/crates/x/tests/data/README.md"
check 0 'ADR 0088 I1, I2: documented fixture'
printf x >"$tmp/crates/x/tests/data/A.OTF"
git -C "$tmp" add crates
check 1 'ADR 0088 I2: uppercase extension needs heading'
printf '\n## A.OTF\n' >>"$tmp/crates/x/tests/data/README.md"
check 0 'ADR 0088 I1, I2: uppercase extension'
mkdir "$tmp/fonts"
printf x >"$tmp/fonts/a.ttf"
git -C "$tmp" add fonts
check 1 'ADR 0088 I1: forbidden location'
grep -F 'fonts/a.ttf' "$tmp/output"
echo 'ok: fixture checks'
