#!/bin/sh
# ADR 0038 I1-I3: balanced, ordered units and fail-closed syntax.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel-hb-units.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
for n in 1 2 4 5 74; do
  src=$tmp/source-$n.cc
  out=$tmp/units-$n
  mkdir "$out"
  i=0
  while [ "$i" -lt "$n" ]; do
    printf '#include "sub/source-%03d.cc"\n' "$i"
    i=$((i + 1))
  done >"$src"
  awk -v out="$out" -f "$root/bench/split-harfbuzz.awk" "$src"
  set -- "$out"/part-*.cc
  expected=$n
  if [ "$expected" -gt 4 ]; then expected=4; fi
  [ "$#" -eq "$expected" ] || { echo 'FAIL: I1 unit count' >&2; exit 1; }
  min=$n max=0
  for unit do
    lines=$(wc -l <"$unit")
    [ "$lines" -gt 0 ] || { echo 'FAIL: I1 empty unit' >&2; exit 1; }
    if [ "$lines" -lt "$min" ]; then min=$lines; fi
    if [ "$lines" -gt "$max" ]; then max=$lines; fi
  done
  [ $((max - min)) -le 1 ] || { echo 'FAIL: I1 unbalanced units' >&2; exit 1; }
  cat "$@" >"$tmp/joined"
  cmp "$src" "$tmp/joined" || { echo 'FAIL: I2 changed include sequence' >&2; exit 1; }
done
mkdir "$tmp/invalid"
for invalid in empty macro absolute; do
  case "$invalid" in
    empty) : >"$tmp/bad" ;;
    macro) printf '#include "good.cc"\n#define FEATURE\n' >"$tmp/bad" ;;
    absolute) printf '#include "/bad.cc"\n' >"$tmp/bad" ;;
  esac
  if awk -v out="$tmp/invalid" -f "$root/bench/split-harfbuzz.awk" "$tmp/bad"; then
    echo 'FAIL: I3 accepted unsupported input' >&2; exit 1
  fi
  set -- "$tmp/invalid"/part-*.cc
  [ ! -f "$1" ] || { echo 'FAIL: I3 left a partial translation unit' >&2; exit 1; }
done
echo 'ok: ADR 0038 I1-I3 HarfBuzz translation units'
