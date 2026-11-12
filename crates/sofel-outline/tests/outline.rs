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
use sofel_geom::{Command, FnPen, Point, div_fix, mul_fix, vector_length};
use sofel_outline::{CBox, OutlineError, OutlineInfo, Scale, Scratch, draw};

fn p(x: i32, y: i32) -> Point {
    Point { x, y }
}

fn run(
    font: &[u8],
    glyph: u32,
    scale: impl Fn(&Font) -> Scale,
) -> (Vec<Command>, Result<OutlineInfo, OutlineError>) {
    let f = Font::new(font, 0).unwrap();
    let mut log = Vec::new();
    let mut scratch = Scratch::default();
    let r = draw(
        f.default_instance(),
        GlyphId(glyph),
        scale(&f),
        &mut scratch,
        &mut FnPen(|c| log.push(c)),
    );
    (log, r)
}

fn unscaled(font: &[u8], glyph: u32) -> Vec<Command> {
    let (log, r) = run(font, glyph, |_| Scale::Unscaled);
    r.unwrap();
    log
}

fn one(glyph: Vec<u8>) -> Vec<u8> {
    glyph_font(&[Vec::new(), glyph], false, false)
}

use Command::{Close as Z, CubicTo as C, LineTo as L, MoveTo as M, QuadTo as Q};

#[test]
fn lines() {
    let font = one(simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::On(0, 700),
            Pt::On(500, 700),
            Pt::On(500, 0),
        ]],
        false,
        &[],
    ));
    assert_eq!(
        unscaled(&font, 1),
        vec![
            M(p(0, 0)),
            L(p(0, 700)),
            L(p(500, 700)),
            L(p(500, 0)),
            L(p(0, 0)),
            Z
        ]
    );
}

#[test]
fn implied_points_truncate_toward_zero() {
    let font = one(simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::Off(100, 101),
            Pt::Off(201, -3),
            Pt::Off(-100, -6),
            Pt::On(300, 0),
        ]],
        false,
        &[],
    ));
    assert_eq!(
        unscaled(&font, 1),
        vec![
            M(p(0, 0)),
            Q(p(100, 101), p(150, 49)),
            // (-3 + -6) / 2 is -4, truncated toward zero.
            Q(p(201, -3), p(50, -4)),
            Q(p(-100, -6), p(300, 0)),
            L(p(0, 0)),
            Z
        ]
    );
}

#[test]
fn contours_starting_off_curve() {
    // The last point is on the curve: start there.
    let font = one(simple_glyph(
        &[&[Pt::Off(0, 100), Pt::On(100, 100), Pt::On(100, 0)]],
        false,
        &[],
    ));
    assert_eq!(
        unscaled(&font, 1),
        vec![M(p(100, 0)), Q(p(0, 100), p(100, 100)), L(p(100, 0)), Z]
    );
    // All off: start at the middle of the first and last points.
    let font = one(simple_glyph(
        &[&[Pt::Off(0, 0), Pt::Off(100, 0), Pt::Off(100, 100)]],
        false,
        &[],
    ));
    assert_eq!(
        unscaled(&font, 1),
        vec![
            M(p(50, 50)),
            Q(p(0, 0), p(50, 0)),
            Q(p(100, 0), p(100, 50)),
            Q(p(100, 100), p(50, 50)),
            Z
        ]
    );
}

#[test]
fn single_points_and_empty_glyphs() {
    let font = one(simple_glyph(&[&[Pt::On(5, 5)]], false, &[]));
    assert_eq!(unscaled(&font, 1), vec![M(p(5, 5)), L(p(5, 5)), Z]);
    let (log, r) = run(&font, 0, |_| Scale::Unscaled);
    assert!(log.is_empty());
    assert_eq!(
        r.unwrap(),
        OutlineInfo {
            overlap: false,
            cbox: CBox::default()
        }
    );
}

#[test]
fn cubic_curves_only_in_extended_fonts() {
    let g = simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::Cubic(10, 20),
            Pt::Cubic(30, 40),
            Pt::On(50, 0),
        ]],
        false,
        &[],
    );
    let ext = glyph_font(&[Vec::new(), g.clone()], false, true);
    assert_eq!(
        unscaled(&ext, 1),
        vec![M(p(0, 0)), C(p(10, 20), p(30, 40), p(50, 0)), L(p(0, 0)), Z]
    );
    let plain = one(g);
    assert_eq!(
        unscaled(&plain, 1),
        vec![
            M(p(0, 0)),
            Q(p(10, 20), p(20, 30)),
            Q(p(30, 40), p(50, 0)),
            L(p(0, 0)),
            Z
        ]
    );
}

#[test]
fn scaling_uses_mul_fix_without_rounding() {
    let font = one(simple_glyph(
        &[&[Pt::On(-7, 13), Pt::On(999, -1000)]],
        false,
        &[],
    ));
    let (log, r) = run(&font, 1, |f| Scale::pixels(f, 16, 16));
    r.unwrap();
    let s = div_fix(16 * 64, 1000);
    let sp = |x: i32, y: i32| p(mul_fix(x, s), mul_fix(y, s));
    assert_eq!(
        log,
        vec![M(sp(-7, 13)), L(sp(999, -1000)), L(sp(-7, 13)), Z]
    );
    // Different x and y sizes.
    let (log, _) = run(&font, 1, |f| Scale::pixels(f, 10, 20));
    assert_eq!(
        log[0],
        M(p(
            mul_fix(-7, div_fix(640, 1000)),
            mul_fix(13, div_fix(1280, 1000))
        ))
    );
}

#[test]
fn control_box_includes_off_curve_points() {
    let font = one(simple_glyph(
        &[&[Pt::On(0, 0), Pt::Off(-50, 900), Pt::On(100, 0)]],
        true,
        &[],
    ));
    let (_, r) = run(&font, 1, |_| Scale::Unscaled);
    assert_eq!(
        r.unwrap(),
        OutlineInfo {
            overlap: true,
            cbox: CBox {
                x_min: -50,
                y_min: 0,
                x_max: 100,
                y_max: 900
            }
        }
    );
}

fn tri() -> Vec<u8> {
    simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(100, 0), Pt::On(0, 200)]],
        false,
        &[],
    )
}

fn moves(cmds: &[Command]) -> Vec<Point> {
    cmds.iter()
        .filter_map(|c| match c {
            M(p) | L(p) => Some(*p),
            _ => None,
        })
        .collect()
}

#[test]
fn composite_offsets() {
    let font = glyph_font(
        &[
            Vec::new(),
            tri(),
            composite_glyph(
                &[comp(1, Args::Xy(10, -20)), comp(1, Args::Xy(0, 0))],
                [0; 4],
                false,
            ),
        ],
        false,
        false,
    );
    let m = moves(&unscaled(&font, 2));
    assert_eq!(&m[..4], &[p(10, -20), p(110, -20), p(10, 180), p(10, -20)]);
    assert_eq!(&m[4..], &[p(0, 0), p(100, 0), p(0, 200), p(0, 0)]);
    // Scaled: the offset is scaled like the points.
    let (log, _) = run(&font, 2, |f| Scale::pixels(f, 16, 16));
    let s = div_fix(1024, 1000);
    assert_eq!(
        log[0],
        M(p(
            mul_fix(0, s) + mul_fix(10, s),
            mul_fix(0, s) + mul_fix(-20, s)
        ))
    );
}

#[test]
fn composite_transforms() {
    let half = Comp {
        glyph: 1,
        args: Args::Xy(40, 40),
        xf: Xf::Scale(0x2000),
        flags: 0,
    };
    let half_scaled_offset = Comp {
        flags: SCALED_COMPONENT_OFFSET,
        ..half
    };
    let swap = Comp {
        glyph: 1,
        args: Args::Xy(0, 0),
        xf: Xf::TwoByTwo(0, 0x4000, 0x4000, 0),
        flags: 0,
    };
    let font = glyph_font(
        &[
            Vec::new(),
            tri(),
            composite_glyph(&[half], [0; 4], false),
            composite_glyph(&[half_scaled_offset], [0; 4], false),
            composite_glyph(&[swap], [0; 4], false),
        ],
        false,
        false,
    );
    assert_eq!(
        &moves(&unscaled(&font, 2))[..3],
        &[p(40, 40), p(90, 40), p(40, 140)]
    );
    // SCALED_COMPONENT_OFFSET scales the offset by hypot(xx, xy).
    let k = vector_length(0x8000, 0);
    let o = mul_fix(40, k);
    assert_eq!(
        &moves(&unscaled(&font, 3))[..3],
        &[p(o, o), p(50 + o, o), p(o, 100 + o)]
    );
    // xx=0, yx=1, xy=1, yy=0 swaps x and y.
    assert_eq!(
        &moves(&unscaled(&font, 4))[..3],
        &[p(0, 0), p(0, 100), p(200, 0)]
    );
}

#[test]
fn point_matching() {
    // Put component 2's point 0 on component 1's point 1.
    let c = composite_glyph(
        &[comp(1, Args::Xy(0, 0)), comp(1, Args::Points(1, 0))],
        [0; 4],
        false,
    );
    let font = glyph_font(&[Vec::new(), tri(), c], false, false);
    let m = moves(&unscaled(&font, 2));
    assert_eq!(&m[4..7], &[p(100, 0), p(200, 0), p(100, 200)]);
    // A point number out of range fails.
    let bad = composite_glyph(
        &[comp(1, Args::Xy(0, 0)), comp(1, Args::Points(3, 0))],
        [0; 4],
        false,
    );
    let font = glyph_font(&[Vec::new(), tri(), bad], false, false);
    assert_eq!(
        run(&font, 2, |_| Scale::Unscaled).1,
        Err(OutlineError::InvalidComposite)
    );
}

#[test]
fn nested_composites_and_their_limits() {
    let inner = composite_glyph(&[comp(1, Args::Xy(1, 1))], [0; 4], false);
    let outer = composite_glyph(&[comp(2, Args::Xy(10, 10))], [0; 4], false);
    let cycle = composite_glyph(&[comp(4, Args::Xy(0, 0))], [0; 4], false);
    let out_of_range = composite_glyph(&[comp(99, Args::Xy(0, 0))], [0; 4], false);
    let font = glyph_font(
        &[Vec::new(), tri(), inner, outer, cycle, out_of_range],
        false,
        false,
    );
    assert_eq!(moves(&unscaled(&font, 3))[0], p(11, 11));
    assert_eq!(
        run(&font, 4, |_| Scale::Unscaled).1,
        Err(OutlineError::InvalidComposite)
    );
    assert_eq!(
        run(&font, 5, |_| Scale::Unscaled).1,
        Err(OutlineError::InvalidComposite)
    );
    // A chain deeper than 100 levels fails.
    let mut glyphs = vec![Vec::new(), tri()];
    for i in 2..105u32 {
        glyphs.push(composite_glyph(
            &[comp(i - 1, Args::Xy(0, 0))],
            [0; 4],
            false,
        ));
    }
    let font = glyph_font(&glyphs, true, false);
    assert!(run(&font, 100, |_| Scale::Unscaled).1.is_ok());
    assert_eq!(
        run(&font, 104, |_| Scale::Unscaled).1,
        Err(OutlineError::InvalidComposite)
    );
}

#[test]
fn overlap_flags() {
    let first = Comp {
        flags: OVERLAP_COMPOUND,
        ..comp(1, Args::Xy(0, 0))
    };
    let font = glyph_font(
        &[
            Vec::new(),
            tri(),
            composite_glyph(&[first, comp(1, Args::Xy(5, 5))], [0; 4], false),
            composite_glyph(&[comp(1, Args::Xy(5, 5)), first], [0; 4], false),
            simple_glyph(&[&[Pt::On(0, 0), Pt::On(1, 1)]], true, &[]),
            composite_glyph(&[comp(4, Args::Xy(5, 5))], [0; 4], false),
        ],
        false,
        false,
    );
    let overlap = |g| run(&font, g, |_| Scale::Unscaled).1.unwrap().overlap;
    assert!(overlap(2));
    assert!(!overlap(3));
    assert!(overlap(4));
    assert!(overlap(5)); // a simple component with OVERLAP_SIMPLE
}

#[test]
fn errors() {
    let font = one(tri());
    assert_eq!(
        run(&font, 2, |_| Scale::Unscaled).1,
        Err(OutlineError::InvalidGlyphIndex)
    );
    let mut broken = tri();
    broken.truncate(broken.len() - 1);
    // Long loca: no padding byte that could stand in for the cut one.
    let font = glyph_font(&[Vec::new(), broken], true, false);
    assert_eq!(
        run(&font, 1, |_| Scale::Unscaled).1,
        Err(OutlineError::InvalidOutline)
    );
    let font = one(vec![0; 6]);
    assert_eq!(
        run(&font, 1, |_| Scale::Unscaled).1,
        Err(OutlineError::InvalidOutline)
    );
}

#[test]
fn loca_is_read_like_freetype() {
    // Glyphs 1 and 2 are triangles; break loca entries in the long format.
    let glyphs = vec![Vec::new(), tri(), tri()];
    let (glyf, loca) = glyf_loca(&glyphs, true);
    let build = |loca: &[u8]| {
        let base = glyph_font(&glyphs, true, false);
        // Replace the loca bytes in place (same length).
        let s = sofel_font::read::sfnt::Sfnt::new(&base).unwrap();
        let r = s.record(sofel_font::read::Tag::new(b"loca")).unwrap();
        let mut f = base.clone();
        f[r.offset as usize..r.offset as usize + loca.len()].copy_from_slice(loca);
        f
    };
    let n = glyf.len() as u32;
    // The last glyph ends past glyf: cut at the end of glyf.
    let mut l = loca.clone();
    l[12..16].copy_from_slice(&(n + 100).to_be_bytes());
    assert!(run(&build(&l), 2, |_| Scale::Unscaled).1.is_ok());
    // Another glyph ends past glyf: empty.
    let mut l = loca.clone();
    l[8..12].copy_from_slice(&(n + 100).to_be_bytes());
    let (log, r) = run(&build(&l), 1, |_| Scale::Unscaled);
    assert!(r.is_ok() && log.is_empty());
    // Unsorted: from the start to the end of glyf.
    let mut l = loca.clone();
    l[4..8].copy_from_slice(&(n / 2).to_be_bytes());
    l[8..12].copy_from_slice(&0u32.to_be_bytes());
    let (log, r) = run(&build(&l), 1, |_| Scale::Unscaled);
    assert!(r.is_ok() && !log.is_empty());
}

#[test]
fn scratch_is_reused() {
    let font = one(tri());
    let f = Font::new(&font, 0).unwrap();
    let mut scratch = Scratch::default();
    let mut a = Vec::new();
    let mut b = Vec::new();
    draw(
        f.default_instance(),
        GlyphId(1),
        Scale::Unscaled,
        &mut scratch,
        &mut FnPen(|c| a.push(c)),
    )
    .unwrap();
    draw(
        f.default_instance(),
        GlyphId(1),
        Scale::Unscaled,
        &mut scratch,
        &mut FnPen(|c| b.push(c)),
    )
    .unwrap();
    assert_eq!(a, b);
}

#[test]
fn decompose_moves_points_before_finding_implied_ones() {
    // Two off-curve points whose sum is odd and negative: -1 / 2 is 0
    // (toward zero), but after moving by 64, 127 / 2 is 63, i.e. -1.
    let font = one(simple_glyph(
        &[&[Pt::On(-10, 0), Pt::Off(-1, 5), Pt::Off(0, 5), Pt::On(10, 0)]],
        false,
        &[],
    ));
    let f = Font::new(&font, 0).unwrap();
    let mut scratch = Scratch::default();
    sofel_outline::load(
        f.default_instance(),
        GlyphId(1),
        Scale::Unscaled,
        &mut scratch,
    )
    .unwrap();
    let mut plain = Vec::new();
    scratch
        .decompose(p(0, 0), 1, &mut FnPen(|c| plain.push(c)))
        .unwrap();
    assert_eq!(plain[1], Q(p(-1, 5), p(0, 5)));
    let mut moved = Vec::new();
    scratch
        .decompose(p(64, 0), 1, &mut FnPen(|c| moved.push(c)))
        .unwrap();
    assert_eq!(moved[1], Q(p(63, 5), p(63, 5)));
    // Multiplying first leaves no remainder.
    let mut times4 = Vec::new();
    scratch
        .decompose(p(0, 0), 4, &mut FnPen(|c| times4.push(c)))
        .unwrap();
    assert_eq!(times4[1], Q(p(-4, 20), p(-2, 20)));
}

// Phantom point 1 (ADR 0018).

/// A font with `glyphs` and the given `hhea` count and `hmtx` bytes, or
/// neither.
fn metrics_font(glyphs: &[Vec<u8>], metrics: Option<(u16, Vec<u8>)>) -> Vec<u8> {
    let (glyf, loca) = glyf_loca(glyphs, true);
    let mut tables: Vec<(&'static [u8; 4], Vec<u8>)> = vec![
        (
            b"head",
            head(HeadSpec {
                index_to_loc_format: 1,
                ..Default::default()
            }),
        ),
        (b"maxp", maxp05(glyphs.len() as u16)),
        (b"glyf", glyf),
        (b"loca", loca),
    ];
    if let Some((n, data)) = metrics {
        tables.push((b"hhea", hhea(n)));
        tables.push((b"hmtx", data));
    }
    tables.sort_by_key(|(t, _)| **t);
    sfnt(&tables)
}

/// A triangle whose xMin is 100.
fn tri100() -> Vec<u8> {
    simple_glyph(
        &[&[Pt::On(100, 0), Pt::On(200, 0), Pt::On(100, 200)]],
        false,
        &[],
    )
}

fn info(font: &[u8], glyph: u32, scale: impl Fn(&Font) -> Scale) -> (Vec<Point>, CBox) {
    let (log, r) = run(font, glyph, scale);
    (moves(&log), r.unwrap().cbox)
}

#[test]
fn no_shift_when_lsb_is_x_min() {
    let font = metrics_font(
        &[Vec::new(), tri100()],
        Some((2, hmtx(&[(500, 0), (500, 100)], &[]))),
    );
    let (m, cbox) = info(&font, 1, |_| Scale::Unscaled);
    assert_eq!(&m[..3], &[p(100, 0), p(200, 0), p(100, 200)]);
    assert_eq!(cbox.x_min, 100);
}

#[test]
fn shift_by_x_min_minus_lsb() {
    // pp1.x = 100 - 30 = 70.
    let font = metrics_font(
        &[Vec::new(), tri100()],
        Some((2, hmtx(&[(500, 0), (500, 30)], &[]))),
    );
    let (m, cbox) = info(&font, 1, |_| Scale::Unscaled);
    assert_eq!(&m[..3], &[p(30, 0), p(130, 0), p(30, 200)]);
    assert_eq!(
        cbox,
        CBox {
            x_min: 30,
            y_min: 0,
            x_max: 130,
            y_max: 200
        }
    );
    // 48 pixels: the points and pp1.x are scaled with mul_fix, not rounded.
    let s = div_fix(48 * 64, 1000);
    let (m, cbox) = info(&font, 1, |f| Scale::pixels(f, 48, 48));
    let sx = |x: i32| mul_fix(x, s) - mul_fix(70, s);
    assert_eq!(
        &m[..3],
        &[p(sx(100), 0), p(sx(200), 0), p(sx(100), mul_fix(200, s))]
    );
    assert_eq!(cbox.x_min, sx(100));
    assert_eq!(cbox.x_max, sx(200));
    // A negative lsb shifts right.
    let font = metrics_font(
        &[Vec::new(), tri100()],
        Some((2, hmtx(&[(500, 0), (500, -20)], &[]))),
    );
    assert_eq!(info(&font, 1, |_| Scale::Unscaled).1.x_min, -20);
}

#[test]
fn lsb_is_read_like_freetype() {
    let glyphs = [Vec::new(), tri100(), tri100()];
    let x_min = |metrics| {
        info(&metrics_font(&glyphs, metrics), 2, |_| Scale::Unscaled)
            .1
            .x_min
    };
    // Beyond numberOfHMetrics: from the leftSideBearings array.
    assert_eq!(x_min(Some((1, hmtx(&[(500, 0)], &[7, 40])))), 40);
    // The array is cut short: 0.
    assert_eq!(x_min(Some((1, hmtx(&[(500, 0)], &[7])))), 0);
    // A long metric cut short: 0.
    let mut h = hmtx(&[(500, 0), (500, 0), (500, 40)], &[]);
    h.truncate(10);
    assert_eq!(x_min(Some((3, h))), 0);
    // numberOfHMetrics is 0: 0.
    assert_eq!(x_min(Some((0, hmtx(&[], &[40, 40, 40])))), 0);
    // No hhea and hmtx: 0.
    assert_eq!(x_min(None), 0);
}

#[test]
fn composites_use_their_own_phantom_points() {
    let glyphs = |flags: u16, x_min: i16, lsb: i16| {
        // Glyph 1 has pp1.x = 100 - 60 = 40; glyph 2 (the composite) has
        // pp1.x = x_min - lsb.
        let c = Comp {
            flags,
            ..comp(1, Args::Xy(10, 0))
        };
        metrics_font(
            &[
                Vec::new(),
                tri100(),
                composite_glyph(&[c], [x_min, 0, 210, 200], false),
            ],
            Some((3, hmtx(&[(500, 0), (500, 60), (500, lsb)], &[]))),
        )
    };
    // Without USE_MY_METRICS: the composite's own header and lsb.
    let (m, _) = info(&glyphs(0, 110, 100), 2, |_| Scale::Unscaled);
    assert_eq!(m[0], p(100, 0));
    let (m, _) = info(&glyphs(0, 110, 110), 2, |_| Scale::Unscaled);
    assert_eq!(m[0], p(110, 0));
    // With USE_MY_METRICS: the component's pp1.x (40), whatever the
    // composite says, and not moved by the component's offset.
    let (m, _) = info(&glyphs(USE_MY_METRICS, 110, 110), 2, |_| Scale::Unscaled);
    assert_eq!(m[0], p(70, 0));
    let s = div_fix(48 * 64, 1000);
    let (m, _) = info(&glyphs(USE_MY_METRICS, 110, 110), 2, |f| {
        Scale::pixels(f, 48, 48)
    });
    assert_eq!(m[0].x, mul_fix(100, s) + mul_fix(10, s) - mul_fix(40, s));
}

#[test]
fn use_my_metrics_in_nested_composites_and_empty_components() {
    let my = |glyph| Comp {
        flags: USE_MY_METRICS,
        ..comp(glyph, Args::Xy(0, 0))
    };
    let font = metrics_font(
        &[
            Vec::new(),
            tri100(),
            // 2: takes glyph 1's pp1.x = 100 - 60 = 40.
            composite_glyph(&[my(1)], [100, 0, 200, 200], false),
            // 3: plain component 2: its own pp1.x = 100 - 100 = 0.
            composite_glyph(&[comp(2, Args::Xy(0, 0))], [100, 0, 200, 200], false),
            // 4: USE_MY_METRICS on 2, which took 1's: 40.
            composite_glyph(&[my(2)], [100, 0, 200, 200], false),
            // 5: USE_MY_METRICS on the empty glyph 0 (lsb 15): pp1.x = 0 - 15,
            // and then the triangle.
            composite_glyph(&[my(0), comp(1, Args::Xy(0, 0))], [100, 0, 200, 200], false),
        ],
        Some((
            6,
            hmtx(
                &[
                    (500, 15),
                    (500, 60),
                    (500, 0),
                    (500, 100),
                    (500, 0),
                    (500, 100),
                ],
                &[],
            ),
        )),
    );
    let x = |g| info(&font, g, |_| Scale::Unscaled).0[0].x;
    assert_eq!(x(2), 60);
    assert_eq!(x(3), 100);
    assert_eq!(x(4), 60);
    assert_eq!(x(5), 115);
}

#[test]
fn empty_glyphs_do_not_move() {
    let font = metrics_font(
        &[Vec::new(), tri100()],
        Some((2, hmtx(&[(500, 40), (500, 30)], &[]))),
    );
    let (log, r) = run(&font, 0, |_| Scale::Unscaled);
    assert!(log.is_empty());
    assert_eq!(r.unwrap().cbox, CBox::default());
}

#[test]
fn adr0064_i1_mono_loader_executes_the_glyph_program() {
    let bytes = one(simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(100, 101), Pt::On(200, 0)]],
        false,
        &[0x00, 0xb1, 1, 128, 0x48],
    ));
    let font = Font::new(&bytes, 0).unwrap();
    let outlines = sofel_outline::Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    outlines
        .load_truetype_mono(GlyphId(1), 16, &mut scratch)
        .unwrap();
    assert_eq!(scratch.points()[1].y, 128);
}

#[test]
fn adr0064_i1_small_ppem_prep_disables_glyph_hinting() {
    let g = simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(100, 101), Pt::On(200, 0)]],
        false,
        &[0x00, 0xb1, 1, 128, 0x48],
    );
    let bytes = glyph_font_with(
        &[Vec::new(), g],
        false,
        false,
        vec![(
            b"prep",
            vec![0x4b, 0xb0, 9, 0x50, 0x58, 0xb1, 1, 1, 0x8e, 0x59],
        )],
    );
    let font = Font::new(&bytes, 0).unwrap();
    let outlines = sofel_outline::Outlines::new(font.default_instance());
    for ppem in [8, 9] {
        let mut s = Scratch::default();
        outlines
            .load(GlyphId(1), Scale::pixels(&font, ppem, ppem), &mut s)
            .unwrap();
        let unhinted = s.points()[1].y;
        outlines
            .load_truetype_mono(GlyphId(1), ppem, &mut s)
            .unwrap();
        assert_eq!(s.points()[1].y, if ppem == 8 { unhinted } else { 128 });
    }
}
