#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

#[path = "../../../conformance/svg/color_svg.rs"]
mod color_svg;

use sofel_cli::color_raster::render_scene;
use sofel_font::geom::{Pen, Point};
use sofel_raster::{BgraImage, FillRule, Rasterizer, RenderOptions};
use sofel_svg::{Gradient, GradientKind, Scene, Shape, ShapeKind, Stop};
use std::process::Command;

#[test]
fn adr0072_i3_linear_rect_raster_anchor_colors() {
    let scene = Scene {
        gradients: vec![Gradient {
            id: "g".into(),
            kind: GradientKind::Linear {
                x1: 0.0,
                y1: 0.0,
                x2: 1.0,
                y2: 0.0,
            },
            user_space: false,
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            stops: vec![
                Stop {
                    offset: 0.0,
                    rgba: [0, 0, 0, 255],
                },
                Stop {
                    offset: 1.0,
                    rgba: [255, 0, 0, 255],
                },
            ],
        }],
        shapes: vec![Shape {
            kind: ShapeKind::Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            gradient: 0,
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }],
    };
    let mut raster = Rasterizer::new();
    let mut image = BgraImage::default();
    assert!(render_scene(&scene, 10, 10, &mut raster, &mut image));
    assert_eq!(
        (image.left, image.top, image.width, image.rows),
        (0, 0, 10, 10)
    );
    assert_eq!(
        &image.pixels[(5 * 10 + 2) * 4..(5 * 10 + 2) * 4 + 4],
        &[0, 0, 64, 255]
    );
    assert_eq!(
        &image.pixels[(5 * 10 + 7) * 4..(5 * 10 + 7) * 4 + 4],
        &[0, 0, 191, 255]
    );
}

#[test]
fn adr0072_i3_circle_boundary_matches_explicit_path() {
    let scene = Scene {
        gradients: vec![Gradient {
            id: "circle".into(),
            kind: GradientKind::Radial {
                cx: 0.5,
                cy: 0.5,
                r: 0.5,
                fx: 0.5,
                fy: 0.5,
            },
            user_space: false,
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            stops: vec![
                Stop {
                    offset: 0.0,
                    rgba: [255, 255, 255, 255],
                },
                Stop {
                    offset: 1.0,
                    rgba: [255, 255, 255, 255],
                },
            ],
        }],
        shapes: vec![Shape {
            kind: ShapeKind::Circle {
                cx: 5.0,
                cy: 5.0,
                r: 2.0,
            },
            gradient: 0,
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }],
    };
    let mut raster = Rasterizer::new();
    let mut image = BgraImage::default();
    assert!(render_scene(&scene, 10, 10, &mut raster, &mut image));

    let mut reference = Rasterizer::new();
    let p = |x: f32, y: f32| Point {
        x: (x * 64.0).round() as i32,
        y: (-y * 64.0).round() as i32,
    };
    let k = 0.552_284_8_f32 * 2.0;
    reference.move_to(p(7.0, 5.0));
    reference.cubic_to(p(7.0, 5.0 + k), p(5.0 + k, 7.0), p(5.0, 7.0));
    reference.cubic_to(p(5.0 - k, 7.0), p(3.0, 5.0 + k), p(3.0, 5.0));
    reference.cubic_to(p(3.0, 5.0 - k), p(5.0 - k, 3.0), p(5.0, 3.0));
    reference.cubic_to(p(5.0 + k, 3.0), p(7.0, 5.0 - k), p(7.0, 5.0));
    reference.line_to(p(7.0, 5.0));
    reference.close();
    let mask = reference
        .render(RenderOptions {
            fill: FillRule::NonZero,
            overlap: false,
            ppem: Some((10, 10)),
        })
        .unwrap();
    assert_eq!(
        (image.left, image.top, image.width, image.rows),
        (mask.left, mask.top, mask.width, mask.rows)
    );
    assert_eq!(
        image
            .pixels
            .iter()
            .skip(3)
            .step_by(4)
            .copied()
            .collect::<Vec<_>>(),
        mask.pixels
    );
}

#[test]
fn adr0072_i3_real_svg_glyphs_render_as_color() {
    let base = std::env::var_os("SOFEL_TESTDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/testdata")
        });
    let font = base.join("extra-fonts/fonts/color-svg.ttf");
    if !font.exists() {
        assert!(
            std::env::var_os("SOFEL_REQUIRE_TESTDATA").is_none(),
            "missing {}",
            font.display()
        );
        return;
    }
    for glyph in [19, 26] {
        let output = Command::new(env!("CARGO_BIN_EXE_sofel-render"))
            .args(["--ppem", "48", "--color", "--glyph", &glyph.to_string()])
            .arg(&font)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "glyph {glyph}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        color_svg::check_cli_pixels(glyph, &output.stdout).unwrap();
    }
}
