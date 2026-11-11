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
    assert_stream_matches(&out);
}

#[test]
fn outlines_agree_with_freetype() {
    let testdata = paths::testdata();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    let list = std::fs::read_to_string(paths::conformance_file(LIST)).unwrap_or_default();
    let mut expected = expect::parse(&list);
    expected.retain(|id| id.contains(":outline:"));
    let mut results = ft_stream::Results::default();
    let mut scratch = Scratch::default();
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap_or_else(|e| panic!("{e}"));
    for path in fonts::harfbuzz_fonts(&testdata) {
        let id = fonts::id(&path, &testdata);
        let bytes = std::fs::read(&path).unwrap();
        for size in SIZES {
            let mut pending = ft_stream::Results::default();
            let mut cached_face = None;
            let mut outlines_and_scale = None;
            let success = ft_stream::reference(
                Command::new(&exe).args(["--outline", size]).arg(&path),
                |face, glyph, ft_lines| {
                    if cached_face != Some(face) {
                        cached_face = Some(face);
                        outlines_and_scale =
                            Font::new(&bytes, face).ok().filter(has_glyf).map(|font| {
                                (
                                    sofel_outline::Outlines::new(font.default_instance()),
                                    scale_for(&font, size),
                                )
                            });
                    }
                    let Some((outlines, scale)) = outlines_and_scale.as_ref() else {
                        return;
                    };
                    let ours = sofel_glyph(outlines, glyph, *scale, &mut scratch);
                    pending.compare_glyph(
                        format!("{id}#{face}:outline:{size}:{glyph}"),
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
    eprintln!("{} outline items, {} failed", results.count, results.failed);
    results.assert_expected(&expected, LIST, "outline.problems");
}

#[path = "support/ft_stream.rs"]
mod ft_stream;

fn assert_stream_matches(input: &str) {
    let old: Vec<_> = parse(input)
        .unwrap()
        .into_iter()
        .flat_map(|(face, value)| {
            value
                .glyphs
                .into_iter()
                .map(move |(glyph, lines)| (face, glyph, lines))
        })
        .collect();
    let mut new = Vec::new();
    ft_stream::stream(std::io::Cursor::new(input), |face, glyph, lines| {
        new.push((face, glyph, lines.to_vec()));
    })
    .unwrap();
    assert_eq!(new, old);
}

#[test]
fn adr0040_i1_i2_error_then_normal_outline() {
    assert_stream_matches(
        "face 0\nnum_glyphs 2\nglyph 0\nerror 8\nglyph 1\nM 1 2\nZ\noverlap 0\ncbox 1 2 1 2\nend\n",
    );
}

#[test]
fn adr0027_variable_benchmark_outlines_match_every_coordinate() {
    let root = paths::testdata().join("harfbuzz/test/subset/data/fonts");
    for name in [
        "RobotoFlex-Variable.ttf",
        "SourceSerifVariable-Roman.ttf",
        "AdobeVFPrototype.otf",
    ] {
        let path = root.join(name);
        let bytes = std::fs::read(&path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let weight = sofel_font::var::Variation::parse("wght=500").unwrap();
        let location = sofel_font::var::Location::new(&font, None, &[weight]);
        let outlines = sofel_outline::Outlines::new(font.instance(&location));
        let mut scratch = Scratch::default();
        for size in SIZES {
            let scale = scale_for(&font, size);
            let mut glyphs = 0;
            let ran = ft_stream::reference(
                Command::new(paths::ft_dump())
                    .args(["--outline", size, "--variations=wght=500"])
                    .arg(&path),
                |face, glyph, expected| {
                    assert_eq!(face, 0);
                    let ours = sofel_glyph(&outlines, glyph, scale, &mut scratch);
                    assert_eq!(
                        ours, expected,
                        "{name}#{face}:outline:{size}:{glyph}@wght=500"
                    );
                    glyphs += 1;
                },
            )
            .unwrap();
            assert!(ran);
            assert_eq!(glyphs, font.num_glyphs(), "{name} at {size}");
        }
    }
}
