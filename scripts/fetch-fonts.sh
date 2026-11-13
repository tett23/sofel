#!/bin/sh
# Fetch and verify the fixed extra-font corpus (ADR 0031).
# Usage: scripts/fetch-fonts.sh [LOCK [MANIFEST [DEST]]]
set -eu
LC_ALL=C
export LC_ALL

root=$(cd "$(dirname "$0")/.." && pwd)
lock=${1:-$root/fonts.lock}
manifest=${2:-$root/conformance/fonts.tsv}
dest=${3:-$root/target/testdata/extra-fonts}
offline=${SOFEL_FONTS_OFFLINE:-0}
tab=$(printf '\t')

fail() { echo "fetch-fonts.sh: $*" >&2; exit 1; }
safe_path() {
  case "$1" in ''|/*|.|..|../*|*/../*|*/..|./*|*/./*|*/.|*'//'*|*\\*) fail "unsafe path: $1";; esac
}
safe_id() {
  case "$1" in ''|*[!a-zA-Z0-9_-]*) fail "invalid id: $1";; esac
}
safe_dest() {
  _safe_rel=$1
  safe_path "$_safe_rel"
  current=$dest
  old_ifs=$IFS
  IFS=/
  set -f
  for component in $_safe_rel; do
    current=$current/$component
    [ ! -L "$current" ] || fail "symlink in destination: $current"
  done
  set +f
  IFS=$old_ifs
}
sha() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
verify() {
  [ -f "$1" ] && [ ! -L "$1" ] || return 1
  [ "$(wc -c < "$1" | tr -d ' ')" = "$3" ] && [ "$(sha "$1")" = "$2" ]
}

[ -f "$lock" ] || fail "missing lock: $lock"
[ -f "$manifest" ] || fail "missing manifest: $manifest"
awk -F '\t' '
  /^[#]/ || /^$/ { next }
  NF != 5 || $1 == "" || $2 == "" || $3 !~ /^[0-9a-f]{64}$/ || $4 !~ /^[0-9]+$/ || $5 == "" { bad=1; next }
  { if (seen[$1]++ || hash[$3]++ || path[$5]++) bad=1 }
  { n++ }
  END { if (bad || n == 0) exit 1 }
' "$lock" || fail 'invalid fonts.lock (columns, duplicate ID/hash/path, or empty)'
awk -F '\t' '
  /^[#]/ || /^$/ { next }
  NF != 7 || $1 == "" || $2 == "" || $3 == "" || $4 == "" || $5 !~ /^[0-9a-f]{64}$/ || $6 == "" || $7 == "" { bad=1; next }
  { if (seen[$1]++ || hash[$5]++ || path[$4]++) bad=1 }
  { n++ }
  END { if (bad || n == 0) exit 1 }
' "$manifest" || fail 'invalid fonts.tsv (columns, duplicate ID, or empty)'

[ ! -L "$dest" ] || fail "symlink destination: $dest"
mkdir -p "$dest"
stage=$(mktemp -d "$dest/.fetch.XXXXXX")
trap 'rm -rf "$stage"' EXIT HUP INT TERM

while IFS="$tab" read -r id url digest bytes rel || [ -n "${id:-}" ]; do
  case "$id" in ''|'#'*) continue;; esac
  safe_id "$id"
  safe_dest "$rel"
  case "$url" in https://*) :;; *) fail "non-HTTPS URL: $id";; esac
  case "$bytes" in ''|*[!0-9]*) fail "invalid byte count: $id";; esac
  file=$dest/$rel
  if verify "$file" "$digest" "$bytes"; then
    echo "$id: verified"
    continue
  fi
  [ "$offline" != 1 ] || fail "$id missing or corrupt (offline)"
  [ ! -L "$file" ] || fail "symlink cache: $file"
  rm -f "$file"
  curl --fail --location --silent --show-error --retry 3 --output "$stage/$id" "$url" || fail "fetch failed: $id"
  verify "$stage/$id" "$digest" "$bytes" || fail "size or SHA-256 mismatch: $id"
  mkdir -p "$(dirname "$file")"
  mv -f "$stage/$id" "$file"
  echo "$id: fetched and verified"
done < "$lock"

while IFS="$tab" read -r id source member rel digest license features || [ -n "${id:-}" ]; do
  case "$id" in ''|'#'*) continue;; esac
  safe_id "$id"
  safe_id "$source"
  safe_id "$license"
  safe_dest "$rel"
  if [ "$member" != - ]; then safe_path "$member"; fi
  case "$features" in ''|,*|*,|*,,*) fail "invalid features: $id";; esac
  old_ifs=$IFS; IFS=,
  for feature in $features; do
    case "$feature" in shape|rtl|indic|use|variable|cff|cff2|math|ttc|cjk|vertical|avar2|colr|cbdt|sbix|svg|metrics|outline) :;; *) fail "unknown feature: $id/$feature";; esac
  done
  IFS=$old_ifs
  source_rel=$(awk -F '\t' -v id="$source" '$1==id { print $5 }' "$lock")
  license_rel=$(awk -F '\t' -v id="$license" '$1==id { print $5 }' "$lock")
  [ -n "$source_rel" ] && [ -n "$license_rel" ] || fail "missing source/license record: $id"
  safe_dest "$source_rel"; safe_dest "$license_rel"
  [ -f "$dest/$license_rel" ] && [ ! -L "$dest/$license_rel" ] || fail "missing license: $id"
  if [ "$member" = - ]; then
    [ "$rel" = "$source_rel" ] || fail "direct font path differs from source: $id"
  else
    [ "$(unzip -Z1 "$dest/$source_rel" | grep -x -F "$member" | wc -l | tr -d ' ')" = 1 ] || fail "archive member not unique: $id"
    mode=$(zipinfo -l "$dest/$source_rel" "$member" | awk 'NR == 1 { print substr($1,1,1) }')
    [ "$mode" = - ] || fail "archive member is not a regular file: $id"
    if [ ! -f "$dest/$rel" ] || [ -L "$dest/$rel" ] || [ "$(sha "$dest/$rel")" != "$digest" ]; then
      [ ! -L "$dest/$rel" ] || fail "symlink font: $id"
      rm -f "$dest/$rel"
      unzip -p "$dest/$source_rel" "$member" > "$stage/$id" || fail "extract failed: $id"
      [ "$(sha "$stage/$id")" = "$digest" ] || fail "extracted SHA-256 mismatch: $id"
      mkdir -p "$(dirname "$dest/$rel")"
      mv -f "$stage/$id" "$dest/$rel"
    fi
  fi
  [ "$(sha "$dest/$rel")" = "$digest" ] || fail "font SHA-256 mismatch: $id"
  echo "$id: registered ($features)"
done < "$manifest"
