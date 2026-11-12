//! Validated Apple feature name and selector records.
use crate::be;

/// A feature table whose feature names and selector arrays fit in the input.
#[derive(Clone, Copy, Debug)]
pub struct Feat<'a>(&'a [[u8; 12]]);

impl<'a> Feat<'a> {
    /// Checks the version and every selector array before exposing any feature.
    pub fn new(data: &'a [u8]) -> Option<Self> {
        if be::u16_at(data, 0)? != 1 {
            return None;
        }
        let names = be::chunks_at::<12>(data, 12, usize::from(be::u16_at(data, 4)?))?;
        for row in names {
            be::chunks_at::<4>(
                data,
                be::u32_at(row, 4)? as usize,
                usize::from(be::u16_at(row, 2)?),
            )?;
        }
        Some(Self(names))
    }

    /// Reports whether the named feature has mutually exclusive settings.
    /// Returns None for a missing feature or a feature with no selectors.
    pub fn exclusive(self, kind: u16) -> Option<bool> {
        let index = self
            .0
            .binary_search_by_key(&kind, |r| u16::from_be_bytes([r[0], r[1]]))
            .ok()?;
        let row = self.0.get(index)?;
        (be::u16_at(row, 2)? != 0).then_some(be::u16_at(row, 8)? & 0x8000 != 0)
    }
}
