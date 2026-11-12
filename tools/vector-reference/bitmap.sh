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
compare_pdf() {
    for name in actual reference; do
        LC_ALL=C pdfinfo -box "$work/$name.pdf" > "$work/$name.info"
        grep "^MediaBox:" "$work/$name.info" > "$work/$name.box"
        pdftoppm -singlefile -scale-to 640 "$work/$name.pdf" "$work/$name"
    done
    cmp "$work/reference.box" "$work/actual.box"
    cmp "$work/reference.ppm" "$work/actual.ppm"
}
compare() {
    "$work/bitmap" "$data/harfbuzz/test/$1" "$work/reference.pdf" "$work/actual.pdf" "$2"
    compare_pdf
}
compare fuzzing/fonts/noto_handwriting-sbix.ttf 270d
compare api/fonts/NotoColorEmoji.subset.default.2049.ttf 2049
compare fuzzing/fonts/noto_handwriting-glyf_colr_1.ttf 270d
compare fuzzing/fonts/noto_handwriting-cff_colr_1.otf 270d
compare fuzzing/fonts/noto_handwriting-cff2_colr_1.otf 270d

# Compare the rendered sweep paths, including their colors and clipping.
font=$data/harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1.ttf
"$work/bitmap" "$font" "$work/reference.pdf" "$work/actual.pdf" 0 empty
compare_pdf
for glyph in 12 13 14 15 16 17 18 19 20 24 28 30 40; do
    "$work/bitmap" "$font" "$work/reference.svg" "$work/actual.svg" "$glyph" svg-gid
    for name in actual reference; do
        "${RSVG_CONVERT:-rsvg-convert}" -w 640 -h 640 -o "$work/$name.png" "$work/$name.svg"
    done
    cmp "$work/reference.png" "$work/actual.png"
done
# Sweep patches, blend groups, and shadings with varying alpha.
for glyph in 12 13 14 20 40 125 131 132 149 151 153; do
    "$work/bitmap" "$font" "$work/reference.pdf" "$work/actual.pdf" "$glyph" pdf-gid
    compare_pdf
done
