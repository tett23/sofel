//! FreeType's CFF hinter on built fonts that reach its corners (sofel
//! ADR 0064 I1): stem pairs, ghost and inverted stems, hint replacement
//! in the middle of a contour, counter masks, seac accents hinted at
//! their offset, overlapping stems, too many stems, blue zones with
//! OtherBlues, FamilyBlues, BlueShift, BlueFuzz and a large BlueScale,
//! and the em box hints of LanguageGroup 1. The slot before rendering
//! (advance, points, tags and contour ends) of every glyph is compared
//! with `ft-dump --native-hint` from 1 to 48 ppem for the five targets.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::collections::BTreeMap;
use std::process::Command;

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_outline::{HintMode, HintTarget, Outlines, Scratch};

#[path = "../../sofel-read/tests/support/build.rs"]
#[allow(dead_code)]
mod build;

use build::{Cff1Spec, Cs, cff1, charstring, d_int, d_op, otto_font};

const TARGETS: [(&str, HintTarget); 5] = [
    ("normal", HintTarget::Normal),
    ("light", HintTarget::Light),
    ("mono", HintTarget::Mono),
    ("lcd", HintTarget::Lcd),
    ("lcd-v", HintTarget::VerticalLcd),
];

/// A Private DICT entry with delta-encoded operands.
fn deltas(values: &[i32], op: u16) -> Vec<u8> {
    let mut out = Vec::new();
    let mut last = 0;
    for &v in values {
        out.extend(d_int(v - last));
        last = v;
    }
    out.extend(d_op(op));
    out
}

fn entry(values: &[i32], op: u16) -> Vec<u8> {
    let mut out: Vec<u8> = values.iter().flat_map(|&v| d_int(v)).collect();
    out.extend(d_op(op));
    out
}

use Cs::{I, Op};

const HSTEMHM: Cs = Op(18);
const VSTEMHM: Cs = Op(23);
const HINTMASK: Cs = Op(19);
const CNTRMASK: Cs = Op(20);
const RMOVETO: Cs = Op(21);
const RLINETO: Cs = Op(5);
const RRCURVETO: Cs = Op(8);
const ENDCHAR: Cs = Op(14);

fn raw(b: &[u8]) -> Cs {
    Cs::Raw(b.to_vec())
}

/// A box from (x, y) of width w and height h, as rlineto.
fn rect(x: i32, y: i32, w: i32, h: i32) -> Vec<Cs> {
    vec![
        I(x),
        I(y),
        RMOVETO,
        I(w),
        I(0),
        I(0),
        I(h),
        I(-w),
        I(0),
        RLINETO,
    ]
}

/// The glyphs: index 34 is the seac base (`A`), 125 the accent
/// (`acute`); the others are empty.
fn glyphs() -> Vec<Vec<u8>> {
    let mut glyphs = vec![charstring(&[ENDCHAR]); 130];
    // 1: two stem pairs, two vertical stems, every hint at once.
    let mut g = vec![
        I(0),
        I(52),
        I(447),
        I(53),
        HSTEMHM,
        I(100),
        I(60),
        I(240),
        I(60),
        VSTEMHM,
        HINTMASK,
        raw(&[0xF0]),
    ];
    g.extend(rect(100, 0, 360, 52));
    g.extend([I(0), I(447), RMOVETO]);
    g.extend([I(360), I(0), I(0), I(53), I(-360), I(0), RLINETO]);
    g.extend([I(0), I(-250), RMOVETO]);
    g.extend([
        I(60),
        I(-30),
        I(120),
        I(30),
        I(180),
        I(100),
        RRCURVETO,
        I(-300),
        I(0),
        RLINETO,
        ENDCHAR,
    ]);
    glyphs[1] = charstring(&g);
    // 2: ghost bottom (-21), ghost top (-20) and an inverted pair.
    let mut g = vec![
        I(10),
        I(-21),
        I(531),
        I(-20),
        I(-100),
        I(-40),
        HSTEMHM,
        HINTMASK,
        raw(&[0xE0]),
    ];
    g.extend(rect(50, -11, 300, 520));
    g.extend([
        I(50),
        I(-200),
        RMOVETO,
        I(100),
        I(-60),
        I(-100),
        I(0),
        RLINETO,
        ENDCHAR,
    ]);
    glyphs[2] = charstring(&g);
    // 3: hint replacement in the middle of a contour, with curves.
    let g = vec![
        I(0),
        I(40),
        I(160),
        I(40),
        I(160),
        I(40),
        HSTEMHM,
        HINTMASK,
        raw(&[0xC0]),
        I(80),
        I(0),
        RMOVETO,
        I(200),
        I(0),
        I(0),
        I(240),
        RLINETO,
        HINTMASK,
        raw(&[0x60]),
        I(30),
        I(40),
        I(30),
        I(80),
        I(0),
        I(120),
        RRCURVETO,
        HINTMASK,
        raw(&[0xA0]),
        I(-230),
        I(0),
        I(0),
        I(-200),
        I(0),
        I(-120),
        I(0),
        I(-120),
        RLINETO,
        ENDCHAR,
    ];
    glyphs[3] = charstring(&g);
    // 4: a counter mask over three stems, then two hint masks.
    let mut g4 = vec![
        I(0),
        I(50),
        I(200),
        I(50),
        I(200),
        I(50),
        HSTEMHM,
        CNTRMASK,
        raw(&[0xE0]),
        HINTMASK,
        raw(&[0xA0]),
    ];
    g4.extend(rect(60, 0, 380, 50));
    g4.extend([
        I(0),
        I(500),
        RMOVETO,
        I(380),
        I(0),
        I(0),
        I(50),
        I(-380),
        I(0),
        RLINETO,
    ]);
    g4.extend([HINTMASK, raw(&[0x40])]);
    g4.extend([
        I(0),
        I(-300),
        RMOVETO,
        I(380),
        I(0),
        I(0),
        I(50),
        I(-380),
        I(0),
        RLINETO,
    ]);
    g4.push(ENDCHAR);
    glyphs[4] = charstring(&g4);
    // 5: stems that touch and overlap; only some reach the map.
    let mut g = vec![
        I(0),
        I(100),
        I(0),
        I(50),
        I(-75),
        I(150),
        I(200),
        I(30),
        HSTEMHM,
    ];
    g.extend(rect(40, 0, 300, 180));
    g.extend([
        I(0),
        I(150),
        RMOVETO,
        I(300),
        I(0),
        I(0),
        I(260),
        I(-300),
        I(0),
        RLINETO,
        ENDCHAR,
    ]);
    glyphs[5] = charstring(&g);
    // 6: 97 stems, more than FreeType's 96.
    let mut g = Vec::new();
    for _ in 0..24 {
        g.extend([I(5), I(5)]);
    }
    g.push(HSTEMHM);
    for _ in 0..24 {
        g.extend([I(5), I(5)]);
    }
    g.push(HSTEMHM);
    for _ in 0..24 {
        g.extend([I(5), I(5)]);
    }
    g.push(VSTEMHM);
    for _ in 0..25 {
        g.extend([I(5), I(5)]);
    }
    g.push(VSTEMHM);
    g.extend(rect(0, 0, 100, 100));
    g.push(ENDCHAR);
    glyphs[6] = charstring(&g);
    // 7: a seac of glyph 34 and the accent 125 raised by 480.
    glyphs[7] = charstring(&[I(0), I(120), I(480), I(65), I(194), ENDCHAR]);
    // 8: points between, below and above the stems, unhinted moves.
    let mut g = vec![I(-30), I(30), I(300), I(25), HSTEMHM];
    g.extend([
        I(10),
        I(-60),
        RMOVETO,
        I(100),
        I(30),
        I(100),
        I(60),
        I(50),
        I(150),
        RRCURVETO,
        I(30),
        I(500),
        RLINETO,
        I(-200),
        I(0),
        I(-80),
        I(-200),
        I(0),
        I(-300),
        RRCURVETO,
        ENDCHAR,
    ]);
    glyphs[8] = charstring(&g);
    // 34: the base, with its own stems.
    let mut g = vec![I(0), I(60), I(380), I(60), HSTEMHM];
    g.extend(rect(30, 0, 400, 60));
    g.extend([
        I(0),
        I(440),
        RMOVETO,
        I(400),
        I(0),
        I(0),
        I(60),
        I(-400),
        I(0),
        RLINETO,
        ENDCHAR,
    ]);
    glyphs[34] = charstring(&g);
    // 125: the accent, hinted from its own origin.
    let mut g = vec![I(20), I(45), HSTEMHM];
    g.extend(rect(100, 20, 120, 45));
    g.push(ENDCHAR);
    glyphs[125] = charstring(&g);
    glyphs
}

fn font(private: Vec<u8>) -> Vec<u8> {
    let glyphs = glyphs();
    let n = glyphs.len() as u16;
    let mut spec = Cff1Spec::new(glyphs);
    spec.private = Some(private);
    otto_font(n, vec![(b"CFF ", cff1(&spec))])
}

/// The Private DICTs of the fonts.
fn privates() -> Vec<(&'static str, Vec<u8>)> {
    let blue_scale_large = [30u8, 0x0a, 0x2f]; // 0.2
    vec![
        ("plain", Vec::new()),
        (
            "blues",
            [
                deltas(&[-15, 0, 500, 512, 700, 715], 6),
                deltas(&[-250, -240], 7),
                entry(&[2], 0x0C0A),
                entry(&[1], 0x0C0B),
                entry(&[50], 10),
                entry(&[60], 11),
            ]
            .concat(),
        ),
        (
            "family",
            [
                deltas(&[-16, 0, 498, 510, 705, 717], 6),
                deltas(&[-252, -242], 7),
                deltas(&[-14, 1, 499, 509, 703, 715], 8),
                deltas(&[-250, -241, -100, -95], 9),
                {
                    let mut v = blue_scale_large.to_vec();
                    v.extend(d_op(0x0C09));
                    v
                },
                entry(&[0], 0x0C0B),
                entry(&[20], 0x0C0A),
            ]
            .concat(),
        ),
        ("cjk", [entry(&[1], 0x0C11)].concat()),
        (
            "cjk-icf",
            [deltas(&[-250, -240, 1100, 1110], 6), entry(&[1], 0x0C11)].concat(),
        ),
    ]
}

/// `ft-dump --native-hint` slots (without bitmaps), keyed by ppem and
/// glyph.
fn reference(path: &std::path::Path, target: &str) -> BTreeMap<(u16, u32), Vec<String>> {
    let out = Command::new(paths::ft_dump())
        .args(["--native-hint", "1", "48", target])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut map = BTreeMap::new();
    let (mut size, mut glyph) = (0u16, 0u32);
    for line in String::from_utf8(out.stdout).unwrap().lines() {
        if let Some(rest) = line.strip_prefix("size ") {
            size = rest.parse().unwrap();
        } else if let Some(rest) = line.strip_prefix("glyph ") {
            glyph = rest.parse().unwrap();
            map.insert((size, glyph), Vec::new());
        } else if line.starts_with("advance")
            || line.starts_with("point")
            || line.starts_with("end")
            || line.starts_with("error")
        {
            map.get_mut(&(size, glyph)).unwrap().push(line.to_string());
        }
    }
    map
}

#[test]
fn adr0064_i1_built_cff_hints_match_freetype() {
    paths::require(&paths::ft_dump(), "ft-dump").unwrap();
    let dir = std::env::temp_dir();
    let mut compared = 0;
    let mut errors = 0;
    for (name, private) in privates() {
        let bytes = font(private);
        let path = dir.join(format!(
            "sofel-adr0064-cff-hint-{name}-{}.otf",
            std::process::id()
        ));
        std::fs::write(&path, &bytes).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let outlines = Outlines::new(font.default_instance());
        let mut scratch = Scratch::default();
        for (target_name, target) in TARGETS {
            for ((ppem, gid), expected) in reference(&path, target_name) {
                let mut actual = Vec::new();
                match outlines.load_with_hint(
                    GlyphId(gid),
                    ppem,
                    HintMode::NativeV40,
                    target,
                    &mut scratch,
                ) {
                    Err(_) => actual.push("error".to_string()),
                    Ok((_, advance)) => {
                        actual.push(format!("advance {advance} 0"));
                        for (p, tag) in scratch.points().iter().zip(scratch.point_tags()) {
                            actual.push(format!("point {} {} {tag}", p.x, p.y));
                        }
                        for end in scratch.contour_ends() {
                            actual.push(format!("end {end}"));
                        }
                    }
                }
                if expected.first().is_some_and(|l| l.starts_with("error")) {
                    assert_eq!(actual, ["error"], "{name} {target_name} {ppem} {gid}");
                    errors += 1;
                } else {
                    assert_eq!(actual, expected, "{name} {target_name} {ppem} {gid}");
                }
                compared += 1;
            }
        }
        std::fs::remove_file(path).unwrap();
    }
    // Five fonts, 130 glyphs, 48 sizes, five targets; glyph 6 fails.
    assert_eq!(compared, 5 * 130 * 48 * 5);
    assert_eq!(errors, 5 * 48 * 5);
}
