//! sofel-raster against FreeType (ADR 0012): `ft-dump --raster` on
//! every glyph of HarfBuzz's test fonts with glyf, at 12, 24 and 48 px.

#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::*;
use sofel_conformance::outline::{has_glyf, parse};
use sofel_conformance::raster::sofel_glyph;
use sofel_conformance::{expect, fonts, paths};
use sofel_font::Font;
use sofel_outline::Scratch;
use sofel_raster::Rasterizer;

const LIST: &str = "expected-failures-freetype.txt";
const PPEMS: [&str; 3] = ["12", "24", "48"];

fn ft_dump_raster(ppem: &str, font: &Path) -> Option<String> {
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new(exe)
        .args(["--raster", ppem])
        .arg(font)
        .output()
        .unwrap();
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).unwrap())
}

#[test]
fn rasters_agree_with_freetype() {
    let testdata = paths::testdata();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    let list = std::fs::read_to_string(paths::conformance_file(LIST)).unwrap_or_default();
    let mut expected = expect::parse(&list);
    expected.retain(|id| id.contains(":raster:"));
    let mut results = ft_stream::Results::default();
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap_or_else(|e| panic!("{e}"));
    for path in fonts::harfbuzz_fonts(&testdata) {
        let id = fonts::id(&path, &testdata);
        let bytes = std::fs::read(&path).unwrap();
        for size in PPEMS {
            let mut pending = ft_stream::Results::default();
            let mut cached_face = None;
            let mut outlines = None;
            let success = ft_stream::reference(
                Command::new(&exe).args(["--raster", size]).arg(&path),
                |face, glyph, ft_lines| {
                    if cached_face != Some(face) {
                        cached_face = Some(face);
                        outlines = Font::new(&bytes, face)
                            .ok()
                            .filter(has_glyf)
                            .map(|font| sofel_outline::Outlines::new(font.default_instance()));
                    }
                    let Some(outlines) = outlines.as_ref() else {
                        return;
                    };
                    let ours = sofel_glyph(
                        outlines,
                        glyph,
                        size.parse().unwrap(),
                        &mut scratch,
                        &mut raster,
                    );
                    pending.compare_glyph(
                        format!("{id}#{face}:raster:{size}:{glyph}"),
                        &ours,
                        ft_lines,
                        &expected,
                    );
                },
            )
            .unwrap_or_else(|e| panic!("{id} {size}px: {e}"));
            if success {
                results.merge(pending);
            }
        }
    }
    eprintln!("{} raster items, {} failed", results.count, results.failed);
    results.assert_expected(&expected, LIST, "raster.problems");
}

#[test]
fn ft_dump_renders_a_known_square() {
    // 1000 units at 12 px with unitsPerEm 1000 is exactly 12 pixels.
    let square = simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::On(0, 1000),
            Pt::On(1000, 1000),
            Pt::On(1000, 0),
        ]],
        false,
        &[],
    );
    let font = glyph_font(&[Vec::new(), square], true, false);
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!("sofel-raster-known-{}.ttf", std::process::id()));
    std::fs::write(&path, &font).unwrap();
    let out = ft_dump_raster("12", &path).unwrap();
    let _ = std::fs::remove_file(&path);
    let mut expected =
        String::from("face 0\nnum_glyphs 2\nglyph 0\nbitmap 0 0 0 0\nglyph 1\nbitmap 0 12 12 12\n");
    for _ in 0..12 {
        expected.push_str(&"ff".repeat(12));
        expected.push('\n');
    }
    expected.push_str("end\n");
    assert_eq!(out, expected);
}

#[path = "support/ft_stream.rs"]
mod ft_stream;

#[cfg(test)]
mod stream_tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::io::{BufReader, Cursor, Read};
    const SAMPLE: &str = "face 0\nnum_glyphs 2\nglyph 0\nbitmap 0 0 0 0\nglyph 1\nerror 8\nend\nface 1\nerror 3\nend\nface 2\nnum_glyphs 0\nend\n";

    #[test]
    fn adr0037_i1_i3_stream_matches_old_and_chunk_widths() {
        let old: Vec<_> = parse(SAMPLE)
            .unwrap()
            .into_iter()
            .flat_map(|(face, f)| f.glyphs.into_iter().map(move |(g, lines)| (face, g, lines)))
            .collect();
        for capacity in [1, 2, 7, 8192] {
            let mut got = Vec::new();
            let count = ft_stream::stream(
                BufReader::with_capacity(capacity, Cursor::new(SAMPLE)),
                |f, g, lines| {
                    got.push((f, g, lines.to_vec()));
                },
            )
            .unwrap();
            assert_eq!(count, 3);
            assert_eq!(got, old);
        }
    }

    #[test]
    fn adr0037_i2_empty_missing_duplicate_and_truncated() {
        assert_eq!(ft_stream::stream(Cursor::new(""), |_, _, _| {}).unwrap(), 0);
        for bad in [
            "face 0\nnum_glyphs 1\nend\n",
            "face 0\nnum_glyphs 1\nglyph 0\nerror 2\n",
            "face 0\nnum_glyphs 2\nglyph 0\nerror 2\nglyph 0\nerror 3\nend\n",
            "face 0\nnum_glyphs 1\nglyph 1\nerror 2\nend\n",
            "face 0\nend\n",
        ] {
            assert!(
                ft_stream::stream(Cursor::new(bad), |_, _, _| {}).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn adr0037_i3_reader_error_and_early_callback() {
        struct Fails;
        impl Read for Fails {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("injected"))
            }
        }
        assert!(ft_stream::stream(BufReader::new(Fails), |_, _, _| {}).is_err());
        let consumed = std::rc::Rc::new(std::cell::Cell::new(0));
        struct Count<R>(R, std::rc::Rc<std::cell::Cell<usize>>);
        impl<R: Read> Read for Count<R> {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                let n = self.0.read(out)?;
                self.1.set(self.1.get() + n);
                Ok(n)
            }
        }
        let input = Count(Cursor::new(SAMPLE), consumed.clone());
        let mut visits = 0;
        ft_stream::stream(BufReader::with_capacity(1, input), |_, _, _| {
            visits += 1;
            assert!(consumed.get() < SAMPLE.len());
        })
        .unwrap();
        assert_eq!(visits, 2);
    }

    #[test]
    fn adr0037_i2_i4_process_status_and_drain() {
        assert!(
            ft_stream::reference(Command::new("sh").args(["-c", "exit 0"]), |_, _, _| {}).is_err()
        );
        assert!(
            !ft_stream::reference(Command::new("sh").args(["-c", "exit 1"]), |_, _, _| {}).unwrap()
        );
        let mut pending = ft_stream::Results::default();
        let expected = BTreeSet::new();
        let success = ft_stream::reference(
            Command::new("sh").args([
                "-c",
                "printf 'face 0\nnum_glyphs 1\nglyph 0\nerror 7\nend\n'; exit 1",
            ]),
            |f, g, _| {
                pending.record(format!("{f}:{g}"), false, "detail".into(), &expected);
            },
        )
        .unwrap();
        assert!(!success);
        assert_eq!(pending.count, 1);
        let mut final_results = ft_stream::Results::default();
        if success {
            final_results.merge(pending);
        }
        assert_eq!(final_results.count, 0);
        // More than a pipe's capacity after the malformed first line.
        assert!(
            ft_stream::reference(
                Command::new("sh").args([
                    "-c",
                    "printf 'bad\n'; dd if=/dev/zero bs=65536 count=4 2>/dev/null"
                ]),
                |_, _, _| {}
            )
            .is_err()
        );
    }

    #[test]
    fn adr0037_i4_judgment_matches_old_and_discards_failed_call() {
        let expected: BTreeSet<_> = ["fixed", "known", "unknown"].map(str::to_string).into();
        let cases = [
            ("pass", true),
            ("regression", false),
            ("known", false),
            ("fixed", true),
        ];
        let old = expect::judge(cases, &expected);
        let mut got = ft_stream::Results::default();
        for (id, pass) in cases {
            got.record(id.into(), pass, "detail".into(), &expected);
        }
        let mut discarded = ft_stream::Results::default();
        discarded.record("discarded".into(), false, "detail".into(), &expected);
        drop(discarded);
        assert_eq!(got.count, 4);
        assert_eq!(got.failed, 2);
        assert_eq!(got.finish(&expected), old);
        let mut success = ft_stream::Results::default();
        for i in 0..1000 {
            success.record(i.to_string(), true, String::new(), &expected);
        }
        assert!(success.problems.is_empty());
        assert!(success.seen_expected.is_empty());
        assert!(success.details.is_empty());
    }
}

#[test]
fn i2_render_mode_harmony_agrees_with_freetype() {
    use sofel_font::geom::{Pen, Point};
    use sofel_raster::{FillRule, LcdMode, RenderOptions};
    let square = simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::On(0, 1000),
            Pt::On(1000, 1000),
            Pt::On(1000, 0),
        ]],
        false,
        &[],
    );
    let font = glyph_font(&[Vec::new(), square], true, false);
    let path = std::env::temp_dir().join(format!("sofel-harmony-{}.ttf", std::process::id()));
    std::fs::write(&path, font).unwrap();
    let mut raster = Rasterizer::new();
    raster.move_to(Point { x: 0, y: 0 });
    for (x, y) in [(0, 768), (768, 768), (768, 0), (0, 0)] {
        raster.line_to(Point { x, y });
    }
    raster.close();
    for (filtered, reference) in [
        (false, paths::ft_dump()),
        (true, paths::root().join("target/ft-dump/ft-dump-filtered")),
    ] {
        for (flag, mode) in [
            ("--lcd", LcdMode::Horizontal),
            ("--lcd-v", LcdMode::Vertical),
        ] {
            let out = Command::new(&reference)
                .args(["--raster", "12", flag])
                .arg(&path)
                .output()
                .unwrap();
            assert!(out.status.success());
            let dump = parse(&String::from_utf8(out.stdout).unwrap()).unwrap();
            let glyph = &dump[&0].glyphs[&1];
            let options = RenderOptions {
                fill: FillRule::NonZero,
                overlap: false,
                ppem: Some((12, 12)),
            };
            let b = if filtered {
                raster.render_filtered_lcd(mode, [8, 77, 86, 77, 8], options)
            } else {
                raster.render_harmony(mode, options)
            }
            .unwrap();
            assert_eq!(
                glyph[0],
                format!("bitmap {} {} {} {}", b.left, b.top, b.width, b.rows)
            );
            assert!(b.width > 0 && b.rows > 0);
            assert_eq!(glyph.len(), 1 + b.rows as usize);
            for (y, row) in b.pixels.chunks_exact(b.width as usize).enumerate() {
                let hex: String = row.iter().map(|v| format!("{v:02x}")).collect();
                assert_eq!(glyph[1 + y], hex, "{flag} row {y}");
            }
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i2_reference_renders_mono_and_both_sdf_inputs() {
    let square = simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::On(0, 1000),
            Pt::On(1000, 1000),
            Pt::On(1000, 0),
        ]],
        false,
        &[],
    );
    let path = std::env::temp_dir().join(format!("sofel-mono-sdf-{}.ttf", std::process::id()));
    std::fs::write(&path, glyph_font(&[Vec::new(), square], true, false)).unwrap();
    for size in [7usize, 8, 9] {
        for flag in ["--mono", "--sdf", "--bitmap-sdf"] {
            let out = Command::new(paths::ft_dump())
                .args(["--raster", &size.to_string(), flag])
                .arg(&path)
                .output()
                .unwrap();
            assert!(out.status.success());
            let dump = parse(&String::from_utf8(out.stdout).unwrap()).unwrap();
            let glyph = &dump[&0].glyphs[&1];
            if flag == "--mono" {
                assert_eq!(glyph[0], format!("bitmap 0 {size} {size} {size}"));
                let row = match size {
                    7 => "fe",
                    8 => "ff",
                    9 => "ff80",
                    _ => unreachable!(),
                };
                assert_eq!(glyph.len(), 1 + size);
                assert!(glyph[1..].iter().all(|actual| actual == row));
            } else {
                // The reference defaults to an eight-pixel distance spread.
                let width = size + 16;
                assert_eq!(glyph[0], format!("bitmap -8 {} {width} {width}", size + 8));
                assert_eq!(glyph.len(), width + 1);
                assert!(glyph[1..].iter().all(|row| row.len() == width * 2));
                let middle = &glyph[1 + width / 2];
                assert!(
                    u8::from_str_radix(&middle[width / 2 * 2..width / 2 * 2 + 2], 16).unwrap()
                        > 128
                );
                assert!(u8::from_str_radix(&middle[..2], 16).unwrap() < 128);
            }
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i2_selawik_bitmap_sdf_matches_freetype() {
    let path = paths::testdata()
        .join("harfbuzz/test/shape/data/text-rendering-tests/fonts/Selawik-variable.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let outlines = sofel_outline::Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let mut output = Vec::new();
    let mut count = 0;
    for ppem in [12, 24, 48] {
        let reference = Command::new(paths::ft_dump())
            .args(["--raster", &ppem.to_string(), "--bitmap-sdf"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(reference.status.success());
        let faces = parse(&String::from_utf8(reference.stdout).unwrap()).unwrap();
        for (&gid, expected) in &faces[&0].glyphs {
            let source = sofel_cli::glyph::render(
                &outlines,
                sofel_font::read::GlyphId(gid),
                ppem,
                &mut scratch,
                &mut raster,
            )
            .unwrap();
            let b = sofel_raster::bitmap_sdf(source, 8, &mut output).unwrap();
            let mut actual = vec![format!(
                "bitmap {} {} {} {}",
                b.left, b.top, b.width, b.rows
            )];
            if b.width > 0 {
                actual.extend(
                    b.pixels
                        .chunks_exact(b.width as usize)
                        .map(|row| row.iter().map(|v| format!("{v:02x}")).collect::<String>()),
                );
            }
            assert_eq!(&actual, expected, "{gid} at {ppem} ppem");
            count += 1;
        }
    }
    assert_eq!(count, 1152);
}

#[test]
fn adr0064_i2_selawik_outline_sdf_matches_freetype() {
    let path = paths::testdata()
        .join("harfbuzz/test/shape/data/text-rendering-tests/fonts/Selawik-variable.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let outlines = sofel_outline::Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let mut count = 0;
    for ppem in [12, 24, 48] {
        let reference = Command::new(paths::ft_dump())
            .args(["--raster", &ppem.to_string(), "--sdf"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(reference.status.success());
        let faces = parse(&String::from_utf8(reference.stdout).unwrap()).unwrap();
        for (&gid, expected) in &faces[&0].glyphs {
            let info = outlines
                .load(
                    sofel_font::read::GlyphId(gid),
                    sofel_outline::Scale::pixels(&font, ppem, ppem),
                    &mut scratch,
                )
                .unwrap();
            let c = info.cbox;
            let mut bounds = sofel_raster::pixel_box(c.x_min, c.y_min, c.x_max, c.y_max);
            if bounds.width > 0 && bounds.rows > 0 {
                bounds.width += 16;
                bounds.rows += 16;
                bounds.left -= 8;
                bounds.top += 8;
            }
            raster.clear();
            scratch.decompose(bounds.shift(), 1, &mut raster).unwrap();
            let b = raster
                .render_outline_sdf_placed(bounds, 8, scratch.fill_left())
                .unwrap();
            let mut actual = vec![format!(
                "bitmap {} {} {} {}",
                b.left, b.top, b.width, b.rows
            )];
            if b.width > 0 {
                actual.extend(
                    b.pixels
                        .chunks_exact(b.width as usize)
                        .map(|row| row.iter().map(|v| format!("{v:02x}")).collect::<String>()),
                );
            }
            assert_eq!(&actual, expected, "{gid} at {ppem} ppem");
            count += 1;
        }
    }
    assert_eq!(count, 1152);
}

#[test]
fn adr0064_i2_selawik_monochrome_bitmaps_match_freetype() {
    use sofel_font::read::GlyphId;
    use sofel_outline::Outlines;
    let path = paths::testdata()
        .join("harfbuzz/test/shape/data/text-rendering-tests/fonts/Selawik-variable.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let mut mismatches = Vec::new();
    let mut count = 0;
    for ppem in [12, 24, 48] {
        let reference = Command::new(paths::ft_dump())
            .args(["--raster", &ppem.to_string(), "--mono"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(reference.status.success());
        let faces = parse(&String::from_utf8(reference.stdout).unwrap()).unwrap();
        for (&gid, expected) in &faces[&0].glyphs {
            let bitmap = sofel_cli::glyph::render_mono(
                &outlines,
                GlyphId(gid),
                ppem,
                &mut scratch,
                &mut raster,
            )
            .unwrap();
            let b = bitmap.bounds;
            let mut actual = vec![format!(
                "bitmap {} {} {} {}",
                b.left, b.top, b.width, b.rows
            )];
            actual.extend(
                bitmap
                    .pixels
                    .chunks_exact(bitmap.stride)
                    .map(|row| row.iter().map(|v| format!("{v:02x}")).collect::<String>()),
            );
            if &actual != expected {
                eprintln!(
                    "gid {gid} size {ppem}: {:?}",
                    actual
                        .iter()
                        .zip(expected)
                        .enumerate()
                        .filter(|(_, (a, b))| a != b)
                        .take(3)
                        .collect::<Vec<_>>()
                );
                mismatches.push((ppem, gid));
            }
            count += 1;
        }
    }
    assert!(count > 0);
    eprintln!(
        "{} mismatched of {count} monochrome glyphs",
        mismatches.len()
    );
    assert!(
        mismatches.is_empty(),
        "{:?}",
        mismatches.iter().take(20).collect::<Vec<_>>()
    );
}
