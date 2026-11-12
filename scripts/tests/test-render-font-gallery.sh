#!/bin/sh
# sofel ADR 0069 I03: a corrupt fixed input and an unusable output fail.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sofel-gallery-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

mkdir -p "$tmp/cache/fonts" "$tmp/cache/licenses"
printf font > "$tmp/cache/fonts/one.ttf"
printf license > "$tmp/cache/licenses/one.txt"
font_sha=$(shasum -a 256 "$tmp/cache/fonts/one.ttf" | cut -d ' ' -f 1)
license_sha=$(shasum -a 256 "$tmp/cache/licenses/one.txt" | cut -d ' ' -f 1)
printf 'one\thttps://example.invalid/one.ttf\t%s\t4\tfonts/one.ttf\nlicense\thttps://example.invalid/OFL.txt\t%s\t7\tlicenses/one.txt\n' "$font_sha" "$license_sha" > "$tmp/fonts.lock"
printf 'one\tone\t-\tfonts/one.ttf\t%s\tlicense\tshape\n' "$font_sha" > "$tmp/fonts.tsv"
printf wrong > "$tmp/cache/fonts/one.ttf"
if SOFEL_FONTS_OFFLINE=1 sh "$root/scripts/fetch-fonts.sh" "$tmp/fonts.lock" "$tmp/fonts.tsv" "$tmp/cache" >/dev/null 2>&1; then
  echo 'corrupt input unexpectedly accepted' >&2
  exit 1
fi

test -f "$root/tools/font-gallery/gallery.rb"
# The gallery generator must reject a path whose parent is a regular file.
printf block > "$tmp/block"
error=$(ruby "$root/tools/font-gallery/gallery.rb" "$tmp/block/gallery" "$tmp" 2>&1 >/dev/null) && {
  echo 'unwritable output unexpectedly accepted' >&2
  exit 1
}
case "$error" in
  *'gallery output directory is not writable'*) ;;
  *) echo "unexpected output error: $error" >&2; exit 1 ;;
esac

# A failed refresh must remove the old successful artifact.
mkdir -p "$tmp/project/scripts" "$tmp/project/target/font-gallery"
cp "$root/scripts/render-font-gallery.sh" "$tmp/project/scripts/render-font-gallery.sh"
printf '#!/bin/sh\nexit 1\n' > "$tmp/project/scripts/fetch-testdata.sh"
printf stale > "$tmp/project/target/font-gallery/index.html"
if sh "$tmp/project/scripts/render-font-gallery.sh" >/dev/null 2>&1; then
  echo 'failed refresh unexpectedly accepted' >&2
  exit 1
fi
if [ -e "$tmp/project/target/font-gallery/index.html" ]; then
  echo 'stale gallery survived a failed refresh' >&2
  exit 1
fi
# A glyph failure after the build must likewise leave no old output.
printf '#!/bin/sh\nexit 0\n' > "$tmp/project/scripts/fetch-testdata.sh"
cp "$tmp/project/scripts/fetch-testdata.sh" "$tmp/project/scripts/fetch-fonts.sh"
mkdir -p "$tmp/bin" "$tmp/project/target/font-gallery"
printf stale > "$tmp/project/target/font-gallery/index.html"
printf '#!/bin/sh\nexit 0\n' > "$tmp/bin/cargo"
cat > "$tmp/bin/ruby" <<'RUBY'
#!/bin/sh
case "$1" in
  */gallery.rb) echo 'missing glyph' >&2; exit 1 ;;
  *) exit 0 ;;
esac
RUBY
chmod +x "$tmp/bin/cargo" "$tmp/bin/ruby"
if PATH="$tmp/bin:$PATH" sh "$tmp/project/scripts/render-font-gallery.sh" >/dev/null 2>&1; then
  echo 'missing glyph unexpectedly accepted' >&2
  exit 1
fi
if [ -e "$tmp/project/target/font-gallery/index.html" ]; then
  echo 'stale gallery survived a missing glyph' >&2
  exit 1
fi
echo 'ok: gallery input hash, output path, and two failed refreshes (4 cases)'
