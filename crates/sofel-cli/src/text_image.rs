//! Draw a shaped line of text as an 8-bit PGM image (ADR 0071).

use std::io::Write;

use sofel_font::{Font, HbScale, read::GlyphId, var::Location};
use sofel_outline::{Outlines, Scratch};
use sofel_raster::Rasterizer;
use sofel_shape::{
    Feature, Scale, SegmentProperties, ShapePlan, UnicodeBuffer, shape_with_extents,
};

use crate::glyph;

struct PlacedGlyph {
    left: i64,
    top: i64,
    width: u32,
    rows: u32,
    pixels: Vec<u8>,
}

/// Shapes one line, renders each outline, then composites a PGM image.
/// Positions remain in 26.6 units until each bitmap is placed.
pub(crate) fn draw(
    font: &Font<'_>,
    location: &Location<'_>,
    ppem: u16,
    text: &str,
    features: &[Feature],
    out: &mut dyn Write,
) -> Result<(), String> {
    let mut input = UnicodeBuffer::new();
    input.push_utf8(text.as_bytes(), 0, None);
    input.number_clusters();
    let props = SegmentProperties::default().guess(&input, None);
    let instance = font.instance(location);
    let plan = ShapePlan::new(instance, props, features);
    let scale_value = i32::from(ppem).saturating_mul(64);
    let scale = Scale::new(
        HbScale::for_font(font, scale_value, scale_value),
        u32::from(ppem),
        u32::from(ppem),
    );
    let outlines = Outlines::new(instance);
    let glyphs = shape_with_extents(instance, &scale, &plan, input, &|g| {
        instance
            .glyf_extents(g)
            .or_else(|| outlines.glyph_extents(g))
    });
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let mut placed = Vec::new();
    let (mut pen_x, mut pen_y) = (0i64, 0i64);
    let (mut x_min, mut x_max) = (0i64, 0i64);
    let (mut y_min, mut y_max) = (0i64, 0i64);
    let mut has_ink = false;
    for (info, pos) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
        let bitmap = glyph::render(
            &outlines,
            GlyphId(info.glyph),
            ppem,
            &mut scratch,
            &mut raster,
        )
        .map_err(|e| format!("glyph {}: {e}", info.glyph))?;
        let left = round_26_6(
            pen_x
                .checked_add(i64::from(pos.x_offset))
                .ok_or("text position overflow")?,
        )
        .checked_add(i64::from(bitmap.left))
        .ok_or("text position overflow")?;
        let top = round_26_6(
            pen_y
                .checked_add(i64::from(pos.y_offset))
                .ok_or("text position overflow")?,
        )
        .checked_add(i64::from(bitmap.top))
        .ok_or("text position overflow")?;
        if bitmap.width != 0 && bitmap.rows != 0 {
            let right = left
                .checked_add(i64::from(bitmap.width))
                .ok_or("image width overflow")?;
            let bottom = top
                .checked_sub(i64::from(bitmap.rows))
                .ok_or("image height overflow")?;
            x_min = x_min.min(left);
            x_max = x_max.max(right);
            if has_ink {
                y_min = y_min.min(bottom);
                y_max = y_max.max(top);
            } else {
                y_min = bottom;
                y_max = top;
            }
            has_ink = true;
            let mut bitmap_pixels = Vec::new();
            bitmap_pixels
                .try_reserve_exact(bitmap.pixels.len())
                .map_err(|_| "cannot allocate glyph image")?;
            bitmap_pixels.extend_from_slice(bitmap.pixels);
            placed.push(PlacedGlyph {
                left,
                top,
                width: bitmap.width,
                rows: bitmap.rows,
                pixels: bitmap_pixels,
            });
        }
        pen_x = pen_x
            .checked_add(i64::from(pos.x_advance))
            .ok_or("text position overflow")?;
        pen_y = pen_y
            .checked_add(i64::from(pos.y_advance))
            .ok_or("text position overflow")?;
    }
    x_min = x_min.min(pen_x.div_euclid(64));
    x_max = x_max.max(ceil_26_6(pen_x));
    let width = usize::try_from(
        x_max
            .checked_sub(x_min)
            .ok_or("image width overflow")?
            .max(1),
    )
    .map_err(|_| "image width overflow")?;
    let height = if has_ink {
        usize::try_from(
            y_max
                .checked_sub(y_min)
                .ok_or("image height overflow")?
                .max(1),
        )
        .map_err(|_| "image height overflow")?
    } else {
        1
    };
    let count = width.checked_mul(height).ok_or("image size overflow")?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(count)
        .map_err(|_| "cannot allocate image")?;
    pixels.resize(count, 0);
    for bitmap in placed {
        let start_x = usize::try_from(
            bitmap
                .left
                .checked_sub(x_min)
                .ok_or("image position overflow")?,
        )
        .map_err(|_| "image position overflow")?;
        let start_y = usize::try_from(
            y_max
                .checked_sub(bitmap.top)
                .ok_or("image position overflow")?,
        )
        .map_err(|_| "image position overflow")?;
        let bw = usize::try_from(bitmap.width).map_err(|_| "image width overflow")?;
        let bh = usize::try_from(bitmap.rows).map_err(|_| "image height overflow")?;
        let expected = bw.checked_mul(bh).ok_or("glyph image size overflow")?;
        if bitmap.pixels.len() != expected {
            return Err("invalid glyph image length".into());
        }
        for (row, src) in bitmap.pixels.chunks_exact(bw).enumerate() {
            let dst_row = start_y
                .checked_add(row)
                .and_then(|v| v.checked_mul(width))
                .and_then(|v| v.checked_add(start_x))
                .ok_or("image position overflow")?;
            let end = dst_row.checked_add(bw).ok_or("image position overflow")?;
            let dst = pixels
                .get_mut(dst_row..end)
                .ok_or("image position overflow")?;
            for (pixel, cover) in dst.iter_mut().zip(src) {
                *pixel = blend(*pixel, *cover);
            }
        }
    }
    for pixel in &mut pixels {
        *pixel = 255u8.saturating_sub(*pixel);
    }
    let header = format!("P5\n{} {}\n255\n", width, height);
    out.write_all(header.as_bytes())
        .and_then(|()| out.write_all(&pixels))
        .map_err(|e| format!("cannot write the image: {e}"))
}

fn round_26_6(value: i64) -> i64 {
    value.saturating_add(32).div_euclid(64)
}

fn ceil_26_6(value: i64) -> i64 {
    value.saturating_add(63).div_euclid(64)
}

fn blend(dst: u8, src: u8) -> u8 {
    let d = u16::from(dst);
    let s = u16::from(src);
    let remain = 255u16.saturating_sub(s);
    let under = d.saturating_mul(remain).saturating_add(127) / 255;
    u8::try_from(s.saturating_add(under)).unwrap_or(255)
}

#[cfg(test)]
mod tests {
    use super::{blend, round_26_6};

    #[test]
    fn adr0071_inv5_rounds_half_toward_positive() {
        assert_eq!(round_26_6(31), 0);
        assert_eq!(round_26_6(32), 1);
        assert_eq!(round_26_6(-32), 0);
        assert_eq!(round_26_6(-33), -1);
    }

    #[test]
    fn adr0071_inv5_source_over_coverage() {
        assert_eq!(blend(0, 0), 0);
        assert_eq!(blend(0, 255), 255);
        assert_eq!(blend(255, 128), 255);
        assert_eq!(blend(128, 128), 192);
    }
}
