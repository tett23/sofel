//! Transformed outlines and ink bounds for the synthetic SVG glyphs in ADR 0074.
use sofel_geom::{Pen, Point};
use sofel_raster::{FillRule, Rasterizer, RenderOptions};

const DOCUMENT: &[u8] = include_bytes!("../../../conformance/svg/geometry.svg");

fn glyph(case: &str) -> Option<u32> {
    Some(match case {
        "path-line" => 1,
        "path-curve" => 2,
        "rect" => 3,
        "circle" => 4,
        "nested-transform" => 5,
        _ => return None,
    })
}

// Coordinates below are calculated from the SVG at 32 ppem and 1000 upem.
// One pixel is 31.25 document units. The Pen receives 26.6 coordinates
// with y increasing upwards. No SVG parser computes the reference outline.
fn draw_reference(case: &str, pen: &mut impl Pen) -> Result<(), String> {
    let px = |x: i32, y: i32| Point {
        x: 64 * x,
        y: -64 * y,
    };
    match case {
        "path-line" => {
            pen.move_to(px(4, 4));
            pen.line_to(px(12, 4));
            pen.line_to(px(8, 12));
            pen.line_to(px(4, 4));
        }
        "path-curve" => {
            pen.move_to(px(4, 12));
            pen.cubic_to(px(4, 0), px(16, 0), px(16, 12));
            pen.line_to(px(4, 12));
        }
        "rect" | "nested-transform" => {
            // For the nested rectangle, scale(2,1) acts before translate(4,2).
            let (left, bottom) = if case == "rect" { (4, 12) } else { (8, 8) };
            pen.move_to(px(left, 4));
            pen.line_to(px(16, 4));
            pen.line_to(px(16, bottom));
            pen.line_to(px(left, bottom));
            pen.line_to(px(left, 4));
        }
        "circle" => {
            // Quarter-circle cubic control distance, in 26.6 units:
            // round(4/3 * tan(pi/8) * radius(4px) * 64) = 141.
            let point = |x, y| Point { x, y };
            pen.move_to(point(768, -512));
            pen.cubic_to(point(768, -653), point(653, -768), point(512, -768));
            pen.cubic_to(point(371, -768), point(256, -653), point(256, -512));
            pen.cubic_to(point(256, -371), point(371, -256), point(512, -256));
            pen.cubic_to(point(653, -256), point(768, -371), point(768, -512));
        }
        other => return Err(format!("unknown SVG geometry case: {other}")),
    }
    pen.close();
    Ok(())
}

// Nontransparent sample bounds in inclusive pixel coordinates, right/down
// from the baseline origin. The bitmap itself can have transparent margins.
fn ink_bounds(
    left: i32,
    top: i32,
    width: u32,
    coverage: impl Iterator<Item = u8>,
) -> Result<(i64, i64, i64, i64), String> {
    if width == 0 {
        return Err("empty glyph bitmap".into());
    }
    let mut bounds: Option<(i64, i64, i64, i64)> = None;
    let width = i64::from(width);
    for (index, _) in coverage.enumerate().filter(|(_, alpha)| *alpha != 0) {
        let i = i64::try_from(index).map_err(|_| "bitmap index exceeds i64")?;
        let x = i64::from(left) + i % width;
        let y = i / width - i64::from(top);
        bounds = Some(bounds.map_or((x, y, x, y), |(x0, y0, x1, y1)| {
            (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
        }));
    }
    bounds.ok_or_else(|| "glyph has no visible ink".into())
}

/// Rasterizes the equivalent independent path and measures its nonempty bounds.
pub fn reference_bounds(case: &str) -> Result<(i64, i64, i64, i64), String> {
    let mut raster = Rasterizer::new();
    draw_reference(case, &mut raster)?;
    let mask = raster
        .render(RenderOptions {
            fill: FillRule::NonZero,
            overlap: false,
            ppem: Some((32, 32)),
        })
        .map_err(|e| e.to_string())?;
    ink_bounds(mask.left, mask.top, mask.width, mask.pixels.iter().copied())
}

/// Compares registered geometry observations with independently drawn paths.
pub fn observe(case: &str, observation: &str) -> Option<Result<(), String>> {
    let gid = glyph(case)?;
    if observation != "bounds" {
        return None;
    }
    Some((|| {
        let expected = reference_bounds(case)?;
        let image = crate::svg_input::rendered(DOCUMENT, 5, gid, [255; 4])?;
        let actual = ink_bounds(
            image.left,
            image.top,
            image.width,
            image.pixels.as_chunks::<4>().0.iter().map(|bgra| bgra[3]),
        )?;
        if actual == expected {
            Ok(())
        } else {
            Err(format!(
                "glyph {gid} ink bounds: expected {expected:?}, got {actual:?}"
            ))
        }
    })())
}
