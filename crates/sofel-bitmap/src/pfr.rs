//! PFR embedded monochrome strikes: exact-size bitmap loading from the strike
//! programs of a PFR face, following FreeType's src/pfr/pfrgload.c (FTL).

use alloc::vec::Vec;
use sofel_font::PfrFace;
use sofel_font::geom::mul_div;
use sofel_font::read::{GlyphId, ReadError, be};

struct Bytes<'a>(&'a [u8]);
impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ReadError> {
        let (value, tail) = self.0.split_at_checked(n).ok_or(ReadError::OutOfBounds)?;
        self.0 = tail;
        Ok(value)
    }
    fn byte(&mut self) -> Result<u8, ReadError> {
        self.take(1)?.first().copied().ok_or(ReadError::OutOfBounds)
    }
    fn signed(&mut self) -> Result<i32, ReadError> {
        Ok(i32::from(i8::from_be_bytes([self.byte()?])))
    }
    fn word(&mut self) -> Result<i32, ReadError> {
        be::i16_at(self.take(2)?, 0)
            .map(i32::from)
            .ok_or(ReadError::OutOfBounds)
    }
    fn long(&mut self) -> Result<i32, ReadError> {
        let n = be::u24_at(self.take(3)?, 0).ok_or(ReadError::OutOfBounds)?;
        Ok(i32::from_be_bytes((n << 8).to_be_bytes()) >> 8)
    }
}

/// A monochrome PFR strike image. Rows run from top to bottom, MSB first.
#[derive(Debug, PartialEq, Eq)]
pub struct PfrBitmap {
    /// Number of columns.
    pub width: u16,
    /// Number of rows.
    pub height: u16,
    /// Horizontal bearing in pixels.
    pub left: i32,
    /// Distance from baseline to the top in pixels.
    pub top: i32,
    /// Horizontal advance in 26.6 pixels.
    pub advance: i32,
    /// Packed rows with a byte boundary at the start of each row.
    pub pixels: Vec<u8>,
}

/// Loads an embedded monochrome bitmap of a PFR glyph at an exact strike size.
pub fn pfr_bitmap(
    face: &PfrFace<'_>,
    glyph: GlyphId,
    x: u16,
    y: u16,
) -> Result<Option<PfrBitmap>, ReadError> {
    let Some(sofel_font::PfrBitmapProgram {
        bytes: program,
        top_down,
        advance: raw,
        units,
    }) = face.bitmap_program(glyph, x, y)?
    else {
        return Ok(None);
    };
    let mut bytes = Bytes(program);
    let flags = bytes.byte()?;
    let (left, bottom) = match flags & 3 {
        0 => {
            let b = bytes.byte()?;
            (
                i32::from(i8::from_be_bytes([b])) >> 4,
                i32::from(i8::from_be_bytes([b << 4])) >> 4,
            )
        }
        1 => (bytes.signed()?, bytes.signed()?),
        2 => (bytes.word()?, bytes.word()?),
        _ => (bytes.long()?, bytes.long()?),
    };
    let (width, height) = match (flags >> 2) & 3 {
        0 => (0, 0),
        1 => {
            let b = bytes.byte()?;
            (u16::from(b >> 4), u16::from(b & 15))
        }
        2 => (u16::from(bytes.byte()?), u16::from(bytes.byte()?)),
        _ => (
            be::u16_at(bytes.take(2)?, 0).ok_or(ReadError::OutOfBounds)?,
            be::u16_at(bytes.take(2)?, 0).ok_or(ReadError::OutOfBounds)?,
        ),
    };
    let advance = match (flags >> 4) & 3 {
        0 => mul_div(i32::from(x) << 8, i32::from(raw), i32::from(units)),
        1 => bytes.signed()? << 8,
        2 => bytes.word()?,
        _ => bytes.long()?,
    };
    let pixels = bitmap_pixels(bytes.0, width, height, flags >> 6, top_down)?;
    Ok(Some(PfrBitmap {
        width,
        height,
        left,
        top: bottom.saturating_add(i32::from(height)),
        advance: ((advance >> 2).saturating_add(32)) & !63,
        pixels,
    }))
}

fn bitmap_pixels(
    data: &[u8],
    width: u16,
    height: u16,
    format: u8,
    top_down: bool,
) -> Result<Vec<u8>, ReadError> {
    let width = usize::from(width);
    let rows = usize::from(height);
    let count = width.checked_mul(rows).ok_or(ReadError::OutOfBounds)?;
    let pitch = width.div_ceil(8);
    let len = pitch.checked_mul(rows).ok_or(ReadError::OutOfBounds)?;
    let mut runs = data
        .iter()
        .copied()
        .flat_map(|byte| [(byte >> 4), (byte & 15)]);
    let mut full = data.iter().copied();
    let limit = match format {
        0 => data.len().saturating_mul(8),
        1 => runs
            .clone()
            .map(usize::from)
            .fold(0usize, usize::saturating_add),
        2 => full
            .clone()
            .map(usize::from)
            .fold(0usize, usize::saturating_add),
        _ => return Err(ReadError::InvalidValue),
    };
    if count > limit {
        return Err(ReadError::OutOfBounds);
    }
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(len)
        .map_err(|_| ReadError::OutOfBounds)?;
    pixels.resize(len, 0);
    let mut left = 0u8;
    let mut black = true;
    for i in 0..count {
        let ink = if format == 0 {
            data.get(i >> 3).is_some_and(|b| b & (0x80 >> (i & 7)) != 0)
        } else {
            while left == 0 {
                left = if format == 1 {
                    runs.next()
                } else {
                    full.next()
                }
                .ok_or(ReadError::OutOfBounds)?;
                black = !black;
            }
            left = left.saturating_sub(1);
            black
        };
        if ink {
            let row = i.checked_div(width).ok_or(ReadError::InvalidValue)?;
            let col = i.checked_rem(width).ok_or(ReadError::InvalidValue)?;
            let row = if top_down {
                row
            } else {
                rows.saturating_sub(1).saturating_sub(row)
            };
            let at = row.saturating_mul(pitch).saturating_add(col >> 3);
            *pixels.get_mut(at).ok_or(ReadError::OutOfBounds)? |= 0x80 >> (col & 7);
        }
    }
    Ok(pixels)
}
