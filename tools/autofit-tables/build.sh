#!/bin/sh
# Build tools/autofit-tables against the FreeType source fetched by
# scripts/fetch-testdata.sh and regenerate the automatic hinter's tables
# in crates/sofel-outline/src/autofit/tables.rs (sofel ADR 0064).
#
# Usage: tools/autofit-tables/build.sh [FREETYPE_SOURCE]
#   FREETYPE_SOURCE  default: target/testdata/freetype
# The C compiler is $CC (default: cc).

set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
src=${1:-$root/target/testdata/freetype}
cc=${CC:-cc}
out=$root/target/autofit-tables/autofit-tables
tables=$root/crates/sofel-outline/src/autofit/tables.rs

if [ ! -f "$src/include/ft2build.h" ]; then
  echo "build.sh: no FreeType source in $src; run scripts/fetch-testdata.sh first" >&2
  exit 1
fi

# The FreeType modules of tools/ft-dump, without autofit/autofit.c,
# which the generator includes itself.
files="
  base/ftsystem.c base/ftinit.c base/ftdebug.c base/ftbase.c base/ftstroke.c
  base/ftbbox.c base/ftglyph.c base/ftbitmap.c base/ftmm.c
  base/fttype1.c base/ftgasp.c base/ftbdf.c base/ftcid.c base/ftpfr.c
  base/ftwinfnt.c base/ftfstype.c base/ftotval.c
  sfnt/sfnt.c truetype/truetype.c cff/cff.c cid/type1cid.c
  type1/type1.c type42/type42.c pfr/pfr.c winfonts/winfnt.c
  pcf/pcf.c bdf/bdf.c psaux/psaux.c psnames/psnames.c
  pshinter/pshinter.c smooth/smooth.c
  raster/raster.c sdf/sdf.c gzip/ftgzip.c lzw/ftlzw.c
  otvalid/otvalid.c
"

sources=
for f in $files; do
  sources="$sources $src/src/$f"
done

mkdir -p "$(dirname "$out")"
# shellcheck disable=SC2086 # $sources is a list of paths without spaces
"$cc" -O1 -w \
  -DFT2_BUILD_LIBRARY \
  '-DFT_CONFIG_MODULES_H=<ftmodule.h>' \
  -I "$root/tools/ft-dump" -I "$src/include" -I "$src/src/autofit" \
  "$here/autofit-tables.c" $sources \
  -o "$out"
"$out" > "$tables.tmp"
mv "$tables.tmp" "$tables"
echo "$tables"
