#!/bin/sh
# Uses the native archive produced by bench/build-c.sh.
set -eu
data=${SOFEL_TESTDATA:-target/testdata}
src=$data/harfbuzz/src
host=$(rustc -vV | sed -n 's/^host: //p')
hb=${SOFEL_GPU_HARFBUZZ:-target/bench-c/$host/libharfbuzz.a}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cargo build -p sofel-capi
"${CXX:-c++}" -O2 -std=c++11 -DHAVE_ZLIB -DHB_EXPERIMENTAL_API \
    -I"$src" -Icrates/sofel-capi/include tools/vector-reference/bitmap.cc \
    "$src"/hb-vector*.cc "$hb" "${CARGO_TARGET_DIR:-target}/debug/libsofel_capi.a" \
    -lz -lpthread -ldl -lm -o "$work/bitmap"
compare() {
    "$work/bitmap" "$data/harfbuzz/test/$1" "$work/reference.pdf" "$work/actual.pdf" "$2"
    for name in actual reference; do
        pdftoppm -singlefile -scale-to 640 "$work/$name.pdf" "$work/$name"
    done
    cmp "$work/reference.ppm" "$work/actual.ppm"
}
compare fuzzing/fonts/noto_handwriting-sbix.ttf 270d
compare api/fonts/NotoColorEmoji.subset.default.2049.ttf 2049
