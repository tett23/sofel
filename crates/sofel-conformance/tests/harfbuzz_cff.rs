//! sofel's CFF and CFF2 glyph boxes and names against HarfBuzz on small
//! hand-made fonts (ADR 0021): each one exercises a rule of HarfBuzz's
//! accelerators or interpreter (the cases of `sofel-font`'s and
//! `sofel-outline`'s unit tests), and `hb-dump` must agree with sofel on
//! every item, at every scale.

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

use std::path::PathBuf;
use std::process::Command;

use build::*;
use sofel_conformance::{hb, paths};
use sofel_font::Font;

fn hb_dump(mode: &str, name: &str, bytes: &[u8]) -> String {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!("sofel-hb-cff-{name}-{}.otf", std::process::id()));
    std::fs::write(&p, bytes).unwrap();
    let out = Command::new(exe).arg(mode).arg(&p).output().unwrap();
    std::fs::remove_file(&p).ok();
    assert!(out.status.success(), "hb-dump {mode} failed on {name}");
    String::from_utf8(out.stdout).unwrap()
}

/// Every item of the metrics and names comparisons of `bytes`.
fn check(name: &str, bytes: &[u8]) -> Vec<String> {
    let font = Font::new(bytes, 0).unwrap();
    let mut failed = Vec::new();
    for face in hb::parse(&hb_dump("--metrics", name, bytes)).unwrap() {
        let ours = hb::sofel_face(&font, &face);
        for i in hb::compare(name, &face, &ours) {
            if !i.ok {
                failed.push(format!("{}: {}", i.id, i.detail));
            }
        }
    }
    for face in hb::parse_names(&hb_dump("--names", name, bytes)).unwrap() {
        let ours = hb::sofel_names(&font, &face);
        for i in hb::compare_names(name, &face, &ours) {
            if !i.ok {
                failed.push(format!("{}: {}", i.id, i.detail));
            }
        }
    }
    failed
}

const RMOVETO: u16 = 21;
const RLINETO: u16 = 5;
const RRCURVETO: u16 = 8;
const ENDCHAR: u16 = 14;
const CALLSUBR: u16 = 10;
const RETURN: u16 = 11;

fn line_glyph(dx: i32, dy: i32) -> Vec<u8> {
    use Cs::*;
    charstring(&[
        I(0),
        I(0),
        Op(RMOVETO),
        I(dx),
        I(dy),
        Op(RLINETO),
        Op(ENDCHAR),
    ])
}

fn cff_font(spec: &Cff1Spec) -> Vec<u8> {
    otto_font(spec.charstrings.len() as u16, vec![(b"CFF ", cff1(spec))])
}

fn cff2_font(spec: &Cff2Spec, extra: Vec<(&'static [u8; 4], Vec<u8>)>, glyphs: u16) -> Vec<u8> {
    let mut tables = vec![(b"CFF2", cff2(spec))];
    tables.extend(extra);
    otto_font(glyphs, tables)
}

fn cases() -> Vec<(&'static str, Vec<u8>)> {
    use Cs::*;
    let mut out = Vec::new();

    // Paths: control points, moves only, an empty axis, rounding,
    // arithmetic operators, no endchar.
    let glyphs = vec![
        charstring(&[
            I(10),
            I(20),
            Op(RMOVETO),
            I(100),
            I(200),
            I(0),
            I(0),
            I(-50),
            I(-150),
            Op(RRCURVETO),
            Op(ENDCHAR),
        ]),
        charstring(&[
            I(-500),
            I(-500),
            Op(RMOVETO),
            I(510),
            I(520),
            Op(RMOVETO),
            I(10),
            I(10),
            Op(RLINETO),
            Op(ENDCHAR),
        ]),
        charstring(&[I(5), I(5), Op(RMOVETO), Op(ENDCHAR)]),
        charstring(&[
            I(10),
            I(20),
            Op(RMOVETO),
            I(30),
            I(0),
            Op(RLINETO),
            Op(ENDCHAR),
        ]),
        charstring(&[
            F((300 << 16) | 0x7FFF),
            F(-0x8000),
            Op(RMOVETO),
            I(10),
            I(10),
            Op(RLINETO),
            Op(ENDCHAR),
        ]),
        charstring(&[
            F(-(300 << 16) - 0x8000),
            F(0x8000),
            Op(RMOVETO),
            F(0x1_8000),
            F(0x2_8000),
            Op(RLINETO),
            Op(ENDCHAR),
        ]),
        charstring(&[I(1), I(2), Op(0x0C0A), I(5), I(5), Op(RLINETO), Op(ENDCHAR)]),
        charstring(&[I(0), I(0), Op(RMOVETO), I(5), I(5), Op(RLINETO)]),
        // hv/vh curves with odd counts, flexes.
        charstring(&[
            I(0),
            I(0),
            Op(RMOVETO),
            I(10),
            I(20),
            I(30),
            I(40),
            I(50),
            I(60),
            I(70),
            I(80),
            I(5),
            Op(31),
            I(1),
            I(2),
            I(3),
            I(4),
            I(5),
            Op(30),
            I(7),
            I(1),
            I(2),
            I(3),
            I(4),
            Op(26),
            I(7),
            I(1),
            I(2),
            I(3),
            I(4),
            Op(27),
            Op(ENDCHAR),
        ]),
        charstring(&[
            I(0),
            I(0),
            Op(RMOVETO),
            I(10),
            I(20),
            I(30),
            I(-5),
            I(40),
            I(60),
            I(70),
            I(-80),
            I(90),
            I(10),
            I(-20),
            I(30),
            I(50),
            Op(0x0C23),
            I(5),
            I(6),
            I(7),
            I(8),
            I(9),
            I(10),
            I(11),
            Op(0x0C22),
            I(1),
            I(2),
            I(3),
            I(4),
            I(5),
            I(6),
            I(7),
            I(8),
            I(9),
            Op(0x0C24),
            I(1),
            I(2),
            I(3),
            I(4),
            I(5),
            I(6),
            I(7),
            I(8),
            I(9),
            I(10),
            I(11),
            Op(0x0C25),
            Op(ENDCHAR),
        ]),
        // rcurveline, rlinecurve, hintmask after stems.
        charstring(&[
            I(1),
            I(2),
            I(3),
            I(4),
            Op(1),
            I(5),
            I(6),
            Op(19),
            Raw(vec![0xC0]),
            I(0),
            I(0),
            Op(RMOVETO),
            I(1),
            I(2),
            I(3),
            I(4),
            I(5),
            I(6),
            I(7),
            I(8),
            Op(24),
            I(1),
            I(2),
            I(3),
            I(4),
            I(5),
            I(6),
            I(7),
            I(8),
            Op(25),
            Op(ENDCHAR),
        ]),
    ];
    out.push(("paths", cff_font(&Cff1Spec::new(glyphs))));

    // seac: base and accent, a nested seac, a code without a glyph.
    let mut cs = vec![charstring(&[Op(ENDCHAR)]); 125];
    cs[34] = line_glyph(100, 100);
    cs[124] = line_glyph(10, 10);
    cs[1] = charstring(&[I(50), I(200), I(65), I(193), Op(ENDCHAR)]);
    cs[2] = charstring(&[I(0), I(0), I(65), I(66), Op(ENDCHAR)]);
    cs[35] = charstring(&[I(0), I(0), I(65), I(193), Op(ENDCHAR)]);
    cs[3] = charstring(&[I(0), I(0), I(65), I(0), Op(ENDCHAR)]);
    cs[4] = charstring(&[I(9), I(-30), I(-40), I(65), I(193), Op(ENDCHAR)]);
    out.push(("seac", cff_font(&Cff1Spec::new(cs))));

    // Subroutine nesting 10 and 11: subroutine k calls k + 1.
    let mut subrs: Vec<Vec<u8>> = (0..10)
        .map(|k| charstring(&[I(k + 1 - 107), Op(CALLSUBR), Op(RETURN)]))
        .collect();
    subrs.push(charstring(&[I(1), I(1), Op(RLINETO), Op(RETURN)]));
    let mut spec = Cff1Spec::new(vec![
        charstring(&[
            I(0),
            I(0),
            Op(RMOVETO),
            I(1 - 107),
            Op(CALLSUBR),
            Op(ENDCHAR),
        ]),
        charstring(&[I(0), I(0), Op(RMOVETO), I(-107), Op(CALLSUBR), Op(ENDCHAR)]),
    ]);
    spec.lsubrs = subrs;
    out.push(("nesting", cff_font(&spec)));

    // The operator limit.
    let long = |n: usize| {
        let mut items = vec![I(0), I(0), Op(RMOVETO)];
        for _ in 0..n {
            items.extend([I(0), I(0), Op(RLINETO)]);
        }
        items.push(Op(ENDCHAR));
        charstring(&items)
    };
    out.push((
        "limit",
        cff_font(&Cff1Spec::new(vec![long(66665), long(66666)])),
    ));

    // Names: standard strings, strings, an empty one, a missing one,
    // duplicates of both kinds.
    let mut spec = Cff1Spec::new((0..8).map(|i| line_glyph(i, i)).collect());
    spec.charset = Some(vec![0, 0, 34, 1, 135, 1, 136, 1, 137, 0, 34, 1, 138, 0, 35]);
    spec.strings = ["foo", "", "A", "B"]
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .collect();
    out.push(("names", cff_font(&spec)));
    let mut spec = Cff1Spec::new((0..4).map(|i| line_glyph(i, i)).collect());
    spec.charset = Some(vec![1, 1, 135, 2]);
    spec.strings = ["x", "y", "x"]
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .collect();
    out.push(("names-ranges", cff_font(&spec)));

    // Validity: a charset range that overshoots (HarfBuzz does not trim),
    // an encoding supplement after no codes, a Top DICT of 514 operands.
    let mut spec = Cff1Spec::new(vec![line_glyph(1, 1), line_glyph(2, 2), line_glyph(3, 3)]);
    spec.charset = Some(vec![1, 0, 34, 5]);
    out.push(("charset-overshoot", cff_font(&spec)));
    let mut spec = Cff1Spec::new(vec![line_glyph(1, 1), line_glyph(2, 2)]);
    spec.encoding = Some(vec![0x80, 0, 1, 66, 0, 1]);
    out.push(("encoding-supplement", cff_font(&spec)));
    let mut spec = Cff1Spec::new(vec![line_glyph(1, 1), line_glyph(2, 2)]);
    spec.top = [vec![139u8; 514], vec![13]].concat();
    out.push(("top-514", cff_font(&spec)));
    let mut spec = Cff1Spec::new(vec![line_glyph(1, 1), line_glyph(2, 2)]);
    spec.top = [vec![139u8; 513], vec![13]].concat();
    out.push(("top-513", cff_font(&spec)));

    // CID-keyed: FDSelect formats, local subroutines per Font DICT.
    let cid = |fd_select: Vec<u8>| {
        let mut s = Cff1Spec::new(vec![
            line_glyph(1, 1),
            charstring(&[I(0), I(0), Op(RMOVETO), I(-107), Op(CALLSUBR), Op(ENDCHAR)]),
            charstring(&[I(0), I(0), Op(RMOVETO), I(-107), Op(CALLSUBR), Op(ENDCHAR)]),
        ]);
        s.private = None;
        s.charset = Some(vec![0, 0, 1, 0, 2]);
        s.strings = vec![b"Adobe".to_vec(), b"Identity".to_vec()];
        s.cid = Some(CidSpec {
            ros: (391, 392, 0),
            fds: vec![
                (
                    Vec::new(),
                    Vec::new(),
                    vec![charstring(&[I(5), I(5), Op(RLINETO), Op(RETURN)])],
                ),
                (
                    Vec::new(),
                    Vec::new(),
                    vec![charstring(&[I(7), I(9), Op(RLINETO), Op(RETURN)])],
                ),
            ],
            fd_select,
        });
        cff_font(&s)
    };
    out.push(("cid-fd0", cid(vec![0, 0, 1, 1])));
    out.push(("cid-fd3", cid(vec![3, 0, 2, 0, 0, 0, 0, 2, 1, 0, 3])));
    out.push(("cid-fd-past", cid(vec![0, 0, 1, 7])));
    out.push(("cid-sentinel", cid(vec![3, 0, 1, 0, 0, 0, 0, 4])));

    // CFF2: blend keeps the default, a short blend, no endchar, vsindex.
    let vs = item_variation_store(
        1,
        &[vec![(0, 0, 0)], vec![(-1, 1, 1)]],
        &[vec![0, 1], vec![1]],
    );
    let spec = Cff2Spec {
        charstrings: vec![
            charstring(&[
                I(0),
                I(0),
                Op(RMOVETO),
                I(100),
                I(50),
                I(500),
                I(500),
                I(-70),
                I(70),
                I(2),
                Op(16),
                Op(RLINETO),
            ]),
            charstring(&[I(0), I(0), I(5), I(2), Op(16), Op(RLINETO)]),
            charstring(&[I(0), I(0), Op(RMOVETO), I(7), I(9), Op(RLINETO)]),
            charstring(&[
                I(1),
                Op(15),
                I(0),
                I(0),
                Op(RMOVETO),
                I(30),
                I(40),
                I(1),
                Op(16),
                I(9),
                Op(RLINETO),
            ]),
            charstring(&[I(1), I(2), Op(16), I(1), Op(15), Op(RLINETO)]),
        ],
        fds: vec![(Vec::new(), Vec::new())],
        vstore: Some(vs),
        ..Default::default()
    };
    out.push(("cff2-blend", cff2_font(&spec, vec![], 5)));

    // CFF2 FDSelect format 4 and local subroutines.
    let spec = Cff2Spec {
        charstrings: vec![
            line_glyph(1, 1)[..6].to_vec(),
            charstring(&[I(0), I(0), Op(RMOVETO), I(-107), Op(CALLSUBR)]),
            charstring(&[I(0), I(0), Op(RMOVETO), I(-107), Op(CALLSUBR)]),
        ],
        fds: vec![
            (Vec::new(), vec![charstring(&[I(3), I(4), Op(RLINETO)])]),
            (Vec::new(), vec![charstring(&[I(-3), I(-4), Op(RLINETO)])]),
        ],
        fd_select: vec![
            4, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 0, 0, 3,
        ],
        ..Default::default()
    };
    out.push(("cff2-fd4", cff2_font(&spec, vec![], 3)));

    // CFF2 with fewer glyphs than maxp: only with VARC.
    let spec = Cff2Spec {
        charstrings: vec![
            line_glyph(1, 1)[..6].to_vec(),
            line_glyph(2, 3)[..6].to_vec(),
        ],
        fds: vec![(Vec::new(), Vec::new())],
        ..Default::default()
    };
    out.push(("cff2-short", cff2_font(&spec, vec![], 3)));

    // A broken CFF2 next to a CFF: HarfBuzz uses the CFF.
    let bad = Cff2Spec {
        charstrings: vec![line_glyph(1, 1)[..6].to_vec()],
        fds: vec![(Vec::new(), Vec::new()), (Vec::new(), Vec::new())],
        fd_select: vec![3, 0, 1, 0, 0, 0, 0, 9],
        ..Default::default()
    };
    let mut tables = vec![(b"CFF2", cff2(&bad))];
    tables.push((b"CFF ", cff1(&Cff1Spec::new(vec![line_glyph(3, 4)]))));
    out.push(("cff2-fallback", otto_font(1, tables)));

    out
}

#[test]
fn hand_made_cff_fonts_agree_with_harfbuzz() {
    let mut failed = Vec::new();
    for (name, bytes) in cases() {
        failed.extend(check(name, &bytes));
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
