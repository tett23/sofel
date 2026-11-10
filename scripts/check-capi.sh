#!/bin/sh
# Check that every public function of the library crates has a C
# counterpart or is excluded on purpose (ADR 0015).
#
# Usage: scripts/check-capi.sh [HEADER [UNMAPPED [API]]]
#   HEADER    default: crates/sofel-capi/include/sofel.h
#   UNMAPPED  default: crates/sofel-capi/unmapped.txt
#   API       a file with the public functions, one path per line;
#             default: the output of scripts/public-api.sh
#
# A function is covered when a "Rust:" line of HEADER names it (paths
# separated by ", "; "none" for C functions without a Rust counterpart),
# or when UNMAPPED lists it (`<path> <reason>`, `<prefix>::*` for all
# under a prefix). It also fails for "Rust:" paths and UNMAPPED entries
# of the checked crates that name no public function, and for UNMAPPED
# entries without a reason.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
header=${1:-$root/crates/sofel-capi/include/sofel.h}
unmapped=${2:-$root/crates/sofel-capi/unmapped.txt}
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

if [ $# -ge 3 ]; then
  sort -u "$3" >"$tmp/api"
else
  sh "$root/scripts/public-api.sh" >"$tmp/api"
fi

sed -n 's/^ \* Rust: //p' "$header" | tr ',' '\n' | tr -d ' ' |
  grep -v -x -e none -e '' | sort -u >"$tmp/mapped" || true
sed -e 's/#.*//' "$unmapped" | grep -v '^[[:space:]]*$' >"$tmp/unmapped" || true
cut -d ' ' -f 1 "$tmp/unmapped" | sort -u >"$tmp/excluded"
cut -d : -f 1 "$tmp/api" | sort -u >"$tmp/crates"

status=0
fail() {
  echo "check-capi.sh: $*" >&2
  status=1
}

while read -r line; do
  case $line in
    *" "?*) ;;
    *) fail "$unmapped: no reason given for $line" ;;
  esac
done <"$tmp/unmapped"

grep '::\*$' "$tmp/excluded" | sed 's/\*$//' >"$tmp/prefixes" || true
while read -r f; do
  grep -qxF "$f" "$tmp/mapped" && continue
  grep -qxF "$f" "$tmp/excluded" && continue
  covered=0
  while read -r p; do
    case $f in "$p"*) covered=1 ;; esac
  done <"$tmp/prefixes"
  [ $covered = 1 ] && continue
  fail "$f has no C function (add one to sofel.h with a \"Rust:\" line, or list it in $unmapped with a reason)"
done <"$tmp/api"

# Stale names: only for crates whose functions are checked.
grep -v '::\*$' "$tmp/excluded" | cat "$tmp/mapped" - | sort -u >"$tmp/named"
while read -r f; do
  grep -qxF "${f%%::*}" "$tmp/crates" || continue
  grep -qxF "$f" "$tmp/api" || fail "$f is named but is not a public function"
done <"$tmp/named"
while read -r p; do
  grep -q "^$p" "$tmp/api" || fail "${p}* matches no public function"
done <"$tmp/prefixes"

[ $status = 0 ] && echo "ok: $(wc -l <"$tmp/api" | tr -d ' ') public functions covered"
exit $status
