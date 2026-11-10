#!/bin/sh
# Tests for scripts/check-capi.sh and scripts/public-api.sh (ADR 0015).

set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
script=$root/scripts/check-capi.sh
header=$root/crates/sofel-capi/include/sofel.h
unmapped=$root/crates/sofel-capi/unmapped.txt
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# Small, made-up files.
cat >"$tmp/api" <<'EOF'
a::f
a::T::g
a::m::h
a::m::i
EOF
cat >"$tmp/h" <<'EOF'
/*
 * Rust: a::f, a::T::g
 */
/*
 * Rust: none
 */
EOF
printf 'a::m::* lower layer\n' >"$tmp/u"
sh "$script" "$tmp/h" "$tmp/u" "$tmp/api" >/dev/null 2>&1 ||
  fail "a covered API was rejected"

grep -v 'a::T::g' "$tmp/h" >"$tmp/h2"
if sh "$script" "$tmp/h2" "$tmp/u" "$tmp/api" >/dev/null 2>&1; then
  fail "a function without a C counterpart was accepted"
fi

: >"$tmp/u2"
if sh "$script" "$tmp/h" "$tmp/u2" "$tmp/api" >/dev/null 2>&1; then
  fail "a module missing from the exclusions was accepted"
fi

printf 'a::m::*\n' >"$tmp/u3"
if sh "$script" "$tmp/h" "$tmp/u3" "$tmp/api" >/dev/null 2>&1; then
  fail "an exclusion without a reason was accepted"
fi

sed 's/a::f,/a::gone, a::f,/' "$tmp/h" >"$tmp/h4"
if sh "$script" "$tmp/h4" "$tmp/u" "$tmp/api" >/dev/null 2>&1; then
  fail "a Rust: line naming no public function was accepted"
fi

printf 'a::m::* lower layer\nb::x::* other crates are not checked\na::q::* nothing\n' >"$tmp/u5"
if sh "$script" "$tmp/h" "$tmp/u5" "$tmp/api" >/dev/null 2>&1; then
  fail "an exclusion matching nothing was accepted"
fi

# The real header, against the public functions rustdoc finds.
sh "$root/scripts/public-api.sh" >"$tmp/real"
grep -qx 'sofel_font::Font::new' "$tmp/real" || fail "public-api.sh missed Font::new"
grep -qx 'sofel_geom::Pen::move_to' "$tmp/real" || fail "public-api.sh missed a trait method"
grep -qx 'sofel_geom::mul_fix' "$tmp/real" || fail "public-api.sh missed a re-exported function"
sh "$script" "$header" "$unmapped" "$tmp/real" >/dev/null ||
  fail "sofel.h does not cover the public API"
grep -v 'Rust: sofel_font::Font::advance$' "$header" >"$tmp/broken.h"
if sh "$script" "$tmp/broken.h" "$unmapped" "$tmp/real" >/dev/null 2>&1; then
  fail "a header without Font::advance was accepted"
fi

echo "ok: check-capi.sh"
