#!/bin/sh
# I1_webfont_roundtrip: compare all tables of full and subset fonts using FreeType.
set -eu
fonts=$1
encoder=$2
subsetter=$3
compare=$4
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
for name in Roboto-Regular.ttf Cantarell-VF-ABC.otf AdobeVFPrototype.otf; do
    font=$fonts/$name
    set -- --font-file="$font" --gids=0
    if [ "$name" = AdobeVFPrototype.otf ]; then
        set -- "$@" --drop-tables=GDEF,GPOS,GSUB --desubroutinize
    fi
    "$subsetter" "$@" -o "$work/subset.sfnt"
    for format in woff woff2; do
        "$encoder" --format "$format" "$font" -o "$work/full"
        "$compare" "$format" "$font" "$work/full"
        "$subsetter" "$@" -o - |
            "$encoder" --format "$format" - -o - >"$work/web-subset"
        "$compare" "$format" "$work/subset.sfnt" "$work/web-subset"
    done
done
