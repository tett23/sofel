#!/bin/sh
# Rebuild the fixed font gallery (sofel ADR 0069).
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

output=$root/target/font-gallery
staging=$root/target/font-gallery.tmp.$$
lock=$root/target/font-gallery.lock
mkdir -p "$root/target"
if ! mkdir "$lock" 2>/dev/null; then
  echo 'font gallery generation is already running' >&2
  exit 1
fi
trap 'rm -rf "$staging"; rmdir "$lock"' EXIT HUP INT TERM

# An earlier success must never masquerade as the result of a failed run.
rm -rf "$output" "$staging"
mkdir "$staging"

sh scripts/fetch-testdata.sh
sh scripts/fetch-fonts.sh
cargo build --locked --release -p sofel-cli --bin sofel-render --bin sofel-shape

ruby tools/font-gallery/test_inputs.rb
ruby tools/font-gallery/test_pixels.rb
ruby tools/font-gallery/test_sheet.rb
ruby tools/font-gallery/gallery.rb "$staging" "$root/target/release"

test -s "$staging/index.html"
test -s "$staging/all-samples.png"
mv "$staging" "$output"
