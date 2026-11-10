#!/bin/bash
# ADR 0032 I01-I03 and ADR 0034 I01-I03.
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
  local want=$1 got
  got=$(run)
  [[ $got == "docs_only=$want" ]] || { echo "expected $want, got $got" >&2; exit 1; }
}
expect false # Empty difference (I03).
printf 'more\n' >>docs/adr/one.md
git add . && git commit -qm document
expect true
printf 'code\n' >crates/code.rs
git add . && git commit -qm code
expect false
git reset -q --hard HEAD~1
git rm -q docs/adr/one.md
git commit -qm delete-document
expect true
git reset -q --hard HEAD~1
mkdir -p crates
git mv docs/adr/one.md crates/moved.md
git commit -qm move
expect false
git reset -q --hard HEAD~1
printf 'change\n' >>docs/ci-fixtures/input.txt
git add . && git commit -qm input
expect false
git reset -q --hard HEAD~1
printf 'input\tdocs/adr/one.md\n' >>ci/documentation-only.rules
git add . && git commit -qm rule
expect false
CI_REPO_DIR="$tmp" CI_EVENT_NAME=push CI_BEFORE_SHA=0000000000000000000000000000000000000000 CI_HEAD_SHA="$(git rev-parse HEAD)" \
  bash "$root/scripts/ci-docs-only.sh" | grep -qx 'docs_only=false'
git reset -q --hard "$base"
printf 'line\n' >"docs/adr/space and*pattern.md"
printf 'line\n' >"docs/adr/line
break.md"
git add . && git commit -qm special-paths
expect true
CI_REPO_DIR="$tmp" CI_EVENT_NAME=push CI_BEFORE_SHA="$base" CI_HEAD_SHA="$(git rev-parse HEAD)" \
  bash "$root/scripts/ci-docs-only.sh" | grep -qx 'docs_only=true'
git reset -q --hard "$base"
for n in $(seq 1 301); do printf 'x\n' >"docs/adr/$n.md"; done
mkdir -p crates
printf 'code\n' >crates/code.rs
git add . && git commit -qm many-paths
expect false
git reset -q --hard "$base"
# A rule committed at the comparison base is used; input wins in either order.
printf 'input\tdocs/ci-fixtures/input.txt\ndocument\tdocs/\n' >ci/documentation-only.rules
git add . && git commit -qm reordered-base-rules
base=$(git rev-parse HEAD)
printf 'change\n' >>docs/ci-fixtures/input.txt
git add . && git commit -qm changed-input
expect false
git reset -q --hard "$base"
# Directory, question mark and bracket patterns use Bash's path matching.
printf 'document\tdocs/adr/\ndocument\tdocs/q?.md\ndocument\tdocs/[ab].md\n' >ci/documentation-only.rules
git add . && git commit -qm glob-base-rules
base=$(git rev-parse HEAD)
printf 'x\n' >docs/q1.md
printf 'x\n' >docs/a.md
git add . && git commit -qm matched-globs
expect true
git reset -q --hard "$base"
printf 'x\n' >docs/q12.md
git add . && git commit -qm unmatched-glob
expect false
git reset -q --hard "$base"
printf 'document docs/\n' >ci/documentation-only.rules
git add . && git commit -qm invalid-base-rules
base=$(git rev-parse HEAD)
printf 'x\n' >>docs/adr/one.md
git add . && git commit -qm document-with-invalid-rules
expect false
echo 'ok: ADR 0032 I01-I03 and ADR 0034 I01-I03'
