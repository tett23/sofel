#!/bin/sh
# ADR 0032 I5: committed ADRs and audit/benchmark records stay protected.
set -eu
guard=${ADR_GUARD_BIN:-adr-guard}
command -v "$guard" >/dev/null 2>&1 || { echo 'adr-guard is required' >&2; exit 1; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"
git init -q
git config user.name CI
git config user.email ci@example.invalid
mkdir -p docs/adr docs/audit docs/audits docs/benchmark docs/benchmarks
printf '# ADR 0001: Test\n\nStatus: proposed\n' >docs/adr/0001-test.md
for dir in audit audits benchmark benchmarks; do printf 'record\n' >"docs/$dir/one.md"; done
git add . && git -c core.hooksPath=/dev/null commit -qm base
base=$(git rev-parse HEAD)
check() { "$guard" check-range --allow-status --allow-links "$base" HEAD >/dev/null 2>&1; }
printf '# ADR 0001: Test\n\nStatus: accepted\n' >docs/adr/0001-test.md
git add . && git -c core.hooksPath=/dev/null commit -qm status
check || { echo 'ADR 0032 I5: ADR status update rejected' >&2; exit 1; }
git reset -q --hard "$base"
printf 'body change\n' >>docs/adr/0001-test.md
git add . && git -c core.hooksPath=/dev/null commit -qm adr-body
if check; then echo 'ADR 0032 I5: changed ADR body accepted' >&2; exit 1; fi
git reset -q --hard "$base"
git rm -q docs/adr/0001-test.md
git -c core.hooksPath=/dev/null commit -qm adr-delete
if check; then echo 'ADR 0032 I5: deleted ADR accepted' >&2; exit 1; fi
git reset -q --hard "$base"
git mv docs/adr/0001-test.md docs/adr/0001-moved.md
git -c core.hooksPath=/dev/null commit -qm adr-move
if check; then echo 'ADR 0032 I5: moved ADR accepted' >&2; exit 1; fi
git reset -q --hard "$base"
printf 'new\n' >docs/audit/two.md
git add . && git -c core.hooksPath=/dev/null commit -qm add
check || { echo 'ADR 0032 I5: new record rejected' >&2; exit 1; }
git reset -q --hard "$base"
for dir in audit audits benchmark benchmarks; do
  printf 'changed\n' >>"docs/$dir/one.md"
  git add . && git -c core.hooksPath=/dev/null commit -qm change
  if check; then echo "ADR 0032 I5: changed $dir accepted" >&2; exit 1; fi
  git reset -q --hard "$base"
  git rm -q "docs/$dir/one.md"
  git -c core.hooksPath=/dev/null commit -qm delete
  if check; then echo "ADR 0032 I5: deleted $dir accepted" >&2; exit 1; fi
  git reset -q --hard "$base"
  git mv "docs/$dir/one.md" "docs/$dir/moved.md"
  git -c core.hooksPath=/dev/null commit -qm move
  if check; then echo "ADR 0032 I5: moved $dir accepted" >&2; exit 1; fi
  git reset -q --hard "$base"
done
echo 'ADR 0032 I5: record protection'
