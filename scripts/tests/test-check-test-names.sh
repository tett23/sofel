#!/bin/sh
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
mkdir -p "$tmp/docs/adr" "$tmp/crates/x/tests" "$tmp/scripts/tests"
printf '# ADR 0064\n\n## 不変条件\n\n- I1: matches.\n- I2: preserves.\n\n## 受けいれ条件\n\n- A1: works.\n' >"$tmp/docs/adr/0064-example.md"
cp "$tmp/docs/adr/0064-example.md" "$tmp/docs/adr/0061-example.md"
cp "$tmp/docs/adr/0064-example.md" "$tmp/docs/adr/0022-example.md"
check() {
  expected=$1
  label=$2
  result=0
  sh "$root/scripts/check-test-names.sh" "$tmp" >"$tmp/output" 2>&1 || result=$?
  if [ "$expected" -ne "$result" ]; then
    cat "$tmp/output"
    echo "FAIL: $label" >&2
    exit 1
  fi
}
check 0 'ADR 0089 I1, I2: no tests'
for name in adr0064_i1_matches adr0064_a1_works adr0064_i1_i2_matches adr0061_i1_adr0022_i2_matches; do
  printf '#[test]\n#[ignore]\n// intervening comment\n\n#[allow(dead_code)]\n// fifth intervening line\nfn %s() {}\n' "$name" >"$tmp/crates/x/tests/test.rs"
  check 0 'ADR 0089 I1, I2: valid Rust name after attributes'
done
for name in adr0071_inv5_old adr0064_i9_missing adr9999_i1_missing wide_adr0064_i1_prefix adr0064_i01_leading_zero; do
  printf '#[test]\nfn %s() {}\n' "$name" >"$tmp/crates/x/tests/test.rs"
  check 1 'ADR 0089 I1, I2: invalid Rust name'
  grep -F "$name" "$tmp/output" >/dev/null
done
printf '#[test]\nfn ordinary_unit_test() {}\n' >"$tmp/crates/x/tests/test.rs"
for name in 'ADR 0064 I1, I2: valid' 'ADR 0064 A1: valid' 'ADR 0061 I1, ADR 0022 I2: valid'; do
  printf 'check 0 "%s"\n' "$name" >"$tmp/scripts/tests/test.sh"
  check 0 'ADR 0089 I1, I2: valid shell name'
done
for name in 'ADR 0064 I9: missing' 'ADR 0064 I1,I2: separator' 'ADR 9999 I1: missing'; do
  printf 'check 0 "%s"\n' "$name" >"$tmp/scripts/tests/test.sh"
  check 1 'ADR 0089 I1, I2: invalid shell name'
done
: >"$tmp/scripts/tests/test.sh"
printf '#[test]\nfn adr0022_legacy_name() {}\n' >"$tmp/crates/x/tests/test.rs"
printf 'crates/x/tests/test.rs\tadr0022_legacy_name\tADR has no numbered conditions\n' >"$tmp/scripts/test-name-exceptions.tsv"
check 0 'ADR 0099 I2: exact registered exception'
printf '#[test]\nfn adr0022_legacy_name() {}\n' >"$tmp/crates/x/tests/other.rs"
check 1 'ADR 0099 I1, I2: same name in different file'
grep -F 'crates/x/tests/other.rs' "$tmp/output" >/dev/null
grep -F 'adr0022_legacy_name' "$tmp/output" >/dev/null
rm "$tmp/crates/x/tests/other.rs"
printf 'crates/x/tests/test.rs\tadr0022_legacy_name\tduplicate\n' >>"$tmp/scripts/test-name-exceptions.tsv"
check 1 'ADR 0099 I3: duplicate registration'
grep -F 'scripts/test-name-exceptions.tsv' "$tmp/output" >/dev/null
grep -F 'adr0022_legacy_name' "$tmp/output" >/dev/null
printf 'crates/x/tests/test.rs\tadr0022_legacy_name\t\n' >"$tmp/scripts/test-name-exceptions.tsv"
check 1 'ADR 0099 I3: empty reason'
grep -F 'scripts/test-name-exceptions.tsv' "$tmp/output" >/dev/null
grep -F 'adr0022_legacy_name' "$tmp/output" >/dev/null
printf 'crates/x/tests/missing.rs\tadr0022_legacy_name\tremoved\n' >"$tmp/scripts/test-name-exceptions.tsv"
check 1 'ADR 0099 I3: removed test'
grep -F 'scripts/test-name-exceptions.tsv' "$tmp/output" >/dev/null
grep -F 'adr0022_legacy_name' "$tmp/output" >/dev/null
printf 'crates/x/tests/test.rs\tadr0022_legacy_name\tkept name\n' >"$tmp/scripts/test-name-exceptions.tsv"
printf '#[test]\nfn adr0022_renamed_test() {}\n' >"$tmp/crates/x/tests/test.rs"
check 1 'ADR 0099 I1, I2, I3: another name in the same file is not exempt'
grep -F 'adr0022_renamed_test' "$tmp/output" >/dev/null
grep -F 'missing test: crates/x/tests/test.rs adr0022_legacy_name' "$tmp/output" >/dev/null
: >"$tmp/scripts/test-name-exceptions.tsv"
printf '#[test]\nfn adr0064_i1_matches() {}\n' >"$tmp/crates/x/tests/test.rs"
check 0 'ADR 0099 I1: empty exception list'
printf 'check "ADR 0022: legacy shell name"\n' >"$tmp/scripts/tests/test.sh"
printf 'scripts/tests/test.sh\tADR 0022: legacy shell name\tADR has no numbered conditions\n' >"$tmp/scripts/test-name-exceptions.tsv"
check 0 'ADR 0099 I2: exact shell registration'
printf 'check "ADR 0022: another shell name"\n' >>"$tmp/scripts/tests/test.sh"
check 1 'ADR 0099 I1, I2: an unregistered shell name is checked'
grep -F 'ADR 0022: another shell name' "$tmp/output" >/dev/null
echo 'ok: test name checks'
