//! sofel ADR 0076: exact FreeType 2.15.0 stroker command comparison.
#![allow(clippy::all, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::{Pt, glyph_font, simple_glyph};
use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_geom::{Command, Fixed, FnPen, Point};
use sofel_outline::{Outlines, Scale, Scratch, StrokeBorder, StrokeError, StrokeJoin, StrokeStyle};
use std::process::Command as Process;

fn command_line(command: Command) -> String {
    match command {
        Command::MoveTo(p) => format!("M {} {}", p.x, p.y),
        Command::LineTo(p) => format!("L {} {}", p.x, p.y),
        Command::QuadTo(c, p) => format!("Q {} {} {} {}", c.x, c.y, p.x, p.y),
        Command::CubicTo(a, b, p) => format!("C {} {} {} {} {} {}", a.x, a.y, b.x, b.y, p.x, p.y),
        Command::Close => "Z".to_owned(),
    }
}

#[test]
fn inv1_stroke_reference_closed_contours() {
    let triangle = simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(600, 0), Pt::On(200, 700)]],
        false,
        &[],
    );
    let curves = simple_glyph(
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
    let font_bytes = glyph_font(&[Vec::new(), triangle, curves], true, false);
    let font_path = std::env::temp_dir().join(format!("sofel-adr0076-{}.ttf", std::process::id()));
    std::fs::write(&font_path, &font_bytes).unwrap();
    let font = Font::new(&font_bytes, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let exe = sofel_conformance::paths::ft_dump();
    sofel_conformance::paths::require(&exe, "ft-dump").unwrap();
    let mut scratch = Scratch::default();
    let mut count = 0;
    for glyph in [1, 2] {
        outlines
            .load(GlyphId(glyph), Scale::Unscaled, &mut scratch)
            .unwrap();
        for radius in [0, 1, 64] {
            for (join, join_arg) in [
                (StrokeJoin::Round, "0"),
                (StrokeJoin::Bevel, "1"),
                (StrokeJoin::MiterVariable, "2"),
                (StrokeJoin::MiterFixed, "3"),
            ] {
                for (border, border_arg) in [
                    (StrokeBorder::Full, "0"),
                    (StrokeBorder::Outer, "1"),
                    (StrokeBorder::Inner, "2"),
                ] {
                    let output = Process::new(&exe)
                        .args([
                            "--stroke",
                            "unscaled",
                            &glyph.to_string(),
                            &radius.to_string(),
                            join_arg,
                            "65536",
                            border_arg,
                        ])
                        .arg(&font_path)
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let reference = String::from_utf8(output.stdout).unwrap();
                    let expected = reference.lines().skip(1).collect::<Vec<_>>().join("\n");
                    let mut ours = Vec::new();
                    scratch
                        .stroke(
                            StrokeStyle {
                                radius,
                                join,
                                miter_limit: Fixed(65536),
                            },
                            border,
                            &mut FnPen(|cmd| ours.push(command_line(cmd))),
                        )
                        .unwrap();
                    assert_eq!(
                        ours.join("\n"),
                        expected,
                        "glyph {glyph}, radius {radius}, join {join_arg}, border {border_arg}"
                    );
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 72);
    std::fs::remove_file(font_path).unwrap();
}

#[test]
fn inv2_stroke_reuse_preserves_source() {
    let glyph = simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(600, 0), Pt::On(200, 700)]],
        false,
        &[],
    );
    let bytes = glyph_font(&[Vec::new(), glyph], true, false);
    let font = Font::new(&bytes, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    outlines
        .load(GlyphId(1), Scale::Unscaled, &mut scratch)
        .unwrap();
    let source_points = scratch.points().to_vec();
    let mut source_commands = Vec::new();
    scratch
        .decompose(Point::default(), 1, &mut FnPen(|c| source_commands.push(c)))
        .unwrap();
    let mut outputs = Vec::new();
    for (radius, border) in [
        (1, StrokeBorder::Full),
        (64, StrokeBorder::Outer),
        (1, StrokeBorder::Full),
    ] {
        let mut commands = Vec::new();
        scratch
            .stroke(
                StrokeStyle {
                    radius,
                    join: StrokeJoin::MiterVariable,
                    miter_limit: Fixed(65536),
                },
                border,
                &mut FnPen(|c| commands.push(c)),
            )
            .unwrap();
        outputs.push(commands);
    }
    assert_eq!(outputs.first(), outputs.get(2));
    assert_eq!(scratch.points(), source_points);
    let mut after_commands = Vec::new();
    scratch
        .decompose(Point::default(), 1, &mut FnPen(|c| after_commands.push(c)))
        .unwrap();
    assert_eq!(after_commands, source_commands);
}

#[test]
fn inv3_stroke_invalid_and_overflow_are_atomic() {
    let glyph = simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(600, 0), Pt::On(200, 700)]],
        false,
        &[],
    );
    let bytes = glyph_font(&[Vec::new(), glyph], true, false);
    let font = Font::new(&bytes, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    outlines
        .load(GlyphId(1), Scale::Unscaled, &mut scratch)
        .unwrap();
    let source_points = scratch.points().to_vec();
    for (radius, expected) in [
        (-1, StrokeError::InvalidRadius),
        (i32::MAX, StrokeError::TooLarge),
    ] {
        let mut commands = Vec::new();
        let result = scratch.stroke(
            StrokeStyle {
                radius,
                join: StrokeJoin::Round,
                miter_limit: Fixed(65536),
            },
            StrokeBorder::Full,
            &mut FnPen(|c| commands.push(c)),
        );
        assert_eq!(result, Err(expected));
        assert!(commands.is_empty());
        assert_eq!(scratch.points(), source_points);
    }
}

#[test]
fn inv1_stroke_real_glyf_and_cff_commands_match_freetype() {
    let exe = sofel_conformance::paths::ft_dump();
    sofel_conformance::paths::require(&exe, "ft-dump").unwrap();
    let testdata = sofel_conformance::paths::testdata();
    let mut cases = 0;
    for name in [
        "harfbuzz/perf/fonts/Roboto-Regular.ttf",
        "harfbuzz/perf/fonts/NotoSansDuployan-Regular.otf",
    ] {
        let path = testdata.join(name);
        sofel_conformance::paths::require(&path, name).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let outlines = Outlines::new(font.default_instance());
        let mut scratch = Scratch::default();
        for glyph in 0..20 {
            outlines
                .load(GlyphId(glyph), Scale::Unscaled, &mut scratch)
                .unwrap();
            for radius in [0, 1, 64] {
                for (join, join_arg) in [
                    (StrokeJoin::Round, "0"),
                    (StrokeJoin::Bevel, "1"),
                    (StrokeJoin::MiterVariable, "2"),
                    (StrokeJoin::MiterFixed, "3"),
                ] {
                    for (border, border_arg) in [
                        (StrokeBorder::Full, "0"),
                        (StrokeBorder::Outer, "1"),
                        (StrokeBorder::Inner, "2"),
                    ] {
                        let output = Process::new(&exe)
                            .args([
                                "--stroke",
                                "unscaled",
                                &glyph.to_string(),
                                &radius.to_string(),
                                join_arg,
                                "65536",
                                border_arg,
                            ])
                            .arg(&path)
                            .output()
                            .unwrap();
                        assert!(output.status.success(), "{name} glyph {glyph}");
                        let reference = String::from_utf8(output.stdout).unwrap();
                        let expected = reference.lines().skip(1).collect::<Vec<_>>().join("\n");
                        let mut ours = Vec::new();
                        scratch
                            .stroke(
                                StrokeStyle {
                                    radius,
                                    join,
                                    miter_limit: Fixed(65536),
                                },
                                border,
                                &mut FnPen(|cmd| ours.push(command_line(cmd))),
                            )
                            .unwrap();
                        assert_eq!(
                            ours.join("\n"),
                            expected,
                            "{name} glyph {glyph} radius {radius} join {join_arg} border {border_arg}"
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 1440);
}
