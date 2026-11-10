//! The `CFF ` and `CFF2` tables: the Compact Font Format, versions 1 and
//! 2 (ADR 0021).
//!
//! Views over the header, the INDEXes, the DICTs, the charset, the
//! encoding and the FDSelect, the lexers of DICT data and Type 2
//! charstrings, FreeType's Type 2 charstring decoder, and the validator.
//! Numbers in DICTs are converted as FreeType 2.15.0 converts them
//! (`src/cff/cffparse.c`) on 64-bit targets, so that the parsed DICTs are
//! FreeType's.
//!
//! The charstring decoder is here and not in `sofel-outline` because the
//! validator runs it over every glyph (ADR 0021); `sofel-outline` turns
//! its output into points.
//!
//! Specifications: Adobe Technical Note #5176 (CFF), #5177 (Type 2
//! charstrings), <https://learn.microsoft.com/typography/opentype/spec/cff2>

pub mod blend;
pub mod charset;
pub mod charstring;
mod check;
mod data;
pub mod dict;
pub mod encoding;
pub mod fdselect;
pub mod ft;
mod index;

pub use blend::FtBlend;
pub use charset::Charset;
pub use check::{validate, validate2};
pub use data::{
    EXPERT_CHARSET, EXPERT_ENCODING, EXPERT_SUBSET_CHARSET, ISO_ADOBE_CHARSET, STANDARD_ENCODING,
    STANDARD_STRINGS,
};
pub use encoding::Encoding;
pub use fdselect::FdSelect;
pub use index::Index;

use crate::be::{self, to_usize};
use crate::{ReadError, Tag};

/// The `CFF ` tag.
pub const TAG: Tag = Tag::new(b"CFF ");
/// The `CFF2` tag.
pub const TAG2: Tag = Tag::new(b"CFF2");

/// The number of standard strings: SIDs below this are standard strings.
pub const NUM_STANDARD_STRINGS: u32 = 391;

/// CFF (version 1) or CFF2. INDEX counts, DICT operators and charstring
/// operators depend on it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Version {
    /// The `CFF ` table: 16-bit INDEX counts.
    Cff,
    /// The `CFF2` table: 32-bit INDEX counts.
    Cff2,
}

impl Version {
    pub(crate) const fn count_len(self) -> usize {
        match self {
            Version::Cff => 2,
            Version::Cff2 => 4,
        }
    }
}

/// The header of a `CFF ` or `CFF2` table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Header {
    /// `major`: 1 for CFF, 2 for CFF2.
    pub major: u8,
    /// `minor`.
    pub minor: u8,
    /// `hdrSize`: where the data after the header starts.
    pub header_size: u8,
    /// `offSize` (CFF): the absolute offset size. 0 in CFF2.
    pub off_size: u8,
    /// `topDictLength` (CFF2): the length of the Top DICT. 0 in CFF.
    pub top_dict_length: u16,
}

impl Header {
    /// The header at the start of `data`: four bytes, five when `major`
    /// is 2. The values are not checked.
    pub fn new(data: &[u8]) -> Result<Header, ReadError> {
        let h = data
            .first_chunk::<4>()
            .ok_or(ReadError::truncated(4, data.len()))?;
        let [major, minor, header_size, off_size] = *h;
        if major == 2 {
            let len = be::u16_at(data, 3).ok_or(ReadError::truncated(5, data.len()))?;
            Ok(Header {
                major,
                minor,
                header_size,
                off_size: 0,
                top_dict_length: len,
            })
        } else {
            Ok(Header {
                major,
                minor,
                header_size,
                off_size,
                top_dict_length: 0,
            })
        }
    }
}

/// A view over a `CFF ` table: the header and the four INDEXes that
/// follow it, read one after the other.
#[derive(Clone, Copy, Debug)]
pub struct Cff1<'a> {
    data: &'a [u8],
    header: Header,
    names: Index<'a>,
    top_dicts: Index<'a>,
    strings: Index<'a>,
    global_subrs: Index<'a>,
    /// Where the Name INDEX starts.
    names_at: usize,
}

fn index_at(data: &[u8], at: usize, version: Version) -> Result<Index<'_>, ReadError> {
    let tail = be::tail_at(data, at).ok_or(ReadError::OutOfBounds)?;
    Index::new(tail, version)
}

fn after(index: &Index<'_>, at: usize) -> Result<usize, ReadError> {
    index
        .end()
        .map(|e| e.saturating_add(at))
        .ok_or(ReadError::InvalidValue)
}

impl<'a> Cff1<'a> {
    /// A view over the table.
    ///
    /// Fails with [`ReadError::UnknownVersion`] when `major` is not 1,
    /// with [`ReadError::InvalidValue`] when `hdrSize` is below 4, and
    /// with the INDEX errors of [`Index::new`] (an INDEX whose last
    /// offset is 0 is [`ReadError::InvalidValue`]).
    pub fn new(data: &'a [u8]) -> Result<Cff1<'a>, ReadError> {
        let header = Header::new(data)?;
        if header.major != 1 {
            return Err(ReadError::UnknownVersion(u32::from(header.major)));
        }
        if header.header_size < 4 {
            return Err(ReadError::InvalidValue);
        }
        let names_at = usize::from(header.header_size);
        let names = index_at(data, names_at, Version::Cff)?;
        let top_at = after(&names, names_at)?;
        let top_dicts = index_at(data, top_at, Version::Cff)?;
        let strings_at = after(&top_dicts, top_at)?;
        let strings = index_at(data, strings_at, Version::Cff)?;
        let gsubrs_at = after(&strings, strings_at)?;
        let global_subrs = index_at(data, gsubrs_at, Version::Cff)?;
        Ok(Cff1 {
            data,
            header,
            names,
            top_dicts,
            strings,
            global_subrs,
            names_at,
        })
    }

    /// The table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// The header.
    pub fn header(&self) -> Header {
        self.header
    }

    /// The Name INDEX.
    pub fn names(&self) -> Index<'a> {
        self.names
    }

    /// Where the Name INDEX starts in the table.
    pub fn names_offset(&self) -> usize {
        self.names_at
    }

    /// The Top DICT INDEX.
    pub fn top_dicts(&self) -> Index<'a> {
        self.top_dicts
    }

    /// The String INDEX.
    pub fn strings(&self) -> Index<'a> {
        self.strings
    }

    /// The Global Subrs INDEX.
    pub fn global_subrs(&self) -> Index<'a> {
        self.global_subrs
    }

    /// The string of `sid`: a standard string below 391, else element
    /// `sid - 391` of the String INDEX ([`Index::get`]).
    pub fn string(&self, sid: u32) -> Option<&'a [u8]> {
        string(&self.strings, sid)
    }
}

/// The string of `sid` with `strings` as the String INDEX: a standard
/// string below 391, else element `sid - 391` ([`Index::get`]).
pub fn string<'a>(strings: &Index<'a>, sid: u32) -> Option<&'a [u8]> {
    match sid.checked_sub(NUM_STANDARD_STRINGS) {
        None => STANDARD_STRINGS
            .get(to_usize(sid))
            .map(|s: &&'static str| s.as_bytes()),
        Some(k) => strings.get(k),
    }
}

/// A view over a `CFF2` table: the header, the Top DICT and the Global
/// Subrs INDEX.
#[derive(Clone, Copy, Debug)]
pub struct Cff2<'a> {
    data: &'a [u8],
    header: Header,
    top_dict: &'a [u8],
    global_subrs: Index<'a>,
}

impl<'a> Cff2<'a> {
    /// A view over the table.
    ///
    /// Fails with [`ReadError::UnknownVersion`] when `major` is not 2,
    /// with [`ReadError::InvalidValue`] when `hdrSize` is below 5, with
    /// [`ReadError::Truncated`] when the Top DICT does not fit, and with
    /// the errors of [`Index::new`] for the Global Subrs INDEX.
    pub fn new(data: &'a [u8]) -> Result<Cff2<'a>, ReadError> {
        let header = Header::new(data)?;
        if header.major != 2 {
            return Err(ReadError::UnknownVersion(u32::from(header.major)));
        }
        if header.header_size < 5 {
            return Err(ReadError::InvalidValue);
        }
        let at = usize::from(header.header_size);
        let len = usize::from(header.top_dict_length);
        let top_dict = be::slice_at(data, at, len)
            .ok_or(ReadError::truncated(at.saturating_add(len), data.len()))?;
        let global_subrs = index_at(data, at.saturating_add(len), Version::Cff2)?;
        Ok(Cff2 {
            data,
            header,
            top_dict,
            global_subrs,
        })
    }

    /// The table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// The header.
    pub fn header(&self) -> Header {
        self.header
    }

    /// The Top DICT data.
    pub fn top_dict(&self) -> &'a [u8] {
        self.top_dict
    }

    /// The Global Subrs INDEX.
    pub fn global_subrs(&self) -> Index<'a> {
        self.global_subrs
    }
}

/// The subroutine bias for a subroutine INDEX of `count` objects: 107
/// below 1240, 1131 below 33900, else 32768.
pub fn subr_bias(count: u32) -> i32 {
    if count < 1240 {
        107
    } else if count < 33900 {
        1131
    } else {
        32768
    }
}
