//! The paint values, transforms, trigonometry and helper functions
//! (ADR 0023).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic
)]

use sofel_geom::{
    Brush, Color, ColorRef, ColorStop, CompositeMode, Extend, Image, Painter, PointF, Rect, Solid,
    Transform, cos, normalize_color_line, reduce_linear_anchors, sin, sin_cos,
    sweep_gradient_tiles, tan,
};

/// The distance in units in the last place between two finite `f32`s.
fn ulps(a: f32, b: f32) -> u32 {
    if a == b {
        return 0;
    }
    let key = |v: f32| {
        let i = v.to_bits() as i32;
        if i < 0 { i32::MIN - i } else { i }
    };
    (key(a) as i64 - key(b) as i64).unsigned_abs() as u32
}

fn assert_close(what: &str, x: f32, got: f32, want: f64) {
    let want = want as f32;
    assert!(
        ulps(got, want) <= 1,
        "{what}({x:e} = {:#010x}): {got:e} ({:#010x}), want {want:e} ({:#010x})",
        x.to_bits(),
        got.to_bits(),
        want.to_bits()
    );
}

fn check_trig(x: f32) {
    let d = f64::from(x);
    let (s, c) = sin_cos(x);
    assert_close("sin", x, s, d.sin());
    assert_close("cos", x, c, d.cos());
    assert_eq!(sin(x).to_bits(), s.to_bits());
    assert_eq!(cos(x).to_bits(), c.to_bits());
    assert_close("tan", x, tan(x), d.tan());
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn trigonometry_at_special_points() {
    let quarter = std::f32::consts::FRAC_PI_4;
    let mut xs = vec![
        0.0f32,
        -0.0,
        1e-30,
        1e-5,
        0.5,
        1.0,
        2.0,
        3.0,
        10.0,
        100.0,
        1e6,
        1e10,
        1e20,
        1e30,
        f32::MAX,
    ];
    for base in [
        quarter,
        std::f32::consts::PI,
        3.0 * quarter,
        5.0 * quarter,
        7.0 * quarter,
        9.0 * quarter,
        2.0 * std::f32::consts::PI,
    ] {
        let mut v = base;
        for _ in 0..4 {
            v = f32::from_bits(v.to_bits() - 1);
        }
        for _ in 0..9 {
            xs.push(v);
            v = f32::from_bits(v.to_bits() + 1);
        }
    }
    // 2^28·π/2, where the reduction changes method.
    let big = f32::from_bits(0x4dc9_0fdb);
    xs.extend([
        f32::from_bits(big.to_bits() - 1),
        big,
        f32::from_bits(big.to_bits() + 1),
    ]);
    for x in xs.clone() {
        check_trig(x);
        check_trig(-x);
    }
    assert_eq!(sin_cos(0.0), (0.0, 1.0));
    assert!(sin(-0.0).is_sign_negative());
    assert!(sin(f32::INFINITY).is_nan());
    assert!(cos(f32::NAN).is_nan());
    assert!(tan(f32::NEG_INFINITY).is_nan());
}

#[test]
fn trigonometry_within_one_ulp_of_a_million_arguments() {
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    for i in 0..1_000_000u32 {
        let r = rng.next();
        let x = match i % 4 {
            // Any finite f32.
            0 => {
                let f = f32::from_bits(r as u32);
                if f.is_finite() { f } else { 1.0 }
            }
            // Angles of COLR: within a few turns.
            1 => ((r >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 8.0 * std::f32::consts::PI,
            // Up to ±1e6.
            2 => ((r >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 2e6,
            // Large values near multiples of π/2.
            _ => f32::from_bits(0x4dc9_0fdb + (r as u32 % 0x3000_0000)),
        };
        check_trig(x);
    }
}

/// FNV-1a over the bits of a million results; the same constant on every
/// target (CI runs this on x86-64, aarch64 and wasm32).
#[test]
fn trigonometry_and_transforms_are_deterministic() {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |v: f32| {
        for b in v.to_bits().to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    let mut rng = XorShift(0x2545_F491_4F6C_DD1D);
    for _ in 0..1_000_000 {
        let r = rng.next();
        let a = ((r >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 16.0;
        let c = ((r & 0xffff) as f32) - 32768.0;
        let (s, co) = sin_cos(a);
        eat(s);
        eat(co);
        eat(tan(a));
        let t = Transform::rotate_around(a, c, -c).multiply(&Transform::skew_around(a, -a, c, c));
        for v in [t.xx, t.yx, t.xy, t.yy, t.dx, t.dy] {
            eat(v);
        }
    }
    assert_eq!(h, DETERMINISM_HASH, "hash {h:#018x}");
}

const DETERMINISM_HASH: u64 = 0xacda_56a7_1ddd_4e8a;

/// HarfBuzz's formulas in f64.
fn reference(t: Transform, want: [f64; 6]) {
    let got = [t.xx, t.yx, t.xy, t.yy, t.dx, t.dy];
    for (g, w) in got.iter().zip(want) {
        assert!(ulps(*g, w as f32) <= 1, "{got:?} vs {want:?}");
    }
}

#[test]
fn transforms_follow_harfbuzz() {
    assert_eq!(Transform::default(), Transform::IDENTITY);
    reference(
        Transform::translate(3.5, -2.0),
        [1.0, 0.0, 0.0, 1.0, 3.5, -2.0],
    );
    reference(Transform::scale(2.0, 0.5), [2.0, 0.0, 0.0, 0.5, 0.0, 0.0]);
    reference(
        Transform::scale_around(2.0, 0.5, 100.0, 40.0),
        [2.0, 0.0, 0.0, 0.5, (1.0 - 2.0) * 100.0, (1.0 - 0.5) * 40.0],
    );
    let a = 0.3f32;
    let (s, c) = (f64::from(a).sin(), f64::from(a).cos());
    reference(Transform::rotate(a), [c, s, -s, c, 0.0, 0.0]);
    // The centered forms cancel in f32 as HarfBuzz's do: the reference
    // evaluates HarfBuzz's float expression from correctly rounded sine
    // and cosine.
    let (sf, cf) = (s as f32, c as f32);
    reference(
        Transform::rotate_around(a, 50.0, -20.0),
        [
            c,
            s,
            -s,
            c,
            f64::from((1.0 - cf) * 50.0 + sf * -20.0),
            f64::from(-sf * 50.0 + (1.0 - cf) * -20.0),
        ],
    );
    let (tx, ty) = (f64::from(0.2f32).tan(), f64::from(-0.4f32).tan());
    reference(Transform::skew(0.2, -0.4), [1.0, ty, tx, 1.0, 0.0, 0.0]);
    reference(
        Transform::skew_around(0.2, -0.4, 10.0, 30.0),
        [1.0, ty, tx, 1.0, -tx * 30.0, -ty * 10.0],
    );
    // Zero centers and angles give exact zeros, as HarfBuzz's checks do.
    assert_eq!(
        Transform::scale_around(2.0, 2.0, 0.0, 0.0),
        Transform::scale(2.0, 2.0)
    );
    assert_eq!(Transform::skew(0.0, 0.0), Transform::IDENTITY);
    assert_eq!(Transform::rotate(0.0), Transform::IDENTITY);
    // multiply applies the argument first.
    let t = Transform::translate(10.0, 0.0).multiply(&Transform::scale(2.0, 2.0));
    assert_eq!(t.apply(PointF::new(1.0, 1.0)), PointF::new(12.0, 2.0));
}

fn stop(offset: f32, r: u8) -> ColorStop {
    ColorStop {
        offset,
        color: Solid {
            color: Color::new(r, 0, 0, 255),
            source: ColorRef {
                index: Some(u16::from(r)),
                alpha: 1.0,
            },
        },
    }
}

#[test]
fn normalize_color_line_sorts_stably_and_rescales() {
    let mut s = [stop(0.75, 1), stop(0.25, 2), stop(0.25, 3), stop(-0.25, 4)];
    assert_eq!(normalize_color_line(&mut s), (-0.25, 0.75));
    let got: Vec<(f32, u8)> = s.iter().map(|s| (s.offset, s.color.color.r)).collect();
    assert_eq!(got, [(0.0, 4), (0.5, 2), (0.5, 3), (1.0, 1)]);

    let mut one = [stop(0.4, 1)];
    assert_eq!(normalize_color_line(&mut one), (0.4, 0.4));
    assert_eq!(one[0].offset, 0.4);
    let mut same = [stop(2.0, 1), stop(2.0, 2)];
    assert_eq!(normalize_color_line(&mut same), (2.0, 2.0));
    assert_eq!((same[0].color.color.r, same[1].color.color.r), (1, 2));
    assert_eq!(normalize_color_line(&mut []), (0.0, 0.0));
    let mut outside = [stop(1.5, 1), stop(3.5, 2)];
    assert_eq!(normalize_color_line(&mut outside), (1.5, 3.5));
    assert_eq!((outside[0].offset, outside[1].offset), (0.0, 1.0));
}

#[test]
fn reduce_linear_anchors_projects_p1() {
    let p = PointF::new;
    // p2 on the line p0p1: the perpendicular foot of p1 is p0.
    assert_eq!(
        reduce_linear_anchors(p(0.0, 0.0), p(10.0, 0.0), p(5.0, 0.0)),
        (p(0.0, 0.0), p(0.0, 0.0))
    );
    // p2 perpendicular to p0p1: p1 stays.
    assert_eq!(
        reduce_linear_anchors(p(0.0, 0.0), p(10.0, 0.0), p(0.0, 10.0)),
        (p(0.0, 0.0), p(10.0, 0.0))
    );
    // A 45° rotation: the foot of (10, 0) on the line perpendicular to (1, 1).
    assert_eq!(
        reduce_linear_anchors(p(0.0, 0.0), p(10.0, 0.0), p(4.0, 4.0)),
        (p(0.0, 0.0), p(5.0, -5.0))
    );
    // Degenerate: p2 == p0.
    assert_eq!(
        reduce_linear_anchors(p(1.0, 2.0), p(3.0, 4.0), p(1.0, 2.0)),
        (p(1.0, 2.0), p(3.0, 4.0))
    );
}

#[test]
fn sweep_gradient_tiles_cover_the_circle() {
    // HarfBuzz's test_hb_paint_sweep_gradient_tiles_clipped.
    let two_pi = 2.0 * std::f32::consts::PI;
    let tests = [
        (Extend::Pad, 1, 1e6f32, 1e6f32),
        (Extend::Pad, 1, -1e6, -1e6),
        (Extend::Pad, 1, 1e6, 2e6),
        (Extend::Pad, 2, 1e6, 2e6),
        (Extend::Pad, 2, -2e6, -1e6),
        (Extend::Pad, 2, -1e6, 1e6),
        (Extend::Pad, 2, 1e6, -1e6),
        (Extend::Pad, 2, 1.0, 1e6),
        (Extend::Repeat, 2, -1e6, 1e6),
        (Extend::Repeat, 2, 1e6, 3e6),
        (Extend::Repeat, 2, 1e20, 3e20),
        (Extend::Reflect, 2, -1e6, 1e6),
        (Extend::Reflect, 2, -3e6, -1e6),
    ];
    for (extend, n, start, end) in tests {
        let mut stops = [stop(0.0, 0), stop(1.0, 255)];
        stops[1].color.color = Color::new(255, 255, 255, 255);
        stops[0].color.color = Color::new(0, 0, 0, 255);
        let (mut tiles, mut covered, mut last) =
            (0u32, 0.0f32, (Color::default(), Color::default()));
        sweep_gradient_tiles(
            &mut stops[..n],
            extend,
            start,
            end,
            &mut |a0, c0, a1, c1| {
                assert!(a0 >= 0.0 && a0 <= a1 && a1 <= two_pi, "{a0} {a1}");
                tiles += 1;
                covered += a1 - a0;
                last = (c0, c1);
            },
        );
        assert!(
            (1..=3).contains(&tiles),
            "{extend:?} {n} {start} {end}: {tiles} tiles"
        );
        assert!(
            (covered - two_pi).abs() < 1e-3,
            "{extend:?} {n} {start} {end}: {covered}"
        );
        if n == 1 {
            assert_eq!(last, (Color::new(0, 0, 0, 255), Color::new(0, 0, 0, 255)));
        }
    }
}

#[test]
fn sweep_gradient_tiles_by_hand() {
    let black = Color::new(0, 0, 0, 255);
    let white = Color::new(255, 255, 255, 255);
    let line = || {
        let mut s = [stop(0.0, 0), stop(1.0, 0)];
        s[0].color.color = black;
        s[1].color.color = white;
        s
    };
    let pi = std::f32::consts::PI;
    let collect = |extend, start, end| {
        let mut s = line();
        let mut out = Vec::new();
        sweep_gradient_tiles(&mut s, extend, start, end, &mut |a0, c0, a1, c1| {
            out.push((a0, c0, a1, c1))
        });
        out
    };
    // Pad from π/2 to π: black before, the gradient, white after.
    assert_eq!(
        collect(Extend::Pad, pi / 2.0, pi),
        [
            (0.0, black, pi / 2.0, black),
            (pi / 2.0, black, pi, white),
            (pi, white, 2.0 * pi, white)
        ]
    );
    // Reversed angles swap the stops.
    assert_eq!(
        collect(Extend::Pad, pi, pi / 2.0),
        [
            (0.0, white, pi / 2.0, white),
            (pi / 2.0, white, pi, black),
            (pi, black, 2.0 * pi, black)
        ]
    );
    // Equal angles with Pad: the first color before, the last after.
    assert_eq!(
        collect(Extend::Pad, pi, pi),
        [(0.0, black, pi, black), (pi, white, 2.0 * pi, white)]
    );
    assert!(collect(Extend::Repeat, pi, pi).is_empty());
    // Repeat over a half turn: two tiles, after the empty end of the
    // repeat before 0 (HarfBuzz emits it too).
    assert_eq!(
        collect(Extend::Repeat, 0.0, pi),
        [
            (0.0, white, 0.0, white),
            (0.0, black, pi, white),
            (pi, black, 2.0 * pi, white)
        ]
    );
    // Reflect: the second half reversed.
    assert_eq!(
        collect(Extend::Reflect, 0.0, pi),
        [
            (0.0, black, 0.0, black),
            (0.0, black, pi, white),
            (pi, white, 2.0 * pi, black)
        ]
    );
    // Crossing 2π: the tile is cut and its end color interpolated.
    let gray = collect(Extend::Pad, pi, 3.0 * pi);
    assert_eq!(gray.len(), 2);
    assert_eq!(gray[1].0, pi);
    assert_eq!(gray[1].2, 2.0 * pi);
    assert_eq!(gray[1].3, Color::new(128, 128, 128, 255));
}

#[derive(Default)]
struct Log(Vec<String>);

impl Painter for Log {
    fn push_transform(&mut self, t: Transform) {
        self.0.push(format!("transform {t:?}"));
    }
    fn pop_transform(&mut self) {
        self.0.push("pop transform".into());
    }
    fn push_clip_glyph(&mut self, glyph: u32) {
        self.0.push(format!("clip glyph {glyph}"));
    }
    fn push_clip_rectangle(&mut self, r: Rect) {
        self.0.push(format!("clip rect {r:?}"));
    }
    fn pop_clip(&mut self) {
        self.0.push("pop clip".into());
    }
    fn fill(&mut self, brush: &Brush<'_>) {
        match brush {
            Brush::Solid(s) => self.0.push(format!("fill {:?}", s.color)),
            _ => self.0.push("fill gradient".into()),
        }
    }
    fn image(&mut self, _image: &Image<'_>) -> bool {
        false
    }
    fn push_group(&mut self, mode: CompositeMode) {
        self.0.push(format!("group {mode:?}"));
    }
    fn pop_group(&mut self, mode: CompositeMode) {
        self.0.push(format!("pop group {mode:?}"));
    }
}

#[test]
fn painter_defaults() {
    let mut log = Log::default();
    let solid = Solid {
        color: Color::new(1, 2, 3, 4),
        source: ColorRef {
            index: None,
            alpha: 1.0,
        },
    };
    log.fill_glyph(7, &solid);
    assert_eq!(
        log.0,
        [
            "clip glyph 7",
            "fill Color { r: 1, g: 2, b: 3, a: 4 }",
            "pop clip"
        ]
    );
    assert!(!log.color_glyph(3));
    assert_eq!(
        CompositeMode::from_u8(27),
        Some(CompositeMode::HslLuminosity)
    );
    assert_eq!(CompositeMode::from_u8(28), None);
    assert_eq!(CompositeMode::SrcOver as u8, 3);
}
