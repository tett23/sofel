#!/bin/sh
# ADR 0033 invariants 1-3: every Cargo test artifact runs once; failures
# propagate; at most JOBS processes run. No font data or Rust build needed.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel-conformance.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/artifacts with spaces"
export TEST_STATE=$tmp
cat >"$tmp/bin/cargo" <<'EOF'
#!/bin/sh
set -eu
case " $* " in
  *' --no-run '*)
    [ "${BUILD_FAIL:-0}" = 0 ] || exit 1
    [ "${BUILD_EMPTY:-0}" = 0 ] || exit 0
    if [ "${BUILD_BAD_JSON:-0}" = 1 ]; then echo '{bad json'; exit 0; fi
    for n in ${TEST_NAMES:-one two three}; do
      jq -nc --arg exe "$TEST_STATE/artifacts with spaces/$n" \
        '{reason:"compiler-artifact",profile:{test:true},executable:$exe}'
    done
    printf '%s\n' '{"reason":"compiler-artifact","profile":{"test":false},"executable":"/not/a/test"}'
    ;;
  *' --doc '*)
    echo doc >>"$TEST_STATE/done"
    [ "${DOC_FAIL:-0}" = 0 ] || exit 1
    ;;
  *) exit 2 ;;
esac
EOF
cat >"$tmp/test" <<'EOF'
#!/bin/sh
set -eu
[ "$*" = '--nocapture --test-threads=1' ] || exit 2
lock() { while ! mkdir "$TEST_STATE/lock" 2>/dev/null; do sleep 0.01; done; }
lock
n=$(cat "$TEST_STATE/active")
n=$((n + 1))
echo "$n" >"$TEST_STATE/active"
echo "$n" >>"$TEST_STATE/counts"
rmdir "$TEST_STATE/lock"
sleep 0.1
lock
n=$(cat "$TEST_STATE/active")
echo "$((n - 1))" >"$TEST_STATE/active"
basename "$0" >>"$TEST_STATE/done"
rmdir "$TEST_STATE/lock"
[ "${TEST_FAIL:-}" != "$(basename "$0")" ] || exit 1
EOF
chmod +x "$tmp/bin/cargo" "$tmp/test"
for n in one two three; do cp "$tmp/test" "$tmp/artifacts with spaces/$n"; done
export PATH="$tmp/bin:$PATH"
reset_state() {
  echo 0 >"$tmp/active"
  : >"$tmp/counts"
  : >"$tmp/done"
}
fail() { echo "FAIL: $*" >&2; exit 1; }
reset_state
sh "$root/scripts/test-conformance.sh" 2 || fail 'runner failed'
printf 'doc\none\nthree\ntwo\n' >"$tmp/expected"
sort "$tmp/done" >"$tmp/sorted"
cmp "$tmp/expected" "$tmp/sorted" || fail 'I1: artifacts or doc tests omitted/duplicated'
[ "$(sort -nr "$tmp/counts" | head -1)" = 2 ] || fail 'I3: not bounded at two'
reset_state
sh "$root/scripts/test-conformance.sh" 1 || fail 'serial runner failed'
[ "$(sort -nr "$tmp/counts" | head -1)" = 1 ] || fail 'I3: serial limit'
reset_state
TEST_NAMES=one sh "$root/scripts/test-conformance.sh" 2 || fail 'I1: single artifact'
printf 'one\ndoc\n' >"$tmp/expected"
cmp "$tmp/expected" "$tmp/done" || fail 'I1: single artifact omitted/duplicated'
for invalid in 0 00 -1 x 1.5 ''; do
  if sh "$root/scripts/test-conformance.sh" "$invalid" >/dev/null 2>&1; then
    fail 'I3: invalid job count accepted'
  fi
done
reset_state
if BUILD_FAIL=1 sh "$root/scripts/test-conformance.sh" 2; then fail 'I2: build failure lost'; fi
[ ! -s "$tmp/done" ] || fail 'I2: ran tests after failed build'
reset_state
if BUILD_EMPTY=1 sh "$root/scripts/test-conformance.sh" 2; then fail 'I2: empty artifact list accepted'; fi
[ ! -s "$tmp/done" ] || fail 'I2: ran tests with empty artifact list'
if BUILD_BAD_JSON=1 sh "$root/scripts/test-conformance.sh" 2; then fail 'I2: malformed JSON accepted'; fi
reset_state
if TEST_FAIL=two sh "$root/scripts/test-conformance.sh" 2; then fail 'I2: test failure lost'; fi
reset_state
if DOC_FAIL=1 sh "$root/scripts/test-conformance.sh" 2; then fail 'I2: doc failure lost'; fi
echo 'ok: ADR 0033 I1-I3 conformance runner'
