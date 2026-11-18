#!/bin/sh
# ADR 0097 I1: the atan2f.o that tools/hb-raster-reference/build.sh links
# defines atan2f as its only global symbol and has no undefined symbols.
# ADR 0097 I2: with SOFEL_HB_RASTER_ATAN2F=libm, build.sh neither builds
# atan2f.o nor passes it to the link.
#
# build.sh runs with CXX set to a stub that records its arguments, so the
# HarfBuzz source is not needed.

set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
build=$root/tools/hb-raster-reference/build.sh
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
# Shared with other runs: the crate builds sofel-geom, which is slow cold.
target=${SOFEL_ATAN2F_TEST_TARGET_DIR:-$root/target/test-adr0097}

cat >"$tmp/cxx" <<'EOF'
#!/bin/sh
printf '%s\n' "$@" >"$SOFEL_TEST_CXX_ARGS"
EOF
chmod +x "$tmp/cxx"

fail() {
  echo "$1" >&2
  exit 1
}

# only_atan2f OBJECT: the global symbols of OBJECT are one definition of
# atan2f (with the platform's leading underscore, if any) and nothing undefined.
only_atan2f() {
  nm -g "$1" >"$tmp/nm"
  if awk 'NF == 2 && $1 == "U"' "$tmp/nm" | grep -q .; then
    return 1
  fi
  defined=$(awk 'NF == 3 && $2 != "U" { print $3 }' "$tmp/nm")
  [ "$defined" = atan2f ] || [ "$defined" = _atan2f ]
}

# ADR 0097 I1
SOFEL_TEST_CXX_ARGS=$tmp/args CXX=$tmp/cxx SOFEL_ATAN2F_TARGET_DIR=$target \
  sh "$build" "$tmp/hb" "$tmp/out"
grep -qxF "$target/atan2f.o" "$tmp/args" ||
  fail "ADR 0097 I1: build.sh did not pass atan2f.o to the link"
only_atan2f "$target/atan2f.o" || {
  cat "$tmp/nm" >&2
  fail "ADR 0097 I1: atan2f.o has symbols other than the definition of atan2f"
}

# ADR 0097 I1: the check fails for the crate built without fat LTO, whose
# object leaves sofel-geom's functions undefined.
cp -R "$root/tools/hb-raster-reference/atan2f" "$tmp/nolto"
sed -e '/^lto = /d' -e "s|path = \"../../../crates/sofel-geom\"|path = \"$root/crates/sofel-geom\"|" \
  "$root/tools/hb-raster-reference/atan2f/Cargo.toml" >"$tmp/nolto/Cargo.toml"
cargo rustc --quiet --release --manifest-path "$tmp/nolto/Cargo.toml" --target-dir "$target/nolto" \
  -- --emit=obj="$tmp/nolto.o"
if only_atan2f "$tmp/nolto.o"; then
  fail "ADR 0097 I1: the check accepted an object built without fat LTO"
fi

# ADR 0097 I2
SOFEL_TEST_CXX_ARGS=$tmp/args CXX=$tmp/cxx SOFEL_HB_RASTER_ATAN2F=libm \
  SOFEL_ATAN2F_TARGET_DIR=$tmp/libm sh "$build" "$tmp/hb" "$tmp/out"
[ ! -e "$tmp/libm" ] || fail "ADR 0097 I2: build.sh built atan2f.o for libm"
if grep -q 'atan2f\.o$' "$tmp/args"; then
  fail "ADR 0097 I2: build.sh passed atan2f.o to the link for libm"
fi

echo "ok: ADR 0097 I1, I2 atan2f object"
