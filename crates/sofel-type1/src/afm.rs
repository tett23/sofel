//! AFM pair metrics used by Type 1 faces (sofel ADR 0065).

use alloc::vec::Vec;

pub use crate::noalloc::afm::{AfmError, validate};
use crate::noalloc::afm::{Pair, scan};

/// Parsed AFM pair metrics, borrowing glyph names from the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Afm<'a> {
    pairs: Vec<Pair<'a>>,
}

impl<'a> Afm<'a> {
    /// Parse an AFM file with checked pair counts and a required end marker.
    ///
    /// Allocates the table of kerning pairs (ADR 0094).
    pub fn parse(data: &'a [u8]) -> Result<Self, AfmError> {
        let mut pairs = Vec::new();
        scan(data, &mut |pair| pairs.push(pair)).map_err(|_| AfmError)?;
        pairs.sort_by(|a, b| (a.left, a.right).cmp(&(b.left, b.right)));
        Ok(Self { pairs })
    }

    /// Return the horizontal and vertical adjustment for two glyph names.
    pub fn kerning(&self, left: &[u8], right: &[u8]) -> (i32, i32) {
        self.pairs
            .binary_search_by(|pair| (pair.left, pair.right).cmp(&(left, right)))
            .ok()
            .and_then(|index| self.pairs.get(index))
            .map_or((0, 0), |pair| (pair.x, pair.y))
    }
}
