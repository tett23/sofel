#!/bin/sh
# Check that every crate depends only on what scripts/deps.txt allows,
# which mirrors the table in ADR 0003 (ADR 0008).
#
# Usage: scripts/check-deps.sh [WORKSPACE [DEPS]]
#   WORKSPACE  default: the repository root
#   DEPS       default: scripts/deps.txt
#
# Each line of DEPS is `<crate> <dependency>`; `<crate> *` allows any
# dependency; `#` starts a comment. All dependency kinds are checked. A
# line `!dev <crate> <dependency>` allows the dependency only as a
# development dependency (ADR 0087).
#
# It also checks that every crate forbids `unsafe` (ADR 0003, ADR 0015):
# through the workspace lints (`[lints] workspace = true` with
# `unsafe_code = "forbid"` in the workspace) or `#![forbid(unsafe_code)]`
# at the root of each library and binary. A line `!unsafe <crate>` in
# DEPS exempts a crate.
#
# Only crates marked `!bin <crate>` may have binary targets, and crates
# marked `!no_std <crate>` must declare `#![no_std]` at the root of their
# library (ADR 0081), and also #![forbid(unsafe_code)] unless marked
# `!unsafe` (ADR 0003). Crates marked `!no_alloc <crate>` must not use the
# alloc crate (`extern crate alloc`, `alloc::`) in their library sources
# (ADR 0094). A line `!no_alloc_dir <crate> <dir>` names a directory of
# the crate (relative to its manifest) that must exist, must not name what
# allocates, and may reach only `core`, itself and `!no_alloc` crates
# (ADR 0098).
# Needs cargo and jq.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
ws=${1:-$root}
deps=${2:-$root/scripts/deps.txt}
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

sed -e 's/#.*//' -e 's/[[:space:]]*$//' "$deps" | grep -v '^$' >"$tmp/lines" || true
grep -v '^!' "$tmp/lines" >"$tmp/allowed" || true
sed -n 's/^!unsafe //p' "$tmp/lines" >"$tmp/unsafe"
sed -n 's/^!bin //p' "$tmp/lines" >"$tmp/bin"
sed -n 's/^!no_std //p' "$tmp/lines" >"$tmp/no_std"
sed -n 's/^!no_alloc //p' "$tmp/lines" >"$tmp/no_alloc"
sed -n 's/^!no_alloc_dir //p' "$tmp/lines" >"$tmp/no_alloc_dir"
sed -n 's/^!dev //p' "$tmp/lines" >"$tmp/dev"

cargo metadata --format-version 1 --no-deps --offline --manifest-path "$ws/Cargo.toml" >"$tmp/meta"
jq -r '.packages[] | .name as $p | .dependencies[] | "\($p) \(.name) \(.kind // "normal")"' "$tmp/meta" |
  sort -u >"$tmp/actual"

status=0
while read -r crate dep kind; do
  if grep -qxF "$crate $dep" "$tmp/allowed" || grep -qxF "$crate *" "$tmp/allowed"; then
    continue
  fi
  if [ "$kind" = dev ] && grep -qxF "$crate $dep" "$tmp/dev"; then
    continue
  fi
  echo "check-deps.sh: $crate depends on $dep ($kind), which $deps does not allow" >&2
  status=1
done <"$tmp/actual"

# `[lints]` followed by `workspace = true` in the manifest.
workspace_lints() {
  awk '/^\[/ { in_lints = ($0 == "[lints]") } in_lints && /^workspace[[:space:]]*=[[:space:]]*true/ { found = 1 } END { exit !found }' "$1"
}

ws_forbids=0
if awk '/^\[/ { in_rust = ($0 == "[workspace.lints.rust]") } in_rust && /^unsafe_code[[:space:]]*=[[:space:]]*"forbid"/ { found = 1 } END { exit !found }' "$ws/Cargo.toml"; then
  ws_forbids=1
fi

jq -r '.packages[] | "\(.name) \(.manifest_path)"' "$tmp/meta" >"$tmp/packages"
while read -r crate manifest; do
  grep -qxF "$crate" "$tmp/unsafe" && continue
  if [ $ws_forbids = 1 ] && workspace_lints "$manifest"; then
    continue
  fi
  roots=$(jq -r --arg p "$crate" '.packages[] | select(.name == $p) | .targets[]
    | select(.kind | any(. == "lib" or . == "bin" or . == "rlib" or . == "staticlib" or . == "cdylib"))
    | .src_path' "$tmp/meta")
  ok=1
  for r in $roots; do
    grep -qF '#![forbid(unsafe_code)]' "$r" || ok=0
  done
  if [ $ok = 0 ] || [ -z "$roots" ]; then
    echo "check-deps.sh: $crate does not forbid unsafe code; only crates marked \`!unsafe\` in $deps may use it" >&2
    status=1
  fi
done <"$tmp/packages"

# ADR 0081 I4: binaries only where `!bin` allows them.
jq -r '.packages[] | .name as $p | .targets[] | select(.kind | any(. == "bin")) | "\($p) \(.name)"' \
  "$tmp/meta" | while read -r crate bin; do
  if ! grep -qxF "$crate" "$tmp/bin"; then
    echo "check-deps.sh: $crate has the binary $bin; only crates marked \`!bin\` in $deps may" >&2
    echo fail
  fi
done >"$tmp/bin-failures"
[ -s "$tmp/bin-failures" ] && status=1

# ADR 0081 I2: `!no_std` crates declare it at the library root.
while read -r crate; do
  lib=$(jq -r --arg p "$crate" '.packages[] | select(.name == $p) | .targets[]
    | select(.kind | any(. == "lib")) | .src_path' "$tmp/meta")
  if [ -z "$lib" ] || ! grep -qxF '#![no_std]' "$lib"; then
    echo "check-deps.sh: $crate is marked \`!no_std\` but its library does not declare #![no_std]" >&2
    status=1
  fi
  # ADR 0003: library crates write #![forbid(unsafe_code)] at their root.
  if [ -n "$lib" ] && ! grep -qxF "$crate" "$tmp/unsafe" && ! grep -qxF '#![forbid(unsafe_code)]' "$lib"; then
    echo "check-deps.sh: $crate is marked \`!no_std\` but its library does not declare #![forbid(unsafe_code)]" >&2
    status=1
  fi
done <"$tmp/no_std"

# ADR 0094 I1: `!no_alloc` crates neither import nor use the alloc crate
# anywhere in the directory of their library root (comments aside).
while read -r crate; do
  lib=$(jq -r --arg p "$crate" '.packages[] | select(.name == $p) | .targets[]
    | select(.kind | any(. == "lib")) | .src_path' "$tmp/meta")
  if [ -z "$lib" ]; then
    echo "check-deps.sh: $crate is marked \`!no_alloc\` but has no library" >&2
    status=1
    continue
  fi
  find "$(dirname "$lib")" -name '*.rs' -type f -exec grep -nE \
    '(^|[^:[:alnum:]_])alloc::|extern[[:space:]]+crate[[:space:]]+alloc' {} + 2>/dev/null |
    grep -vE '^([^:]*:)?[0-9]+:[[:space:]]*//' >"$tmp/alloc-uses" || true
  if [ -s "$tmp/alloc-uses" ]; then
    echo "check-deps.sh: $crate is marked \`!no_alloc\` but uses the alloc crate:" >&2
    cat "$tmp/alloc-uses" >&2
    status=1
  fi
done <"$tmp/no_alloc"

# ADR 0098 I1, I2: a `!no_alloc_dir` directory exists, names nothing that
# allocates, and its `use` lines and paths start only with `core`,
# `crate::noalloc`, `self`, `super` (not from its mod.rs), a primitive
# type, one of its own modules, a name a checked `use` line brings in, a
# lint tool or a `!no_alloc` crate. Comments and
# text after ` //` are not read.
tr '-' '_' <"$tmp/no_alloc" >"$tmp/no_alloc_crates"
while read -r crate dir; do
  manifest=$(jq -r --arg p "$crate" '.packages[] | select(.name == $p) | .manifest_path' "$tmp/meta")
  path=$(dirname "$manifest")/$dir
  if [ -z "$manifest" ] || [ ! -d "$path" ]; then
    echo "check-deps.sh: $crate has no \`!no_alloc_dir\` directory $dir" >&2
    status=1
    continue
  fi
  find "$path" -name '*.rs' -type f | sed -e 's#.*/##' -e 's#\.rs$##' >"$tmp/own"
  # Lint tools name lints in attributes, not code.
  printf '%s\n' core self super u8 u16 u32 u64 u128 usize i8 i16 i32 i64 i128 isize \
    f32 f64 bool char str clippy rustdoc >>"$tmp/own"
  cat "$tmp/no_alloc_crates" >>"$tmp/own"
  for file in $(find "$path" -name '*.rs' -type f); do
    grep -vE '^[[:space:]]*//' "$file" | sed -e 's#[[:space:]]//.*$##' >"$tmp/code"
    grep -nE '(^|[^:[:alnum:]_])alloc::|(^|[^[:alnum:]_])(Vec|Box|String|Rc|Arc)([^[:alnum:]_]|$)|(vec|format)!|\.(to_vec|to_string|to_owned)\(|\.collect([^[:alnum:]_]|$)|extern[[:space:]]+crate' \
      "$tmp/code" >"$tmp/names" || true
    if [ -s "$tmp/names" ]; then
      echo "check-deps.sh: $file names what allocates (ADR 0098):" >&2
      cat "$tmp/names" >&2
      status=1
    fi
    grep -oE '(^|[^:.[:alnum:]_])crate::[[:alnum:]_]*' "$tmp/code" |
      grep -v 'crate::noalloc$' >"$tmp/outside" || true
    if [ "$(basename "$file")" = mod.rs ] && [ "$(dirname "$file")" = "${path%/}" ]; then
      grep -oE '(^|[^:.[:alnum:]_])super::' "$tmp/code" >>"$tmp/outside" || true
    fi
    grep -oE '(^|[^:.[:alnum:]_])super::super::' "$tmp/code" >>"$tmp/outside" || true
    # Names a `use` line brings in may start paths; the line itself is
    # checked like any other path.
    cp "$tmp/own" "$tmp/names-here"
    grep -E '^[[:space:]]*(pub[[:space:]]+)?use[[:space:]]' "$tmp/code" |
      grep -oE '[a-z_][a-z0-9_]*' >>"$tmp/names-here" || true
    grep -E '^[[:space:]]*(pub[[:space:]]+)?use[[:space:]]' "$tmp/code" |
      sed -E 's/^[[:space:]]*(pub[[:space:]]+)?use[[:space:]]+//' |
      grep -vE '^(core|crate|self|super)::' |
      grep -vE "^($(paste -sd '|' "$tmp/no_alloc_crates"))::" >>"$tmp/outside" || true
    grep -oE '(^|[^:.[:alnum:]_])[a-z_][a-z0-9_]*::' "$tmp/code" |
      sed -e 's/^[^a-z_]//' -e 's/::$//' | grep -vxF -e crate -f "$tmp/names-here" >>"$tmp/outside" || true
    if [ -s "$tmp/outside" ]; then
      echo "check-deps.sh: $file reaches outside its directory (ADR 0098):" >&2
      cat "$tmp/outside" >&2
      status=1
    fi
  done
done <"$tmp/no_alloc_dir"
exit $status
