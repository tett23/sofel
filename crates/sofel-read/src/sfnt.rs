//! The sfnt table directory and TTC/OTC collections.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/otff>

use crate::be::{self, to_usize};
use crate::{ReadError, Tag};

/// sfntVersion values: TrueType, CFF (`OTTO`), Apple TrueType (`true`)
/// and Type 1 wrapped in sfnt (`typ1`).
pub const SFNT_VERSIONS: [u32; 4] = [0x0001_0000, 0x4F54_544F, 0x7472_7565, 0x7479_7031];

const DIRECTORY_HEADER: usize = 12;
const RECORD: usize = 16;

/// One font's table directory. Table offsets are from the start of the
/// file, also for fonts in a collection.
#[derive(Clone, Copy, Debug)]
pub struct Sfnt<'a> {
    file: &'a [u8],
    start: usize,
    header: &'a [u8; DIRECTORY_HEADER],
    records: &'a [[u8; RECORD]],
}

/// One entry of the table directory.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TableRecord {
    /// `tableTag`.
    pub tag: Tag,
    /// `checksum` of the table, as stored.
    pub checksum: u32,
    /// `offset`: from the start of the file, in bytes.
    pub offset: u32,
    /// `length` of the table, in bytes.
    pub length: u32,
}

impl TableRecord {
    fn parse(r: &[u8; RECORD]) -> TableRecord {
        TableRecord {
            tag: Tag([r[0], r[1], r[2], r[3]]),
            checksum: u32::from_be_bytes([r[4], r[5], r[6], r[7]]),
            offset: u32::from_be_bytes([r[8], r[9], r[10], r[11]]),
            length: u32::from_be_bytes([r[12], r[13], r[14], r[15]]),
        }
    }

    /// The table's bytes in `file`, or `None` if they are outside it.
    pub fn data<'a>(&self, file: &'a [u8]) -> Option<&'a [u8]> {
        be::slice_at(file, to_usize(self.offset), to_usize(self.length))
    }
}

impl<'a> Sfnt<'a> {
    /// A single font at the start of `file`.
    pub fn new(file: &'a [u8]) -> Result<Sfnt<'a>, ReadError> {
        Sfnt::at(file, 0)
    }

    /// The font whose directory starts at `start` in `file`.
    pub fn at(file: &'a [u8], start: usize) -> Result<Sfnt<'a>, ReadError> {
        let rest = be::tail_at(file, start).ok_or(ReadError::OutOfBounds)?;
        let header = rest
            .first_chunk::<DIRECTORY_HEADER>()
            .ok_or(ReadError::truncated(DIRECTORY_HEADER, rest.len()))?;
        let version = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        if !SFNT_VERSIONS.contains(&version) {
            return Err(ReadError::UnknownVersion(version));
        }
        let num_tables = usize::from(u16::from_be_bytes([header[4], header[5]]));
        let records = be::chunks_at::<RECORD>(rest, DIRECTORY_HEADER, num_tables).ok_or(
            ReadError::truncated(
                num_tables
                    .saturating_mul(RECORD)
                    .saturating_add(DIRECTORY_HEADER),
                rest.len(),
            ),
        )?;
        Ok(Sfnt {
            file,
            start,
            header,
            records,
        })
    }

    /// `sfntVersion`: one of [`SFNT_VERSIONS`].
    pub fn version(&self) -> u32 {
        let h = self.header;
        u32::from_be_bytes([h[0], h[1], h[2], h[3]])
    }

    /// `numTables`: the number of table records.
    pub fn num_tables(&self) -> u16 {
        u16::from_be_bytes([self.header[4], self.header[5]])
    }

    /// `searchRange` from the directory header, as stored.
    pub fn search_range(&self) -> u16 {
        u16::from_be_bytes([self.header[6], self.header[7]])
    }

    /// `entrySelector` from the directory header, as stored.
    pub fn entry_selector(&self) -> u16 {
        u16::from_be_bytes([self.header[8], self.header[9]])
    }

    /// `rangeShift` from the directory header, as stored.
    pub fn range_shift(&self) -> u16 {
        u16::from_be_bytes([self.header[10], self.header[11]])
    }

    /// The whole file this font is in.
    pub fn file(&self) -> &'a [u8] {
        self.file
    }

    /// Where this font's directory starts in the file.
    pub fn start(&self) -> usize {
        self.start
    }

    /// The table records in file order.
    pub fn records(&self) -> impl Iterator<Item = TableRecord> + 'a {
        self.records.iter().map(TableRecord::parse)
    }

    /// The first record with `tag`. Records are searched linearly, since
    /// some fonts do not sort them.
    pub fn record(&self, tag: Tag) -> Option<TableRecord> {
        self.records().find(|r| r.tag == tag)
    }

    /// The bytes of the table with `tag`, or `None` if it is missing or
    /// outside the file.
    pub fn table(&self, tag: Tag) -> Option<&'a [u8]> {
        self.record(tag)?.data(self.file)
    }
}

/// A TrueType or OpenType collection (`ttcf`), version 1 or 2.
#[derive(Clone, Copy, Debug)]
pub struct Collection<'a> {
    file: &'a [u8],
    major: u16,
    minor: u16,
    offsets: &'a [[u8; 4]],
}

const TTCF: u32 = 0x7474_6366;

impl<'a> Collection<'a> {
    /// The collection header at the start of `file`.
    ///
    /// Fails with [`ReadError::UnknownVersion`] if the tag is not `ttcf`
    /// or the major version is not 1 or 2, and with
    /// [`ReadError::Truncated`] if the header or offsets do not fit.
    pub fn new(file: &'a [u8]) -> Result<Collection<'a>, ReadError> {
        let header = file
            .first_chunk::<12>()
            .ok_or(ReadError::truncated(12, file.len()))?;
        let tag = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        if tag != TTCF {
            return Err(ReadError::UnknownVersion(tag));
        }
        let major = u16::from_be_bytes([header[4], header[5]]);
        let minor = u16::from_be_bytes([header[6], header[7]]);
        if major != 1 && major != 2 {
            return Err(ReadError::UnknownVersion(u32::from_be_bytes([
                header[4], header[5], header[6], header[7],
            ])));
        }
        let num_fonts = to_usize(u32::from_be_bytes([
            header[8], header[9], header[10], header[11],
        ]));
        let offsets = be::chunks_at::<4>(file, 12, num_fonts).ok_or(ReadError::truncated(
            num_fonts.saturating_mul(4).saturating_add(12),
            file.len(),
        ))?;
        Ok(Collection {
            file,
            major,
            minor,
            offsets,
        })
    }

    /// `majorVersion` of the collection header (1 or 2).
    pub fn major_version(&self) -> u16 {
        self.major
    }

    /// `minorVersion` of the collection header.
    pub fn minor_version(&self) -> u16 {
        self.minor
    }

    /// `numFonts`: the number of fonts in the collection.
    pub fn num_fonts(&self) -> u32 {
        be::to_u32(self.offsets.len())
    }

    /// The offset of font `index`'s directory.
    pub fn offset(&self, index: u32) -> Option<u32> {
        self.offsets
            .get(to_usize(index))
            .map(|o| u32::from_be_bytes(*o))
    }

    /// Font `index`'s table directory.
    ///
    /// Fails with [`ReadError::OutOfBounds`] if `index` is not below
    /// [`num_fonts`](Self::num_fonts), or as [`Sfnt::at`] does.
    pub fn font(&self, index: u32) -> Result<Sfnt<'a>, ReadError> {
        let offset = self.offset(index).ok_or(ReadError::OutOfBounds)?;
        Sfnt::at(self.file, to_usize(offset))
    }

    /// The whole collection file.
    pub fn file(&self) -> &'a [u8] {
        self.file
    }
}

/// A font file: a single sfnt or a collection.
#[derive(Clone, Copy, Debug)]
pub enum FontFile<'a> {
    /// A single font.
    Sfnt(Sfnt<'a>),
    /// A `ttcf` collection.
    Collection(Collection<'a>),
}

impl<'a> FontFile<'a> {
    /// A collection if `file` starts with `ttcf`, otherwise a single
    /// font at its start.
    pub fn new(file: &'a [u8]) -> Result<FontFile<'a>, ReadError> {
        if be::u32_at(file, 0) == Some(TTCF) {
            Collection::new(file).map(FontFile::Collection)
        } else {
            Sfnt::new(file).map(FontFile::Sfnt)
        }
    }
}
