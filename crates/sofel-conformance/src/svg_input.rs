//! OT-SVG input observations for ADR 0074.
mod references;

use sofel_cli::glyph::{ColorGlyph, render_color};
use sofel_color::{PaintOptions, Painted, Scratch};
use sofel_font::Font;
use sofel_geom::{Brush, Color, CompositeMode, Image, ImageFormat, Painter, Rect, Transform};
use sofel_raster::{BgraImage, Rasterizer};
use sofel_read::Tag;

const DOCUMENT: &[u8] = include_bytes!("../../../conformance/svg/two-glyphs.svg");
// Generated independently with gzip -n -c conformance/svg/two-glyphs.svg.
const GZIP: &[u8] = include_bytes!("../../../conformance/svg/two-glyphs.svg.gz");

/// Returns the measurement for a supported OT-SVG observation.
pub fn observe(feature: &str, case: &str, observation: &str) -> Option<Result<(), String>> {
    match (feature, case, observation) {
        ("in-document", "gzip", "decompressed-bytes") => Some(check_document(GZIP, DOCUMENT)),
        ("in-document", "plain" | "gzip" | "shared-first" | "shared-second", "rgba") => {
            Some(document_pixels(case))
        }
        ("in-style", _, _) => style_pixel(case, observation),
        ("in-gradient", _, _) => gradient_observation(case, observation),
        ("in-reference", _, _) => references::reference_observation(case, observation),
        ("in-restrictions", _, _) => references::restriction_observation(case, observation),
        _ => None,
    }
}

#[derive(Default)]
struct ImageCapture {
    images: Vec<(ImageFormat, Vec<u8>)>,
}

impl Painter for ImageCapture {
    fn image(&mut self, image: &Image<'_>) -> bool {
        self.images.push((image.format, image.data.to_vec()));
        true
    }
    fn push_transform(&mut self, _: Transform) {}
    fn pop_transform(&mut self) {}
    fn push_clip_glyph(&mut self, _: u32) {}
    fn push_clip_rectangle(&mut self, _: Rect) {}
    fn pop_clip(&mut self) {}
    fn fill(&mut self, _: &Brush<'_>) {}
    fn push_group(&mut self, _: CompositeMode) {}
    fn pop_group(&mut self, _: CompositeMode) {}
}

fn glyph_font(stored: &[u8], last: u16) -> Result<Vec<u8>, String> {
    // SVG Document List at offset 10, one record for glyphs 1..=last,
    // document at list-relative offset 14.
    let mut table = vec![0, 0, 0, 0, 0, 10, 0, 0, 0, 0, 0, 1, 0, 1];
    table.extend_from_slice(&last.to_be_bytes());
    table.extend_from_slice(&14u32.to_be_bytes());
    let length = u32::try_from(stored.len()).map_err(|_| "SVG fixture exceeds u32")?;
    table.extend_from_slice(&length.to_be_bytes());
    table.extend_from_slice(stored);
    let mut head = [0u8; 54];
    head[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    head[12..16].copy_from_slice(&sofel_read::head::MAGIC_NUMBER.to_be_bytes());
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    let count = last.checked_add(1).ok_or("glyph count overflow")?;
    let mut maxp = [0; 32];
    maxp[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    maxp[4..6].copy_from_slice(&count.to_be_bytes());
    let loca = vec![0; 2 * (usize::from(count) + 1)];
    sofel_write::build_sfnt(&[
        (Tag::new(b"SVG "), &table),
        (Tag::new(b"head"), &head),
        (Tag::new(b"maxp"), &maxp),
        (Tag::new(b"glyf"), &[0, 0]),
        (Tag::new(b"loca"), &loca),
    ])
    .map_err(|e| format!("building SVG fixture: {e:?}"))
}

fn check_document(stored: &[u8], expected: &[u8]) -> Result<(), String> {
    let bytes = glyph_font(stored, 2)?;
    let font = Font::new(&bytes, 0).map_err(|e| format!("reading SVG fixture: {e:?}"))?;
    let mut capture = ImageCapture::default();
    let painted = sofel_color::paint(
        &font,
        1,
        &PaintOptions::default(),
        &mut Scratch::default(),
        &mut capture,
    );
    let [(ImageFormat::Svg { gzip: false }, actual)] = capture.images.as_slice() else {
        let formats: Vec<_> = capture.images.iter().map(|(format, _)| format).collect();
        return Err(format!(
            "glyph 1: expected one expanded SVG image, got {painted:?} with {formats:?}"
        ));
    };
    if painted != Painted::Svg {
        return Err(format!("glyph 1: expected SVG paint, got {painted:?}"));
    }
    if actual != expected {
        let offset = actual
            .iter()
            .zip(expected)
            .position(|(a, e)| a != e)
            .unwrap_or(actual.len().min(expected.len()));
        return Err(format!(
            "glyph 1 SVG byte {offset}: expected {:?}, got {:?}",
            expected.get(offset),
            actual.get(offset)
        ));
    }
    Ok(())
}
const STYLE: &[u8] = include_bytes!("../../../conformance/svg/style.svg");
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

// At 32 ppem the SVG 8-by-8 viewBox maps one user unit to four pixels.
// Pixel coordinates below denote columns and rows from the baseline origin,
// increasing right/down. Their centers are (column + 0.5, row + 0.5).
fn rendered(
    stored: &[u8],
    last: u16,
    glyph: u32,
    foreground: [u8; 4],
) -> Result<BgraImage, String> {
    let bytes = glyph_font(stored, last)?;
    let font = Font::new(&bytes, 0).map_err(|e| format!("reading SVG fixture: {e:?}"))?;
    let outlines = sofel_outline::Outlines::new((&font).into());
    let [r, g, b, a] = foreground;
    let options = PaintOptions {
        ppem: 32,
        foreground: Color::new(r, g, b, a),
        ..PaintOptions::default()
    };
    let mut image = BgraImage::default();
    let mut raster = Rasterizer::new();
    let result = render_color(
        &outlines,
        sofel_read::GlyphId(glyph),
        32,
        &options,
        &mut sofel_outline::Scratch::default(),
        &mut raster,
        &mut image,
    )
    .map_err(|e| format!("glyph {glyph}: color rendering failed: {e}"))?;
    match result {
        ColorGlyph::Color => Ok(image),
        ColorGlyph::Gray(_) => Err(format!(
            "glyph {glyph}: expected SVG color rendering, got grayscale fallback"
        )),
    }
}

// Outside the cropped bitmap the original transparent canvas is unchanged.
fn pixel(image: &BgraImage, x: i32, y: i32) -> Result<[u8; 4], String> {
    let column = i64::from(x) - i64::from(image.left);
    let row = i64::from(y) + i64::from(image.top);
    let actual = if (0..i64::from(image.width)).contains(&column)
        && (0..i64::from(image.rows)).contains(&row)
    {
        let offset = (row as usize * image.width as usize + column as usize) * 4;
        let [b, g, r, a] = image
            .pixels
            .get(offset..offset + 4)
            .ok_or("truncated BGRA image")?
        else {
            return Err("missing BGRA pixel".to_owned());
        };
        [*r, *g, *b, *a]
    } else {
        [0; 4]
    };
    Ok(actual)
}

fn check_pixel(image: &BgraImage, x: i32, y: i32, expected: [u8; 4]) -> Result<(), String> {
    let actual = pixel(image, x, y)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "pixel ({x}, {y}): expected premultiplied RGBA {expected:?}, got {actual:?}"
        ))
    }
}

fn document_pixels(case: &str) -> Result<(), String> {
    let stored = if case == "gzip" { GZIP } else { DOCUMENT };
    let second = case == "shared-second";
    let image = rendered(stored, 2, if second { 2 } else { 1 }, RED)?;
    // (8.5,8.5) lies in the red rectangle only; (24.5,24.5) is
    // inside the blue circle only. Selecting both glyphs fails either case.
    check_pixel(&image, 8, 8, if second { [0; 4] } else { RED })?;
    check_pixel(&image, 24, 24, if second { BLUE } else { [0; 4] })
}

fn style_pixel(case: &str, name: &str) -> Option<Result<(), String>> {
    let (glyph, foreground, inside) = match case {
        "inherited-fill" => (1, RED, RED),
        "override-fill" => (2, RED, BLUE),
        "context-fill-red" => (3, RED, RED),
        "context-fill-blue" => (3, BLUE, BLUE),
        // Half-opacity blue over opaque red in premultiplied RGBA.
        "overlap" => (4, RED, [127, 0, 128, 255]),
        "reversed-overlap" => (5, RED, RED),
        _ => return None,
    };
    let (point, expected) = match (glyph, name) {
        (1..=3, "inside-rgba") | (4..=5, "intersection-rgba") => ((12, 12), inside),
        (1..=3, "outside-rgba") => ((28, 28), [0; 4]),
        _ => return None,
    };
    Some(
        rendered(STYLE, 5, glyph, foreground)
            .and_then(|image| check_pixel(&image, point.0, point.1, expected)),
    )
}

type Sample = (i32, i32, [u8; 4]);

fn check_samples(image: &BgraImage, samples: &[Sample]) -> Result<(), String> {
    for &(x, y, expected) in samples {
        check_pixel(image, x, y, expected)?;
    }
    Ok(())
}

fn gradient_observation(case: &str, observation: &str) -> Option<Result<(), String>> {
    const GRADIENTS: &[u8] = include_bytes!("../../../conformance/svg/gradients.svg");
    const QUARTER: [u8; 4] = [60, 0, 180, 255];
    const HALF: [u8; 4] = [120, 0, 120, 255];
    // The 32×32 viewBox at 32 ppem has unit scale. For each glyph the second
    // sample is halfway through the gradient. Scan its row independently to
    // locate that color; for the radial gradient scan only right of center.
    let (glyph, scan, samples) = match case {
        // t = (x + 0.5 - 0.5) / 16.
        "linear" => (1, 1..=15, [(4, 8, QUARTER), (8, 8, HALF)]),
        // t = distance from center (8.5, 8.5) / 8.
        "radial" => (2, 8..=15, [(10, 8, QUARTER), (12, 8, HALF)]),
        // Project onto the diagonal in bounding-box coordinates, not in
        // physical pixels: t = ((x - 2) / 16 + (y - 3) / 8) / 2.
        "aspect-ratio" => (3, 3..=17, [(6, 5, QUARTER), (6, 9, HALF)]),
        // Straight red at alpha 0.2 + 0.8t, converted to premultiplied RGBA.
        "stop-opacity" => (
            4,
            1..=15,
            [(4, 8, [102, 0, 0, 102]), (8, 8, [153, 0, 0, 153])],
        ),
        // Translate after scaling: t = (x + 0.5 - 2.5) / 8.
        "transformed" => (5, 3..=9, [(4, 8, QUARTER), (6, 8, HALF)]),
        _ => return None,
    };
    if !matches!(observation, "rgba" | "sample-position") {
        return None;
    }
    Some(rendered(GRADIENTS, 5, glyph, RED).and_then(|image| {
        if observation == "rgba" {
            return check_samples(&image, &samples);
        }
        let (expected_x, y, color) = samples[1];
        let mut matches = Vec::new();
        for x in scan {
            if pixel(&image, x, y)? == color {
                matches.push(x);
            }
        }
        if matches == [expected_x] {
            Ok(())
        } else {
            Err(format!(
                "row {y}: expected RGBA {color:?} at x={expected_x}, got x={matches:?}"
            ))
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i2_svg_observations_check_painted_document_bytes() {
        assert_eq!(check_document(DOCUMENT, DOCUMENT), Ok(()));
        assert!(check_document(b"<svg/>", DOCUMENT).is_err());
        assert!(check_document(&[], DOCUMENT).is_err());
    }

    #[test]
    fn i2_svg_observations_do_not_promote_bytes_to_pixels() {
        assert!(observe("in-document", "gzip", "decompressed-bytes").is_some());
        assert!(observe("in-document", "gzip", "target-element").is_none());
    }

    #[test]
    fn i2_svg_samples_detect_wrong_color_transparency_and_order() {
        let image = sofel_raster::BgraImage {
            left: 2,
            top: -3,
            width: 1,
            rows: 1,
            pixels: vec![128, 0, 127, 255],
        };
        assert_eq!(check_pixel(&image, 2, 3, [127, 0, 128, 255]), Ok(()));
        for wrong in [[255, 0, 0, 255], [0, 0, 0, 0], [80, 80, 80, 255]] {
            assert!(check_pixel(&image, 2, 3, wrong).is_err());
        }
        assert_eq!(check_pixel(&image, 1, 3, [0; 4]), Ok(()));
        assert!(check_pixel(&image, 3, 3, [0, 0, 255, 255]).is_err());
        assert!(check_pixel(&image, 2, 4, [0, 0, 255, 255]).is_err());
    }

    #[test]
    fn i2_svg_gradient_anchors_reject_a_shifted_or_mutated_stop() {
        let mut image = BgraImage {
            left: 2,
            top: -3,
            width: 2,
            rows: 1,
            pixels: vec![180, 0, 60, 255, 120, 0, 120, 255],
        };
        let anchors = [(2, 3, [60, 0, 180, 255]), (3, 3, [120, 0, 120, 255])];
        assert!(check_samples(&image, &anchors).is_ok());
        image.left = 3;
        assert!(check_samples(&image, &anchors).is_err());
        image.left = 2;
        image.pixels[2] = 59;
        assert!(check_samples(&image, &anchors).is_err());
    }
}
