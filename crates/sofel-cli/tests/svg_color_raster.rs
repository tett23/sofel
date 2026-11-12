#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use sofel_cli::color_raster::render_scene;
use sofel_raster::{BgraImage, Rasterizer};
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
        assert!(
            output.stdout.starts_with(b"P7\n"),
            "glyph {glyph}: grayscale output"
        );
        let split = b"ENDHDR\n";
        let header_end = output
            .stdout
            .windows(split.len())
            .position(|w| w == split)
            .unwrap();
        let header = std::str::from_utf8(&output.stdout[..header_end]).unwrap();
        let value = |label: &str| -> i32 {
            header
                .lines()
                .find_map(|line| line.strip_prefix(label))
                .unwrap()
                .parse()
                .unwrap()
        };
        let pos = header
            .lines()
            .find(|line| line.starts_with("# left "))
            .unwrap();
        let parts: Vec<_> = pos.split_whitespace().collect();
        let left: i32 = parts[2].parse().unwrap();
        let top: i32 = parts[4].parse().unwrap();
        let width = value("WIDTH ");
        let rows = value("HEIGHT ");
        let pixels = &output.stdout[header_end + split.len()..];
        assert_eq!(pixels.len(), (width * rows * 4) as usize);
        let expected: &[(f64, f64, [u8; 4])] = if glyph == 19 {
            &[
                (2.0, 5.0, [255, 195, 0, 140]),
                (8.0, 5.0, [255, 19, 0, 244]),
                (5.0, 2.0, [255, 198, 0, 245]),
                (5.0, 8.0, [255, 18, 0, 138]),
                (5.0, 5.0, [255, 204, 0, 255]),
                (6.7, 5.6, [255, 70, 57, 183]),
                (2.0, 2.0, [0, 0, 0, 0]),
            ]
        } else {
            &[
                (2.0, 5.0, [255, 194, 0, 255]),
                (8.0, 5.0, [255, 19, 0, 255]),
                (5.0, 2.0, [255, 198, 0, 255]),
                (5.0, 8.0, [255, 18, 0, 255]),
                (5.0, 5.0, [255, 108, 0, 255]),
                (6.7, 5.6, [255, 56, 0, 104]),
                (2.0, 2.0, [0, 0, 0, 0]),
            ]
        };
        // Map each SVG anchor to the nearest output pixel center, as the ADR specifies.
        for &(x, y, rgba) in expected {
            let scale = 48.0 / 1024.0;
            let column = ((120.0 * x + 37.5) * scale - 0.5).round() as i32 - left;
            let row = top - 1 - ((950.0 - 120.0 * y) * scale - 0.5).round() as i32;
            assert!(column >= 0 && column < width && row >= 0 && row < rows);
            let at = ((row * width + column) * 4) as usize;
            assert_eq!(
                &pixels[at..at + 4],
                &rgba,
                "glyph {glyph} anchor ({x}, {y})"
            );
        }
    }
}
