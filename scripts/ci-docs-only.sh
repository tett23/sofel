#!/bin/bash
# ADR 0032 and ADR 0034: classify a Git event using the base-side rules.
set -euo pipefail
LC_ALL=C
export LC_ALL
shopt -u extglob
cd "${CI_REPO_DIR:-$(dirname "$0")/..}"
result() {
  printf 'docs_only=%s\n' "$1"
  if [[ -n ${GITHUB_OUTPUT:-} ]]; then printf 'docs_only=%s\n' "$1" >>"$GITHUB_OUTPUT"; fi
  if [[ $# -ge 2 ]]; then printf 'ci-docs-only: %s\n' "$2" >&2; fi
}
zero=0000000000000000000000000000000000000000
head=${CI_HEAD_SHA:-}
base=${CI_BASE_SHA:-}
if [[ -z $head || $head == "$zero" ]]; then result false 'missing head'; exit 0; fi
case ${CI_EVENT_NAME:-} in
  pull_request)
    if [[ -z $base || $base == "$zero" ]]; then result false 'missing base'; exit 0; fi
    if ! start=$(git merge-base "$base" "$head" 2>/dev/null); then
      result false 'merge-base unavailable'; exit 0
    fi ;;
  push)
    start=${CI_BEFORE_SHA:-}
    if [[ -z $start || $start == "$zero" ]]; then result false 'missing before'; exit 0; fi
    if ! git cat-file -e "$start^{commit}" 2>/dev/null; then
      result false 'before unavailable'; exit 0
    fi ;;
  *) result false 'unknown event'; exit 0 ;;
esac
if ! git cat-file -e "$head^{commit}" 2>/dev/null; then result false 'head unavailable'; exit 0; fi
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
if ! git show "$start:ci/documentation-only.rules" >"$tmp/rules" 2>/dev/null; then
  result false 'base rules missing'; exit 0
fi
if ! git diff --no-renames --name-only -z "$start" "$head" >"$tmp/paths"; then
  echo 'ci-docs-only: git diff failed' >&2; exit 1
fi
if [[ ! -s $tmp/paths ]]; then result false 'empty diff'; exit 0; fi
declare -a kinds=() patterns=() seen=()
while IFS= read -r line || [[ -n $line ]]; do
  [[ -z $line || $line == \#* ]] && continue
  if [[ $line != *$'\t'* ]]; then result false 'invalid rule columns'; exit 0; fi
  kind=${line%%$'\t'*}
  pattern=${line#*$'\t'}
  if [[ $pattern == *$'\t'* || -z $pattern || $pattern == /* || $pattern == . || $pattern == .. || /$pattern/ == */./* || /$pattern/ == */../* ]]; then
    result false 'invalid rule path'; exit 0
  fi
  if [[ $kind != document && $kind != input ]]; then result false 'invalid rule role'; exit 0; fi
  for old in "${seen[@]}"; do
    if [[ $old == "$line" ]]; then result false 'duplicate rule'; exit 0; fi
  done
  seen+=("$line")
  [[ $pattern == */ ]] && pattern+='*'
  kinds+=("$kind") patterns+=("$pattern")
done <"$tmp/rules"
while IFS= read -r -d '' path; do
  if [[ $path == ci/documentation-only.rules ]]; then result false 'rules changed'; exit 0; fi
  document=false input=false
  for ((i=0; i<${#patterns[@]}; i++)); do
    case "$path" in
      ${patterns[i]})
        if [[ ${kinds[i]} == input ]]; then input=true; else document=true; fi ;;
    esac
  done
  if [[ $document != true || $input == true ]]; then result false "input or unknown: $path"; exit 0; fi
done <"$tmp/paths"
result true
