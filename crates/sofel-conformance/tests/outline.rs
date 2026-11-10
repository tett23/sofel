//! sofel-outline against FreeType (ADR 0011): `ft-dump --outline` on
//! known glyphs, and on every glyph of HarfBuzz's test fonts with glyf.

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
mod build;

use std::path::{Path, PathBuf};
use std::process::Command;

use build::*;
use sofel_conformance::outline::{has_glyf, parse, scale_for, sofel_glyph};
use sofel_conformance::{expect, fonts, paths};
use sofel_font::Font;
use sofel_outline::Scratch;

const LIST: &str = "expected-failures-freetype.txt";
const SIZES: [&str; 3] = ["unscaled", "16", "48"];

fn ft_dump_outline(size: &str, font: &Path) -> Option<String> {
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new(exe)
        .args(["--outline", size])
        .arg(font)
        .output()
        .unwrap();
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).unwrap())
}

fn temp_font(name: &str, bytes: &[u8]) -> PathBuf {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!("sofel-outline-{name}-{}.ttf", std::process::id()));
    std::fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn ft_dump_writes_known_outlines() {
    let square = simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::On(0, 700),
            Pt::On(500, 700),
            Pt::On(500, 0),
        ]],
        false,
        &[],
    );
    let quad = simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::Off(100, 101),
            Pt::Off(201, -3),
            Pt::On(300, 0),
        ]],
        true,
        &[],
    );
    let font = glyph_font(&[Vec::new(), square, quad], true, false);
    let path = temp_font("known", &font);
    let out = ft_dump_outline("unscaled", &path).unwrap();
    let _ = std::fs::remove_file(&path);
    let expected = "\
face 0
num_glyphs 3
glyph 0
overlap 0
cbox 0 0 0 0
glyph 1
M 0 0
L 0 700
L 500 700
L 500 0
L 0 0
Z
overlap 0
cbox 0 0 500 700
glyph 2
M 0 0
Q 100 101 150 49
Q 201 -3 300 0
L 0 0
Z
overlap 1
cbox 0 -3 300 101
end
";
    assert_eq!(out, expected);
}

#[test]
fn outlines_agree_with_freetype() {
    let testdata = paths::testdata();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    let mut items: Vec<(String, bool, String)> = Vec::new();
    let mut scratch = Scratch::default();
    for path in fonts::harfbuzz_fonts(&testdata) {
        let id = fonts::id(&path, &testdata);
        let bytes = std::fs::read(&path).unwrap();
        for size in SIZES {
            let Some(text) = ft_dump_outline(size, &path) else {
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
                let scale = scale_for(&font, size);
                for (glyph, ft_lines) in &ft.glyphs {
                    let ours = sofel_glyph(&font, *glyph, scale, &mut scratch);
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
                    items.push((format!("{id}#{face}:outline:{size}:{glyph}"), ok, detail));
                }
            }
        }
    }
    let failed = items.iter().filter(|i| !i.1).count();
    eprintln!("{} outline items, {failed} failed", items.len());

    let list = std::fs::read_to_string(paths::conformance_file(LIST)).unwrap_or_default();
    let mut expected = expect::parse(&list);
    expected.retain(|id| id.contains(":outline:"));
    let problems = expect::judge(
        items.iter().map(|(id, ok, _)| (id.as_str(), *ok)),
        &expected,
    );
    if !problems.is_empty() {
        // Every problem, for reviewing or updating the list.
        let dir = paths::root().join("target/conformance");
        let _ = std::fs::create_dir_all(&dir);
        let all: String = expect::report(&problems, LIST);
        let _ = std::fs::write(dir.join("outline.problems"), all);
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
