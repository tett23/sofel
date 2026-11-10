//! [`SubsetError`].

use core::fmt;

use sofel_font::read::Tag;

/// Why subsetting failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SubsetError {
    /// The font has no glyphs.
    NoGlyphs,
    /// An input HarfBuzz refuses: a glyph mapping that is not one-to-one,
    /// maps a glyph other than 0 to 0, or is combined with `RETAIN_GIDS`;
    /// an axis the font does not have.
    InvalidInput,
    /// A table could not be read.
    Malformed {
        /// The table.
        tag: Tag,
    },
    /// A table, flag or axis setting this version does not subset yet.
    Unsupported {
        /// The table (`    ` when not about a table).
        tag: Tag,
        /// What is missing.
        reason: &'static str,
    },
    /// An offset overflow that repacking did not resolve.
    OffsetOverflow {
        /// The table.
        tag: Tag,
    },
    /// A limit of HarfBuzz's was reached.
    Limit {
        /// The table.
        tag: Tag,
    },
    /// The table could not be written as HarfBuzz would (HarfBuzz fails the
    /// subset as well).
    Failed {
        /// The table.
        tag: Tag,
    },
}

impl fmt::Display for SubsetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubsetError::NoGlyphs => f.write_str("the font has no glyphs"),
            SubsetError::InvalidInput => f.write_str("invalid subsetting input"),
            SubsetError::Malformed { tag } => write!(f, "malformed table {tag}"),
            SubsetError::Unsupported { tag, reason } => write!(f, "unsupported ({tag}): {reason}"),
            SubsetError::OffsetOverflow { tag } => write!(f, "offset overflow in {tag}"),
            SubsetError::Limit { tag } => write!(f, "limit reached in {tag}"),
            SubsetError::Failed { tag } => write!(f, "subsetting {tag} failed"),
        }
    }
}
