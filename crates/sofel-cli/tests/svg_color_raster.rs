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

fn gzip_stored(data: &[u8]) -> Vec<u8> {
    let len = u16::try_from(data.len()).unwrap();
    let mut out = vec![31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 1];
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(&sofel_inflate::crc32(data).to_le_bytes());
    out.extend_from_slice(&u32::from(len).to_le_bytes());
    out
}

#[test]
fn adr0066_i3_plain_and_gzip_two_glyphs_have_identical_pixels() {
    let xml = br##"<svg viewBox="0 0 10 10"><g id="glyph1"><rect width="10" height="10" fill="red"/></g><g id="glyph2"><rect width="10" height="10" fill="blue"/></g></svg>"##;
    let gzip = gzip_stored(xml);
    let mut rendered = Vec::new();
    for source in [xml.as_slice(), gzip.as_slice()] {
        let document = sofel_svg::document(source, 4096).unwrap();
        for glyph in [1, 2] {
            let scene = sofel_svg::parse_glyph(&document, glyph, 10, [0, 0, 0, 255]).unwrap();
            let mut raster = Rasterizer::new();
            let mut image = BgraImage::default();
            assert!(sofel_cli::color_raster::render_glyph_scene(
                &scene,
                10,
                10,
                &mut raster,
                &mut image
            ));
            assert_eq!((image.width, image.rows), (10, 10));
            let expected = if glyph == 1 {
                [0, 0, 255, 255]
            } else {
                [255, 0, 0, 255]
            };
            let (pixels, tail) = image.pixels.as_chunks::<4>();
            assert!(tail.is_empty());
            assert!(pixels.iter().all(|pixel| *pixel == expected));
            rendered.push(image);
        }
    }
    assert_eq!(rendered[0], rendered[2]);
    assert_eq!(rendered[1], rendered[3]);
    assert_ne!(rendered[0], rendered[1]);
}

// Independent I3 oracle for the two source SVG documents. It uses the
// numeric rect/circle/stop declarations, without calling the SVG parser,
// its gradient sampler, or the production BGRA blending function.
fn reference_anchor(glyph: u32, local_x: f64, local_y: f64) -> [u8; 4] {
    let pixel_size = 1024.0 / (48.0 * 120.0);
    let half = pixel_size / 2.0;
    let coverage = |x0: f64, y0: f64, x1: f64, y1: f64| -> u8 {
        let x = (x1.min(local_x + half) - x0.max(local_x - half)).max(0.0);
        let y = (y1.min(local_y + half) - y0.max(local_y - half)).max(0.0);
        ((x * y / (pixel_size * pixel_size)) * 255.0).round() as u8
    };
    let interpolate = |start: [u8; 4], end: [u8; 4], t: f64| -> [u8; 4] {
        let t = t.clamp(0.0, 1.0);
        std::array::from_fn(|channel| {
            (f64::from(start[channel]) + (f64::from(end[channel]) - f64::from(start[channel])) * t)
                .round() as u8
        })
    };
    let mut premul = [0u8; 4]; // BGRA, premultiplied.
    let mut over = |rgba: [u8; 4], cover: u8| {
        let alpha = (u32::from(rgba[3]) * u32::from(cover) + 127) / 255;
        let remain = 255 - alpha;
        for (channel, source) in [rgba[2], rgba[1], rgba[0]].into_iter().enumerate() {
            let foreground = (u32::from(source) * alpha + 127) / 255;
            let background = (u32::from(premul[channel]) * remain + 127) / 255;
            premul[channel] = (foreground + background).min(255) as u8;
        }
        premul[3] = (alpha + (u32::from(premul[3]) * remain + 127) / 255).min(255) as u8;
    };

    let horizontal_t = ((local_x - 1.0) / 8.0 - 0.05) / 0.90;
    let h_alpha = if glyph == 19 { 128 } else { 255 };
    let h = interpolate([255, 215, 0, h_alpha], [255, 0, 0, 255], horizontal_t);
    over(h, coverage(1.0, 4.5, 9.0, 5.5));

    // rotate(90) turns the default horizontal gradient into the vertical
    // direction in each rectangle's object-bounding-box coordinates.
    let vertical_t = ((local_y - 1.0) / 8.0 - 0.05) / 0.90;
    let v_alpha = if glyph == 19 { 128 } else { 255 };
    let v = interpolate([255, 215, 0, 255], [255, 0, 0, v_alpha], vertical_t);
    over(v, coverage(4.5, 1.0, 5.5, 9.0));

    if glyph == 19 {
        let dx = (local_x - 5.0).abs();
        let dy = (local_y - 5.0).abs();
        let far = ((dx + half).powi(2) + (dy + half).powi(2)).sqrt();
        let near = ((dx - half).max(0.0).powi(2) + (dy - half).max(0.0).powi(2)).sqrt();
        assert!(
            far <= 2.0 || near >= 2.0,
            "I3 anchor lies on the circle boundary"
        );
        let circle_coverage = if far <= 2.0 { 255 } else { 0 };
        let distance = ((local_x - 5.0).powi(2) + (local_y - 5.0).powi(2)).sqrt() / 2.0;
        let c = if distance <= 0.85 {
            interpolate([255, 215, 0, 255], [255, 0, 0, 128], distance / 0.85)
        } else {
            interpolate(
                [255, 0, 0, 128],
                [255, 255, 255, 179],
                (distance - 0.85) / 0.15,
            )
        };
        over(c, circle_coverage);
    }

    let a = u32::from(premul[3]);
    let channel = |value: u8| -> u8 {
        ((u32::from(value) * 255 + a / 2).checked_div(a).unwrap_or(0)).min(255) as u8
    };
    [
        channel(premul[2]),
        channel(premul[1]),
        channel(premul[0]),
        premul[3],
    ]
}

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
    for glyph in [19u32, 26] {
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
        for ((x, y), rgba) in color_svg::samples(glyph).unwrap() {
            let scale = 48.0 / 1024.0;
            let local_x = ((f64::from(x) + 0.5) / scale - 37.5) / 120.0;
            let local_y = (950.0 - (f64::from(y) + 0.5) / scale) / 120.0;
            assert_eq!(
                reference_anchor(glyph, local_x, local_y),
                rgba,
                "independent glyph {glyph} pixel ({x}, {y})"
            );
        }
    }
}

#[test]
fn adr0066_i3_clipped_foreground_overlapping_a_background_matches_each_pixel() {
    let xml = br##"<svg viewBox="0 0 4 4"><defs><clipPath id="clip"><rect x="1" y="1" width="2" height="2"/></clipPath></defs><g id="glyph1"><path d="M0 0H4V4H0Z" fill="#123456"/><path d="M0 0H3V4H0Z" clip-path="url(#clip)" fill="context-fill"/></g></svg>"##;
    let compressed = gzip_stored(xml);
    for source in [xml.as_slice(), compressed.as_slice()] {
        let document = sofel_svg::document(source, 4096).unwrap();
        let scene = sofel_svg::parse_glyph(&document, 1, 4, [9, 27, 81, 255]).unwrap();
        let mut image = BgraImage::default();
        assert!(sofel_cli::color_raster::render_glyph_scene(
            &scene,
            4,
            4,
            &mut Rasterizer::new(),
            &mut image,
        ));
        assert_eq!((image.width, image.rows), (4, 4));
        for (index, &pixel) in image.pixels.as_chunks::<4>().0.iter().enumerate() {
            let (x, y) = (index % 4, index / 4);
            let expected = if (1..3).contains(&x) && (1..3).contains(&y) {
                [81, 27, 9, 255]
            } else {
                [0x56, 0x34, 0x12, 255]
            };
            assert_eq!(pixel, expected, "pixel ({x}, {y})");
        }
    }
}
