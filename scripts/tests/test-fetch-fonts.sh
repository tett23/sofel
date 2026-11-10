#!/bin/sh
# Local fixtures for the fixed extra-font corpus (ADR 0031, I01-I04).
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
script=$root/scripts/fetch-fonts.sh
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel-fonts.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }
reject() { if SOFEL_FONTS_OFFLINE=1 sh "$script" "$tmp/fonts.lock" "$tmp/fonts.tsv" "$tmp/cache" >/dev/null 2>&1; then fail "$1"; fi; }

mkdir -p "$tmp/cache/licenses" "$tmp/cache/fonts"
printf font > "$tmp/cache/fonts/one.ttf"
printf license > "$tmp/cache/licenses/one.txt"
font_sha=$(shasum -a 256 "$tmp/cache/fonts/one.ttf" | cut -d ' ' -f 1)
license_sha=$(shasum -a 256 "$tmp/cache/licenses/one.txt" | cut -d ' ' -f 1)
printf 'one\thttps://example.invalid/one.ttf\t%s\t4\tfonts/one.ttf\nlicense\thttps://example.invalid/OFL.txt\t%s\t7\tlicenses/one.txt\n' "$font_sha" "$license_sha" > "$tmp/fonts.lock"
printf 'one\tone\t-\tfonts/one.ttf\t%s\tlicense\tshape\n' "$font_sha" > "$tmp/fonts.tsv"
SOFEL_FONTS_OFFLINE=1 sh "$script" "$tmp/fonts.lock" "$tmp/fonts.tsv" "$tmp/cache" >/dev/null || fail 'I01 valid font and license'

mv "$tmp/cache/licenses/one.txt" "$tmp/license-backup"
reject 'I02 missing license'
mv "$tmp/license-backup" "$tmp/cache/licenses/one.txt"
printf bad > "$tmp/cache/fonts/one.ttf"
reject 'I02 corrupt font'
printf font > "$tmp/cache/fonts/one.ttf"

printf 'two\thttps://example.invalid/two.ttf\t%s\t4\tfonts/two.ttf\n' "$font_sha" >> "$tmp/fonts.lock"
reject 'I04 duplicate hash'
sed -i.bak '$d' "$tmp/fonts.lock"
printf '' > "$tmp/fonts.tsv"
reject 'I01 empty manifest'
printf 'one\tone\t-\tfonts/one.ttf\t%s\tlicense\tshape\n' "$font_sha" > "$tmp/fonts.tsv"

printf 'one\tone\t../outside\tfonts/one.ttf\t%s\tlicense\tshape\n' "$font_sha" > "$tmp/fonts.tsv"
reject 'I03 escaping archive member'
printf 'one\tone\t-\t../outside\t%s\tlicense\tshape\n' "$font_sha" > "$tmp/fonts.tsv"
reject 'I03 escaping destination'
printf 'one\tone\t-\tfonts/one.ttf\t%s\tlicense\tshape\n' "$font_sha" > "$tmp/fonts.tsv"
rm "$tmp/cache/fonts/one.ttf"
ln -s "$tmp/license-backup" "$tmp/cache/fonts/one.ttf"
reject 'I03 symlink in destination'

rm "$tmp/cache/fonts/one.ttf"
mkdir "$tmp/zip-src"
printf font > "$tmp/zip-src/inside.ttf"
(cd "$tmp/zip-src" && zip -q "$tmp/cache/archive.zip" inside.ttf)
archive_sha=$(shasum -a 256 "$tmp/cache/archive.zip" | cut -d ' ' -f 1)
archive_bytes=$(wc -c < "$tmp/cache/archive.zip" | tr -d ' ')
printf 'archive\thttps://example.invalid/archive.zip\t%s\t%s\tarchive.zip\nlicense\thttps://example.invalid/OFL.txt\t%s\t7\tlicenses/one.txt\n' "$archive_sha" "$archive_bytes" "$license_sha" > "$tmp/fonts.lock"
printf 'one\tarchive\tinside.ttf\tfonts/extracted.ttf\t%s\tlicense\tshape\n' "$font_sha" > "$tmp/fonts.tsv"
SOFEL_FONTS_OFFLINE=1 sh "$script" "$tmp/fonts.lock" "$tmp/fonts.tsv" "$tmp/cache" >/dev/null || fail 'I03 fixed archive member'
[ "$(shasum -a 256 "$tmp/cache/fonts/extracted.ttf" | cut -d ' ' -f 1)" = "$font_sha" ] || fail 'I03 extracted hash'

ln -s inside.ttf "$tmp/zip-src/link.ttf"
(cd "$tmp/zip-src" && zip -y -q "$tmp/cache/symlink.zip" link.ttf)
archive_sha=$(shasum -a 256 "$tmp/cache/symlink.zip" | cut -d ' ' -f 1)
archive_bytes=$(wc -c < "$tmp/cache/symlink.zip" | tr -d ' ')
printf 'archive\thttps://example.invalid/symlink.zip\t%s\t%s\tsymlink.zip\nlicense\thttps://example.invalid/OFL.txt\t%s\t7\tlicenses/one.txt\n' "$archive_sha" "$archive_bytes" "$license_sha" > "$tmp/fonts.lock"
printf 'one\tarchive\tlink.ttf\tfonts/link.ttf\t%s\tlicense\tshape\n' "$font_sha" > "$tmp/fonts.tsv"
reject 'I03 symlink archive member'

echo 'ok: fetch-fonts.sh I01-I04 (10 cases)'
