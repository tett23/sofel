use core::fmt;

/// Why a view could not be built.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReadError {
    /// `needed` bytes were required but only `available` were there.
    Truncated {
        /// Bytes required.
        needed: usize,
        /// Bytes present.
        available: usize,
    },
    /// An unknown table or file version.
    UnknownVersion(u32),
    /// An unknown subtable format.
    UnknownFormat(u16),
    /// An offset or index points outside the data.
    OutOfBounds,
    /// A value makes the structure unreadable (for example, no long
    /// metrics in `hmtx`).
    InvalidValue,
}

impl ReadError {
    pub(crate) fn truncated(needed: usize, available: usize) -> ReadError {
        ReadError::Truncated { needed, available }
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Truncated { needed, available } => {
                write!(f, "truncated: {needed} bytes needed, {available} available")
            }
            ReadError::UnknownVersion(v) => write!(f, "unknown version {v:#010x}"),
            ReadError::UnknownFormat(v) => write!(f, "unknown format {v}"),
            ReadError::OutOfBounds => f.write_str("offset out of bounds"),
            ReadError::InvalidValue => f.write_str("invalid value"),
        }
    }
}

impl core::error::Error for ReadError {}
