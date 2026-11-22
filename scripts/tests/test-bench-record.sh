#!/bin/sh
# Tests for scripts/bench-record.sh (ADR 0080 I1, I2, I3, I4, I5, I6), with fake measurements.

set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
script=$root/scripts/bench-record.sh
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# A fake measurement: records its arguments, prints a table, exits with $2.
fake() {
  name=$1
  code=$2
  table=$3
  cat >"$tmp/$name" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >"$tmp/$name.args"
printf '%s' '$table'
exit $code
EOF
  chmod +x "$tmp/$name"
}

# A fake uname answering -s and -m.
fake_uname() {
  mkdir -p "$tmp/uname-$1-$2"
  cat >"$tmp/uname-$1-$2/uname" <<EOF
#!/bin/sh
case "\$1" in
  -s) echo $1 ;;
  -m) echo $2 ;;
  *) echo $1 ;;
esac
EOF
  chmod +x "$tmp/uname-$1-$2/uname"
}

native_table='| 項目 | フォント |
|---|---|
| cmap | A |
'
wasm_table='| 項目 | フォント |
|---|---|
| cmap | wasm |
'
today=$(date -u +%Y-%m-%d)

run() {
  out=$1
  shift
  mkdir -p "$out"
  (cd "$root" && sh "$script" --out "$out" "$@")
}

# ADR 0080 I1, I2: one record with the three sections in order.
fake native 0 "$native_table"
fake wasm 0 "$wasm_table"
fake_uname Linux x86_64
SOFEL_BENCH_NATIVE=$tmp/native SOFEL_BENCH_WASM=$tmp/wasm PATH="$tmp/uname-Linux-x86_64:$PATH" \
  run "$tmp/o1" --subject stage2 >/dev/null || fail "ADR 0080 I1: a successful run failed"
[ "$(ls "$tmp/o1" | wc -l | tr -d ' ')" = 1 ] || fail "ADR 0080 I1: not exactly one file: $(ls "$tmp/o1")"
f=$tmp/o1/$today-stage2-ci-linux-x64.md
[ -f "$f" ] || fail "ADR 0080 I1: no $f (got $(ls "$tmp/o1"))"
order=$(grep -n '^## ' "$f" | sed 's/^[0-9]*:## //' | tr '\n' ',')
[ "$order" = "環境,ネイティブ,wasm," ] || fail "ADR 0080 I2: sections are $order"
awk '/^## ネイティブ$/{f=1;next} /^## /{f=0} f' "$f" | grep -qF '| cmap | A |' || fail "ADR 0080 I2: the native table is missing"
awk '/^## wasm$/{f=1;next} /^## /{f=0} f' "$f" | grep -qF '| cmap | wasm |' || fail "ADR 0080 I2: the wasm table is missing"
echo 'ADR 0080 I1, I2: one record with the environment, native and wasm sections'

# ADR 0080 I1: a one-letter subject, and a macOS arm64 runner.
fake_uname Darwin arm64
SOFEL_BENCH_NATIVE=$tmp/native SOFEL_BENCH_WASM=$tmp/wasm PATH="$tmp/uname-Darwin-arm64:$PATH" \
  run "$tmp/o2" --subject a >/dev/null || fail "ADR 0080 I1: subject a failed"
[ -f "$tmp/o2/$today-a-ci-macos-arm64.md" ] || fail "ADR 0080 I1: got $(ls "$tmp/o2")"
fake_uname Linux aarch64
SOFEL_BENCH_NATIVE=$tmp/native SOFEL_BENCH_WASM=$tmp/wasm PATH="$tmp/uname-Linux-aarch64:$PATH" \
  run "$tmp/o2b" --subject a >/dev/null || fail "ADR 0080 I1: aarch64 failed"
[ -f "$tmp/o2b/$today-a-ci-linux-arm64.md" ] || fail "ADR 0080 I1: got $(ls "$tmp/o2b")"
echo 'ADR 0080 I1: runner names and a one-letter subject'

# ADR 0080 I2: empty measurement output still gives the sections.
fake native 0 ''
fake wasm 0 ''
SOFEL_BENCH_NATIVE=$tmp/native SOFEL_BENCH_WASM=$tmp/wasm \
  run "$tmp/o3" --subject empty >/dev/null || fail "ADR 0080 I2: empty output failed"
f=$(ls "$tmp/o3"/*.md)
order=$(grep -n '^## ' "$f" | sed 's/^[0-9]*:## //' | tr '\n' ',')
[ "$order" = "環境,ネイティブ,wasm," ] || fail "ADR 0080 I2: sections with empty output are $order"
echo 'ADR 0080 I2: empty output keeps the sections'

# ADR 0080 I3: every environment row, and 未取得 without lscpu and sysctl.
rows='ランナーのイメージと版
OS の名前と版
CPU の名前
論理 CPU の数
メモリ
計測の直前の負荷平均
Rust の版
C と C++ のコンパイラの版
wasmtime の版
wasi-sdk の版
FreeType と HarfBuzz の版
sofel のコミット
実行の URL'
fake native 0 "$native_table"
fake wasm 0 "$wasm_table"
mkdir -p "$tmp/bin"
for t in sh date sed awk cat mkdir mv rm mktemp grep head tr cut ls; do
  p=$(command -v "$t") && ln -sf "$p" "$tmp/bin/$t"
done
cp "$tmp/uname-Linux-x86_64/uname" "$tmp/bin/uname"
SOFEL_BENCH_NATIVE=$tmp/native SOFEL_BENCH_WASM=$tmp/wasm PATH="$tmp/bin" \
  run "$tmp/o4" --subject bare >/dev/null || fail "ADR 0080 I3: a run with a bare PATH failed"
f=$(ls "$tmp/o4"/*.md)
printf '%s\n' "$rows" | while IFS= read -r row; do
  grep -q "^| $row | " "$f" || fail "ADR 0080 I3: no row $row"
done
grep -q '^| CPU の名前 | 未取得 |$' "$f" || fail "ADR 0080 I3: CPU without lscpu and sysctl: $(grep 'CPU の名前' "$f")"
echo 'ADR 0080 I3: every environment row, 未取得 when a value cannot be read'

# ADR 0080 I4: a failing measurement writes nothing and keeps its status.
fake native 1 "$native_table"
fake wasm 0 "$wasm_table"
set +e
SOFEL_BENCH_NATIVE=$tmp/native SOFEL_BENCH_WASM=$tmp/wasm run "$tmp/o5" --subject x >/dev/null 2>&1
code=$?
set -e
[ "$code" = 1 ] || fail "ADR 0080 I4: native failure exited $code"
[ -z "$(ls "$tmp/o5")" ] || fail "ADR 0080 I4: a record was written after a native failure"
fake native 0 "$native_table"
fake wasm 3 "$wasm_table"
set +e
SOFEL_BENCH_NATIVE=$tmp/native SOFEL_BENCH_WASM=$tmp/wasm run "$tmp/o6" --subject x >/dev/null 2>&1
code=$?
set -e
[ "$code" = 3 ] || fail "ADR 0080 I4: wasm failure exited $code"
[ -z "$(ls "$tmp/o6")" ] || fail "ADR 0080 I4: a record was written after a wasm failure"
echo 'ADR 0080 I4: failures keep their status and write nothing'

# ADR 0080 I5: bad or missing subjects stop before measuring.
fake wasm 0 "$wasm_table"
for s in Stage2 stage2- -x '' 'a_b'; do
  rm -f "$tmp/native.args"
  set +e
  SOFEL_BENCH_NATIVE=$tmp/native SOFEL_BENCH_WASM=$tmp/wasm run "$tmp/o7" --subject "$s" >/dev/null 2>&1
  code=$?
  set -e
  [ "$code" = 2 ] || fail "ADR 0080 I5: subject '$s' exited $code"
  [ ! -f "$tmp/native.args" ] || fail "ADR 0080 I5: subject '$s' ran a measurement"
done
set +e
SOFEL_BENCH_NATIVE=$tmp/native SOFEL_BENCH_WASM=$tmp/wasm run "$tmp/o7" >/dev/null 2>&1
code=$?
set -e
[ "$code" = 2 ] || fail "ADR 0080 I5: no subject exited $code"
[ ! -f "$tmp/native.args" ] || fail "ADR 0080 I5: no subject ran a measurement"
[ -z "$(ls "$tmp/o7")" ] || fail "ADR 0080 I5: a record was written"
echo 'ADR 0080 I5: bad subjects exit 2 before measuring'

# ADR 0080 I6: --filter reaches both measurements, and only when given.
SOFEL_BENCH_NATIVE=$tmp/native SOFEL_BENCH_WASM=$tmp/wasm \
  run "$tmp/o8" --subject f --filter raster >/dev/null || fail "ADR 0080 I6: a filtered run failed"
[ "$(cat "$tmp/native.args")" = "--filter raster" ] || fail "ADR 0080 I6: native got '$(cat "$tmp/native.args")'"
[ "$(cat "$tmp/wasm.args")" = "--filter raster" ] || fail "ADR 0080 I6: wasm got '$(cat "$tmp/wasm.args")'"
SOFEL_BENCH_NATIVE=$tmp/native SOFEL_BENCH_WASM=$tmp/wasm \
  run "$tmp/o9" --subject f >/dev/null || fail "ADR 0080 I6: an unfiltered run failed"
[ "$(cat "$tmp/native.args")" = "" ] || fail "ADR 0080 I6: native got '$(cat "$tmp/native.args")' without --filter"
[ "$(cat "$tmp/wasm.args")" = "" ] || fail "ADR 0080 I6: wasm got '$(cat "$tmp/wasm.args")' without --filter"
echo 'ADR 0080 I6: --filter is passed to both measurements only when given'
