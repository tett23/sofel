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
