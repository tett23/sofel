#!/bin/sh
# Build the HarfBuzz raster reference (sofel ADR 0066, ADR 0072) with
# HarfBuzz's calls to atan2f answered by sofel_geom::float::atan2f (sofel
# ADR 0095): the crate in atan2f/ exports it as the C function atan2f.
# It is built into one object (fat LTO), so that nothing else of Rust's
# runtime is linked, and the object comes before the C library. The sweep
# gradients then get the same angles as sofel on every platform. sqrtf is the platform's: it
# is correctly rounded everywhere, as sofel's is.
# Usage: build.sh HARFBUZZ_SOURCE OUTPUT [PINNED_LIBHARFBUZZ_A]
# Needs a C++ compiler ($CXX, default c++) and cargo.
# SOFEL_HB_RASTER_ATAN2F=libm builds the reference with the platform's
# atan2f instead, for atan2f-changes.sh.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
src=$1/src
out=$2
core=${3:-$src/harfbuzz.cc}
target_dir=${SOFEL_ATAN2F_TARGET_DIR:-$root/target/hb-raster-atan2f}
if [ "${SOFEL_HB_RASTER_ATAN2F:-sofel}" = libm ]; then
  set --
else
  mkdir -p "$target_dir"
  cargo rustc --quiet --release --manifest-path "$here/atan2f/Cargo.toml" --target-dir "$target_dir" \
    -- --emit=obj="$target_dir/atan2f.o"
  set -- "$target_dir/atan2f.o"
fi
if [ "${SOFEL_HB_RASTER_PNG:-0}" = 1 ]; then
  # libpng-dev on the Linux conformance runner supplies this header and library.
  set -- "$@" -DHAVE_PNG -lpng
fi
"${CXX:-c++}" -std=c++11 -O2 -DNDEBUG -DHB_EXPERIMENTAL_API -ffp-contract=off \
  -I "$src" "$here/pixels.cc" "$src/hb-raster-draw.cc" "$src/hb-raster-image.cc" \
  "$src/hb-raster-paint.cc" "$core" "$@" -o "$out"
