#!/bin/sh
# Usage: build.sh HARFBUZZ_SOURCE OUTPUT [PINNED_LIBHARFBUZZ_A]
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src=$1/src
out=$2
core=${3:-$src/harfbuzz.cc}
set --
if [ "${SOFEL_HB_RASTER_PNG:-0}" = 1 ]; then
  # libpng-dev on the Linux conformance runner supplies this header and library.
  set -- -DHAVE_PNG -lpng
fi
"${CXX:-c++}" -std=c++11 -O2 -DNDEBUG -DHB_EXPERIMENTAL_API -ffp-contract=off \
  -I "$src" "$here/pixels.cc" "$src/hb-raster-draw.cc" "$src/hb-raster-image.cc" \
  "$src/hb-raster-paint.cc" "$core" "$@" -o "$out"
