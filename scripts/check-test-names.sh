#!/bin/sh
# Validate invariant references in test names (ADR 0089).
set -eu
script_dir=$(cd "$(dirname "$0")" && pwd)
cd "${1:-$(git rev-parse --show-toplevel)}"
index=$(mktemp)
files=$(mktemp)
trap 'rm -f "$index" "$files"' EXIT HUP INT TERM
awk '
  FNR == 1 { adr = FILENAME; sub(/^.*\//, "", adr); adr = substr(adr, 1, 4); section = "" }
  /^## / { section = "" }
  /^## 不変条件$/ { section = "I" }
  /^## 受けいれ条件$/ { section = "A" }
  section != "" && /^- / {
    label = $0; sub(/^- /, "", label); gsub(/[*`]/, "", label)
    if (match(label, /^[IA][1-9][0-9]*/)) print adr, substr(label, RSTART, RLENGTH)
  }
' docs/adr/*.md >"$index"
for directory in crates tools scripts/tests; do
  if [ -d "$directory" ]; then
    find "$directory" -type d -name target -prune -o -type f \( -name '*.rs' -o -name '*.sh' \) -print
  fi
done >"$files"
awk -v index_file="$index" -v files_file="$files" -v exceptions_file=scripts/test-name-exceptions.tsv \
  -f "$script_dir/check-test-names.awk"
