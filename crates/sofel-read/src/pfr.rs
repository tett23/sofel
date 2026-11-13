//! Portable Font Resource logical faces and physical character descriptors.

use crate::{GlyphId, ReadError, be};

fn take<'a>(bytes: &mut &'a [u8], size: usize) -> Result<&'a [u8], ReadError> {
    let (value, tail) = bytes.split_at_checked(size).ok_or(ReadError::OutOfBounds)?;
    *bytes = tail;
    Ok(value)
}

fn number(bytes: &mut &[u8], width: usize) -> Result<usize, ReadError> {
    Ok(take(bytes, width)?
        .iter()
        .fold(0, |n, b| (n << 8) | usize::from(*b)))
}

fn word16(bytes: &mut &[u8]) -> Result<u16, ReadError> {
    be::u16_at(take(bytes, 2)?, 0).ok_or(ReadError::OutOfBounds)
}
fn signed16(bytes: &mut &[u8]) -> Result<i16, ReadError> {
    be::i16_at(take(bytes, 2)?, 0).ok_or(ReadError::OutOfBounds)
}
fn extras(bytes: &mut &[u8]) -> Result<(), ReadError> {
    for _ in 0..number(bytes, 1)? {
        let size = number(bytes, 1)?;
        take(bytes, size.saturating_add(1))?;
    }
    Ok(())
}

fn section(data: &[u8], offset: usize, size: usize) -> Result<&[u8], ReadError> {
    be::slice_at(data, offset, size).ok_or(ReadError::OutOfBounds)
}

/// A PFR face borrowing its character directory and glyph program section.
#[derive(Debug)]
pub struct PfrFace<'a> {
    faces: u16,
    glyphs: u16,
    units: u16,
    metrics_units: u16,
    flags: u8,
    fixed_advance: i16,
    records: &'a [u8],
    record_size: usize,
    programs: &'a [u8],
}

impl<'a> PfrFace<'a> {
    /// Reads one logical font and validates its glyph program ranges.
    // Counts have at most 16 bits, and character records occupy at most 10 bytes.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn read(data: &'a [u8], index: u32) -> Result<Self, ReadError> {
        let header = data.first_chunk::<58>().ok_or(ReadError::OutOfBounds)?;
        if !header.starts_with(b"PFR0")
            || be::u16_at(header, 6) != Some(0x0d0a)
            || be::u16_at(header, 4) > Some(4)
        {
            return Err(ReadError::InvalidValue);
        }
        let word = |i| {
            be::u16_at(header, i)
                .map(usize::from)
                .ok_or(ReadError::OutOfBounds)
        };
        let triple = |i| {
            be::u24_at(header, i)
                .map(be::to_usize)
                .ok_or(ReadError::OutOfBounds)
        };
        if word(8)? < 58 {
            return Err(ReadError::InvalidValue);
        }
        section(data, 0, word(8)?)?;
        let mut directory = section(data, word(12)?, word(10)?)?;
        let faces = word16(&mut directory)?;
        let entries = take(&mut directory, usize::from(faces) * 5)?;
        if index >= u32::from(faces) {
            return Err(ReadError::OutOfBounds);
        }
        let mut entry = section(entries, be::to_usize(index).saturating_mul(5), 5)?;
        let logical_size = number(&mut entry, 2)?;
        let logical_offset = number(&mut entry, 3)?;
        let within = |start: usize, len: usize, offset: usize, size: usize| {
            section(
                section(data, start, len)?,
                offset.checked_sub(start).ok_or(ReadError::OutOfBounds)?,
                size,
            )
        };
        let mut logical = within(triple(19)?, triple(16)?, logical_offset, logical_size)?;
        take(&mut logical, 12)?;
        let style = number(&mut logical, 1)?;
        if style & 4 != 0 {
            take(
                &mut logical,
                1 + usize::from(style & 8 != 0) + 3 * usize::from(style & 3 == 0),
            )?;
        }
        if style & 16 != 0 {
            take(&mut logical, 1 + usize::from(style & 32 != 0))?;
        }
        if style & 64 != 0 {
            extras(&mut logical)?;
        }
        let mut size = number(&mut logical, 2)?;
        let offset = number(&mut logical, 3)?;
        if header[41] != 0 {
            size |= number(&mut logical, 1)? << 16;
        }
        let mut physical = within(triple(27)?, triple(24)?, offset, size)?;
        take(&mut physical, 2)?;
        let units = word16(&mut physical)?;
        let metrics_units = word16(&mut physical)?;
        if units == 0 || metrics_units == 0 {
            return Err(ReadError::InvalidValue);
        }
        take(&mut physical, 8)?;
        let flags = be::u8_at(take(&mut physical, 1)?, 0).ok_or(ReadError::OutOfBounds)?;
        let fixed_advance = if flags & 4 == 0 {
            signed16(&mut physical)?
        } else {
            0
        };
        if flags & 128 != 0 {
            extras(&mut physical)?;
        }
        let aux = number(&mut physical, 3)?;
        take(&mut physical, aux)?;
        let blues = number(&mut physical, 1)?;
        take(&mut physical, 2 * blues + 6)?;
        let glyphs = word16(&mut physical)?;
        if glyphs == 0 {
            return Err(ReadError::InvalidValue);
        }
        let record_size = 4
            + usize::from(flags & 2 != 0)
            + 2 * usize::from(flags & 4 != 0)
            + usize::from(flags & 8 != 0)
            + usize::from(flags & 16 != 0)
            + usize::from(flags & 32 != 0);
        let face = Self {
            faces,
            glyphs,
            units,
            metrics_units,
            flags,
            fixed_advance,
            record_size,
            records: take(&mut physical, usize::from(glyphs) * record_size)?,
            programs: section(data, triple(35)?, triple(32)?)?,
        };
        let mut prev = None;
        for i in 0..usize::from(glyphs) {
            let (code, _, offset, size) = face.record(i)?;
            if prev.is_some_and(|v| code <= v) {
                return Err(ReadError::InvalidValue);
            }
            prev = Some(code);
            section(face.programs, offset, size)?;
        }
        Ok(face)
    }

    // Each flag contributes at most one byte to a field width.
    #[allow(clippy::arithmetic_side_effects)]
    fn record(&self, index: usize) -> Result<(u32, i16, usize, usize), ReadError> {
        let mut bytes = section(
            self.records,
            index
                .checked_mul(self.record_size)
                .ok_or(ReadError::OutOfBounds)?,
            self.record_size,
        )?;
        let flags = self.flags;
        let code = be::to_u32(number(&mut bytes, 1 + usize::from(flags & 2 != 0))?);
        let advance = if flags & 4 != 0 {
            signed16(&mut bytes)?
        } else {
            self.fixed_advance
        };
        take(&mut bytes, usize::from(flags & 8 != 0))?;
        let size = number(&mut bytes, 1 + usize::from(flags & 16 != 0))?;
        let offset = number(&mut bytes, 2 + usize::from(flags & 32 != 0))?;
        Ok((code, advance, offset, size))
    }

    /// Number of logical faces in this resource.
    pub fn face_count(&self) -> u16 {
        self.faces
    }
    /// Number of glyphs including the missing glyph.
    pub fn glyph_count(&self) -> u32 {
        u32::from(self.glyphs) + 1
    }
    /// Coordinate units per em of the physical outlines.
    pub fn units_per_em(&self) -> u16 {
        self.units
    }
    /// Finds a character in the sorted physical character directory.
    pub fn nominal_glyph(&self, code: u32) -> Option<GlyphId> {
        let (mut lo, mut hi) = (0, usize::from(self.glyphs));
        while lo < hi {
            let mid = lo.midpoint(hi);
            match self.record(mid).ok()?.0.cmp(&code) {
                core::cmp::Ordering::Equal => {
                    return Some(GlyphId(be::to_u32(mid).saturating_add(1)));
                }
                core::cmp::Ordering::Less => lo = mid.saturating_add(1),
                core::cmp::Ordering::Greater => hi = mid,
            }
        }
        None
    }
    /// Unscaled advance along the physical font's writing direction.
    // An i16 advance times a u16 resolution fits i64; read rejects a zero divisor.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn advance(&self, glyph: GlyphId) -> Option<i32> {
        let raw = i64::from(self.record(glyph.0.saturating_sub(1) as usize).ok()?.1);
        let den = i64::from(self.metrics_units);
        i32::try_from(raw.signum() * ((raw.abs() * i64::from(self.units) + den / 2) / den)).ok()
    }
    /// Encoded GPS bytes for an outline.
    pub fn glyph_program(&self, glyph: GlyphId) -> Option<&'a [u8]> {
        let (_, _, offset, size) = self.record(glyph.0.saturating_sub(1) as usize).ok()?;
        be::slice_at(self.programs, offset, size)
    }
}
