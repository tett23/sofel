#!/bin/sh
# Build FreeType, HarfBuzz and the benchmark shim as static libraries from
# the sources fetched by scripts/fetch-testdata.sh (ADR 0013).
#
# Usage: bench/build-c.sh [TARGET]
#   TARGET  a Rust target triple; default: this host (`rustc -vV`).
#           wasm32-wasip1 builds with wasi-sdk from bench/fetch-wasi-sdk.sh.
# Output: target/bench-c/<TARGET>/lib{freetype,harfbuzz,benchshim}.a
# The C and C++ compilers are $CC and $CXX (default: cc and c++).

set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
target=${1:-$(rustc -vV | awk '/^host:/ { print $2 }')}
ft=$root/target/testdata/freetype
hb=$root/target/testdata/harfbuzz
out=$root/target/bench-c/$target
obj=$out/obj

for d in "$ft/include" "$hb/src"; do
  if [ ! -d "$d" ]; then
    echo "build-c.sh: $d is missing; run scripts/fetch-testdata.sh first" >&2
    exit 1
  fi
done

flags="-O2 -DNDEBUG"
fpflags="-ffp-contract=off"
if [ "$target" = wasm32-wasip1 ]; then
  sdk=$(sh "$here/fetch-wasi-sdk.sh")
  # FreeType validates cmaps with setjmp and longjmp, which wasm builds
  # on the exception handling proposal (wasmtime -W exceptions=y).
  cc="$sdk/bin/clang --target=wasm32-wasip1 --sysroot=$sdk/share/wasi-sysroot -mllvm -wasm-enable-sjlj -mllvm -wasm-use-legacy-eh=false"
  cxx="$sdk/bin/clang++ --target=wasm32-wasip1 --sysroot=$sdk/share/wasi-sysroot"
  ar="$sdk/bin/llvm-ar"
  # No threads in wasm32-wasip1.
  hbflags="-DHB_NO_MT"
else
  cc=${CC:-cc}
  cxx=${CXX:-c++}
  ar=ar
  hbflags=""
fi

rm -rf "$obj"
mkdir -p "$obj/ft" "$obj/hb" "$obj/shim"

# FreeType: the files of tools/ft-dump/build.sh (docs/INSTALL.ANY).
for f in \
  base/ftsystem.c base/ftinit.c base/ftdebug.c base/ftbase.c \
  base/ftbbox.c base/ftglyph.c base/ftbitmap.c base/ftmm.c \
  base/fttype1.c base/ftgasp.c base/ftbdf.c base/ftcid.c base/ftpfr.c \
  base/ftwinfnt.c base/ftfstype.c base/ftotval.c \
  sfnt/sfnt.c truetype/truetype.c cff/cff.c cid/type1cid.c \
  type1/type1.c type42/type42.c pfr/pfr.c winfonts/winfnt.c \
  pcf/pcf.c bdf/bdf.c psaux/psaux.c psnames/psnames.c \
  pshinter/pshinter.c autofit/autofit.c smooth/smooth.c \
  raster/raster.c gzip/ftgzip.c lzw/ftlzw.c otvalid/otvalid.c
do
  # shellcheck disable=SC2086 # $cc and $flags are word lists
  $cc $flags $fpflags -w -DFT2_BUILD_LIBRARY '-DFT_CONFIG_MODULES_H=<ftmodule.h>' \
    -I "$root/tools/ft-dump" -I "$ft/include" \
    -c "$ft/src/$f" -o "$obj/ft/$(echo "$f" | tr / _).o"
done

# HarfBuzz: compile the upstream subset include list in four ordered
# translation units using the same flags (ADR 0038).
mkdir "$obj/hb/units"
awk -v out="$obj/hb/units" -f "$here/split-harfbuzz.awk" "$hb/src/harfbuzz-subset.cc"
for unit in "$obj"/hb/units/part-*.cc; do
  name=${unit##*/}
  # shellcheck disable=SC2086
  $cxx $flags $fpflags $hbflags -std=c++11 -fno-exceptions -fno-rtti -fno-threadsafe-statics -w \
    -I "$hb/src" -c "$unit" -o "$obj/hb/${name%.cc}.o"
done

# shellcheck disable=SC2086
$cc $flags -I "$ft/include" -I "$hb/src" -c "$here/c/shim.c" -o "$obj/shim/shim.o"

rm -f "$out/libfreetype.a" "$out/libharfbuzz.a" "$out/libbenchshim.a"
$ar rcs "$out/libfreetype.a" "$obj"/ft/*.o
$ar rcs "$out/libharfbuzz.a" "$obj"/hb/*.o
$ar rcs "$out/libbenchshim.a" "$obj"/shim/*.o
if [ "$target" = wasm32-wasip1 ]; then
  cp "$sdk/share/wasi-sysroot/lib/wasm32-wasip1/libsetjmp.a" "$out/libsetjmp.a"
fi
echo "$out"
