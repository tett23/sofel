//! Windows PFM pair metrics used by Type 1 faces (sofel ADR 0065).

use alloc::vec::Vec;

use crate::noalloc::pfm::scan;
pub use crate::noalloc::pfm::{PfmError, validate};

/// PFM kerning pairs keyed by two encoded character values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pfm {
    pairs: Vec<(u8, u8, i16)>,
}

impl Pfm {
    /// Parse the PFM extension table and its optional kerning pairs.
    ///
    /// Allocates the table of kerning pairs (ADR 0094).
    pub fn parse(data: &[u8]) -> Result<Self, PfmError> {
        let mut pairs = Vec::new();
        scan(data, &mut |left, right, x| pairs.push((left, right, x))).map_err(|_| PfmError)?;
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
