//! FreeType 2.15.0 `FT_Bitmap_Convert` (src/base/ftbitmap.c).

use alloc::vec::Vec;

use crate::PixelMode;

/// A source bitmap with FreeType's signed row pitch. The buffer begins
/// at the first stored row; a negative pitch stores the bottom row first.
#[derive(Clone, Copy, Debug)]
pub struct BitmapView<'a> {
    /// Source width in pixels (or subpixel samples for LCD modes).
    pub width: u32,
    /// Number of rows.
    pub rows: u32,
    /// Signed bytes per row.
    pub pitch: i32,
    /// Source pixel format.
    pub pixel_mode: PixelMode,
    /// At least `abs(pitch) * rows` bytes.
    pub buffer: &'a [u8],
}

/// The 8-bit output of [`convert_bitmap`], with FreeType's gray count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvertedBitmap {
    /// Width in pixels or subpixel samples.
    pub width: u32,
    /// Number of rows.
    pub rows: u32,
    /// Signed bytes per row, including alignment padding.
    pub pitch: i32,
    /// Number of representable gray values (2, 4, 16 or 256).
    pub num_grays: u16,
    /// Rows in the flow specified by [`Self::pitch`].
    pub buffer: Vec<u8>,
}

/// A bitmap conversion input that cannot be represented or safely read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvertError {
    /// The dimensions, pitch, alignment or source bytes are invalid.
    Invalid,
    /// The converted bitmap is too large to allocate.
    TooLarge,
}

/// Convert MONO, GRAY2, GRAY4, GRAY, LCD, LCD_V or BGRA pixels to
/// FreeType's 8-bit GRAY layout.
///
/// `alignment` has FreeType's row-width rounding. A zero target pitch
/// inherits a negative source flow; a negative hint forces negative
/// flow, and a positive hint forces positive flow.
pub fn convert_bitmap(
    source: BitmapView<'_>,
    alignment: i32,
    target_pitch_hint: i32,
) -> Result<ConvertedBitmap, ConvertError> {
    let width = i32::try_from(source.width).map_err(|_| ConvertError::Invalid)?;
    let pitch_abs = source.pitch.checked_abs().ok_or(ConvertError::Invalid)?;
    let row_bytes = match source.pixel_mode {
        PixelMode::Mono => source.width.saturating_add(7) / 8,
        PixelMode::Gray2 => source.width.saturating_add(3) / 4,
        PixelMode::Gray4 => source.width.saturating_add(1) / 2,
        PixelMode::Gray | PixelMode::Lcd | PixelMode::LcdV => source.width,
        PixelMode::Bgra => source.width.checked_mul(4).ok_or(ConvertError::TooLarge)?,
    };
    if u32::try_from(pitch_abs).map_err(|_| ConvertError::Invalid)? < row_bytes {
        return Err(ConvertError::Invalid);
    }
    let source_len = usize::try_from(pitch_abs)
        .ok()
        .and_then(|p| {
            usize::try_from(source.rows)
                .ok()
                .and_then(|r| p.checked_mul(r))
        })
        .ok_or(ConvertError::TooLarge)?;
    if source.buffer.len() < source_len {
        return Err(ConvertError::Invalid);
    }
    let align = alignment.checked_abs().ok_or(ConvertError::Invalid)?;
    let out_width = if align == 0 {
        width
    } else {
        let rem = width.checked_rem(align).ok_or(ConvertError::Invalid)?;
        if rem == 0 {
            width
        } else {
            width
                .checked_sub(rem)
                .and_then(|v| v.checked_add(align))
                .ok_or(ConvertError::TooLarge)?
        }
    };
    let out_pitch = if target_pitch_hint < 0 || (target_pitch_hint == 0 && source.pitch < 0) {
        out_width.checked_neg().ok_or(ConvertError::TooLarge)?
    } else {
        out_width
    };
    let len = usize::try_from(out_width)
        .ok()
        .and_then(|w| {
            usize::try_from(source.rows)
                .ok()
                .and_then(|r| w.checked_mul(r))
        })
        .ok_or(ConvertError::TooLarge)?;
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(len)
        .map_err(|_| ConvertError::TooLarge)?;
    buffer.resize(len, 0);
    let rows = usize::try_from(source.rows).map_err(|_| ConvertError::TooLarge)?;
    let src_pitch = usize::try_from(pitch_abs).map_err(|_| ConvertError::TooLarge)?;
    let dst_pitch = usize::try_from(out_width).map_err(|_| ConvertError::TooLarge)?;
    let width = usize::try_from(source.width).map_err(|_| ConvertError::TooLarge)?;
    for logical_row in 0..rows {
        let reversed = rows.saturating_sub(logical_row.saturating_add(1));
        let src_row = if source.pitch < 0 {
            reversed
        } else {
            logical_row
        };
        let dst_row = if out_pitch < 0 { reversed } else { logical_row };
        let src_start = src_row
            .checked_mul(src_pitch)
            .ok_or(ConvertError::TooLarge)?;
        let dst_start = dst_row
            .checked_mul(dst_pitch)
            .ok_or(ConvertError::TooLarge)?;
        for col in 0..width {
            let value = match source.pixel_mode {
                PixelMode::Mono => packed(source.buffer, src_start, col, 1)?,
                PixelMode::Gray2 => packed(source.buffer, src_start, col, 2)?,
                PixelMode::Gray4 => packed(source.buffer, src_start, col, 4)?,
                PixelMode::Gray | PixelMode::Lcd | PixelMode::LcdV => *source
                    .buffer
                    .get(src_start.saturating_add(col))
                    .ok_or(ConvertError::Invalid)?,
                PixelMode::Bgra => {
                    let at = src_start
                        .checked_add(col.checked_mul(4).ok_or(ConvertError::TooLarge)?)
                        .ok_or(ConvertError::TooLarge)?;
                    gray_bgra(
                        source
                            .buffer
                            .get(at..at.saturating_add(4))
                            .ok_or(ConvertError::Invalid)?,
                    )
                }
            };
            *buffer
                .get_mut(dst_start.saturating_add(col))
                .ok_or(ConvertError::TooLarge)? = value;
        }
    }
    let num_grays = match source.pixel_mode {
        PixelMode::Mono => 2,
        PixelMode::Gray2 => 4,
        PixelMode::Gray4 => 16,
        _ => 256,
    };
    Ok(ConvertedBitmap {
        width: source.width,
        rows: source.rows,
        pitch: out_pitch,
        num_grays,
        buffer,
    })
}

#[allow(clippy::arithmetic_side_effects)] // bits is restricted to 1, 2 or 4 at each call site.
fn packed(bytes: &[u8], start: usize, col: usize, bits: usize) -> Result<u8, ConvertError> {
    let per_byte = 8 / bits;
    let byte = *bytes
        .get(start.saturating_add(col / per_byte))
        .ok_or(ConvertError::Invalid)?;
    let shift = 8 - bits * (col % per_byte + 1);
    Ok((byte >> shift) & ((1 << bits) - 1))
}

#[allow(clippy::arithmetic_side_effects)] // Products fit u64 for four 8-bit channels.
fn gray_bgra(pixel: &[u8]) -> u8 {
    let (Some(&b), Some(&g), Some(&r), Some(&a)) =
        (pixel.first(), pixel.get(1), pixel.get(2), pixel.get(3))
    else {
        return 0;
    };
    if a == 0 {
        return 0;
    }
    let l = (4731u64 * u64::from(b) * u64::from(b)
        + 46868u64 * u64::from(g) * u64::from(g)
        + 13937u64 * u64::from(r) * u64::from(r))
        >> 16;
    u8::try_from(u64::from(a).saturating_sub(l / u64::from(a))).unwrap_or(0)
}
