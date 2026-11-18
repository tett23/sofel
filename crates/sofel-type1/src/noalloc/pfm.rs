use core::fmt;

use crate::noalloc::Problem;

/// Invalid or truncated PFM data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PfmError;

impl fmt::Display for PfmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid PFM metrics")
    }
}

impl core::error::Error for PfmError {}

/// Read the PFM extension table and its optional kerning pairs, passing
/// each `(left, right, x)` to `pair`. Fails with the offset of the field
/// in error.
pub(crate) fn scan(data: &[u8], pair: &mut dyn FnMut(u8, u8, i16)) -> Result<(), usize> {
    if data.len() < 7 {
        return Err(data.len());
    }
    if data.get(1).copied().ok_or(1usize)? >= 4 {
        return Err(1);
    }
    if usize::try_from(le_u32(data, 2)?).map_err(|_| 2usize)? != data.len() {
        return Err(2);
    }
    let width_length = usize::from(le_u16(data, 99)?);
    let extension = 117usize.checked_add(width_length).ok_or(99usize)?;
    let Some(extension_end) = extension.checked_add(18) else {
        return Err(99);
    };
    if extension_end > data.len() || le_u16(data, extension)? < 18 {
        return Ok(());
    }
    let offset_at = extension.checked_add(14).ok_or(extension)?;
    let pair_offset = usize::try_from(le_u32(data, offset_at)?).map_err(|_| offset_at)?;
    if pair_offset == 0 {
        return Ok(());
    }
    let count = usize::from(le_u16(data, pair_offset).map_err(|_| offset_at)?);
    let start = pair_offset.checked_add(2).ok_or(offset_at)?;
    let end = start
        .checked_add(count.checked_mul(4).ok_or(pair_offset)?)
        .ok_or(pair_offset)?;
    let bytes = data.get(start..end).ok_or(pair_offset)?;
    for &[left, right, lo, hi] in bytes.as_chunks::<4>().0 {
        pair(left, right, i16::from_le_bytes([lo, hi]));
    }
    Ok(())
}

/// Validate PFM metrics: the header, the declared length, and the bounds
/// of the extension and kerning tables.
pub fn validate(data: &[u8], report: &mut dyn FnMut(Problem)) {
    if let Err(offset) = scan(data, &mut |_, _, _| {}) {
        report(Problem {
            offset,
            reason: "invalid PFM metrics",
        });
    }
}

fn le_u16(data: &[u8], at: usize) -> Result<u16, usize> {
    let bytes = data.get(at..at.checked_add(2).ok_or(at)?).ok_or(at)?;
    Ok(u16::from_le_bytes(bytes.try_into().map_err(|_| at)?))
}

fn le_u32(data: &[u8], at: usize) -> Result<u32, usize> {
    let bytes = data.get(at..at.checked_add(4).ok_or(at)?).ok_or(at)?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| at)?))
}
