#!/bin/sh
# ADR 0032 I01-I03 and ADR 0034 I01-I03.
[ -n "${BASH_VERSION:-}" ] || exec bash "$0" "$@"
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"
git init -q
git config user.name CI
git config user.email ci@example.invalid
mkdir -p docs/adr docs/ci-fixtures crates
mkdir -p ci
printf 'document\tdocs/\ndocument\tREADME.md\ninput\tdocs/ci-fixtures/input.txt\n' >ci/documentation-only.rules
printf 'base\n' >docs/adr/one.md
cp "$root/docs/ci-fixtures/input.txt" docs/ci-fixtures/input.txt
git add . && git commit -qm base
base=$(git rev-parse HEAD)
run() {
  CI_REPO_DIR="$tmp" CI_EVENT_NAME=pull_request CI_BASE_SHA="$base" CI_HEAD_SHA="$(git rev-parse HEAD)" \
    bash "$root/scripts/ci-docs-only.sh"
}
expect() {
  local want=$1 name=$2 got
  got=$(run)
  [[ $got == "docs_only=$want" ]] || { echo "$name: expected $want, got $got" >&2; exit 1; }
}
expect false I03_empty_difference
printf 'more\n' >>docs/adr/one.md
git add . && git commit -qm document
expect true I01_document
printf 'code\n' >crates/code.rs
git add . && git commit -qm code
expect false I01_code_after_document
git reset -q --hard HEAD~1
git rm -q docs/adr/one.md
git commit -qm delete-document
expect true I01_deleted_document
git reset -q --hard HEAD~1
mkdir -p crates
git mv docs/adr/one.md crates/moved.md
git commit -qm move
expect false I02_move_to_code
git reset -q --hard HEAD~1
printf 'change\n' >>docs/ci-fixtures/input.txt
git add . && git commit -qm input
expect false I02_input_priority
git reset -q --hard HEAD~1
printf 'input\tdocs/adr/one.md\n' >>ci/documentation-only.rules
git add . && git commit -qm rule
expect false I03_rule_change
CI_REPO_DIR="$tmp" CI_EVENT_NAME=push CI_BEFORE_SHA=0000000000000000000000000000000000000000 CI_HEAD_SHA="$(git rev-parse HEAD)" \
  bash "$root/scripts/ci-docs-only.sh" | grep -qx 'docs_only=false' || { echo I03_zero_before >&2; exit 1; }
git reset -q --hard "$base"
printf 'line\n' >"docs/adr/space and*pattern.md"
printf 'line\n' >"docs/adr/line
break.md"
git add . && git commit -qm special-paths
expect true I01_special_paths
CI_REPO_DIR="$tmp" CI_EVENT_NAME=push CI_BEFORE_SHA="$base" CI_HEAD_SHA="$(git rev-parse HEAD)" \
  bash "$root/scripts/ci-docs-only.sh" | grep -qx 'docs_only=true' || { echo I01_push_document >&2; exit 1; }
git reset -q --hard "$base"
for n in $(seq 1 301); do printf 'x\n' >"docs/adr/$n.md"; done
mkdir -p crates
printf 'code\n' >crates/code.rs
git add . && git commit -qm many-paths
expect false I01_more_than_300_paths
git reset -q --hard "$base"
# A rule committed at the comparison base is used; input wins in either order.
printf 'input\tdocs/ci-fixtures/input.txt\ndocument\tdocs/\n' >ci/documentation-only.rules
git add . && git commit -qm reordered-base-rules
base=$(git rev-parse HEAD)
printf 'change\n' >>docs/ci-fixtures/input.txt
git add . && git commit -qm changed-input
expect false I02_input_before_document
git reset -q --hard "$base"
# Directory, question mark and bracket patterns use Bash's path matching.
printf 'document\tdocs/adr/\ndocument\tdocs/q?.md\ndocument\tdocs/[ab].md\n' >ci/documentation-only.rules
git add . && git commit -qm glob-base-rules
base=$(git rev-parse HEAD)
printf 'x\n' >docs/q1.md
printf 'x\n' >docs/a.md
git add . && git commit -qm matched-globs
expect true I01_directory_question_bracket
git reset -q --hard "$base"
printf 'x\n' >docs/q12.md
git add . && git commit -qm unmatched-glob
expect false I03_unmatched_question
git reset -q --hard "$base"
printf 'document\tdocs/@(x|y).md\n' >>ci/documentation-only.rules
git add . && git commit -qm extglob-base-rules
base=$(git rev-parse HEAD)
printf 'x\n' >docs/x.md
git add . && git commit -qm extglob-lookalike
expect false I03_extglob_disabled
git reset -q --hard "$base"
printf 'document docs/\n' >ci/documentation-only.rules
git add . && git commit -qm invalid-base-rules
base=$(git rev-parse HEAD)
printf 'x\n' >>docs/adr/one.md
git add . && git commit -qm document-with-invalid-rules
expect false I03_invalid_rule_columns
echo 'ok: ADR 0032 I01-I03 and ADR 0034 I01-I03'
