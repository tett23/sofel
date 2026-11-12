//! FreeType 2.15.0 `FT_Bitmap_Embolden` (src/base/ftbitmap.c).

use alloc::vec::Vec;

use crate::{BitmapView, ConvertError, PixelMode, convert_bitmap};

/// A bitmap after FreeType-style emboldening.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmboldenedBitmap {
    /// Width in pixels or subpixel samples.
    pub width: u32,
    /// Number of rows.
    pub rows: u32,
    /// Signed row pitch; a negative pitch stores bottom rows first.
    pub pitch: i32,
    /// Pixel format after conversion of GRAY2 or GRAY4.
    pub pixel_mode: PixelMode,
    /// Number of representable gray values.
    pub num_grays: u16,
    /// Stored rows in the flow specified by [`Self::pitch`].
    pub buffer: Vec<u8>,
}

/// The metrics and bitmap placement of a FreeType bitmap glyph slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BitmapSlotMetrics {
    /// Horizontal vector advance in 26.6 units.
    pub advance_x: i32,
    /// Vertical vector advance in 26.6 units.
    pub advance_y: i32,
    /// Glyph metric width in 26.6 units.
    pub width: i32,
    /// Glyph metric height in 26.6 units.
    pub height: i32,
    /// Horizontal left bearing in 26.6 units.
    pub hori_bearing_x: i32,
    /// Horizontal top bearing in 26.6 units.
    pub hori_bearing_y: i32,
    /// Horizontal advance metric in 26.6 units.
    pub hori_advance: i32,
    /// Vertical left bearing in 26.6 units.
    pub vert_bearing_x: i32,
    /// Vertical top bearing in 26.6 units.
    pub vert_bearing_y: i32,
    /// Vertical advance metric in 26.6 units.
    pub vert_advance: i32,
    /// Pixel coordinate of the bitmap's left edge.
    pub bitmap_left: i32,
    /// Pixel coordinate of the bitmap's top edge.
    pub bitmap_top: i32,
}

/// Result of emboldening a FreeType bitmap glyph slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmboldenedBitmapSlot {
    /// The enlarged bitmap.
    pub bitmap: EmboldenedBitmap,
    /// The adjusted slot metrics and bitmap placement.
    pub metrics: BitmapSlotMetrics,
}

/// Apply FreeType's `FT_GlyphSlot_Embolden` to a bitmap and its slot metrics.
///
/// The pixel sizes determine the 26.6 strengths. Bitmap strengths round down
/// to whole pixels, and horizontal strength has a one-pixel minimum.
pub fn embolden_bitmap_slot(
    source: BitmapView<'_>,
    x_ppem: u16,
    y_ppem: u16,
    mut metrics: BitmapSlotMetrics,
) -> Result<EmboldenedBitmapSlot, ConvertError> {
    let mut x_strength = i32::from(x_ppem).saturating_mul(0x0AAA) / 1024;
    let mut y_strength = i32::from(y_ppem).saturating_mul(0x0AAA) / 1024;
    x_strength &= !63;
    if x_strength == 0 {
        x_strength = 64;
    }
    y_strength &= !63;
    let bitmap = embolden_bitmap(source, x_strength, y_strength)?;
    if metrics.advance_x != 0 {
        metrics.advance_x = metrics.advance_x.wrapping_add(x_strength);
    }
    if metrics.advance_y != 0 {
        metrics.advance_y = metrics.advance_y.wrapping_add(y_strength);
    }
    metrics.width = metrics.width.wrapping_add(x_strength);
    metrics.height = metrics.height.wrapping_add(y_strength);
    metrics.hori_advance = metrics.hori_advance.wrapping_add(x_strength);
    metrics.vert_advance = metrics.vert_advance.wrapping_add(y_strength);
    metrics.hori_bearing_y = metrics.hori_bearing_y.wrapping_add(y_strength);
    metrics.bitmap_top = metrics.bitmap_top.wrapping_add(y_strength / 64);
    Ok(EmboldenedBitmapSlot { bitmap, metrics })
}

/// Enlarge a bitmap using FreeType's 26.6 pixel strengths.
///
/// Positive strengths round to whole pixels. MONO caps horizontal
/// strength at eight pixels; GRAY2 and GRAY4 first become 8-bit rows;
/// BGRA is returned untouched, as FreeType leaves color glyphs alone.
pub fn embolden_bitmap(
    source: BitmapView<'_>,
    x_strength: i32,
    y_strength: i32,
) -> Result<EmboldenedBitmap, ConvertError> {
    let x_pixels = rounded_pixels(x_strength)?;
    let y_pixels = rounded_pixels(y_strength)?;
    let (mode, grays, src_width, src_rows, src_pitch, src_bytes): (
        PixelMode,
        u16,
        _,
        _,
        _,
        Vec<u8>,
    ) = match source.pixel_mode {
        PixelMode::Gray2 | PixelMode::Gray4 => {
            let converted = convert_bitmap(source, 1, 0)?;
            (
                PixelMode::Gray,
                converted.num_grays,
                converted.width,
                converted.rows,
                converted.pitch,
                converted.buffer,
            )
        }
        mode => {
            let pitch_abs = source.pitch.checked_abs().ok_or(ConvertError::Invalid)?;
            let len = usize::try_from(pitch_abs)
                .ok()
                .and_then(|p| usize::try_from(source.rows).ok()?.checked_mul(p))
                .ok_or(ConvertError::TooLarge)?;
            if source.buffer.len() < len {
                return Err(ConvertError::Invalid);
            }
            let mut copy = Vec::new();
            copy.try_reserve_exact(len)
                .map_err(|_| ConvertError::TooLarge)?;
            copy.extend_from_slice(source.buffer.get(..len).ok_or(ConvertError::Invalid)?);
            let grays = if mode == PixelMode::Mono { 2 } else { 256 };
            (mode, grays, source.width, source.rows, source.pitch, copy)
        }
    };
    if mode == PixelMode::Bgra || (x_pixels == 0 && y_pixels == 0) {
        return Ok(EmboldenedBitmap {
            width: src_width,
            rows: src_rows,
            pitch: src_pitch,
            pixel_mode: mode,
            num_grays: grays,
            buffer: src_bytes,
        });
    }
    let x_pixels = match mode {
        PixelMode::Mono => x_pixels.min(8),
        PixelMode::Lcd => x_pixels.checked_mul(3).ok_or(ConvertError::TooLarge)?,
        _ => x_pixels,
    };
    let y_pixels = if mode == PixelMode::LcdV {
        y_pixels.checked_mul(3).ok_or(ConvertError::TooLarge)?
    } else {
        y_pixels
    };
    let width = src_width
        .checked_add(x_pixels)
        .ok_or(ConvertError::TooLarge)?;
    let rows = src_rows
        .checked_add(y_pixels)
        .ok_or(ConvertError::TooLarge)?;
    let needed_pitch = if mode == PixelMode::Mono {
        width.checked_add(7).ok_or(ConvertError::TooLarge)? / 8
    } else {
        width
    };
    let src_pitch_abs = src_pitch
        .checked_abs()
        .ok_or(ConvertError::Invalid)?
        .cast_unsigned();
    let pitch_abs = if y_pixels == 0 {
        needed_pitch.max(src_pitch_abs)
    } else {
        needed_pitch
    };
    let pitch = i32::try_from(pitch_abs).map_err(|_| ConvertError::TooLarge)?;
    let pitch = if src_pitch < 0 {
        pitch.checked_neg().ok_or(ConvertError::TooLarge)?
    } else {
        pitch
    };
    let len = usize::try_from(pitch_abs)
        .ok()
        .and_then(|p| usize::try_from(rows).ok()?.checked_mul(p))
        .ok_or(ConvertError::TooLarge)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| ConvertError::TooLarge)?;
    bytes.resize(len, 0);
    let new_rows = usize::try_from(rows).map_err(|_| ConvertError::TooLarge)?;
    let old_rows = usize::try_from(src_rows).map_err(|_| ConvertError::TooLarge)?;
    let y_pixels = usize::try_from(y_pixels).map_err(|_| ConvertError::TooLarge)?;
    let src_stride = usize::try_from(src_pitch_abs).map_err(|_| ConvertError::TooLarge)?;
    let dst_stride = usize::try_from(pitch_abs).map_err(|_| ConvertError::TooLarge)?;
    let copy_len = if mode == PixelMode::Mono {
        usize::try_from(src_width.saturating_add(7) / 8).map_err(|_| ConvertError::TooLarge)?
    } else {
        usize::try_from(src_width).map_err(|_| ConvertError::TooLarge)?
    };
    if copy_len > src_stride {
        return Err(ConvertError::Invalid);
    }
    for row in 0..old_rows {
        let src_at = row_start(row, old_rows, src_stride, src_pitch < 0)?;
        let dst_at = row_start(
            row.saturating_add(y_pixels),
            new_rows,
            dst_stride,
            pitch < 0,
        )?;
        let src = src_bytes
            .get(src_at..src_at.saturating_add(copy_len))
            .ok_or(ConvertError::Invalid)?;
        let dst = bytes
            .get_mut(dst_at..dst_at.saturating_add(copy_len))
            .ok_or(ConvertError::TooLarge)?;
        dst.copy_from_slice(src);
    }
    let x_pixels = usize::try_from(x_pixels).map_err(|_| ConvertError::TooLarge)?;
    for row in 0..old_rows {
        let at = row_start(
            row.saturating_add(y_pixels),
            new_rows,
            dst_stride,
            pitch < 0,
        )?;
        for col in (0..dst_stride).rev() {
            let pos = at.checked_add(col).ok_or(ConvertError::TooLarge)?;
            let old = *bytes.get(pos).ok_or(ConvertError::TooLarge)?;
            for step in 1..=x_pixels {
                if mode == PixelMode::Mono {
                    let right = old
                        .checked_shr(u32::try_from(step).map_err(|_| ConvertError::TooLarge)?)
                        .unwrap_or(0);
                    let left = if col > 0 {
                        bytes
                            .get(pos.saturating_sub(1))
                            .copied()
                            .unwrap_or(0)
                            .checked_shl(
                                u32::try_from(8usize.saturating_sub(step))
                                    .map_err(|_| ConvertError::TooLarge)?,
                            )
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    let current = bytes.get_mut(pos).ok_or(ConvertError::TooLarge)?;
                    *current |= right | left;
                } else if col >= step {
                    let previous = *bytes
                        .get(pos.saturating_sub(step))
                        .ok_or(ConvertError::TooLarge)?;
                    let current = bytes.get_mut(pos).ok_or(ConvertError::TooLarge)?;
                    *current = current
                        .saturating_add(previous)
                        .min(u8::try_from(grays.saturating_sub(1)).unwrap_or(u8::MAX));
                }
            }
        }
        for step in 1..=y_pixels {
            let above_row = row.saturating_add(y_pixels).saturating_sub(step);
            let above = row_start(above_row, new_rows, dst_stride, pitch < 0)?;
            for col in 0..dst_stride {
                let value = *bytes
                    .get(at.saturating_add(col))
                    .ok_or(ConvertError::TooLarge)?;
                let to = bytes
                    .get_mut(above.saturating_add(col))
                    .ok_or(ConvertError::TooLarge)?;
                *to |= value;
            }
        }
    }
    Ok(EmboldenedBitmap {
        width,
        rows,
        pitch,
        pixel_mode: mode,
        num_grays: grays,
        buffer: bytes,
    })
}

fn rounded_pixels(strength: i32) -> Result<u32, ConvertError> {
    let rounded = strength.checked_add(32).ok_or(ConvertError::TooLarge)? & !63;
    if rounded < 0 {
        return Err(ConvertError::Invalid);
    }
    Ok(rounded.cast_unsigned() / 64)
}

fn row_start(row: usize, rows: usize, stride: usize, reverse: bool) -> Result<usize, ConvertError> {
    let physical = if reverse {
        rows.saturating_sub(row.saturating_add(1))
    } else {
        row
    };
    physical.checked_mul(stride).ok_or(ConvertError::TooLarge)
}

#[cfg(test)]
mod slot_tests {
    use super::{BitmapSlotMetrics, BitmapView, ConvertError, PixelMode, embolden_bitmap_slot};

    #[test]
    fn adr0063_i2_bitmap_slot_matches_freetype() -> Result<(), ConvertError> {
        let source = BitmapView {
            width: 2,
            rows: 2,
            pitch: 2,
            pixel_mode: PixelMode::Gray,
            buffer: &[64, 128, 32, 255],
        };
        let metrics = BitmapSlotMetrics {
            advance_x: 576,
            advance_y: 0,
            width: 576,
            height: 768,
            hori_bearing_x: 0,
            hori_bearing_y: 768,
            hori_advance: 576,
            vert_bearing_x: -320,
            vert_bearing_y: 256,
            vert_advance: 1408,
            bitmap_left: 1,
            bitmap_top: 2,
        };
        let result = embolden_bitmap_slot(source, 16, 16, metrics)?;
        assert_eq!(
            (result.bitmap.width, result.bitmap.rows, result.bitmap.pitch),
            (3, 2, 3)
        );
        assert_eq!(result.bitmap.buffer, [64, 192, 128, 32, 255, 255]);
        assert_eq!(
            result.metrics,
            BitmapSlotMetrics {
                advance_x: 640,
                width: 640,
                hori_advance: 640,
                ..metrics
            }
        );
        Ok(())
    }

    #[test]
    fn adr0063_i2_bitmap_slot_vertical_growth_matches_freetype() -> Result<(), ConvertError> {
        let source = BitmapView {
            width: 2,
            rows: 2,
            pitch: 2,
            pixel_mode: PixelMode::Gray,
            buffer: &[64, 128, 32, 255],
        };
        let metrics = BitmapSlotMetrics {
            advance_x: 1088,
            advance_y: 0,
            width: 896,
            height: 1472,
            hori_bearing_x: 128,
            hori_bearing_y: 1472,
            hori_advance: 1088,
            vert_bearing_x: -448,
            vert_bearing_y: 512,
            vert_advance: 2688,
            bitmap_left: 1,
            bitmap_top: 2,
        };
        let result = embolden_bitmap_slot(source, 32, 32, metrics)?;
        assert_eq!(
            (result.bitmap.width, result.bitmap.rows, result.bitmap.pitch),
            (3, 3, 3)
        );
        assert_eq!(
            result.bitmap.buffer,
            [64, 192, 128, 96, 255, 255, 32, 255, 255]
        );
        assert_eq!(
            result.metrics,
            BitmapSlotMetrics {
                advance_x: 1152,
                width: 960,
                height: 1536,
                hori_bearing_y: 1536,
                hori_advance: 1152,
                vert_advance: 2752,
                bitmap_top: 3,
                ..metrics
            }
        );
        Ok(())
    }
}
