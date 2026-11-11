//! Color glyphs against HarfBuzz (ADR 0023): `hb-dump --paint`,
//! `--color` and `--extents` on every face of HarfBuzz's test fonts with
//! color tables, and the 24 expected outputs of HarfBuzz's
//! `test/api/test-paint.c` (`test/api/results-paint/`), which `hb-dump`
//! must reproduce as well. Failures not listed in
//! conformance/expected-failures-harfbuzz.txt, and listed items that
//! pass, fail the test.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use sofel_color::Scratch;
use sofel_conformance::color::{self, sofel_paint_text};
use sofel_conformance::compare::Item;
use sofel_conformance::{expect, fonts, paths};
use sofel_font::Font;

const LIST: &str = "expected-failures-harfbuzz.txt";

fn hb_dump(mode: &str, font: &Path) -> Option<String> {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new(exe).arg(mode).arg(font).output().unwrap();
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).unwrap())
}

fn invalid_paint(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn paint_number<T: std::str::FromStr>(s: &str) -> io::Result<T> {
    s.parse()
        .map_err(|_| invalid_paint(&format!("invalid paint number: {s}")))
}

fn visit_paint_records(
    reader: &mut impl BufRead,
    mut visit: impl FnMut(u32, u16, u16, u32, &str),
) -> io::Result<()> {
    let mut face = None;
    let mut header = String::new();
    let mut body = String::new();
    loop {
        header.clear();
        if reader.read_line(&mut header)? == 0 {
            return if face.is_none() {
                Ok(())
            } else {
                Err(invalid_paint("missing face end"))
            };
        }
        let fields: Vec<_> = header.split_ascii_whitespace().collect();
        match fields.as_slice() {
            [] => {}
            ["face", index] if face.is_none() => face = Some(paint_number(index)?),
            ["end"] if face.is_some() => face = None,
            ["paint", p, pp, glyph, n] => {
                let face = face.ok_or_else(|| invalid_paint("paint outside a face"))?;
                let (p, pp, glyph) = (paint_number(p)?, paint_number(pp)?, paint_number(glyph)?);
                let count: u32 = paint_number(n)?;
                body.clear();
                for _ in 0..count {
                    if reader.read_line(&mut body)? == 0 {
                        return Err(invalid_paint("truncated paint body"));
                    }
                }
                visit(face, p, pp, glyph, &body);
            }
            _ => return Err(invalid_paint(&format!("invalid paint header: {header}"))),
        }
    }
}

/// Commit a command's comparison state only after a successful exit. Drain a
/// malformed stream before waiting so a writer cannot block on the pipe.
fn read_paint_command<T>(
    command: &mut Command,
    mut state: T,
    mut visit: impl FnMut(&mut T, u32, u16, u16, u32, &str),
) -> io::Result<Option<T>> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let parsed = visit_paint_records(&mut reader, |f, p, pp, g, body| {
        visit(&mut state, f, p, pp, g, body)
    });
    let drained = io::copy(&mut reader, &mut io::sink());
    drop(reader);
    let status = child.wait()?;
    drained?;
    match parsed {
        Err(e) if e.kind() != io::ErrorKind::InvalidData || status.success() => Err(e),
        _ if !status.success() => Ok(None),
        _ => Ok(Some(state)),
    }
}

fn paint_command(mode: &str, font: &Path) -> Command {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let mut command = Command::new(exe);
    command.arg(mode).arg(font);
    command
}

/// HarfBuzz's `paint_tests` of `test/api/test-paint.c`: font, slant,
/// glyph, result. The tests paint with palette 0.
/// `results-paint/` has 26 files; `test-02` and `testvf02` are not paint
/// text but logs of a test run (TAP output) that no test reads.
const PAINT_TESTS: [(&str, f32, u32, &str); 24] = [
    ("noto_handwriting-cff2_colr_1.otf", 0.0, 10, "hand-10"),
    ("noto_handwriting-cff2_colr_1.otf", 0.2, 10, "hand-10.2"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 6, "test-6"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 10, "test-10"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 92, "test-92"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 106, "test-106"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 116, "test-116"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 123, "test-123"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 154, "test-154"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 165, "test-165"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 175, "test-175"),
    ("test_glyphs-glyf_colr_1_variable.ttf", 0.0, 6, "testvf-6"),
    ("test_glyphs-glyf_colr_1_variable.ttf", 0.0, 10, "testvf-10"),
    ("test_glyphs-glyf_colr_1_variable.ttf", 0.0, 92, "testvf-92"),
    (
        "test_glyphs-glyf_colr_1_variable.ttf",
        0.0,
        106,
        "testvf-106",
    ),
    (
        "test_glyphs-glyf_colr_1_variable.ttf",
        0.0,
        116,
        "testvf-116",
    ),
    (
        "test_glyphs-glyf_colr_1_variable.ttf",
        0.0,
        123,
        "testvf-123",
    ),
    (
        "test_glyphs-glyf_colr_1_variable.ttf",
        0.0,
        154,
        "testvf-154",
    ),
    (
        "test_glyphs-glyf_colr_1_variable.ttf",
        0.0,
        165,
        "testvf-165",
    ),
    (
        "test_glyphs-glyf_colr_1_variable.ttf",
        0.0,
        175,
        "testvf-175",
    ),
    ("bad_colrv1.ttf", 0.0, 154, "bad-154"),
    ("RocherColorGX.abc.ttf", 0.3, 1, "rocher-1"),
    ("RocherColorGX.abc.ttf", 0.3, 2, "rocher-2"),
    ("RocherColorGX.abc.ttf", 0.0, 3, "rocher-3"),
];

/// The 24 results: sofel's text, and `hb-dump --paint=SLANT`'s.
fn results_paint_items(testdata: &Path) -> Vec<Item> {
    let api = testdata.join("harfbuzz/test/api");
    let mut items = Vec::new();
    let mut scratch = Scratch::default();
    for (file, slant, glyph, name) in PAINT_TESTS {
        let path = api.join("fonts").join(file);
        let expected = std::fs::read_to_string(api.join("results-paint").join(name)).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let ours = sofel_paint_text(&font, glyph, 0, 0, slant, &mut scratch);
        items.push(Item {
            id: format!("results-paint/{name}"),
            ok: ours == expected,
            detail: format!("expected:\n{expected}sofel:\n{ours}"),
        });
        // hb-dump itself writes HarfBuzz's test text.
        let hb = read_paint_command(
            &mut paint_command(&format!("--paint={slant}"), &path),
            None,
            |found, face, p, pp, g, body| {
                if found.is_none() && (face, p, pp, g) == (0, 0, 0, glyph) {
                    *found = Some(body.to_owned());
                }
            },
        )
        .unwrap()
        .expect("hb-dump --paint failed")
        .expect("missing selected glyph");
        assert_eq!(
            hb, expected,
            "hb-dump --paint does not reproduce results-paint/{name}"
        );
    }
    items
}

#[test]
fn colors_agree_with_harfbuzz() {
    let testdata = paths::testdata();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    let mut items = results_paint_items(&testdata);
    let color_fonts = color::color_fonts(&testdata);
    assert!(
        color_fonts.len() > 30,
        "found only {} color fonts",
        color_fonts.len()
    );
    for path in &color_fonts {
        let id = fonts::id(path, &testdata);
        let bytes = std::fs::read(path).unwrap();
        let mut current_face = None;
        let mut scratch = Scratch::default();
        let paints = read_paint_command(
            &mut paint_command("--paint", path),
            Vec::new(),
            |results, face, p, pp, glyph, body| {
                if current_face.as_ref().map(|(index, _)| *index) != Some(face) {
                    current_face = Some((face, Font::new(&bytes, face).ok()));
                }
                if let Some((_, Some(font))) = &current_face {
                    results.push(color::compare_paint_record(
                        &id,
                        font,
                        face,
                        (p, pp, glyph),
                        body,
                        &mut scratch,
                    ));
                }
            },
        )
        .unwrap_or_else(|e| panic!("{id}: {e}"));
        if let Some(paints) = paints {
            items.extend(paints);
        }
        if let Some(text) = hb_dump("--color", path) {
            for (index, part) in color::split_faces(&text) {
                if let Ok(font) = Font::new(&bytes, index) {
                    items.push(color::compare_color(&id, index, &font, &part));
                }
            }
        }
        if let Some(text) = hb_dump("--extents", path) {
            for (index, part) in color::split_faces(&text) {
                if let Ok(font) = Font::new(&bytes, index) {
                    items.extend(color::compare_extents(&id, index, &font, &part));
                }
            }
        }
    }
    let failed = items.iter().filter(|i| !i.ok).count();
    if let Some(path) = std::env::var_os("SOFEL_FAILED_ITEMS") {
        let text: String = items
            .iter()
            .filter(|i| !i.ok)
            .map(|i| format!("{}\t{}\n", i.id, i.detail.replace('\n', "|")))
            .collect();
        std::fs::write(path, text).unwrap();
    }
    eprintln!(
        "{} color fonts, {} items, {failed} failed",
        color_fonts.len(),
        items.len()
    );
    let list = std::fs::read_to_string(paths::conformance_file(LIST)).unwrap_or_default();
    let expected = expect::parse(&list)
        .into_iter()
        .filter(|id| !id.contains(":hb:"))
        .collect();
    let problems = expect::judge(items.iter().map(|i| (i.id.as_str(), i.ok)), &expected);
    if !problems.is_empty() {
        let mut msg = expect::report(&problems, LIST);
        for p in problems.iter().take(40) {
            if let expect::Problem::Regressed(id) = p
                && let Some(i) = items.iter().find(|i| &i.id == id)
            {
                msg.push_str(&format!("  {}: {}\n", i.id, i.detail));
            }
        }
        panic!("{msg}\n{} items, {failed} failed", items.len());
    }
}

#[test]
fn format_g3_matches_printf() {
    // C's printf("%.3g") on these values.
    let cases: [(f64, &str); 14] = [
        (1000.0, "1e+03"),
        (0.977, "0.977"),
        (-0.0, "-0"),
        (0.0, "0"),
        (1300.0, "1.3e+03"),
        (100.0, "100"),
        (1220.0, "1.22e+03"),
        (999.5, "1e+03"),
        (0.0001, "0.0001"),
        (0.00001234, "1.23e-05"),
        (3.14159, "3.14"),
        (-224.0, "-224"),
        (0.5, "0.5"),
        (2.0f32 as f64 * std::f32::consts::PI as f64, "6.28"),
    ];
    for (v, want) in cases {
        assert_eq!(color::g3(v), want, "{v}");
    }
}

#[test]
fn adr0043_i1_i2_paint_records_match_buffered_parser_at_every_chunk_size() {
    use std::io::{BufReader, Cursor};
    let valid =
        "face 0\npaint 0 0 4 0\npaint 1 24 5 2\nx\n y\nend\nface 3\npaint 0 12 8 1\nend\nend\n";
    let expected: Vec<_> = color::parse_paint(valid)
        .unwrap()
        .into_iter()
        .flat_map(|face| {
            face.paints
                .into_iter()
                .map(move |(p, pp, g, s)| (face.index, p, pp, g, s))
        })
        .collect();
    for width in [1, 2, 7, 8192] {
        let mut got = Vec::new();
        visit_paint_records(
            &mut BufReader::with_capacity(width, Cursor::new(valid)),
            |face, p, pp, g, body| {
                got.push((face, p, pp, g, body.to_owned()));
            },
        )
        .unwrap();
        assert_eq!(got, expected);
    }
    for valid_empty in ["", "face 0\nend\n"] {
        visit_paint_records(&mut Cursor::new(valid_empty), |_, _, _, _, _| {
            panic!("unexpected paint")
        })
        .unwrap();
    }
    for invalid in [
        "face 0\n",
        "end\n",
        "paint 0 0 1 0\n",
        "face bad\n",
        "face 0\nface 1\nend\n",
        "face 0\npaint 0 0 0 2\none\n",
        "face 0\npaint 0 0 0 bad\nend\n",
        "face 0\nextra\nend\n",
    ] {
        assert!(
            visit_paint_records(&mut Cursor::new(invalid), |_, _, _, _, _| {}).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn adr0043_i3_first_glyph_is_compared_before_a_later_read_error() {
    use std::io::{self, BufReader, Cursor, Read as _};
    struct Fails;
    impl io::Read for Fails {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("late read error"))
        }
    }
    let data = Cursor::new("face 0\npaint 0 0 9 1\nblue\n").chain(Fails);
    let mut got = Vec::new();
    assert!(
        visit_paint_records(&mut BufReader::new(data), |_, _, _, glyph, text| got
            .push((glyph, text.to_owned())))
        .is_err()
    );
    assert_eq!(got, [(9, "blue\n".to_owned())]);
}

#[test]
fn adr0043_i4_nonzero_reference_discards_all_partial_results() {
    let run = |script: &str| {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(script);
        read_paint_command(&mut cmd, Vec::new(), |ids, _, _, _, g, _| ids.push(g))
    };
    assert_eq!(
        run("printf 'face 0\\npaint 0 0 9 0\\nend\\n'").unwrap(),
        Some(vec![9])
    );
    assert_eq!(
        run("printf 'face 0\\npaint 0 0 9 0\\n'; exit 9").unwrap(),
        None
    );
    assert_eq!(run("printf 'bad\\n'; exit 9").unwrap(), None);
    assert!(run("printf 'face 0\\npaint 0 0 9 0\\n'").is_err());
}

#[test]
fn adr0027_variable_clip_rounds_each_edge_outward() {
    let bytes =
        std::fs::read(paths::testdata().join("harfbuzz/test/api/fonts/colr-clip-rounding.ttf"))
            .unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    for (weight, expected) in [(0.0, [50., 50., 500., 500.]), (0.5, [49., 49., 501., 501.])] {
        let location = sofel_font::var::Location::from_design(&font, &[weight, 0.0]);
        let mut recording = sofel_color::Recording::new();
        sofel_color::paint_recording_at(
            font.instance(&location),
            1,
            &Default::default(),
            &mut Scratch::default(),
            &mut recording,
        );
        let rect = recording.commands().iter().find_map(|c| match c {
            sofel_color::PaintCommand::PushClipRectangle(r) => {
                Some([r.x_min, r.y_min, r.x_max, r.y_max])
            }
            _ => None,
        });
        assert_eq!(rect, Some(expected));
    }
}
