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
    let mut items: Vec<(String, bool, String)> = Vec::new();
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    for path in fonts::harfbuzz_fonts(&testdata) {
        let id = fonts::id(&path, &testdata);
        let bytes = std::fs::read(&path).unwrap();
        for size in PPEMS {
            let Some(text) = ft_dump_raster(size, &path) else {
                continue;
            };
            for (face, ft) in parse(&text).unwrap() {
                if ft.error {
                    continue;
                }
                let Ok(font) = Font::new(&bytes, face) else {
                    continue;
                };
                if !has_glyf(&font) {
                    continue;
                }
                for (glyph, ft_lines) in &ft.glyphs {
                    let ours = sofel_glyph(
                        &font,
                        *glyph,
                        size.parse().unwrap(),
                        &mut scratch,
                        &mut raster,
                    );
                    let ok = &ours == ft_lines;
                    let detail = if ok {
                        String::new()
                    } else {
                        let i = ft_lines
                            .iter()
                            .zip(&ours)
                            .position(|(a, b)| a != b)
                            .unwrap_or(0);
                        format!("FreeType {:?} / sofel {:?}", ft_lines.get(i), ours.get(i))
                    };
                    items.push((format!("{id}#{face}:raster:{size}:{glyph}"), ok, detail));
                }
            }
        }
    }
    let failed = items.iter().filter(|i| !i.1).count();
    eprintln!("{} raster items, {failed} failed", items.len());

    let list = std::fs::read_to_string(paths::conformance_file(LIST)).unwrap_or_default();
    let mut expected = expect::parse(&list);
    expected.retain(|id| id.contains(":raster:"));
    let problems = expect::judge(
        items.iter().map(|(id, ok, _)| (id.as_str(), *ok)),
        &expected,
    );
    if !problems.is_empty() {
        // Every problem, for reviewing or updating the list.
        let dir = paths::root().join("target/conformance");
        let _ = std::fs::create_dir_all(&dir);
        let all: String = expect::report(&problems, LIST);
        let _ = std::fs::write(dir.join("raster.problems"), all);
        let mut msg = expect::report(&problems[..problems.len().min(200)], LIST);
        for p in problems.iter().take(50) {
            if let expect::Problem::Regressed(id) = p
                && let Some(i) = items.iter().find(|i| &i.0 == id)
            {
                msg.push_str(&format!("  {id}: {}\n", i.2));
            }
        }
        panic!("{msg}\n{} problems", problems.len());
    }
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
