#!/bin/sh
# root ADR 0090 I1, I2: a verified tar archive yields only listed regular members.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel-legacy-fonts.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
fail() { echo "FAIL: $*" >&2; exit 1; }
reject() { if SOFEL_FONTS_OFFLINE=1 sh "$root/scripts/fetch-fonts.sh" "$tmp/fonts.lock" "$tmp/fonts.tsv" "$tmp/cache" >/dev/null 2>&1; then fail "$1"; fi; }

mkdir -p "$tmp/archive" "$tmp/cache/archives" "$tmp/cache/licenses"
printf font > "$tmp/archive/inside.bdf"
printf hidden > "$tmp/archive/unlisted.bdf"
printf license > "$tmp/cache/licenses/LICENSE"
tar -czf "$tmp/cache/archives/fonts.tar.gz" -C "$tmp/archive" inside.bdf unlisted.bdf
archive_sha=$(shasum -a 256 "$tmp/cache/archives/fonts.tar.gz" | cut -d ' ' -f 1)
archive_bytes=$(wc -c < "$tmp/cache/archives/fonts.tar.gz" | tr -d ' ')
font_sha=$(shasum -a 256 "$tmp/archive/inside.bdf" | cut -d ' ' -f 1)
license_sha=$(shasum -a 256 "$tmp/cache/licenses/LICENSE" | cut -d ' ' -f 1)
printf 'archive\thttps://example.invalid/fonts.tar.gz\t%s\t%s\tarchives/fonts.tar.gz\nlicense\thttps://example.invalid/LICENSE\t%s\t7\tlicenses/LICENSE\n' "$archive_sha" "$archive_bytes" "$license_sha" > "$tmp/fonts.lock"
printf 'sample\tarchive\tinside.bdf\tfonts/inside.bdf\t%s\tlicense\tlegacy\n' "$font_sha" > "$tmp/fonts.tsv"
SOFEL_FONTS_OFFLINE=1 sh "$root/scripts/fetch-fonts.sh" "$tmp/fonts.lock" "$tmp/fonts.tsv" "$tmp/cache" >/dev/null || fail 'valid archive'
[ -f "$tmp/cache/fonts/inside.bdf" ] || fail 'listed member absent'
[ ! -e "$tmp/cache/fonts/unlisted.bdf" ] || fail 'unlisted member extracted'

printf corrupt > "$tmp/cache/archives/fonts.tar.gz"
reject 'corrupt archive accepted'
[ ! -e "$tmp/cache/fonts/unlisted.bdf" ] || fail 'corrupt archive extracted'
tar -czf "$tmp/cache/archives/fonts.tar.gz" -C "$tmp/archive" inside.bdf unlisted.bdf

printf 'sample\tarchive\t../outside\tfonts/inside.bdf\t%s\tlicense\tlegacy\n' "$font_sha" > "$tmp/fonts.tsv"
reject 'escaping member accepted'
printf 'sample\tarchive\tmissing.bdf\tfonts/inside.bdf\t%s\tlicense\tlegacy\n' "$font_sha" > "$tmp/fonts.tsv"
reject 'missing member accepted'

ln -s inside.bdf "$tmp/archive/link.bdf"
tar -czf "$tmp/cache/archives/fonts.tar.gz" -C "$tmp/archive" link.bdf
archive_sha=$(shasum -a 256 "$tmp/cache/archives/fonts.tar.gz" | cut -d ' ' -f 1)
archive_bytes=$(wc -c < "$tmp/cache/archives/fonts.tar.gz" | tr -d ' ')
printf 'archive\thttps://example.invalid/fonts.tar.gz\t%s\t%s\tarchives/fonts.tar.gz\nlicense\thttps://example.invalid/LICENSE\t%s\t7\tlicenses/LICENSE\n' "$archive_sha" "$archive_bytes" "$license_sha" > "$tmp/fonts.lock"
printf 'sample\tarchive\tlink.bdf\tfonts/inside.bdf\t%s\tlicense\tlegacy\n' "$font_sha" > "$tmp/fonts.tsv"
reject 'symlink member accepted'

echo 'ADR 0090 I1, I2: tar archive boundaries (6 cases)'
