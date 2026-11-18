#!/bin/sh
# List the COLRv1 reference images that change when HarfBuzz's raster
# uses sofel's atan2f instead of the platform's (sofel ADR 0095 I4): the
# glyphs that gallery_colr_pixels and check.sh compare, one per line as
# <font>-<glyph>@<arguments>. Prints nothing when no image changes.
# Usage: atan2f-changes.sh TESTDATA LIBM-REFERENCE SOFEL-REFERENCE
#   LIBM-REFERENCE   built with SOFEL_HB_RASTER_ATAN2F=libm build.sh
#   SOFEL-REFERENCE  built with build.sh
set -eu
data=$1
libm=$2
sofel=$3
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
variable=$data/harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1_variable.ttf
cff=$data/extra-fonts/fonts/color-cff.otf

# compare NAME FONT ARGUMENTS...
compare() {
  name=$1
  font=$2
  shift 2
  "$libm" "$font" "$@" >"$tmp/libm" || echo failed >"$tmp/libm"
  "$sofel" "$font" "$@" >"$tmp/sofel" || echo failed >"$tmp/sofel"
  cmp -s "$tmp/libm" "$tmp/sofel" || echo "$name@$*"
}

glyph=8
while [ "$glyph" -le 220 ]; do
  case $glyph in
    161 | 162 | 163 | 164 | 165 | 168 | 170 | 171 | 172 | 173 | 174 | 175 | 176) ;;
    *) compare "variable-$glyph" "$variable" "$glyph" 48 ;;
  esac
  glyph=$((glyph + 1))
done
for glyph in 12 27 38; do
  compare "variable-$glyph" "$variable" "$glyph" 96 1 SWPS=45
done
if [ -f "$cff" ]; then
  for glyph in 19 26; do
    compare "cff-$glyph" "$cff" "$glyph" 48
  done
fi
