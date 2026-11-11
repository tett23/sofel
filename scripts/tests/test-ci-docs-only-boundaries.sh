#!/bin/sh
# ADR 0032 I01/I03 and ADR 0034 I01/I03: base and malformed-rule boundaries.
[ -n "${BASH_VERSION:-}" ] || exec bash "$0" "$@"
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"
git init -q
git config user.name CI
git config user.email ci@example.invalid
mkdir -p ci docs
cp "$root/ci/documentation-only.rules" ci/documentation-only.rules
printf 'a\n' >docs/a.md
git add . && git commit -qm base
original=$(git rev-parse HEAD)
check() {
  local want=$1 name=$2 base=$3 head=$4 got
  got=$(CI_REPO_DIR="$tmp" CI_EVENT_NAME=pull_request CI_BASE_SHA="$base" CI_HEAD_SHA="$head" \
    bash "$root/scripts/ci-docs-only.sh")
  [[ $got == "docs_only=$want" ]] || { echo "$name: expected $want, got $got" >&2; exit 1; }
}
printf 'readme\n' >README
printf 'contributing\n' >CONTRIBUTING.md
git add . && git commit -qm root-documents
check true I01_readme_contributing "$original" HEAD
git reset -q --hard "$original"
for invalid in \
  $'unknown\tdocs/\n' \
  $'document\t\n' \
  $'document\t/absolute\n' \
  $'document\tdocs/./a\n' \
  $'document\tdocs/../a\n' \
  $'document\tdocs/\textra\n' \
  $'document\tdocs/\ndocument\tdocs/\n'; do
  printf '%s' "$invalid" >ci/documentation-only.rules
  git add . && git commit -qm invalid-rules
  base=$(git rev-parse HEAD)
  printf 'change\n' >>docs/a.md
  git add . && git commit -qm document
  check false I03_invalid_rules "$base" HEAD
  git reset -q --hard "$original"
done
git rm -q ci/documentation-only.rules
git commit -qm missing-rules
base=$(git rev-parse HEAD)
printf 'change\n' >>docs/a.md
git add . && git commit -qm document
check false I03_missing_rules "$base" HEAD
git reset -q --hard "$original"
git checkout -qb feature
printf 'feature\n' >>docs/a.md
git add . && git commit -qm feature
feature=$(git rev-parse HEAD)
git checkout -qb advanced-base "$original"
printf 'code\n' >code.rs
git add . && git commit -qm advanced-base
check true I01_pull_request_merge_base HEAD "$feature"
git checkout -q feature
mkdir -p bin
real_git=$(command -v git)
cat >bin/git <<'EOF'
#!/bin/sh
if [ "$1" = diff ]; then exit 17; fi
exec "$REAL_GIT" "$@"
EOF
chmod +x bin/git
if CI_REPO_DIR="$tmp" CI_EVENT_NAME=pull_request CI_BASE_SHA="$original" CI_HEAD_SHA="$feature" \
  REAL_GIT="$real_git" PATH="$tmp/bin:$PATH" bash "$root/scripts/ci-docs-only.sh" >/dev/null 2>&1; then
  echo 'I03_git_diff_failure: accepted failed diff' >&2
  exit 1
fi
echo 'ok: ADR 0032 I01/I03 and ADR 0034 I01/I03 boundaries'
