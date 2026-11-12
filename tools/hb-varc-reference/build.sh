#!/bin/sh
# Usage: build.sh HARFBUZZ_SOURCE OUTPUT
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src=$1/src
out=$2
mkdir -p "$(dirname "$out")"
"${CXX:-c++}" -std=c++11 -O2 -DNDEBUG -ffp-contract=off -I "$src" \
  "$here/paths.cc" "$src/harfbuzz.cc" -o "$out"
