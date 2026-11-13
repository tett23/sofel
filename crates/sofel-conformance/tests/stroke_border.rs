//! sofel ADR 0076: temporary border points preserve FreeType's tag order.
#![allow(clippy::all, clippy::unwrap_used, dead_code)]

extern crate alloc;

#[path = "../../sofel-outline/src/stroke_border.rs"]
mod stroke_border;
#[path = "../../sofel-outline/src/stroke_trig.rs"]
mod stroke_trig;

use sofel_geom::Point;
use stroke_border::Border;

fn p(x: i32, y: i32) -> Point {
    Point { x, y }
}

#[test]
fn inv1_stroke_border_closure_and_duplicate_lines() {
    let mut border = Border::default();
    border.move_to(p(0, 0)).unwrap();
    border.line_to(p(10, 0), false).unwrap();
    border.line_to(p(10, 0), false).unwrap();
    border.quad_to(p(20, 10), p(10, 20)).unwrap();
    border.cubic_to(p(0, 20), p(-10, 10), p(0, 0)).unwrap();
    border.close(false);
    let mut points = Vec::new();
    let mut tags = Vec::new();
    let mut ends = Vec::new();
    border.export(&mut points, &mut tags, &mut ends).unwrap();
    assert_eq!(
        points,
        [
            p(0, 0),
            p(10, 0),
            p(20, 10),
            p(10, 20),
            p(0, 20),
            p(-10, 10)
        ]
    );
    assert_eq!(tags, [1, 1, 0, 1, 2, 2]);
    assert_eq!(ends, [5]);
    border.reset();
    points.clear();
    tags.clear();
    ends.clear();
    border.export(&mut points, &mut tags, &mut ends).unwrap();
    assert!(points.is_empty());
}

#[test]
fn inv1_stroke_border_arc_has_cubic_segments() {
    let mut border = Border::default();
    border.move_to(p(100, 0)).unwrap();
    border.arc_to(p(0, 0), 100, 0, stroke_trig::PI2).unwrap();
    border.close(false);
    let mut points = Vec::new();
    let mut tags = Vec::new();
    let mut ends = Vec::new();
    border.export(&mut points, &mut tags, &mut ends).unwrap();
    assert_eq!(ends, [2]);
    assert_eq!(tags, [1, 2, 2]);
    assert_eq!(points.first(), Some(&p(0, 100)));
}
