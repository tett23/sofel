//! Transformed outlines and ink bounds for the synthetic SVG glyphs in ADR 0074.
use sofel_geom::{Command, FnPen, Pen, Point};
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

// The Pen contract explicitly closes contours. A line to the current point
// after the last quarter-circle adds no edge and does not change its outline.
fn normalize(commands: &mut Vec<Command>) {
    let mut end = None;
    commands.retain(|cmd| {
        let next = match *cmd {
            Command::LineTo(p) if Some(p) == end => return false,
            Command::MoveTo(p)
            | Command::LineTo(p)
            | Command::QuadTo(_, p)
            | Command::CubicTo(_, _, p) => Some(p),
            Command::Close => None,
        };
        end = next;
        true
    });
}

fn compare_outline(case: &str, gid: u32, document: &[u8]) -> Result<(), String> {
    let scene =
        sofel_svg::parse_shapes(document, gid).map_err(|e| format!("glyph {gid}: {e:?}"))?;
    let [shape] = scene.shapes.as_slice() else {
        return Err(format!(
            "glyph {gid}: expected one outline, got {}",
            scene.shapes.len()
        ));
    };
    let mut actual = Vec::new();
    shape.draw(32.0 / 1000.0, &mut FnPen(|cmd| actual.push(cmd)));
    let mut expected = Vec::new();
    draw_reference(case, &mut FnPen(|cmd| expected.push(cmd)))?;
    normalize(&mut actual);
    normalize(&mut expected);
    for index in 0..actual.len().max(expected.len()) {
        if actual.get(index) != expected.get(index) {
            return Err(format!(
                "glyph {gid} outline command {index}: expected {:?}, got {:?}",
                expected.get(index),
                actual.get(index)
            ));
        }
    }
    Ok(())
}

fn compare_bounds(case: &str, gid: u32) -> Result<(), String> {
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
}

/// Compares transformed SVG commands or nontransparent ink with an independent path.
pub fn observe(case: &str, observation: &str) -> Option<Result<(), String>> {
    let gid = glyph(case)?;
    match observation {
        "outline" => Some(compare_outline(case, gid, DOCUMENT)),
        "bounds" => Some(compare_bounds(case, gid)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i2_outline_detects_changed_selection_and_transform_order() {
        let xml = r##"<svg><defs><linearGradient id="white"><stop stop-color="#fff"/></linearGradient></defs><g id="glyph3" transform="matrix(2 0 0 1 125 62.5)"><rect x="62.5" y="62.5" width="125" height="125" fill="url(#white)"/></g></svg>"##;
        assert_eq!(
            compare_outline("nested-transform", 3, xml.as_bytes()),
            Ok(())
        );
        // Translating before scaling doubles the x offset in the composed matrix.
        let wrong = xml.replace("1 125 62.5", "1 250 62.5");
        assert!(compare_outline("nested-transform", 3, wrong.as_bytes()).is_err());
        assert!(compare_outline("nested-transform", 4, xml.as_bytes()).is_err());
    }
}
