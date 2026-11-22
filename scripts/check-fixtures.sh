#!/bin/sh
# Check the tracked synthetic font fixtures (ADR 0088).
set -eu
cd "${1:-$(git rev-parse --show-toplevel)}"
files=$(mktemp)
trap 'rm -f "$files"' EXIT HUP INT TERM
git -c core.quotePath=false ls-files -- \
  ':(icase)*.ttf' ':(icase)*.otf' ':(icase)*.ttc' ':(icase)*.otc' ':(icase)*.bdf' ':(icase)*.pcf' ':(icase)*.fon' ':(icase)*.fnt' ':(icase)*.pfb' ':(icase)*.pfa' ':(icase)*.pfm' ':(icase)*.afm' ':(icase)*.pfr' ':(icase)*.woff' ':(icase)*.woff2' ':(icase)*.t42' ':(icase)*.cff' ':(icase)*.dfont' >"$files"
status=0
while IFS= read -r file; do
  if ! printf '%s\n' "$file" | grep -E '^crates/[^/]+/tests/(data|support)/' >/dev/null; then
    echo "$file: font fixture outside allowed test directories" >&2
    status=1
  fi
  readme=${file%/*}/README.md
  if [ ! -f "$readme" ] || ! grep -Fx -- "## ${file##*/}" "$readme" >/dev/null; then
    echo "$file: missing filename heading in $readme" >&2
    status=1
  fi
done <"$files"
exit "$status"
