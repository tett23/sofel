#!/bin/sh
# Usage: build.sh HARFBUZZ_SOURCE OUTPUT [PINNED_LIBHARFBUZZ_A]
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src=$1/src
out=$2
core=${3:-$src/harfbuzz.cc}
"${CXX:-c++}" -std=c++11 -O2 -DNDEBUG -DHB_EXPERIMENTAL_API -ffp-contract=off \
  -I "$src" "$here/pixels.cc" "$src/hb-raster-draw.cc" "$src/hb-raster-image.cc" \
  "$src/hb-raster-paint.cc" "$core" -o "$out"
