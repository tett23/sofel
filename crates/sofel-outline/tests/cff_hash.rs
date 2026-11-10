//! The outlines of a CID-keyed CFF font and a CFF2 font of the test data
//! hash to the same value natively and on wasm32 (ADR 0021): CI runs this
//! test in both places against the committed hash. Skipped without the
//! test data unless `SOFEL_REQUIRE_TESTDATA` is set; `SOFEL_TESTDATA`
//! points to it (default: the repository's `target/testdata`).

#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::PathBuf;

use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_geom::{Command, FnPen};
use sofel_outline::{Outlines, Scale, Scratch};

const FONTS: [&str; 2] = [
    "harfbuzz/test/subset/data/fonts/SourceHanSans-Regular_subset.otf",
    "harfbuzz/test/subset/data/fonts/AdobeVFPrototype.otf",
];

/// FNV-1a over the commands, the boxes and the errors of every glyph,
/// unscaled and at 48 pixels.
const EXPECTED: u64 = 0xf4e0_64e9_e33c_52c6;

fn fnv(h: &mut u64, v: i64) {
    for b in v.to_le_bytes() {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(0x100_0000_01b3);
    }
}

#[test]
fn outline_hash() {
    let dir = std::env::var_os("SOFEL_TESTDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/testdata"));
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for f in FONTS {
        let Ok(bytes) = std::fs::read(dir.join(f)) else {
            if std::env::var_os("SOFEL_REQUIRE_TESTDATA").is_some() {
                panic!("{}: missing", dir.join(f).display());
            }
            eprintln!("{f}: no test data; skipped");
            return;
        };
        let font = Font::new(&bytes, 0).unwrap();
        let o = Outlines::new(&font);
        let mut scratch = Scratch::default();
        for scale in [Scale::Unscaled, Scale::pixels(&font, 48, 48)] {
            for g in 0..font.num_glyphs() {
                let mut pen = FnPen(|c| match c {
                    Command::MoveTo(p) => {
                        fnv(&mut h, 1);
                        fnv(&mut h, i64::from(p.x));
                        fnv(&mut h, i64::from(p.y));
                    }
                    Command::LineTo(p) => {
                        fnv(&mut h, 2);
                        fnv(&mut h, i64::from(p.x));
                        fnv(&mut h, i64::from(p.y));
                    }
                    Command::QuadTo(a, p) => {
                        fnv(&mut h, 3);
                        for v in [a.x, a.y, p.x, p.y] {
                            fnv(&mut h, i64::from(v));
                        }
                    }
                    Command::CubicTo(a, b, p) => {
                        fnv(&mut h, 4);
                        for v in [a.x, a.y, b.x, b.y, p.x, p.y] {
                            fnv(&mut h, i64::from(v));
                        }
                    }
                    Command::Close => fnv(&mut h, 5),
                });
                match o.draw(GlyphId(g), scale, &mut scratch, &mut pen) {
                    Ok(i) => {
                        for v in [i.cbox.x_min, i.cbox.y_min, i.cbox.x_max, i.cbox.y_max] {
                            fnv(&mut h, i64::from(v));
                        }
                    }
                    Err(_) => fnv(&mut h, -1),
                }
            }
        }
    }
    assert_eq!(h, EXPECTED, "outline hash {h:#x}");
}
