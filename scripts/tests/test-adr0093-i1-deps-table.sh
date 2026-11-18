#!/bin/sh
# ADR 0093 I1: the dependency pairs that scripts/deps.txt allows equal
# the table of ADR 0003 together with the ADRs that changed it.
#
# Usage: test-adr0093-i1-deps-table.sh [DEPS]   (default: scripts/deps.txt)
#
# When an ADR changes the table, change the list below in the same PR.

set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
deps=${1:-$root/scripts/deps.txt}
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

# <crate> <dependency>, from the tables of the ADRs named in each comment.
sed -e 's/#.*//' -e 's/[[:space:]]*$//' <<'EOF' | grep -v '^$' | sort >"$tmp/expected"
# ADR 0003: crates that do not depend on fonts
sofel-png sofel-inflate
sofel-svg sofel-inflate
sofel-svg sofel-geom
sofel-raster sofel-geom
sofel-gpu sofel-geom
sofel-vector sofel-geom
# ADR 0003, ADR 0081: sofel-woff
sofel-woff sofel-read
sofel-woff sofel-write
sofel-woff sofel-inflate
sofel-woff sofel-brotli
# ADR 0003: crates that handle fonts
sofel-font sofel-read
sofel-font sofel-type1
sofel-font sofel-pfr
sofel-font sofel-bitmapfont
sofel-font sofel-geom
sofel-shape sofel-font
sofel-outline sofel-font
sofel-outline sofel-geom
sofel-bitmap sofel-font
sofel-bitmap sofel-png
sofel-color sofel-outline
sofel-color sofel-bitmap
sofel-color sofel-svg
sofel-write sofel-read
sofel-subset sofel-font
sofel-subset sofel-write
# ADR 0019: sofel-shape uses sofel-unicode
sofel-shape sofel-unicode
# ADR 0022: sofel-subset uses sofel-unicode
sofel-subset sofel-unicode
# ADR 0026 (recorded by ADR 0093): sofel_geom::float
sofel-color sofel-geom
sofel-subset sofel-geom
# ADR 0093: sofel-shape uses sofel-geom
sofel-shape sofel-geom
# The glyph rendering shared by the tools and the C ABI (ADR 0087,
# ADR 0095)
sofel-glyph-render sofel-font
sofel-glyph-render sofel-outline
sofel-glyph-render sofel-raster
sofel-glyph-render sofel-color
sofel-glyph-render sofel-svg
sofel-glyph-render sofel-type1
sofel-glyph-render sofel-geom
sofel-glyph-render sofel-png
# ADR 0003, ADR 0008: the tools and the conformance tests may depend on
# every crate
sofel-cli *
# ADR 0087: the C ABI lists its dependencies; sofel-cli only in its tests
# (a `!dev` line, which this list does not compare)
sofel-capi sofel-png
sofel-capi sofel-gpu
sofel-capi sofel-vector
sofel-capi sofel-read
sofel-capi sofel-geom
sofel-capi sofel-font
sofel-capi sofel-outline
sofel-capi sofel-raster
sofel-capi sofel-bitmap
sofel-capi sofel-color
sofel-capi sofel-unicode
sofel-capi sofel-shape
sofel-capi sofel-subset
sofel-capi sofel-write
sofel-capi sofel-glyph-render
sofel-conformance *
EOF

# The dependency pairs of deps.txt: lines without the `!` directives.
sed -e 's/#.*//' -e 's/[[:space:]]*$//' "$deps" | grep -v '^$' | grep -v '^!' |
  sort >"$tmp/actual" || true

if ! cmp -s "$tmp/expected" "$tmp/actual"; then
  echo "FAIL: ADR 0093 I1: $deps differs from the ADR tables (< ADR, > deps.txt):" >&2
  diff "$tmp/expected" "$tmp/actual" >&2 || true
  exit 1
fi

# The check itself rejects a deps.txt that drops or adds a pair.
if [ -z "${1:-}" ]; then
  grep -v '^sofel-shape sofel-geom$' "$deps" >"$tmp/dropped"
  if sh "$0" "$tmp/dropped" 2>/dev/null; then
    echo "FAIL: ADR 0093 I1: a deps.txt without 'sofel-shape sofel-geom' was accepted" >&2
    exit 1
  fi
  { cat "$deps"; echo 'sofel-read sofel-geom'; } >"$tmp/added"
  if sh "$0" "$tmp/added" 2>/dev/null; then
    echo "FAIL: ADR 0093 I1: a deps.txt with a pair no ADR allows was accepted" >&2
    exit 1
  fi
  echo "ok: ADR 0093 I1: scripts/deps.txt equals the ADR tables"
fi
