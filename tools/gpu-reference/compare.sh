#!/bin/sh
# Run after scripts/fetch-testdata.sh and the native bench/build-c.sh build.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
data=${SOFEL_TESTDATA:-target/testdata}
src=$data/harfbuzz/src
host=$(rustc -vV | sed -n 's/^host: //p')
hb=${SOFEL_GPU_HARFBUZZ:-target/bench-c/$host/libharfbuzz.a}
out=${CARGO_TARGET_DIR:-target}/gpu-reference
mkdir -p "$out"
cargo build -p sofel-capi
cxx=${CXX:-c++}
"$cxx" -O2 -DNDEBUG -DHB_EXPERIMENTAL_API -std=c++11 -ffp-contract=off \
  -fno-exceptions -fno-rtti -fno-threadsafe-statics -I "$src" \
  -I crates/sofel-capi/include tools/gpu-reference/compare.cc \
  "$src/hb-gpu-paint.cc" "$src/hb-gpu-draw.cc" "$hb" \
  "${CARGO_TARGET_DIR:-target}/debug/libsofel_capi.a" -lpthread -ldl -lm -o "$out/compare"
for font in test_glyphs-glyf_colr_1_variable.ttf Roboto-Regular.abc.ttf \
            cff1_flex.otf cff1_seac.C0.otf cff2-empty.otf cff1_caller_subr.otf; do
  "$out/compare" "$data/harfbuzz/test/api/fonts/$font"
done
