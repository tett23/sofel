//! Compare loaded TrueType slots before rendering (sofel ADR 0064).
#![allow(clippy::unwrap_used)]

use sofel_conformance::paths;
use sofel_font::{
    Font,
    read::{GlyphId, Tag},
    var::{Location, Variation},
};
use sofel_outline::{HintTarget, Outlines, Scratch};
use std::{fmt::Write, process::Command};

#[test]
fn adr0064_i1_native_slots_match_freetype() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    check_slots(
        &path,
        &Outlines::new(font.default_instance()),
        &[1, 7],
        None,
    );
}

#[test]
fn adr0064_i1_variable_slots_match_freetype() {
    let path = paths::testdata()
        .join("harfbuzz/test/shape/data/text-rendering-tests/fonts/TestCVARGVARTwo.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let glyphs = [b'h', b'o', b'n'].map(|c| font.nominal_glyph(u32::from(c)).unwrap().0);
    for weight in [28u16, 94] {
        let location = Location::new(
            &font,
            None,
            &[Variation {
                tag: Tag::new(b"wght"),
                value: f32::from(weight),
            }],
        );
        let args = format!("--variations=wght={weight}");
        check_slots(
            &path,
            &Outlines::new(font.instance(&location)),
            &glyphs,
            Some(&args),
        );
    }
}

fn check_slots(
    path: &std::path::Path,
    outlines: &Outlines<'_, '_, '_>,
    glyphs: &[u32],
    variation: Option<&str>,
) {
    let mut scratch = Scratch::default();
    for (name, target) in [
        ("normal", HintTarget::Normal),
        ("light", HintTarget::Light),
        ("mono", HintTarget::Mono),
        ("lcd", HintTarget::Lcd),
        ("lcd-v", HintTarget::VerticalLcd),
    ] {
        for engine in ["v35", "v40"] {
            for &gid in glyphs {
                let reference = Command::new(paths::ft_dump())
                    .args(["--hint-slot", "16", &gid.to_string(), engine, name])
                    .args(variation)
                    .arg(path)
                    .output()
                    .unwrap();
                assert!(
                    reference.status.success(),
                    "{}",
                    String::from_utf8_lossy(&reference.stderr)
                );
                let (_, advance) = if engine == "v35" {
                    outlines.load_truetype_v35_with_advance(GlyphId(gid), 16, target, &mut scratch)
                } else {
                    outlines.load_truetype_v40(GlyphId(gid), 16, target, &mut scratch)
                }
                .unwrap();
                let mut actual = format!("advance {advance} 0\n");
                assert_eq!(scratch.points().len(), scratch.point_tags().len());
                for (point, tag) in scratch.points().iter().zip(scratch.point_tags()) {
                    writeln!(actual, "point {} {} {tag}", point.x, point.y).unwrap();
                }
                for end in scratch.contour_ends() {
                    writeln!(actual, "end {end}").unwrap();
                }
                assert_eq!(
                    actual,
                    String::from_utf8(reference.stdout).unwrap(),
                    "{engine} {name} {gid}"
                );
            }
        }
    }
}
