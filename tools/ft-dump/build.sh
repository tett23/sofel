#!/bin/sh
# Build ft-dump against the FreeType source fetched by
# scripts/fetch-testdata.sh (ADR 0008). The standard build needs a C
# compiler; the files are those listed in FreeType's docs/INSTALL.ANY.
#
# Usage: tools/ft-dump/build.sh [FREETYPE_SOURCE [OUTPUT]]
#   FREETYPE_SOURCE  default: target/testdata/freetype
#   OUTPUT           default: target/ft-dump/ft-dump
# A third argument --harfbuzz builds the automatic hinter with the pinned
# HarfBuzz source and requires a C++ compiler. SOFEL_HB_SRC overrides the
# default target/testdata/harfbuzz source path.
# --webfont-reference builds the table comparison tool
# with WOFF2 enabled and requires Brotli decoder headers and library.
# Use --brotli to keep ft-dump.c and enable WOFF2 decoding.
# Use --filtered-lcd as the third argument for the filtered LCD reference.
# The C compiler is $CC (default: cc).

set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
src=${1:-$root/target/testdata/freetype}
out=${2:-$root/target/ft-dump/ft-dump}
cc=${CC:-cc}
cxx=${CXX:-c++}
driver=$here/ft-dump.c
mode=${3:-}
if [ "$mode" = "--harfbuzz" ]; then
  hb_src=${SOFEL_HB_SRC:-$root/target/testdata/harfbuzz}/src
  if [ ! -f "$hb_src/harfbuzz.cc" ]; then
    echo "build.sh: no HarfBuzz source in $hb_src; run scripts/fetch-testdata.sh first" >&2
    exit 1
  fi
  set -- -DFT_CONFIG_OPTION_USE_HARFBUZZ -I "$hb_src"
elif [ "$mode" = "--webfont-reference" ]; then
  driver=$root/tools/webfont-reference/compare.c
  set -- -DFT_CONFIG_OPTION_USE_BROTLI -lbrotlidec
elif [ "$mode" = "--brotli" ]; then
  set -- -DFT_CONFIG_OPTION_USE_BROTLI -lbrotlidec
elif [ "$mode" = "--filtered-lcd" ]; then
  set -- -DFT_CONFIG_OPTION_SUBPIXEL_RENDERING
else
  set --
fi

if [ ! -f "$src/include/ft2build.h" ]; then
  echo "build.sh: no FreeType source in $src; run scripts/fetch-testdata.sh first" >&2
  exit 1
fi

files="
  base/ftsystem.c base/ftinit.c base/ftdebug.c base/ftbase.c base/ftstroke.c
  base/ftbbox.c base/ftglyph.c base/ftbitmap.c base/ftmm.c
  base/fttype1.c base/ftgasp.c base/ftbdf.c base/ftcid.c base/ftpfr.c
  base/ftwinfnt.c base/ftfstype.c base/ftotval.c
  sfnt/sfnt.c truetype/truetype.c cff/cff.c cid/type1cid.c
  type1/type1.c type42/type42.c pfr/pfr.c winfonts/winfnt.c
  pcf/pcf.c bdf/bdf.c psaux/psaux.c psnames/psnames.c
  pshinter/pshinter.c autofit/autofit.c smooth/smooth.c
  raster/raster.c sdf/sdf.c svg/svg.c gzip/ftgzip.c lzw/ftlzw.c
  otvalid/otvalid.c
"

sources=
for f in $files; do
  sources="$sources $src/src/$f"
done

png_flags=
if [ "${SOFEL_FT_PNG:-0}" = 1 ]; then
  png_flags="-DFT_CONFIG_OPTION_USE_PNG -lpng"
fi
mkdir -p "$(dirname "$out")"
# ft-dump --calc random calls psaux's static cff_random through
# psrandom.c, which needs the psaux sources on its include path; it is
# compiled on its own so that path does not reach the other files.
extra=
if [ "$driver" = "$here/ft-dump.c" ]; then
  extra="$(dirname "$out")/psrandom.o"
  "$cc" -O2 -w \
    -DFT2_BUILD_LIBRARY \
    '-DFT_CONFIG_MODULES_H=<ftmodule.h>' \
    -I "$here" -I "$src/include" -I "$src/src/psaux" \
    -c "$here/psrandom.c" -o "$extra"
fi
# shellcheck disable=SC2086 # $sources and $extra are paths without spaces
if [ "$mode" = "--harfbuzz" ]; then
  hb_obj="$(dirname "$out")/harfbuzz.o"
  "$cxx" -O2 -DNDEBUG -std=c++11 -ffp-contract=off \
    -fno-exceptions -fno-rtti -fno-threadsafe-statics -w \
    -I "$hb_src" -c "$hb_src/harfbuzz.cc" -o "$hb_obj"
  # Compile FreeType and the driver as C, then link the HarfBuzz object
  # with the C++ linker to include its runtime on each host.
  # shellcheck disable=SC2086 # $sources and $extra are paths without spaces
  "$cxx" -O2 -w -DFT2_BUILD_LIBRARY -DSOFEL_FT_DUMP_SVG \
    '-DFT_CONFIG_MODULES_H=<ftmodule.h>' \
    -I "$here" -I "$src/include" "$@" -x c \
    "$driver" $sources -x none $extra "$hb_obj" $png_flags \
    -o "$out"
else
  # shellcheck disable=SC2086 # $sources and $extra are paths without spaces
  "$cc" -O2 -w \
    -DFT2_BUILD_LIBRARY -DSOFEL_FT_DUMP_SVG \
    '-DFT_CONFIG_MODULES_H=<ftmodule.h>' \
    -I "$here" -I "$src/include" \
    "$driver" $sources $extra $png_flags "$@" \
    -o "$out"
fi
echo "$out"
