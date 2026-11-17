#!/bin/sh
# Tests for scripts/check-deps.sh (ADR 0008).

set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
script=$root/scripts/check-deps.sh
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

crate() {
  mkdir -p "$tmp/ws/$1/src"
  printf '[package]\nname = "%s"\nversion = "0.0.0"\nedition = "2024"\n\n[dependencies]\n%s\n\n[lints]\nworkspace = true\n' \
    "$1" "$2" >"$tmp/ws/$1/Cargo.toml"
  : >"$tmp/ws/$1/src/lib.rs"
}

mkdir -p "$tmp/ws"
printf '[workspace]\nresolver = "3"\nmembers = ["a", "b"]\n\n[workspace.lints.rust]\nunsafe_code = "forbid"\n' >"$tmp/ws/Cargo.toml"
crate b ''
crate a 'b = { path = "../b" }'

printf '# allowed\na b\n' >"$tmp/allowed.txt"
sh "$script" "$tmp/ws" "$tmp/allowed.txt" 2>/dev/null || fail "an allowed dependency was rejected"

printf '# nothing allowed\n' >"$tmp/none.txt"
if sh "$script" "$tmp/ws" "$tmp/none.txt" 2>/dev/null; then
  fail "a dependency missing from deps.txt was accepted"
fi

printf 'a *\n' >"$tmp/any.txt"
sh "$script" "$tmp/ws" "$tmp/any.txt" 2>/dev/null || fail "'a *' did not allow every dependency"

# Unsafe code: a crate without the workspace lints must forbid it itself
# or be marked `!unsafe`.
printf '[package]\nname = "b"\nversion = "0.0.0"\nedition = "2024"\n' >"$tmp/ws/b/Cargo.toml"
if sh "$script" "$tmp/ws" "$tmp/allowed.txt" 2>/dev/null; then
  fail "a crate that does not forbid unsafe code was accepted"
fi
printf '# allowed\na b\n!unsafe b\n' >"$tmp/unsafe.txt"
sh "$script" "$tmp/ws" "$tmp/unsafe.txt" 2>/dev/null || fail "'!unsafe b' did not exempt b"
printf '#![forbid(unsafe_code)]\n' >"$tmp/ws/b/src/lib.rs"
sh "$script" "$tmp/ws" "$tmp/allowed.txt" 2>/dev/null ||
  fail "#![forbid(unsafe_code)] in lib.rs was not accepted"
printf '[workspace]\nresolver = "3"\nmembers = ["a", "b"]\n\n[workspace.lints.rust]\nunsafe_code = "warn"\n' >"$tmp/ws/Cargo.toml"
if sh "$script" "$tmp/ws" "$tmp/allowed.txt" 2>/dev/null; then
  fail "workspace lints without unsafe_code = forbid were accepted"
fi

echo "ok: check-deps.sh"

# ADR 0081 I4: only crates marked `!bin` may have binary targets.
printf '[workspace]\nresolver = "3"\nmembers = ["a", "b"]\n\n[workspace.lints.rust]\nunsafe_code = "forbid"\n' >"$tmp/ws/Cargo.toml"
crate b ''
mkdir -p "$tmp/ws/b/src/bin"
printf 'fn main() {}\n' >"$tmp/ws/b/src/bin/tool.rs"
if sh "$script" "$tmp/ws" "$tmp/allowed.txt" 2>/dev/null; then
  fail "ADR 0081 I4: a binary in a crate without !bin was accepted"
fi
printf '# allowed\na b\n!bin b\n' >"$tmp/bin.txt"
sh "$script" "$tmp/ws" "$tmp/bin.txt" 2>/dev/null || fail "ADR 0081 I4: '!bin b' did not allow the binary"
rm -r "$tmp/ws/b/src/bin"
sh "$script" "$tmp/ws" "$tmp/allowed.txt" 2>/dev/null || fail "ADR 0081 I4: a library without binaries was rejected"
echo "ok: ADR 0081 I4: binaries only in crates marked !bin"

# ADR 0081 I2: crates marked `!no_std` must declare #![no_std].
printf '# allowed\na b\n!no_std b\n' >"$tmp/nostd.txt"
if sh "$script" "$tmp/ws" "$tmp/nostd.txt" 2>/dev/null; then
  fail "ADR 0081 I2: a !no_std crate without #![no_std] was accepted"
fi
printf '#![no_std]\n' >"$tmp/ws/b/src/lib.rs"
sh "$script" "$tmp/ws" "$tmp/nostd.txt" 2>/dev/null || fail "ADR 0081 I2: #![no_std] in lib.rs was not accepted"
echo "ok: ADR 0081 I2: crates marked !no_std declare #![no_std]"
