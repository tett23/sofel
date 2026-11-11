//! CFF and CFF2 outlines through `Outlines` (ADR 0021): points, contours,
//! the FontMatrix and the scale, as FreeType loads them without hinting.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::*;
use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_font::read::cff::dict::op;
use sofel_geom::{Command, Fixed, FnPen};
use sofel_outline::{OutlineError, Outlines, Scale, Scratch};

use Cs::{F, I, Op};

fn draw_with(
    table: (&'static [u8; 4], Vec<u8>),
    glyph: u32,
    scale: Scale,
) -> Result<Vec<String>, OutlineError> {
    let bytes = otto_font(3, vec![table]);
    let font = Font::new(&bytes, 0).unwrap();
    let o = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    let mut out = Vec::new();
    let info = o.draw(
        GlyphId(glyph),
        scale,
        &mut scratch,
        &mut FnPen(|c| {
            out.push(match c {
                Command::MoveTo(p) => format!("M {} {}", p.x, p.y),
                Command::LineTo(p) => format!("L {} {}", p.x, p.y),
                Command::QuadTo(c, p) => format!("Q {} {} {} {}", c.x, c.y, p.x, p.y),
                Command::CubicTo(a, b, p) => {
                    format!("C {} {} {} {} {} {}", a.x, a.y, b.x, b.y, p.x, p.y)
                }
                Command::Close => "Z".into(),
            })
        }),
    )?;
    assert!(!info.overlap);
    let b = info.cbox;
    out.push(format!(
        "cbox {} {} {} {}",
        b.x_min, b.y_min, b.x_max, b.y_max
    ));
    Ok(out)
}

fn spec_with(glyph: Vec<u8>) -> Cff1Spec {
    Cff1Spec::new(vec![charstring(&[Op(14)]), glyph, charstring(&[Op(14)])])
}

fn draw1(glyph: &[Cs]) -> Vec<String> {
    draw_with(
        (b"CFF ", cff1(&spec_with(charstring(glyph)))),
        1,
        Scale::Unscaled,
    )
    .unwrap()
}

#[test]
fn square_and_closing() {
    assert_eq!(
        draw1(&[
            I(0),
            I(0),
            Op(21),
            I(500),
            Op(6),
            I(700),
            Op(7),
            I(-500),
            Op(6),
            Op(14)
        ]),
        vec![
            "M 0 0",
            "L 500 0",
            "L 500 700",
            "L 0 700",
            "L 0 0",
            "Z",
            "cbox 0 0 500 700"
        ]
    );
    // An explicit line back to the start: the last point is dropped.
    assert_eq!(
        draw1(&[
            I(0),
            I(0),
            Op(21),
            I(500),
            Op(6),
            I(700),
            Op(7),
            I(-500),
            Op(6),
            I(-700),
            Op(7),
            Op(14)
        ]),
        vec![
            "M 0 0",
            "L 500 0",
            "L 500 700",
            "L 0 700",
            "L 0 0",
            "Z",
            "cbox 0 0 500 700"
        ]
    );
}

#[test]
fn fractions_are_truncated() {
    // 1.5 and -1.5 become 1 and -2 (an arithmetic shift).
    let half = 0x18000;
    assert_eq!(
        draw1(&[
            I(0),
            I(0),
            Op(21),
            F(half),
            Op(6),
            F(-2 * half),
            Op(7),
            Op(14)
        ]),
        vec!["M 0 0", "L 1 0", "L 1 -3", "L 0 0", "Z", "cbox 0 -3 1 0"]
    );
}

#[test]
fn moves_and_single_points() {
    // A move alone makes no contour; a contour of one point is dropped.
    assert_eq!(
        draw1(&[I(5), I(5), Op(21), I(9), I(9), Op(21), Op(14)]),
        vec!["cbox 0 0 0 0"]
    );
    assert_eq!(
        draw1(&[
            I(0),
            I(0),
            Op(21),
            F(16),
            Op(6),
            I(10),
            I(10),
            Op(21),
            I(5),
            Op(6),
            Op(14)
        ]),
        vec!["M 10 10", "L 15 10", "L 10 10", "Z", "cbox 10 10 15 10"]
    );
}

#[test]
fn curves_are_cubic() {
    assert_eq!(
        draw1(&[
            I(0),
            I(0),
            Op(21),
            I(10),
            I(20),
            I(30),
            I(40),
            I(50),
            I(60),
            Op(8),
            Op(14)
        ]),
        vec![
            "M 0 0",
            "C 10 20 40 60 90 120",
            "L 0 0",
            "Z",
            "cbox 0 0 90 120"
        ]
    );
}

#[test]
fn scaled_and_errors() {
    let t = (
        b"CFF ",
        cff1(&spec_with(charstring(&[
            I(0),
            I(0),
            Op(21),
            I(1000),
            Op(6),
            I(1000),
            Op(7),
            Op(14),
        ]))),
    );
    // 16 px at 1000 units per em: x_scale = 1024 / 1000 in 16.16.
    let s = Scale::Scaled {
        x: Fixed(67109),
        y: Fixed(67109),
    };
    let out = draw_with(t.clone(), 1, s).unwrap();
    assert_eq!(out[1], "L 1024 0");
    assert_eq!(
        draw_with(t.clone(), 3, Scale::Unscaled),
        Err(OutlineError::InvalidGlyphIndex)
    );
    let bad = (b"CFF ", cff1(&spec_with(charstring(&[Op(21)]))));
    assert_eq!(
        draw_with(bad, 1, Scale::Unscaled),
        Err(OutlineError::InvalidCharString)
    );
    let mut broken = cff1(&spec_with(Vec::new()));
    broken[0] = 9;
    assert_eq!(
        draw_with((b"CFF ", broken), 1, Scale::Unscaled),
        Err(OutlineError::InvalidCff)
    );
    let none = otto_font(3, vec![]);
    let font = Font::new(&none, 0).unwrap();
    assert_eq!(
        Outlines::new(font.default_instance()).load(
            GlyphId(0),
            Scale::Unscaled,
            &mut Scratch::default()
        ),
        Err(OutlineError::UnsupportedFormat)
    );
}

#[test]
fn font_matrix() {
    // FontMatrix 0.002 0 0.0005 0.001 0 0: units_per_em 1000, x doubled
    // and sheared.
    let mut spec = spec_with(charstring(&[
        I(0),
        I(0),
        Op(21),
        I(100),
        Op(6),
        I(100),
        Op(7),
        Op(14),
    ]));
    let mut m = Vec::new();
    for s in ["0.002", "0", "0.0005", "0.001", "0", "0"] {
        m.extend(d_real(s));
    }
    m.extend(d_op(op::FONT_MATRIX));
    spec.top = m;
    let out = draw_with((b"CFF ", cff1(&spec)), 1, Scale::Unscaled).unwrap();
    assert_eq!(
        out,
        vec![
            "M 0 0",
            "L 200 0",
            "L 250 100",
            "L 0 0",
            "Z",
            "cbox 0 0 250 100"
        ]
    );
}

#[test]
fn font_dict_units_per_em_scales_unscaled_outlines() {
    // A CID font whose Font DICT has 0.002 (units_per_em 500 against the
    // top's 1000): FreeType scales by 1000/500 even without a size.
    let mut spec = spec_with(charstring(&[
        I(0),
        I(0),
        Op(21),
        I(100),
        Op(6),
        I(100),
        Op(7),
        Op(14),
    ]));
    spec.private = None;
    spec.strings = vec![b"A".to_vec(), b"B".to_vec()];
    let mut fm = Vec::new();
    for s in ["0.002", "0", "0", "0.002", "0", "0"] {
        fm.extend(d_real(s));
    }
    fm.extend(d_op(op::FONT_MATRIX));
    spec.cid = Some(CidSpec {
        ros: (391, 392, 0),
        fds: vec![(fm, Vec::new(), Vec::new())],
        fd_select: vec![0, 0, 0, 0],
    });
    let out = draw_with((b"CFF ", cff1(&spec)), 1, Scale::Unscaled).unwrap();
    assert_eq!(
        out,
        vec![
            "M 0 0",
            "L 200 0",
            "L 200 200",
            "L 0 0",
            "Z",
            "cbox 0 0 200 200"
        ]
    );
}

#[test]
fn cff2_default_instance() {
    let store = item_variation_store(1, &[vec![(0, 0x4000, 0x4000)]], &[vec![0]]);
    let t = cff2(&Cff2Spec {
        charstrings: vec![
            Vec::new(),
            charstring(&[
                I(0),
                I(0),
                Op(21),
                I(100),
                I(50),
                I(1),
                Op(16),
                Op(6),
                I(100),
                Op(7),
            ]),
            Vec::new(),
        ],
        fds: vec![(Vec::new(), Vec::new())],
        vstore: Some(store),
        ..Default::default()
    });
    let out = draw_with((b"CFF2", t), 1, Scale::Unscaled).unwrap();
    assert_eq!(
        out,
        vec![
            "M 0 0",
            "L 100 0",
            "L 100 100",
            "L 0 0",
            "Z",
            "cbox 0 0 100 100"
        ]
    );
}

#[test]
fn random_restarts_at_every_draw() {
    let g = charstring(&[
        I(0),
        I(0),
        Op(21),
        Op(0x0C17),
        I(1000),
        Op(0x0C18),
        Op(6),
        I(10),
        Op(7),
        Op(14),
    ]);
    let t = (b"CFF ", cff1(&spec_with(g)));
    assert_eq!(
        draw_with(t.clone(), 1, Scale::Unscaled),
        draw_with(t, 1, Scale::Unscaled)
    );
}
