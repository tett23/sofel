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

c++ -std=c++11 -Icrates/sofel-capi/include tools/vector-reference/positioned.cc \
    "${CARGO_TARGET_DIR:-target}/release/libsofel_capi.a" -o "$work/positioned"
root=${SOFEL_HARFBUZZ:-target/testdata/harfbuzz}/test
suite=basic
check_page() {
    name=$1
    font=$2
    shift 2
    "$work/positioned" "$root/api/fonts/$font" "$@" > "$work/$name.svg"
    cmp "$work/$name.svg" "$root/vector/data/expected/$suite/$name.svg"
}
check_page upem-default Roboto-Regular.abc.ttf -16 -1916 3367 2432
check_page multiline Roboto-Regular.abc.ttf -16 -1916 2253 4832 line-height=2400 'text=a
bc'
check_page cff1 SourceSansPro-Regular.abc.otf -16 -1000 1545 1289
check_page cff2-var-wght900 TestCFF2VF.otf -16 -1000 1832 1289 wght=900
check_page cff2-var-cntr AdobeVFPrototype.abc.otf -16 -934 1728 1285 wght=900 CNTR=100
check_page glyf-var-wght900 Roboto-Variable.abc.ttf -16 -1916 3338 2432 wght=900
check_page ttb Roboto-Regular.abc.ttf -1040 -16 2080 7232 ttb
check_page glyf-vvar-ttb-wght900 SourceSerifVariable-Roman-VVAR.abc.ttf -516 -16 1032 3095 ttb wght=900
nastaliq=$root/vector/data/fonts/2db082c6c462d5056544e56e7ea13bf58c33a832.ttf
"$work/positioned" "$nastaliq" -16 -2216 2442 3632 rtl text=نستعلیق > "$work/nastaliq.svg"
cmp "$work/nastaliq.svg" "$root/vector/data/expected/basic/iran-nastaliq-y-offsets.svg"
suite=varc
check_page default varc-ac00-ac01.ttf -16 -819 1872 1035 text=가각
check_page wght840p3 varc-ac00-ac01.ttf -16 -839 1872 1055 text=가각 wght=840.3
check_page conditional-default varc-ac01-conditional.ttf -16 -816 952 1032 text=각
check_page conditional-wght840p3 varc-ac01-conditional.ttf -16 -839 952 1055 text=각 wght=840.3
"$work/positioned" "$root/fuzzing/fonts/noto_handwriting-sbix.ttf" \
    -16 -966 1307 1235 text=✍ > "$work/handwriting-sbix.svg"
cmp "$work/handwriting-sbix.svg" "$root/vector/data/expected/basic/handwriting-sbix.svg"
for outline in glyf cff cff2; do
    case $outline in
        glyf) ext=ttf; expected=glyf ;;
        *) ext=otf; expected=cff ;;
    esac
    "$work/positioned" "$root/fuzzing/fonts/noto_handwriting-${outline}_colr_1.$ext" \
        -16 -966 1307 1232 text=✍ > "$work/handwriting.svg"
    cmp "$work/handwriting.svg" "$root/vector/data/expected/basic/handwriting-$expected-colr-1.svg"
done
