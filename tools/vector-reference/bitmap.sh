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
"${CXX:-c++}" -O2 -ffp-contract=off -std=c++11 -DHAVE_ZLIB -DHB_EXPERIMENTAL_API \
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

# Compare the rendered sweep paths, including their colors and clipping.
font=$data/harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1.ttf
for glyph in 12 13 14 15 16 17 18 19 20 24 28 30 40; do
    "$work/bitmap" "$font" "$work/reference.svg" "$work/actual.svg" "$glyph" svg-gid
    for name in actual reference; do
        "${RSVG_CONVERT:-rsvg-convert}" -w 640 -h 640 -o "$work/$name.png" "$work/$name.svg"
    done
    cmp "$work/reference.png" "$work/actual.png"
done
