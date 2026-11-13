#!/bin/sh
# Usage: build.sh HARFBUZZ_SOURCE OUTPUT [system|musl]
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src=$1/src
out=$2
mkdir -p "$(dirname "$out")"

case ${3:-system} in
  system)
    "${CXX:-c++}" -std=c++11 -O2 -DNDEBUG -ffp-contract=off -I "$src" \
      "$here/paths.cc" "$src/harfbuzz.cc" -o "$out"
    ;;
  musl)
    objects=$out.objects
    mkdir -p "$objects"
    for name in sincosf tanf __sindf __cosdf __tandf __rem_pio2f __rem_pio2_large wrappers; do
      "${CC:-cc}" -std=c11 -O2 -ffp-contract=off -Dhidden= \
        -fno-builtin-sinf -fno-builtin-cosf -I "$here/musl" \
        -c "$here/musl/$name.c" -o "$objects/$name.o"
    done
    "${CXX:-c++}" -std=c++11 -O2 -DNDEBUG -ffp-contract=off \
      -fno-builtin-sinf -fno-builtin-cosf -fno-builtin-tanf -I "$src" \
      "$here/paths.cc" "$src/harfbuzz.cc" \
      "$objects/sincosf.o" "$objects/tanf.o" "$objects/__sindf.o" \
      "$objects/__cosdf.o" "$objects/__tandf.o" \
      "$objects/__rem_pio2f.o" "$objects/__rem_pio2_large.o" \
      "$objects/wrappers.o" -lm -o "$out"
    ;;
  *)
    echo "expected system or musl math implementation" >&2
    exit 2
    ;;
esac
