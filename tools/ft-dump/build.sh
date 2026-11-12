#!/bin/sh
# Build ft-dump against the FreeType source fetched by
# scripts/fetch-testdata.sh (ADR 0008). Only a C compiler is needed:
# the files are those listed in FreeType's docs/INSTALL.ANY.
#
# Usage: tools/ft-dump/build.sh [FREETYPE_SOURCE [OUTPUT]]
#   FREETYPE_SOURCE  default: target/testdata/freetype
#   OUTPUT           default: target/ft-dump/ft-dump
# A third argument --webfont-reference builds the table comparison tool
# with WOFF2 enabled and requires Brotli decoder headers and library.
# Use --filtered-lcd as the third argument for the filtered LCD reference.
# The C compiler is $CC (default: cc).

set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
src=${1:-$root/target/testdata/freetype}
out=${2:-$root/target/ft-dump/ft-dump}
cc=${CC:-cc}
driver=$here/ft-dump.c
if [ "${3:-}" = "--webfont-reference" ]; then
  driver=$root/tools/webfont-reference/compare.c
  set -- -DFT_CONFIG_OPTION_USE_BROTLI -lbrotlidec
elif [ "${3:-}" = "--filtered-lcd" ]; then
  set -- -DFT_CONFIG_OPTION_SUBPIXEL_RENDERING
else
  set --
fi

if [ ! -f "$src/include/ft2build.h" ]; then
  echo "build.sh: no FreeType source in $src; run scripts/fetch-testdata.sh first" >&2
  exit 1
fi

files="
  base/ftsystem.c base/ftinit.c base/ftdebug.c base/ftbase.c
  base/ftbbox.c base/ftglyph.c base/ftbitmap.c base/ftmm.c
  base/fttype1.c base/ftgasp.c base/ftbdf.c base/ftcid.c base/ftpfr.c
  base/ftwinfnt.c base/ftfstype.c base/ftotval.c
  sfnt/sfnt.c truetype/truetype.c cff/cff.c cid/type1cid.c
  type1/type1.c type42/type42.c pfr/pfr.c winfonts/winfnt.c
  pcf/pcf.c bdf/bdf.c psaux/psaux.c psnames/psnames.c
  pshinter/pshinter.c autofit/autofit.c smooth/smooth.c
  raster/raster.c sdf/sdf.c gzip/ftgzip.c lzw/ftlzw.c
  otvalid/otvalid.c
"

sources=
for f in $files; do
  sources="$sources $src/src/$f"
done

mkdir -p "$(dirname "$out")"
# shellcheck disable=SC2086 # $sources is a list of paths without spaces
"$cc" -O2 -w \
  -DFT2_BUILD_LIBRARY \
  '-DFT_CONFIG_MODULES_H=<ftmodule.h>' \
  -I "$here" -I "$src/include" \
  "$driver" $sources "$@" \
  -o "$out"
echo "$out"
