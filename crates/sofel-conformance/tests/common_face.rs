//! Common face operations over the real legacy corpus (root ADR 0100).
#![allow(clippy::unwrap_used)]

use std::process::Command;

use sofel_bitmap::strikes;
use sofel_conformance::paths;
use sofel_font::FaceData;
use sofel_font::read::GlyphId;
use sofel_glyph_render::render_face;
use sofel_outline::Scratch;
use sofel_raster::Rasterizer;

#[test]
fn adr0100_i1_i2_real_legacy_face_counts_and_unicode_maps() {
    for name in [
        "cmr10.pfb",
        "spleen-8x16.bdf",
        "spleen-8x16.pcf",
        "spleen-8x16.fon",
        "spleen-8x16.dfont",
        "spleen-8x16.woff",
        "spleen-8x16.woff2",
    ] {
        if name.ends_with(".woff2") && std::env::var_os("SOFEL_FT_BROTLI_DUMP").is_none() {
            continue;
        }
        let path = paths::testdata().join("extra-fonts/fonts").join(name);
        let bytes = std::fs::read(&path).unwrap();
        let data = FaceData::new(&bytes).unwrap();
        let dump_command = if name.ends_with(".woff2") {
            std::env::var_os("SOFEL_FT_BROTLI_DUMP")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(paths::ft_dump)
        } else {
            paths::ft_dump()
        };
        let output = Command::new(dump_command).arg(&path).output().unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let dump = String::from_utf8(output.stdout).unwrap();
        let mut face_count = 0;
        let mut unicode_map = false;
        let mut opened = None;
        for line in dump.lines() {
            let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
            match fields.as_slice() {
                ["face", index] => {
                    let index = index.parse::<u32>().unwrap();
                    assert_eq!(index, face_count, "{name}");
                    face_count += 1;
                    opened = Some(data.open(index).unwrap());
                }
                ["num_glyphs", count] => {
                    assert_eq!(
                        opened.as_ref().unwrap().as_face().glyph_count(),
                        count.parse::<u32>().unwrap(),
                        "{name}"
                    );
                }
                ["cmap", platform, encoding, _] => {
                    unicode_map =
                        *platform == "0" || (*platform == "3" && matches!(*encoding, "1" | "10"));
                }
                ["map", codepoint, glyph] if unicode_map => {
                    assert_eq!(
                        opened
                            .as_ref()
                            .unwrap()
                            .as_face()
                            .nominal_glyph(codepoint.parse().unwrap())
                            .map(|g| g.0),
                        Some(glyph.parse().unwrap()),
                        "{name}: {line}"
                    );
                }
                _ => {}
            }
        }
        assert_eq!(data.face_count().unwrap(), face_count, "{name}");
        assert!(data.open(face_count).is_err(), "{name}");
    }
}

#[test]
fn adr0100_i1_i2_real_bitmap_and_type1_advances() {
    for (name, ppem, upem, raw_pixels) in [
        ("spleen-8x16.bdf", 16u16, 0u16, true),
        ("spleen-8x16.pcf", 16, 0, true),
        ("cmr10.pfb", 20, 1000, false),
        ("spleen-8x16.woff", 16, 1024, false),
        ("spleen-8x16.woff2", 16, 1024, false),
    ] {
        if name.ends_with(".woff2") && std::env::var_os("SOFEL_FT_BROTLI_DUMP").is_none() {
            continue;
        }
        let path = paths::testdata().join("extra-fonts/fonts").join(name);
        let data = FaceData::new(&std::fs::read(&path).unwrap()).unwrap();
        let opened = data.open(0).unwrap();
        let face = opened.as_face();
        let dump = if name.ends_with(".woff2") {
            std::env::var_os("SOFEL_FT_BROTLI_DUMP")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(paths::ft_dump)
        } else {
            paths::ft_dump()
        };
        let output = Command::new(dump).arg(&path).output().unwrap();
        assert!(output.status.success(), "{name}");
        let text = String::from_utf8(output.stdout).unwrap();
        let mut checked = 0;
        for line in text.lines() {
            let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
            if let ["advance", id, raw] = fields.as_slice() {
                let id = id.parse::<u32>().unwrap();
                let raw = raw.parse::<i32>().unwrap();
                let expected = if !raw_pixels {
                    let scale = sofel_geom::div_fix(i32::from(ppem) * 64, i32::from(upem));
                    sofel_geom::mul_fix(raw, scale)
                } else {
                    raw
                };
                assert_eq!(
                    face.h_advance(GlyphId(id), ppem, None),
                    Ok(expected),
                    "{name}: {line}"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, face.glyph_count(), "{name}");
    }
}

#[test]
fn adr0100_i1_i2_real_sfnt_fixed_strike_advances() -> Result<(), &'static str> {
    for name in ["spleen-8x16.dfont", "color-noto-cbdt.ttf", "color-sbix.ttf"] {
        let path = paths::testdata().join("extra-fonts/fonts").join(name);
        let bytes = std::fs::read(&path).unwrap();
        let data = FaceData::new(&bytes).unwrap();
        let opened = data.open(0).unwrap();
        let face = opened.as_face();
        let font = match face {
            sofel_font::Face::Sfnt(font) => font,
            _ => return Err("sfnt expected"),
        };
        let sizes = strikes(font).collect::<Vec<_>>();
        assert_eq!(
            face.strike_count(),
            u32::try_from(sizes.len()).unwrap(),
            "{name}"
        );
        if name.ends_with("cbdt.ttf") {
            let first = sizes.first().unwrap();
            let mut scratch = Scratch::default();
            let mut raster = Rasterizer::new();
            let missing = render_face(
                face,
                GlyphId(0),
                first.x_ppem,
                Some(0),
                &mut scratch,
                &mut raster,
            )
            .unwrap();
            assert_eq!((missing.width, missing.rows), (0, 0), "{name}");
        }
        let output = Command::new(paths::ft_dump())
            .arg("--sbit")
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{name}");
        let text = String::from_utf8(output.stdout).unwrap();
        let mut index = None;
        let mut checked = 0;
        for line in text.lines() {
            let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
            match fields.as_slice() {
                ["size", raw] => index = Some(raw.parse::<u32>().unwrap()),
                ["glyph", glyph, "0", _, _, _, _, _, _, advance, ..] => {
                    let index = index.unwrap();
                    let strike = sizes.get(usize::try_from(index).unwrap()).unwrap();
                    let ppem = strike.x_ppem;
                    if ppem != strike.y_ppem {
                        continue;
                    }
                    assert_eq!(
                        face.h_advance(GlyphId(glyph.parse().unwrap()), ppem, Some(index)),
                        Ok(advance.parse::<i32>().unwrap()),
                        "{name}: {line}"
                    );
                    checked += 1;
                }
                _ => {}
            }
        }
        assert_eq!(
            checked,
            if name.ends_with(".dfont") {
                1021
            } else if name.ends_with("cbdt.ttf") {
                42
            } else {
                19
            },
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn adr0100_i2_real_sfnt_fixed_strike_images_match_freetype() -> Result<(), &'static str> {
    let mut checked = 0;
    for name in ["spleen-8x16.dfont", "color-noto-cbdt.ttf", "color-sbix.ttf"] {
        let path = paths::testdata().join("extra-fonts/fonts").join(name);
        let bytes = std::fs::read(&path).unwrap();
        let data = FaceData::new(&bytes).unwrap();
        let opened = data.open(0).unwrap();
        let face = opened.as_face();
        let sofel_font::Face::Sfnt(font) = face else {
            return Err("sfnt expected");
        };
        let sizes = strikes(font).collect::<Vec<_>>();
        let output = Command::new(paths::ft_dump())
            .arg("--sbit")
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{name}");
        let text = String::from_utf8(output.stdout).unwrap();
        let mut lines = text.lines();
        let mut strike_index = None;
        while let Some(line) = lines.next() {
            let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
            match fields.as_slice() {
                ["size", raw] => strike_index = Some(raw.parse::<u32>().unwrap()),
                ["glyph", glyph, "0", left, top, width, rows, pitch, mode, ..] => {
                    let index = strike_index.unwrap();
                    let strike = sizes.get(usize::try_from(index).unwrap()).unwrap();
                    let width = width.parse::<usize>().unwrap();
                    let rows = rows.parse::<usize>().unwrap();
                    let pitch = pitch.parse::<usize>().unwrap();
                    let mode = mode.parse::<u8>().unwrap();
                    let raw = (0..rows)
                        .map(|_| {
                            let row = lines.next().unwrap();
                            assert_eq!(row.len(), pitch * 2, "{name}: {line}");
                            (0..pitch)
                                .map(|x| u8::from_str_radix(&row[x * 2..x * 2 + 2], 16).unwrap())
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    if strike.x_ppem != strike.y_ppem
                        || sofel_bitmap::glyph_bitmap(font, strike, glyph.parse().unwrap())
                            .is_none()
                    {
                        continue;
                    }
                    let mut scratch = Scratch::default();
                    let mut raster = Rasterizer::new();
                    let image = render_face(
                        face,
                        GlyphId(glyph.parse().unwrap()),
                        strike.x_ppem,
                        Some(index),
                        &mut scratch,
                        &mut raster,
                    )
                    .unwrap();
                    assert_eq!(
                        (image.left, image.top),
                        (left.parse().unwrap(), top.parse().unwrap()),
                        "{name}: {line}"
                    );
                    assert_eq!(
                        (image.width as usize, image.rows as usize),
                        (width, rows),
                        "{name}: {line}"
                    );
                    assert!(matches!(mode, 1 | 2 | 3 | 4 | 7), "{name}: {line}");
                    let expected = raw
                        .iter()
                        .flat_map(|row| {
                            (0..width).map(move |x| match mode {
                                1 | 3 | 4 => {
                                    let bits = if mode == 1 { 1 } else { 1usize << (mode - 2) };
                                    let mask = (1u8 << bits) - 1;
                                    let bit = x * bits;
                                    ((row.get(bit / 8).copied().unwrap() >> (8 - bits - bit % 8))
                                        & mask)
                                        * (255 / mask)
                                }
                                2 => row.get(x).copied().unwrap(),
                                7 => row.get(x * 4 + 3).copied().unwrap(),
                                _ => 0,
                            })
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(image.pixels, expected, "{name}: {line}");
                    checked += 1;
                }
                _ => {}
            }
        }
    }
    assert_eq!(checked, 1021);
    Ok(())
}
