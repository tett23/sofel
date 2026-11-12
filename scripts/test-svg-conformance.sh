#!/bin/sh
# Generate this checkout's gallery before comparing its pixels and local/CI measurements.
set -eu
cd "$(dirname "$0")/.."
SOFEL_GALLERY_OUTPUT=$PWD/target/font-gallery
export SOFEL_GALLERY_OUTPUT
if ! sh scripts/render-font-gallery.sh >&2; then
  # Report the other mandatory cases even when the gallery cannot be generated.
  export SOFEL_SVG_GALLERY_FAILED=1
fi
exec cargo run --quiet --locked --release -p sofel-conformance --bin sofel-svg-conformance
