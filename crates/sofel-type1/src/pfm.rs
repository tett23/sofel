//! Windows PFM pair metrics used by Type 1 faces (sofel ADR 0065).

use alloc::vec::Vec;
use core::fmt;

/// Invalid or truncated PFM data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PfmError;

impl fmt::Display for PfmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid PFM metrics")
    }
}

impl core::error::Error for PfmError {}

/// PFM kerning pairs keyed by two encoded character values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pfm {
    pairs: Vec<(u8, u8, i16)>,
}

impl Pfm {
    /// Parse the PFM extension table and its optional kerning pairs.
    pub fn parse(data: &[u8]) -> Result<Self, PfmError> {
        if data.len() < 7 || data.get(1).copied().ok_or(PfmError)? >= 4 {
            return Err(PfmError);
        }
        if usize::try_from(le_u32(data, 2)?).map_err(|_| PfmError)? != data.len() {
            return Err(PfmError);
        }
        let width_length = usize::from(le_u16(data, 99)?);
        let extension = 119usize.checked_add(width_length).ok_or(PfmError)?;
        let Some(extension_end) = extension.checked_add(18) else {
            return Err(PfmError);
        };
        if extension_end > data.len() || le_u16(data, extension)? < 18 {
            return Ok(Self { pairs: Vec::new() });
        }
        let offset_at = extension.checked_add(14).ok_or(PfmError)?;
        let pair_offset = usize::try_from(le_u32(data, offset_at)?).map_err(|_| PfmError)?;
        if pair_offset == 0 {
            return Ok(Self { pairs: Vec::new() });
        }
        let count = usize::from(le_u16(data, pair_offset)?);
        let start = pair_offset.checked_add(2).ok_or(PfmError)?;
        let end = start
            .checked_add(count.checked_mul(4).ok_or(PfmError)?)
            .ok_or(PfmError)?;
        let bytes = data.get(start..end).ok_or(PfmError)?;
        let mut pairs = Vec::with_capacity(count);
        for &[left, right, lo, hi] in bytes.as_chunks::<4>().0 {
            let x = i16::from_le_bytes([lo, hi]);
            pairs.push((left, right, x));
        }
        pairs.sort_unstable_by_key(|pair| (pair.0, pair.1));
        Ok(Self { pairs })
    }

    /// Return the horizontal pair adjustment for two encoded characters.
    pub fn kerning(&self, left: u8, right: u8) -> i16 {
        self.pairs
            .binary_search_by_key(&(left, right), |pair| (pair.0, pair.1))
            .ok()
            .and_then(|index| self.pairs.get(index))
            .map_or(0, |pair| pair.2)
    }

    /// All encoded kerning pairs in character-code order.
    pub fn pairs(&self) -> impl Iterator<Item = (u8, u8, i16)> + '_ {
        self.pairs.iter().copied()
    }
}

fn le_u16(data: &[u8], at: usize) -> Result<u16, PfmError> {
    let bytes = data
        .get(at..at.checked_add(2).ok_or(PfmError)?)
        .ok_or(PfmError)?;
    Ok(u16::from_le_bytes(bytes.try_into().map_err(|_| PfmError)?))
}

fn le_u32(data: &[u8], at: usize) -> Result<u32, PfmError> {
    let bytes = data
        .get(at..at.checked_add(4).ok_or(PfmError)?)
        .ok_or(PfmError)?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| PfmError)?))
}
