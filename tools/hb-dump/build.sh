#!/bin/sh
# Build hb-dump against the HarfBuzz source fetched by
# scripts/fetch-testdata.sh (ADR 0016): one C++ translation unit that
# includes HarfBuzz's src/harfbuzz.cc, as bench/build-c.sh builds
# HarfBuzz. Only a C++ compiler is needed; there is no FreeType
# integration.
#
# Usage: tools/hb-dump/build.sh [HARFBUZZ_SOURCE [OUTPUT]]
#   HARFBUZZ_SOURCE  default: target/testdata/harfbuzz
#   OUTPUT           default: target/hb-dump/hb-dump
# The C++ compiler is $CXX (default: c++).

set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
src=${1:-$root/target/testdata/harfbuzz}
out=${2:-$root/target/hb-dump/hb-dump}
cxx=${CXX:-c++}

if [ ! -f "$src/src/harfbuzz.cc" ]; then
  echo "build.sh: no HarfBuzz source in $src; run scripts/fetch-testdata.sh first" >&2
  exit 1
fi

mkdir -p "$(dirname "$out")"
# The flags of HarfBuzz's own build (as in bench/build-c.sh).
"$cxx" -O2 -DNDEBUG -std=c++11 -fno-exceptions -fno-rtti -fno-threadsafe-statics -w \
  -I "$src/src" "$here/hb-dump.cc" -o "$out"
echo "$out"
