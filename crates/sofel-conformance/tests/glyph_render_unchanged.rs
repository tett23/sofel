//! ADR 0095 I3 (ADR 0087 I3 as ADR 0095 corrects it): the functions moved
//! from sofel-cli to sofel-glyph-render give the same bitmaps, color
//! images and errors as before the move, for every input without a sweep
//! gradient (the only place that used the platform's `atan2f`).
//!
//! The expected hashes come from this same file run against the code
//! before the move (main at 50d02387, with `use sofel_cli::glyph;`).
//! GRAY includes the later VARC slot loading of root ADR 0064: only
//! cff2-varc-short-fdselect0-oob.otf changes among these 120 inputs.
//! That malformed fuzz font lacks hmtx (FreeType rejects the face with
//! Hmtx_Table_Missing); this snapshot covers the raw Rust font views.
//! Valid VARC faces are compared with FreeType in cff_hint.rs.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use sofel_color::{PaintOptions, Palette, RecordedBrush, Recording};
use sofel_conformance::paths;
use sofel_font::{Font, Type1Face, Type1Source};
use sofel_glyph_render as glyph;
use sofel_outline::{Outlines, Scratch};
use sofel_raster::{BgraImage, Rasterizer};
use sofel_read::GlyphId;

#[path = "../../sofel-type1/tests/support/type1_triangle.rs"]
mod type1;

struct Fnv(u64);

impl Fnv {
    fn eat(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

/// Whether the glyph's paint uses a sweep gradient anywhere.
fn has_sweep(font: &Font<'_>, glyph: u32, options: &PaintOptions<'_>) -> bool {
    let mut rec = Recording::new();
    let _ = sofel_color::paint_recording(
        font,
        glyph,
        options,
        &mut sofel_color::Scratch::default(),
        &mut rec,
    );
    rec.commands().iter().any(|c| {
        matches!(
            c,
            sofel_color::PaintCommand::Fill(RecordedBrush::Sweep { .. })
        )
    })
}

/// The hash of `render_color` and `render` over every glyph of the fonts
/// (up to `limit` glyphs each) at the sizes, and the number of glyphs
/// skipped for a sweep gradient.
fn hash_fonts(fonts: &[String], sizes: &[u16], limit: u32) -> (u64, usize, usize) {
    let mut h = Fnv(0xcbf2_9ce4_8422_2325);
    let (mut checked, mut skipped) = (0, 0);
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let mut image = BgraImage::default();
    for name in fonts {
        let bytes = std::fs::read(paths::testdata().join(name)).unwrap();
        h.eat(name.as_bytes());
        let Ok(font) = Font::new(&bytes, 0) else {
            h.eat(b"no font");
            continue;
        };
        let outlines = Outlines::new((&font).into());
        let glyphs = font.num_glyphs().min(limit);
        for palette in [Palette::Font(0), Palette::Font(1)] {
            let options = PaintOptions {
                palette,
                ..Default::default()
            };
            for g in 0..glyphs {
                if has_sweep(&font, g, &options) {
                    skipped += 1;
                    continue;
                }
                checked += 1;
                for &ppem in sizes {
                    let r = glyph::render_color(
                        &outlines,
                        GlyphId(g),
                        ppem,
                        &options,
                        &mut scratch,
                        &mut raster,
                        &mut image,
                    );
                    match r {
                        Ok(glyph::ColorGlyph::Color) => {
                            h.eat(format!("color {image:?}").as_bytes());
                        }
                        other => h.eat(format!("{other:?}").as_bytes()),
                    }
                }
            }
        }
        for g in 0..glyphs {
            for &ppem in sizes {
                let r = glyph::render(&outlines, GlyphId(g), ppem, &mut scratch, &mut raster);
                h.eat(format!("{r:?}").as_bytes());
            }
        }
    }
    (h.0, checked, skipped)
}

fn api_fonts(filter: impl Fn(&str) -> bool) -> Vec<String> {
    let dir = "harfbuzz/test/api/fonts";
    let mut names: Vec<String> = std::fs::read_dir(paths::testdata().join(dir))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| filter(n))
        .map(|n| format!("{dir}/{n}"))
        .collect();
    names.sort();
    names
}

/// COLRv1 (solid, linear and radial gradients, clips, composites), COLRv0
/// and SVG glyphs, and the outlines of the same fonts.
#[test]
fn adr0095_i3_color_glyphs_render_as_before_the_move() {
    let fonts = api_fonts(|n| {
        n.starts_with("test_glyphs-glyf_colr_1")
            || n.starts_with("colr")
            || n.starts_with("COLRv0")
            || n.starts_with("chromacheck")
    });
    assert!(fonts.len() >= 10, "{fonts:?}");
    let (hash, checked, skipped) = hash_fonts(&fonts, &[12, 48], 400);
    assert_eq!(
        (hash, checked, skipped),
        COLOR,
        "hash {hash:#018x}, {checked} checked, {skipped} skipped"
    );
}

/// Gray bitmaps and errors (empty glyphs, outlines that cannot be loaded,
/// rasterizer errors) of the fuzzing fonts.
#[test]
fn adr0095_i3_gray_glyphs_and_errors_preserve_reviewed_output() {
    let dir = "harfbuzz/test/fuzzing/fonts";
    let mut fonts: Vec<String> = std::fs::read_dir(paths::testdata().join(dir))
        .unwrap()
        .map(|e| format!("{dir}/{}", e.unwrap().file_name().into_string().unwrap()))
        .collect();
    fonts.sort();
    fonts.truncate(120);
    let (hash, checked, skipped) = hash_fonts(&fonts, &[9, 33], 64);
    assert_eq!(
        (hash, checked, skipped),
        GRAY,
        "hash {hash:#018x}, {checked} checked, {skipped} skipped"
    );
}

/// Type 1 glyphs through `render_type1`.
#[test]
fn adr0095_i3_type1_glyphs_render_as_before_the_move() {
    let (pfa, pfb) = type1::sample();
    let mut h = Fnv(0xcbf2_9ce4_8422_2325);
    let mut raster = Rasterizer::new();
    for source in [
        Type1Source::parse_pfa(&pfa, 1 << 20).unwrap(),
        Type1Source::parse_pfb(&pfb, 1 << 20).unwrap(),
    ] {
        let face = Type1Face::new(&source).unwrap();
        for g in 0..4 {
            for ppem in [7, 16, 50] {
                let r = glyph::render_type1(&face, g, ppem, &mut raster);
                h.eat(format!("{r:?}").as_bytes());
            }
        }
    }
    assert_eq!(h.0, TYPE1, "hash {:#018x}", h.0);
}

const COLOR: (u64, usize, usize) = (0x5c44_1a1c_d90f_04fd, 1672, 392);
const GRAY: (u64, usize, usize) = (0x70c7_dd76_0cb0_f0f1, 1136, 2);
const TYPE1: u64 = 0x2ee4_18ea_91e2_1545;
