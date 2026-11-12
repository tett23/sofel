#!/bin/sh
# Reproduce premultiplied, top-down BGRA reference images (sofel ADR 0072).
# Usage: check.sh TESTDATA REFERENCE-DIR HB-RASTER-REFERENCE
set -eu
data=$1
reference=$2
helper=$3
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
for entry in cff-19 cff-26 variable-12 variable-27 variable-38 variable-92 variable-93; do
  case "$entry" in
    cff-*) font=$data/extra-fonts/fonts/color-cff.otf ;;
    variable-*) font=$data/harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1_variable.ttf ;;
  esac
  "$helper" "$font" "${entry#*-}" 48 > "$tmp/$entry.bgra"
  cmp "$tmp/$entry.bgra" "$reference/$entry.bgra"
done

glyph=120
while [ "$glyph" -le 147 ]; do
  "$helper" "$data/harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1_variable.ttf" "$glyph" 48
  glyph=$((glyph + 1))
done > "$tmp/composites.bgra"
cmp "$tmp/composites.bgra" "$reference/composites.bgra"
if [ "${SOFEL_HB_RASTER_PNG:-0}" = 1 ]; then
  "$helper" "$data/extra-fonts/fonts/color-sbix.ttf" 19 31 > "$tmp/scaled-sbix-19.bgra"
  "$helper" "$data/harfbuzz/test/subset/data/fonts/NotoColorEmoji.subset.ttf" 1 31 > "$tmp/scaled-cbdt-1.bgra"
  cmp "$tmp/scaled-sbix-19.bgra" "$reference/scaled-sbix-19.bgra"
  cmp "$tmp/scaled-cbdt-1.bgra" "$reference/scaled-cbdt-1.bgra"
fi
