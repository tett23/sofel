//! sofel ADR 0076: straight closed contours against fixed FreeType point/tag export.
#![allow(clippy::all, clippy::unwrap_used, clippy::indexing_slicing, dead_code)]
extern crate alloc;

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;
#[path = "../../sofel-outline/src/stroke_border.rs"]
mod stroke_border;
#[path = "../../sofel-outline/src/stroke_line.rs"]
mod stroke_line;
#[path = "../../sofel-outline/src/stroke_trig.rs"]
mod stroke_trig;

use build::{Pt, glyph_font, simple_glyph};
use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_geom::{Command as PathCommand, FnPen, Point};
use sofel_outline::{Outlines, Scale, Scratch};
use std::process::Command;
use stroke_line::{Join, Stroker};

#[test]
fn inv1_stroke_triangle_points_match_freetype() {
    let glyph = simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(600, 0), Pt::On(200, 700)]],
        false,
        &[],
    );
    let bytes = glyph_font(&[Vec::new(), glyph], true, false);
    let path =
        std::env::temp_dir().join(format!("sofel-stroke-triangle-{}.ttf", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let exe = sofel_conformance::paths::ft_dump();
    sofel_conformance::paths::require(&exe, "ft-dump").unwrap();
    let mut cases = 0;
    for radius in [0, 1, 64] {
        for (join, join_arg) in [
            (Join::Round, "0"),
            (Join::Bevel, "1"),
            (Join::MiterVariable, "2"),
            (Join::MiterFixed, "3"),
        ] {
            let mut stroker = Stroker::new(radius, join, 65536);
            stroker.begin(Point { x: 0, y: 0 });
            stroker.line_to(Point { x: 600, y: 0 }).unwrap();
            stroker.line_to(Point { x: 200, y: 700 }).unwrap();
            stroker.end().unwrap();
            for (border, side) in [("0", None), ("1", Some(1)), ("2", Some(0))] {
                let output = Command::new(&exe)
                    .args([
                        "--stroke-points",
                        "unscaled",
                        "1",
                        &radius.to_string(),
                        join_arg,
                        "65536",
                        border,
                    ])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(output.status.success());
                let reference = String::from_utf8(output.stdout).unwrap();
                let (points, tags, ends) = stroker.export(side).unwrap();
                let mut actual = String::from("glyph 1\n");
                for (point, tag) in points.iter().zip(&tags) {
                    actual.push_str(&format!("point {} {} {}\n", point.x, point.y, tag));
                }
                for end in ends {
                    actual.push_str(&format!("end {end}\n"));
                }
                assert_eq!(
                    actual, reference,
                    "radius {radius}, join {join_arg}, border {border}"
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 36);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn inv1_stroke_winding_and_miter_boundaries() {
    let contours: [&[(i16, i16)]; 4] = [
        &[(0, 0), (600, 0), (200, 700)],
        &[(0, 0), (200, 700), (600, 0)],
        &[(0, 0), (600, 0), (1, 700)],
        &[(0, 0), (600, 0), (600, 0), (200, 700)],
    ];
    let glyphs = contours.iter().map(|contour| {
        let points: Vec<Pt> = contour.iter().map(|&(x, y)| Pt::On(x, y)).collect();
        simple_glyph(&[&points], false, &[])
    });
    let bytes = glyph_font(&[vec![Vec::new()], glyphs.collect()].concat(), true, false);
    let path =
        std::env::temp_dir().join(format!("sofel-stroke-winding-{}.ttf", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let exe = sofel_conformance::paths::ft_dump();
    sofel_conformance::paths::require(&exe, "ft-dump").unwrap();
    let mut cases = 0;
    for (index, contour) in contours.iter().enumerate() {
        let glyph = index + 1;
        let outside_side = usize::from(index != 1);
        for radius in [0, 1, 64] {
            for (join, join_arg) in [
                (Join::Round, "0"),
                (Join::Bevel, "1"),
                (Join::MiterVariable, "2"),
                (Join::MiterFixed, "3"),
            ] {
                for miter in [65535, 65536, 65537] {
                    let mut stroker = Stroker::new(radius, join, miter);
                    let &(x, y) = &contour[0];
                    stroker.begin(Point {
                        x: i32::from(x),
                        y: i32::from(y),
                    });
                    for &(x, y) in &contour[1..] {
                        stroker
                            .line_to(Point {
                                x: i32::from(x),
                                y: i32::from(y),
                            })
                            .unwrap();
                    }
                    stroker.end().unwrap();
                    for (border, side) in [
                        ("0", None),
                        ("1", Some(outside_side)),
                        ("2", Some(1 - outside_side)),
                    ] {
                        let output = Command::new(&exe)
                            .args([
                                "--stroke-points",
                                "unscaled",
                                &glyph.to_string(),
                                &radius.to_string(),
                                join_arg,
                                &miter.to_string(),
                                border,
                            ])
                            .arg(&path)
                            .output()
                            .unwrap();
                        assert!(output.status.success());
                        let reference = String::from_utf8(output.stdout).unwrap();
                        let (points, tags, ends) = stroker.export(side).unwrap();
                        let mut actual = format!("glyph {glyph}\n");
                        for (point, tag) in points.iter().zip(&tags) {
                            actual.push_str(&format!("point {} {} {}\n", point.x, point.y, tag));
                        }
                        for end in ends {
                            actual.push_str(&format!("end {end}\n"));
                        }
                        assert_eq!(
                            actual, reference,
                            "glyph {glyph}, radius {radius}, join {join_arg}, miter {miter}, border {border}"
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 432);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn inv1_stroke_hole_and_reversed_whole_outline() {
    let outer = [(0, 0), (800, 0), (800, 800), (0, 800)];
    let inner = [(200, 200), (200, 600), (600, 600), (600, 200)];
    let make_glyph = |reverse: bool| {
        let contours = [outer.as_slice(), inner.as_slice()];
        let points: Vec<Vec<Pt>> = contours
            .iter()
            .map(|contour| {
                let mut points: Vec<Pt> = contour.iter().map(|&(x, y)| Pt::On(x, y)).collect();
                if reverse {
                    points.reverse();
                }
                points
            })
            .collect();
        simple_glyph(&[&points[0], &points[1]], false, &[])
    };
    let bytes = glyph_font(
        &[Vec::new(), make_glyph(false), make_glyph(true)],
        true,
        false,
    );
    let path = std::env::temp_dir().join(format!("sofel-stroke-hole-{}.ttf", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let exe = sofel_conformance::paths::ft_dump();
    sofel_conformance::paths::require(&exe, "ft-dump").unwrap();
    let mut cases = 0;
    for glyph in [1, 2] {
        for radius in [0, 1, 64] {
            for (join, join_arg) in [
                (Join::Round, "0"),
                (Join::Bevel, "1"),
                (Join::MiterVariable, "2"),
                (Join::MiterFixed, "3"),
            ] {
                for miter in [65535, 65536, 65537] {
                    let mut stroker = Stroker::new(radius, join, miter);
                    for contour in [outer.as_slice(), inner.as_slice()] {
                        let mut points = contour.to_vec();
                        if glyph == 2 {
                            points.reverse();
                        }
                        stroker.begin(Point {
                            x: i32::from(points[0].0),
                            y: i32::from(points[0].1),
                        });
                        for &(x, y) in &points[1..] {
                            stroker
                                .line_to(Point {
                                    x: i32::from(x),
                                    y: i32::from(y),
                                })
                                .unwrap();
                        }
                        stroker.end().unwrap();
                    }
                    let output = Command::new(&exe)
                        .args([
                            "--stroke-points",
                            "unscaled",
                            &glyph.to_string(),
                            &radius.to_string(),
                            join_arg,
                            &miter.to_string(),
                            "0",
                        ])
                        .arg(&path)
                        .output()
                        .unwrap();
                    assert!(output.status.success());
                    let reference = String::from_utf8(output.stdout).unwrap();
                    let (points, tags, ends) = stroker.export(None).unwrap();
                    let mut actual = format!("glyph {glyph}\n");
                    for (point, tag) in points.iter().zip(&tags) {
                        actual.push_str(&format!("point {} {} {}\n", point.x, point.y, tag));
                    }
                    for end in ends {
                        actual.push_str(&format!("end {end}\n"));
                    }
                    assert_eq!(
                        actual, reference,
                        "glyph {glyph}, radius {radius}, join {join_arg}, miter {miter}"
                    );
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 72);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn inv1_stroke_quadratic_points_match_freetype() {
    let glyph = simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::Off(120, 700),
            Pt::On(500, 0),
            Pt::On(500, -100),
            Pt::On(0, -100),
        ]],
        false,
        &[],
    );
    let bytes = glyph_font(&[Vec::new(), glyph], true, false);
    let path = std::env::temp_dir().join(format!("sofel-stroke-quad-{}.ttf", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let exe = sofel_conformance::paths::ft_dump();
    sofel_conformance::paths::require(&exe, "ft-dump").unwrap();
    let mut cases = 0;
    for radius in [0, 1, 64] {
        for (join, join_arg) in [
            (Join::Round, "0"),
            (Join::Bevel, "1"),
            (Join::MiterVariable, "2"),
            (Join::MiterFixed, "3"),
        ] {
            let mut stroker = Stroker::new(radius, join, 65536);
            stroker.begin(Point { x: 0, y: 0 });
            stroker
                .conic_to(Point { x: 120, y: 700 }, Point { x: 500, y: 0 })
                .unwrap();
            stroker.line_to(Point { x: 500, y: -100 }).unwrap();
            stroker.line_to(Point { x: 0, y: -100 }).unwrap();
            stroker.end().unwrap();
            for (border, side) in [("0", None), ("1", Some(0)), ("2", Some(1))] {
                let output = Command::new(&exe)
                    .args([
                        "--stroke-points",
                        "unscaled",
                        "1",
                        &radius.to_string(),
                        join_arg,
                        "65536",
                        border,
                    ])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(output.status.success());
                let reference = String::from_utf8(output.stdout).unwrap();
                let (points, tags, ends) = stroker.export(side).unwrap();
                let mut actual = String::from("glyph 1\n");
                for (point, tag) in points.iter().zip(&tags) {
                    actual.push_str(&format!("point {} {} {}\n", point.x, point.y, tag));
                }
                for end in ends {
                    actual.push_str(&format!("end {end}\n"));
                }
                assert_eq!(
                    actual, reference,
                    "radius {radius}, join {join_arg}, border {border}"
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 36);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn inv1_stroke_real_cff_cubics_match_freetype() {
    let path = sofel_conformance::paths::testdata()
        .join("harfbuzz/perf/fonts/NotoSansDuployan-Regular.otf");
    sofel_conformance::paths::require(&path, "NotoSansDuployan-Regular.otf").unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    outlines
        .load(GlyphId(2), Scale::Unscaled, &mut scratch)
        .unwrap();
    let mut commands = Vec::new();
    scratch
        .decompose(Point::default(), 1, &mut FnPen(|c| commands.push(c)))
        .unwrap();
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, PathCommand::CubicTo(..)))
    );
    let exe = sofel_conformance::paths::ft_dump();
    sofel_conformance::paths::require(&exe, "ft-dump").unwrap();
    let mut cases = 0;
    for radius in [0, 1, 64] {
        for (join, join_arg) in [
            (Join::Round, "0"),
            (Join::Bevel, "1"),
            (Join::MiterVariable, "2"),
            (Join::MiterFixed, "3"),
        ] {
            let mut stroker = Stroker::new(radius, join, 65536);
            for &command in &commands {
                match command {
                    PathCommand::MoveTo(p) => stroker.begin(p),
                    PathCommand::LineTo(p) => stroker.line_to(p).unwrap(),
                    PathCommand::QuadTo(c, p) => stroker.conic_to(c, p).unwrap(),
                    PathCommand::CubicTo(c1, c2, p) => stroker.cubic_to(c1, c2, p).unwrap(),
                    PathCommand::Close => stroker.end().unwrap(),
                }
            }
            let output = Command::new(&exe)
                .args([
                    "--stroke-points",
                    "unscaled",
                    "2",
                    &radius.to_string(),
                    join_arg,
                    "65536",
                    "0",
                ])
                .arg(&path)
                .output()
                .unwrap();
            assert!(output.status.success());
            let reference = String::from_utf8(output.stdout).unwrap();
            let (points, tags, ends) = stroker.export(None).unwrap();
            let mut actual = String::from("glyph 2\n");
            for (point, tag) in points.iter().zip(&tags) {
                actual.push_str(&format!("point {} {} {}\n", point.x, point.y, tag));
            }
            for end in ends {
                actual.push_str(&format!("end {end}\n"));
            }
            assert_eq!(actual, reference, "radius {radius}, join {join_arg}");
            cases += 1;
        }
    }
    assert_eq!(cases, 12);
}
