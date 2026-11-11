//! Writing sfnt fonts (ADR 0022): assembling a file from its tables as
//! HarfBuzz 15.1.0's face builder does ([`build_sfnt`]), and HarfBuzz's
//! object serializer ([`Serializer`]), which builds tables joined by
//! offsets with the same layout and object sharing as HarfBuzz, and
//! HarfBuzz's repacker, which resolves offset overflows
//! ([`Packed::repack`], [`serialize_objects`]).

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod repack;
mod serialize;
mod sfnt;

pub use repack::{GraphError, serialize_objects};
pub use serialize::{Errors, Link, ObjIdx, Object, Packed, Serializer, Snapshot, Whence};
pub use sfnt::{build_sfnt, build_sfnt_ordered, checksum};

use core::fmt;

use sofel_read::Tag;

/// Why writing failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WriteError {
    /// The same table tag was given twice.
    DuplicateTable(Tag),
    /// More than 65534 tables (HarfBuzz's face builder refuses the 65535th).
    TooManyTables,
    /// The file would be 2 GiB or larger.
    TooLarge,
    /// An offset does not fit in its field, even after repacking.
    OffsetOverflow,
    /// The objects could not be repacked (the repacker's error).
    Repack(GraphError),
    /// The serializer failed (its error bits).
    Serialize(Errors),
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteError::DuplicateTable(t) => write!(f, "table {t} given twice"),
            WriteError::TooManyTables => f.write_str("too many tables"),
            WriteError::TooLarge => f.write_str("the font would be 2 GiB or larger"),
            WriteError::OffsetOverflow => f.write_str("offset overflow"),
            WriteError::Repack(e) => write!(f, "repacking failed ({e:?})"),
            WriteError::Serialize(e) => write!(f, "serialization failed (errors {:#x})", e.0),
        }
    }
}
