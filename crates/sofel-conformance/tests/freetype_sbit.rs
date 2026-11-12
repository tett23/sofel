//! Embedded bitmaps against FreeType (ADR 0023, stage 5c): fonts built
//! for every image format (1, 2, 5, 6, 7, 8, 9), bit depth (1, 2, 4, 8,
//! 32) and index format (1 to 5), in EBLC/EBDT, Apple's bloc/bdat/bhed,
//! and CBLC/CBDT next to outlines, read by `ft-dump --sbit` and by
//! `sofel_bitmap`: the strikes, every glyph's metrics and pixels.
//! The test data has no EBLC or bloc font, so these are built (ADR 0023).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

#[path = "../../sofel-read/tests/support/build.rs"]
#[allow(dead_code)]
mod build;

use std::fmt::Write;
use std::path::PathBuf;
use std::process::Command;

use build::*;
use sofel_bitmap::{BitmapError, PixelMode, bitmap_layout, draw_bitmap, strikes};
use sofel_conformance::{color, paths};
use sofel_font::Font;

/// A small xorshift for the pixel data.
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u8 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 24) as u8
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next()).collect()
    }
}

/// Glyph sizes: (width, height).
const SIZES: [(u8, u8); 3] = [(5, 7), (11, 4), (16, 9)];

fn small(w: u8, h: u8, bx: i8, by: i8, adv: u8) -> Vec<u8> {
    vec![h, w, bx as u8, by as u8, adv]
}

fn big(w: u8, h: u8, bx: i8, by: i8, adv: u8) -> Vec<u8> {
    vec![h, w, bx as u8, by as u8, adv, (-2i8) as u8, 1, adv + 2]
}

/// The pixel data of a `w`×`h` image at `depth` bits: byte-aligned rows,
/// or all rows packed (bit-aligned).
fn pixels(rng: &mut Rng, w: u8, h: u8, depth: u32, bit_aligned: bool) -> Vec<u8> {
    let line_bits = u32::from(w) * depth;
    if bit_aligned {
        rng.bytes(((line_bits * u32::from(h) + 7) / 8) as usize)
    } else {
        rng.bytes((((line_bits + 7) / 8) * u32::from(h)) as usize)
    }
}

/// The location and data tables for glyphs 1..=3 (glyph 3 a compound of
/// 1 and 2 for image formats 8 and 9; otherwise a third simple glyph).
fn tables(depth: u8, index_format: u16, image_format: u16, seed: u32) -> (Vec<u8>, Vec<u8>) {
    let mut rng = Rng(seed);
    let d = u32::from(depth);
    let compound = matches!(image_format, 8 | 9);
    let mut images: Vec<Vec<u8>> = Vec::new();
    for (i, &(w, h)) in SIZES.iter().enumerate() {
        let (bx, by, adv) = (i as i8 - 1, h as i8 - 2, w + 1);
        let img = if compound && i == 2 {
            let mut v = if image_format == 8 {
                small(w, h, bx, by, adv)
            } else {
                big(w, h, bx, by, adv)
            };
            if image_format == 8 {
                v.push(0);
            }
            v.extend(
                Buf::new()
                    .u16(2)
                    .u16(1)
                    .u8(2)
                    .u8(1)
                    .u16(2)
                    .u8(0)
                    .u8(3)
                    .done(),
            );
            v
        } else {
            let fmt = if compound { 1 } else { image_format };
            let mut v = match fmt {
                1 | 2 => small(w, h, bx, by, adv),
                6 | 7 => big(w, h, bx, by, adv),
                _ => Vec::new(),
            };
            v.extend(pixels(&mut rng, w, h, d, matches!(fmt, 2 | 5 | 7)));
            v
        };
        images.push(img);
    }
    // Formats 2 and 5 need images of one size: use the first glyph's.
    if matches!(index_format, 2 | 5) {
        let (w, h) = SIZES[0];
        images = (0..3).map(|_| pixels(&mut rng, w, h, d, true)).collect();
    }
    let mut data = Buf::new().u16(2).u16(0).done();
    let base = data.len() as u32;
    let mut offsets = vec![0u32];
    for img in &images {
        data.extend(img);
        offsets.push(data.len() as u32 - base);
    }
    let (w, h) = SIZES[0];
    let index = match index_format {
        1 | 3 => index_offsets(index_format, image_format, base, &offsets),
        2 => {
            let size = images[0].len() as u32;
            Buf::new()
                .u16(2)
                .u16(5)
                .u32(base)
                .u32(size)
                .bytes(&big(w, h, 0, h as i8, w))
                .done()
        }
        4 => {
            let mut b = Buf::new().u16(4).u16(image_format).u32(base).u32(3);
            for (i, o) in offsets.iter().enumerate() {
                b = b.u16(if i < 3 { i as u16 + 1 } else { 0 }).u16(*o as u16);
            }
            b.done()
        }
        5 => {
            let size = images[0].len() as u32;
            Buf::new()
                .u16(5)
                .u16(5)
                .u32(base)
                .u32(size)
                .bytes(&big(w, h, 1, h as i8, w))
                .u32(3)
                .u16(1)
                .u16(2)
                .u16(3)
                .done()
        }
        _ => unreachable!(),
    };
    let loc = cblc(
        2,
        &[SizeSpec {
            ppem_x: 12,
            ppem_y: 12,
            bit_depth: depth,
            start_glyph: 1,
            end_glyph: 3,
            subtables: vec![(1, 3, index)],
            ..SizeSpec::default()
        }],
    );
    (loc, data)
}

/// A font without outlines: `head` (or Apple's `bhed`) and the metrics
/// tables of the sample font, and the bitmap tables under `tags`.
fn font(
    loc: Vec<u8>,
    data: Vec<u8>,
    tags: (&'static [u8; 4], &'static [u8; 4]),
    apple: bool,
) -> Vec<u8> {
    let mut t: Vec<(&'static [u8; 4], Vec<u8>)> = sample_tables()
        .into_iter()
        .map(|(tag, d)| {
            (
                if apple && tag == b"head" {
                    b"bhed"
                } else {
                    tag
                },
                d,
            )
        })
        .collect();
    t.push((tags.0, loc));
    t.push((tags.1, data));
    t.sort_by_key(|(tag, _)| **tag);
    sfnt(&t)
}

/// FreeType's pixel mode numbers.
fn mode(m: PixelMode) -> u32 {
    match m {
        PixelMode::Mono => 1,
        PixelMode::Gray => 2,
        PixelMode::Gray2 => 3,
        PixelMode::Gray4 => 4,
        PixelMode::Lcd => 5,
        PixelMode::LcdV => 6,
        PixelMode::Bgra => 7,
    }
}

/// The `--sbit` text of sofel, for the glyphs of every strike.
fn sofel_sbit(f: &Font<'_>) -> String {
    let mut s = String::new();
    for (i, st) in strikes(f).enumerate() {
        writeln!(s, "size {i}").unwrap();
        for g in 0..f.num_glyphs() {
            match bitmap_layout(f, &st, g) {
                Err(BitmapError::Missing) => writeln!(s, "glyph {g} missing").unwrap(),
                Err(_) => writeln!(s, "glyph {g} error").unwrap(),
                Ok(l) => {
                    let mut buf = vec![0u8; l.len()];
                    match draw_bitmap(f, &st, g, &mut buf) {
                        Ok(l) => {
                            let m = l.metrics;
                            writeln!(
                                s,
                                "glyph {g} 0 {} {} {} {} {} {} {} {} {} {}",
                                m.hori_bearing_x,
                                m.hori_bearing_y,
                                l.width,
                                l.rows,
                                l.pitch,
                                mode(l.pixel_mode),
                                i64::from(m.hori_advance) * 64,
                                i64::from(m.vert_bearing_x) * 64,
                                i64::from(m.vert_bearing_y) * 64,
                                i64::from(m.vert_advance) * 64
                            )
                            .unwrap();
                            for row in buf.chunks(l.pitch.max(1) as usize).take(l.rows as usize) {
                                let hex: String = row.iter().map(|b| format!("{b:02x}")).collect();
                                writeln!(s, "{hex}").unwrap();
                            }
                        }
                        Err(_) => writeln!(s, "glyph {g} error").unwrap(),
                    }
                }
            }
        }
    }
    s
}

/// FreeType's text with missing glyphs written as sofel writes them: an
/// empty bitmap (FreeType's whitespace metrics for bitmap-only fonts)
/// or the outline (-1) is "missing", other errors "error".
fn normalize_ft(text: &str) -> String {
    let mut out = String::new();
    for l in text.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        match w.first() {
            Some(&"glyph") if w.len() == 3 => {
                let what = if w[2] == "-1" || w[2] == "157" {
                    "missing"
                } else {
                    "error"
                };
                writeln!(out, "glyph {} {what}", w[1]).unwrap();
            }
            Some(&"glyph") if w.len() == 13 && w[5] == "0" && w[6] == "0" && w[4] == "0" => {
                writeln!(out, "glyph {} missing", w[1]).unwrap();
            }
            Some(&"strike") | Some(&"face") | Some(&"end") => {}
            _ => writeln!(out, "{l}").unwrap(),
        }
    }
    out
}

fn ft_dump_sbit(name: &str, bytes: &[u8]) -> String {
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap_or_else(|e| panic!("{e}"));
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!("sofel-sbit-{name}-{}.ttf", std::process::id()));
    std::fs::write(&p, bytes).unwrap();
    let out = Command::new(exe).arg("--sbit").arg(&p).output().unwrap();
    std::fs::remove_file(&p).ok();
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn built_bitmap_fonts_agree_with_freetype() {
    let cases: [(u16, u16); 11] = [
        (1, 1),
        (1, 2),
        (1, 6),
        (1, 7),
        (3, 1),
        (3, 7),
        (2, 5),
        (5, 5),
        (4, 6),
        (1, 8),
        (1, 9),
    ];
    let mut failures = Vec::new();
    let mut n = 0;
    for depth in [1u8, 2, 4, 8, 32] {
        for (k, &(index, image)) in cases.iter().enumerate() {
            for (flavor, tags, apple) in [
                ("eblc", (b"EBLC", b"EBDT"), false),
                ("bloc", (b"bloc", b"bdat"), true),
            ] {
                let (loc, data) = tables(depth, index, image, 7 + k as u32 * 31 + u32::from(depth));
                let bytes = font(loc, data, (tags.0, tags.1), apple);
                let name = format!("{flavor}-d{depth}-i{index}-f{image}");
                let ft = ft_dump_sbit(&name, &bytes);
                let f = Font::new(&bytes, 0).unwrap();
                assert!(!f.has_outlines() && f.has_bitmaps(), "{name}");
                let strikes_item = color::compare_strikes(&name, 0, &f, &ft);
                assert!(strikes_item.ok, "{name}: {}", strikes_item.detail);
                let theirs = normalize_ft(&ft);
                let ours = sofel_sbit(&f);
                n += 1;
                if theirs != ours {
                    failures.push(format!("{name}:\nFreeType:\n{theirs}sofel:\n{ours}"));
                }
            }
        }
    }
    eprintln!("{n} fonts, {} differ", failures.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn color_bitmaps_hide_outlines() {
    // CBLC/CBDT next to glyf: FreeType ignores the outlines.
    let (loc, mut data) = tables(32, 1, 1, 99);
    data[1] = 3;
    let square = simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(0, 10), Pt::On(10, 10), Pt::On(10, 0)]],
        false,
        &[],
    );
    let bytes = glyph_font_with(
        &[Vec::new(), square.clone(), square.clone(), square],
        false,
        false,
        vec![
            (b"CBLC", {
                let mut l = loc;
                l[1] = 3;
                l
            }),
            (b"CBDT", data),
        ],
    );
    let f = Font::new(&bytes, 0).unwrap();
    assert!(!f.has_outlines());
    assert!(f.has_bitmaps());
    let ft = ft_dump_sbit("cblc-glyf", &bytes);
    assert_eq!(normalize_ft(&ft), sofel_sbit(&f));
}

#[test]
#[ignore = "requires ft-dump built with SOFEL_FT_PNG=1; run by the PNG reference CI step"]
fn i1_sbit_png_matches_freetype_color_rows() {
    fn bitmaps(text: &str) -> Vec<(String, Vec<String>)> {
        let mut result: Vec<(String, Vec<String>)> = Vec::new();
        let mut size = "";
        let mut keep = false;
        for line in text.lines() {
            if line.starts_with("size ") {
                size = line;
                keep = false;
            } else if line.starts_with("glyph ") {
                let fields: Vec<_> = line.split_whitespace().collect();
                keep = fields.len() == 13 && fields[8] == "7" && fields[5] != "0";
                if keep {
                    result.push((format!("{size} {}", fields[..9].join(" ")), Vec::new()));
                }
            } else if keep && line.bytes().all(|b| b.is_ascii_hexdigit()) {
                result.last_mut().unwrap().1.push(line.to_owned());
            }
        }
        result
    }
    for name in [
        "NotoColorEmoji.subset.ttf",
        "NotoColorEmoji.subset.multiple_size_tables.ttf",
        "sbix.ttf",
    ] {
        let bytes = std::fs::read(
            paths::testdata()
                .join("harfbuzz/test/subset/data/fonts")
                .join(name),
        )
        .unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let theirs = bitmaps(&ft_dump_sbit(name, &bytes));
        assert!(
            !theirs.is_empty(),
            "{name}: reference did not decode a BGRA glyph"
        );
        let ours = bitmaps(&sofel_sbit(&font));
        assert_eq!(ours.len(), theirs.len(), "{name}");
        for ((ours, rows), (expected, pixels)) in ours.iter().zip(&theirs) {
            assert_eq!(
                ours, expected,
                "{name}: strike, origin, dimensions, pitch or pixel mode"
            );
            assert_eq!(rows.len(), pixels.len(), "{name} {ours}");
            for (y, (a, b)) in rows.iter().zip(pixels).enumerate() {
                assert_eq!(a, b, "{name} {ours}, row {y}");
            }
        }
    }
}

#[test]
#[ignore = "requires the gallery corpus and PNG-enabled ft-dump"]
fn i2_sbix_gallery_strike_origin_and_pixels() {
    let bytes = std::fs::read(paths::testdata().join("extra-fonts/fonts/color-sbix.ttf")).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let ours = sofel_sbit(&font);
    let reference = ft_dump_sbit("gallery", &bytes);
    for id in [19, 26] {
        let prefix = format!("glyph {id} 0 4 101 128 128 512 7 ");
        let rows = |text: &str| -> Vec<String> {
            assert!(text.lines().any(|s| s == "size 0"));
            let mut lines = text.lines().skip_while(|s| !s.starts_with(&prefix));
            assert!(
                lines.next().is_some(),
                "glyph {id} missing or has incorrect origin or format"
            );
            lines
                .take_while(|s| s.len() == 1024 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                .map(str::to_owned)
                .collect()
        };
        let expected = rows(&reference);
        assert_eq!(expected.len(), 128, "glyph {id}");
        assert_eq!(rows(&ours), expected, "glyph {id}");
    }
}
