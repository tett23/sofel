#!/bin/sh
# Compare both outputs for the positioned COLRv0 glyph in basic.tests.
set -eu
root=${SOFEL_HARFBUZZ:-target/testdata/harfbuzz}
expected=$root/test/vector/data/expected/basic/colrv0-large-scale
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cargo build --release -p sofel-capi
c++ -std=c++11 -Icrates/sofel-capi/include tools/vector-reference/colrv0.cc \
    "${CARGO_TARGET_DIR:-target}/release/libsofel_capi.a" -o "$work/pages"
"$work/pages" "$root/test/api/fonts/TwemojiMozilla.subset.default.32,3299.ttf" \
    "$work/page.svg" "$work/page.pdf"
# Only the unused namespace and case of hexadecimal colors are insignificant.
normalize_svg() {
    perl -pe 's{ xmlns:xlink="http://www.w3.org/1999/xlink"}{}; s/(fill="#[A-Fa-f0-9]+")/lc($1)/ge' "$1"
}
normalize_svg "$expected.svg" > "$work/expected.svg"
normalize_svg "$work/page.svg" > "$work/actual.svg"
cmp "$work/expected.svg" "$work/actual.svg"
pdftoppm -scale-to 640 -singlefile "$expected.pdf" "$work/expected"
pdftoppm -scale-to 640 -singlefile "$work/page.pdf" "$work/actual"
cmp "$work/expected.ppm" "$work/actual.ppm"
