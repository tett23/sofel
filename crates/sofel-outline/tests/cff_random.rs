//! The rules of CFF `random` when drawing (ADR 0082): the state starts
//! from `initialRandomSeed` at every draw, so outlines do not depend on
//! the order, the count or the thread of the draws.

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
use sofel_geom::{Command, FnPen};
use sofel_outline::{Outlines, Scale, Scratch};

use Cs::{I, Op};

const RANDOM: u16 = 0x0C17;
const MUL: u16 = 0x0C18;

/// A glyph that moves and draws by random amounts.
fn random_glyph(salt: i32) -> Vec<u8> {
    charstring(&[
        Op(RANDOM),
        I(1000),
        Op(MUL),
        Op(RANDOM),
        I(1000),
        Op(MUL),
        Op(21),
        Op(RANDOM),
        I(500 + 300 * salt),
        Op(MUL),
        I(0),
        Op(5),
        I(0),
        Op(RANDOM),
        I(700),
        Op(MUL),
        Op(5),
        Op(14),
    ])
}

/// A font of three glyphs that use random, with `initialRandomSeed`
/// `seed` (`None` for a Private DICT without it).
fn font_bytes(seed: Option<i32>) -> Vec<u8> {
    let mut spec = Cff1Spec::new(vec![
        charstring(&[Op(14)]),
        random_glyph(0),
        random_glyph(1),
        random_glyph(2),
    ]);
    spec.private = Some(match seed {
        Some(s) => dict(&[(&[s], op::INITIAL_RANDOM_SEED)]),
        None => dict(&[(&[0], op::DEFAULT_WIDTH_X)]),
    });
    otto_font(4, vec![(b"CFF ", cff1(&spec))])
}

fn draw(o: &Outlines<'_, '_, '_>, glyph: u32) -> Vec<String> {
    let mut scratch = Scratch::default();
    let mut out = Vec::new();
    o.draw(
        GlyphId(glyph),
        Scale::Unscaled,
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
    )
    .unwrap();
    out
}

fn draw_all(bytes: &[u8], order: &[u32]) -> Vec<(u32, Vec<String>)> {
    let font = Font::new(bytes, 0).unwrap();
    let o = Outlines::new(font.default_instance());
    let mut out: Vec<(u32, Vec<String>)> = order.iter().map(|&g| (g, draw(&o, g))).collect();
    out.sort();
    out
}

#[test]
fn adr0082_i1_order_count_threads_and_other_fonts_do_not_matter() {
    let bytes = font_bytes(Some(12345));
    let once = draw_all(&bytes, &[1, 2, 3]);
    // The glyphs differ from each other and from a constant.
    assert_ne!(once[0].1, once[1].1);
    // Twice, and in the reverse order.
    let twice = draw_all(&bytes, &[1, 2, 3, 1, 2, 3]);
    let mut dedup = twice.clone();
    dedup.dedup();
    assert_eq!(dedup, once);
    assert_eq!(draw_all(&bytes, &[3, 2, 1]), once);
    // Two threads drawing at the same time (wasm32-wasip1 has no threads).
    #[cfg(not(target_os = "wasi"))]
    {
        let threads: Vec<_> = (0..2)
            .map(|t| {
                let bytes = bytes.clone();
                std::thread::spawn(move || {
                    let order: Vec<u32> = if t == 0 { vec![1, 2, 3] } else { vec![3, 1, 2] };
                    draw_all(&bytes, &order)
                })
            })
            .collect();
        for t in threads {
            assert_eq!(t.join().unwrap(), once);
        }
    }
    // After other fonts were opened and drawn.
    for seed in [1, 2, 3] {
        let other = font_bytes(Some(seed));
        draw_all(&other, &[1, 2, 3]);
    }
    assert_eq!(draw_all(&bytes, &[2, 3, 1]), once);
}

#[test]
fn adr0082_i2_initial_random_seed_rules() {
    let at = |seed: Option<i32>| draw_all(&font_bytes(seed), &[1, 2, 3]);
    // 0 (and no entry) is 987654321.
    assert_eq!(at(Some(0)), at(Some(987_654_321)));
    assert_eq!(at(None), at(Some(987_654_321)));
    // A negative seed is its absolute value.
    assert_eq!(at(Some(-1)), at(Some(1)));
    assert_eq!(at(Some(-12345)), at(Some(12345)));
    // Different seeds give different outlines, and i32::MAX is its own.
    assert_ne!(at(Some(1)), at(Some(2)));
    assert_ne!(at(Some(i32::MAX)), at(Some(1)));
    assert_eq!(at(Some(i32::MAX)), at(Some(i32::MAX)));
    // Without a Private DICT, FreeType keeps initialRandomSeed at 0
    // without the replacement (cff_subfont_load), so the state stays 0
    // and every value is 1/65536: the pen does not move by a font unit.
    let mut spec = Cff1Spec::new(vec![charstring(&[Op(14)]), random_glyph(0)]);
    spec.private = None;
    let bytes = otto_font(2, vec![(b"CFF ", cff1(&spec))]);
    assert_eq!(
        draw_all(&bytes, &[1])[0].1,
        vec!["M 0 0", "L 0 0", "L 0 0", "L 0 0", "Z"]
    );
}
