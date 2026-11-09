#![allow(
    clippy::all,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use sofel_geom::{Command, FnPen, Pen, Point, vector_length};

#[test]
fn trivial_lengths() {
    assert_eq!(vector_length(0, 0), 0);
    assert_eq!(vector_length(0, -7), 7);
    assert_eq!(vector_length(-9, 0), 9);
    assert_eq!(vector_length(i32::MIN + 1, 0), i32::MAX);
}

#[test]
fn lengths_are_close_to_the_true_value() {
    // The exact values are checked against FreeType in sofel-conformance;
    // here the CORDIC result is within one unit of the rounded length.
    for (x, y) in [
        (3, 4),
        (300, 400),
        (0x1_0000, 0x1_0000),
        (-5, 12),
        (65535, -65535),
        (1, 1),
        (123_456, 7),
    ] {
        let exact = ((x as f64).hypot(y as f64)).round() as i64;
        let got = i64::from(vector_length(x, y));
        assert!((got - exact).abs() <= 1, "({x}, {y}): {got} vs {exact}");
    }
}

#[test]
fn pens_receive_commands_in_order() {
    let mut log = Vec::new();
    let mut pen = FnPen(|c| log.push(c));
    let p = |x, y| Point { x, y };
    pen.move_to(p(0, 0));
    pen.line_to(p(10, 0));
    pen.quad_to(p(10, 10), p(0, 10));
    pen.cubic_to(p(-1, 9), p(-1, 1), p(0, 0));
    pen.close();
    assert_eq!(
        log,
        vec![
            Command::MoveTo(p(0, 0)),
            Command::LineTo(p(10, 0)),
            Command::QuadTo(p(10, 10), p(0, 10)),
            Command::CubicTo(p(-1, 9), p(-1, 1), p(0, 0)),
            Command::Close,
        ]
    );
}

#[test]
fn commands_replay_into_pens() {
    let p = Point { x: 1, y: 2 };
    let mut log = Vec::new();
    for c in [Command::MoveTo(p), Command::LineTo(p), Command::Close] {
        c.apply(&mut FnPen(|c| log.push(c)));
    }
    assert_eq!(
        log,
        vec![Command::MoveTo(p), Command::LineTo(p), Command::Close]
    );
}
