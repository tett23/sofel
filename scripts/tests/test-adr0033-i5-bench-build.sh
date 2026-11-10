#!/bin/sh
# ADR 0033 I5: wait for both builds and report either failure.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel-bench-build.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/scripts" "$tmp/bench"
cp "$root/scripts/build-bench-c.sh" "$tmp/scripts/"
cat >"$tmp/bench/build-c.sh" <<'EOF'
#!/bin/sh
set -eu
case "${1:-native}" in
  native) sleep 0.05; echo native >>"$BUILD_DONE"; exit "$NATIVE_STATUS" ;;
  wasm32-wasip1) sleep 0.1; echo wasm >>"$BUILD_DONE"; exit "$WASM_STATUS" ;;
  *) exit 2 ;;
esac
EOF
export BUILD_DONE=$tmp/done
for native in 0 1; do
  for wasm in 0 1; do
    : >"$BUILD_DONE"
    status=0
    NATIVE_STATUS=$native WASM_STATUS=$wasm sh "$tmp/scripts/build-bench-c.sh" || status=$?
    want=1
    if [ "$native" = 0 ] && [ "$wasm" = 0 ]; then want=0; fi
    [ "$status" = "$want" ] || { echo 'FAIL: I5 lost a build failure' >&2; exit 1; }
    printf 'native\nwasm\n' >"$tmp/expected"
    sort "$BUILD_DONE" >"$tmp/sorted"
    cmp "$tmp/expected" "$tmp/sorted" || { echo 'FAIL: I5 did not wait for both builds' >&2; exit 1; }
  done
done
echo 'ok: ADR 0033 I5 benchmark build'
