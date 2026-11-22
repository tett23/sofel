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
    for (name, ppem, outline) in [
        ("spleen-8x16.bdf", 16u16, false),
        ("spleen-8x16.pcf", 16, false),
        ("cmr10.pfb", 20, true),
    ] {
        let path = paths::testdata().join("extra-fonts/fonts").join(name);
        let data = FaceData::new(&std::fs::read(&path).unwrap()).unwrap();
        let opened = data.open(0).unwrap();
        let face = opened.as_face();
        let output = Command::new(paths::ft_dump()).arg(&path).output().unwrap();
        assert!(output.status.success(), "{name}");
        let text = String::from_utf8(output.stdout).unwrap();
        let mut checked = 0;
        for line in text.lines() {
            let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
            if let ["advance", id, raw] = fields.as_slice() {
                let id = id.parse::<u32>().unwrap();
                let raw = raw.parse::<i32>().unwrap();
                let expected = if outline {
                    let scale = sofel_geom::div_fix(i32::from(ppem) * 64, 1000);
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
    for name in ["spleen-8x16.dfont", "color-noto-cbdt.ttf"] {
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
            if name.ends_with(".dfont") { 1021 } else { 42 },
            "{name}"
        );
    }
    Ok(())
}
