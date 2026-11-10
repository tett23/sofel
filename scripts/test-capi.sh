#!/bin/sh
# Build the static library of sofel-capi, link the C tests in
# crates/sofel-capi/tests/c against it, and run them (ADR 0015).
#
# Usage: scripts/test-capi.sh [MODE]
#   native   (default) build with the system C compiler, run the tests,
#            and check that the library's sofel_* symbols are exactly the
#            functions of sofel.h
#   tsan     the same tests built with -fsanitize=thread
#   asan     the same tests built with -fsanitize=address
#   wasm     build for wasm32-wasip1 with wasi-sdk (bench/fetch-wasi-sdk.sh)
#            and run with wasmtime; check that linking needs neither the
#            exception-handling setjmp library nor libc++/libc++abi
#
# Environment: CC (default cc), WASMTIME (default wasmtime).
# Needs cargo, nm and a C compiler.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
mode=${1:-native}
crate=$root/crates/sofel-capi
header=$crate/include/sofel.h
tests=$crate/tests/c
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

cflags="-std=c11 -Wall -Wextra -Werror -Wmissing-prototypes -Wstrict-prototypes -I$crate/include"

# The functions sofel.h declares, one per line, sorted.
header_functions() {
  grep -v '^typedef' "$header" |
    grep -o '^[a-z].*[ *]sofel_[a-z0-9_]*(' |
    sed 's/.*[ *]\(sofel_[a-z0-9_]*\)(/\1/' |
    sort -u
}

# Builds the native static library and prints its path, after writing
# the system libraries it needs to $tmp/libs.
native_lib() {
  # Without colors: the libraries are read from the note rustc prints.
  cargo rustc --quiet --color never --manifest-path "$root/Cargo.toml" -p sofel-capi --lib \
    --crate-type staticlib -- --print native-static-libs 2>"$tmp/rustc.err" || {
    cat "$tmp/rustc.err" >&2
    exit 1
  }
  sed -n 's/.*native-static-libs: //p' "$tmp/rustc.err" | tail -n 1 >"$tmp/libs"
  dir=$(cargo metadata --format-version 1 --no-deps --manifest-path "$root/Cargo.toml" |
    jq -r .target_directory)
  echo "$dir/debug/libsofel_capi.a"
}

run_native() {
  sanitize=$1
  lib=$(native_lib)
  # shellcheck disable=SC2086 # word splitting is intended
  ${CC:-cc} $cflags -g -O1 $sanitize -o "$tmp/test" "$tests/test.c" "$lib" \
    $(cat "$tmp/libs") -lpthread
  "$tmp/test"
}

check_symbols() {
  lib=$(native_lib)
  header_functions >"$tmp/declared"
  # Defined, global text symbols; Mach-O prefixes C names with `_`.
  nm -g "$lib" 2>/dev/null |
    awk '$2 == "T" { print $3 }' |
    sed 's/^_//' |
    grep '^sofel_' |
    sort -u >"$tmp/exported"
  if ! cmp -s "$tmp/declared" "$tmp/exported"; then
    echo "test-capi.sh: the symbols of $lib differ from the functions of sofel.h" >&2
    echo "(< declared only, > exported only)" >&2
    diff "$tmp/declared" "$tmp/exported" >&2 || true
    exit 1
  fi
  echo "ok: $(wc -l <"$tmp/declared" | tr -d ' ') functions of sofel.h exported"
}

run_wasm() {
  sdk=$(sh "$root/bench/fetch-wasi-sdk.sh")
  cargo build --quiet --release --manifest-path "$root/Cargo.toml" -p sofel-capi \
    --target wasm32-wasip1
  dir=$(cargo metadata --format-version 1 --no-deps --manifest-path "$root/Cargo.toml" |
    jq -r .target_directory)
  lib=$dir/wasm32-wasip1/release/libsofel_capi.a
  cc="$sdk/bin/clang --target=wasm32-wasip1 --sysroot=$sdk/share/wasi-sysroot"
  # shellcheck disable=SC2086 # word splitting is intended
  $cc $cflags -O2 -o "$tmp/test.wasm" "$tests/test.c" "$lib" -### 2>"$tmp/link"
  if grep -E -e '"-l(setjmp|c\+\+|c\+\+abi)"' -e 'wasm-enable-sjlj|exception-handling' \
    "$tmp/link" >/dev/null; then
    echo "test-capi.sh: linking for wasm needs exception handling or C++:" >&2
    cat "$tmp/link" >&2
    exit 1
  fi
  # shellcheck disable=SC2086 # word splitting is intended
  $cc $cflags -O2 -o "$tmp/test.wasm" "$tests/test.c" "$lib"
  ${WASMTIME:-wasmtime} run "$tmp/test.wasm"
}

case $mode in
  native)
    run_native ""
    check_symbols
    ;;
  tsan) run_native "-fsanitize=thread" ;;
  asan) run_native "-fsanitize=address -fno-omit-frame-pointer" ;;
  wasm) run_wasm ;;
  *)
    echo "usage: scripts/test-capi.sh [native|tsan|asan|wasm]" >&2
    exit 2
    ;;
esac
