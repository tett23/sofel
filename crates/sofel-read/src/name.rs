//! The `name` (naming) table, formats 0 and 1 (ADR 0016).
//!
//! A record whose string lies outside the table reads as `None`; what to
//! do with it (FreeType skips the record, HarfBuzz drops the whole table)
//! is decided in `sofel-font`.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/name>

use crate::be::{self, to_u32, to_usize};
use crate::{Diagnostic, Kind, ReadError, Tag};

/// The `name` tag.
pub const TAG: Tag = Tag::new(b"name");

const HEADER: usize = 6;
const RECORD: usize = 12;

/// One name record.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NameRecord {
    /// `platformID`.
    pub platform_id: u16,
    /// `encodingID`.
    pub encoding_id: u16,
    /// `languageID`. From 0x8000, an index into the language tag
    /// records (format 1) plus 0x8000.
    pub language_id: u16,
    /// `nameID`.
    pub name_id: u16,
    /// `length` of the string, in bytes.
    pub length: u16,
    /// `stringOffset`, from the start of the storage area.
    pub offset: u16,
}

/// One language tag record of format 1.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LangTagRecord {
    /// `length` of the tag string, in bytes.
    pub length: u16,
    /// `langTagOffset`, from the start of the storage area.
    pub offset: u16,
}

/// A view over the `name` table.
#[derive(Clone, Copy, Debug)]
pub struct Name<'a> {
    data: &'a [u8],
    format: u16,
    storage_offset: u16,
    records: &'a [[u8; RECORD]],
    lang_tags: &'a [[u8; 4]],
}

fn record_of(r: &[u8; RECORD]) -> NameRecord {
    let f = |i: usize| be::u16_at(r, i).unwrap_or(0);
    NameRecord {
        platform_id: f(0),
        encoding_id: f(2),
        language_id: f(4),
        name_id: f(6),
        length: f(8),
        offset: f(10),
    }
}

/// The bytes of a structure's header and arrays, for truncation reports.
fn needed(data: &[u8]) -> usize {
    let count = be::u16_at(data, 2).map_or(0, usize::from);
    let base = HEADER.saturating_add(count.saturating_mul(RECORD));
    if be::u16_at(data, 0) == Some(1) {
        let tags = be::u16_at(data, base).map_or(0, usize::from);
        base.saturating_add(2)
            .saturating_add(tags.saturating_mul(4))
    } else {
        base
    }
}

impl<'a> Name<'a> {
    /// A view over the header, the name records and, for format 1, the
    /// language tag records.
    ///
    /// Fails with [`ReadError::Truncated`] if they do not fit, and with
    /// [`ReadError::UnknownFormat`] if the format is neither 0 nor 1.
    pub fn new(data: &'a [u8]) -> Result<Name<'a>, ReadError> {
        let h = data
            .first_chunk::<HEADER>()
            .ok_or(ReadError::truncated(HEADER, data.len()))?;
        let format = u16::from_be_bytes([h[0], h[1]]);
        if format > 1 {
            return Err(ReadError::UnknownFormat(format));
        }
        let count = usize::from(u16::from_be_bytes([h[2], h[3]]));
        let storage_offset = u16::from_be_bytes([h[4], h[5]]);
        let truncated = ReadError::truncated(needed(data), data.len());
        let records = be::chunks_at::<RECORD>(data, HEADER, count).ok_or(truncated)?;
        let end = HEADER.saturating_add(count.saturating_mul(RECORD));
        let lang_tags = if format == 1 {
            let n = be::u16_at(data, end).ok_or(truncated)?;
            be::chunks_at::<4>(data, end.saturating_add(2), usize::from(n)).ok_or(truncated)?
        } else {
            &[]
        };
        Ok(Name {
            data,
            format,
            storage_offset,
            records,
            lang_tags,
        })
    }

    /// `format`: 0 or 1.
    pub fn format(&self) -> u16 {
        self.format
    }

    /// `count`: the number of name records.
    pub fn count(&self) -> u16 {
        u16::try_from(self.records.len()).unwrap_or(u16::MAX)
    }

    /// `storageOffset`: where the strings start, from the start of the table.
    pub fn storage_offset(&self) -> u16 {
        self.storage_offset
    }

    /// Name record `index`.
    pub fn record(&self, index: usize) -> Option<NameRecord> {
        self.records.get(index).map(record_of)
    }

    /// The name records in table order.
    pub fn records(&self) -> impl Iterator<Item = NameRecord> + 'a {
        self.records.iter().map(record_of)
    }

    /// The bytes of a record's string. `None` if they are not all in the table.
    pub fn string(&self, record: &NameRecord) -> Option<&'a [u8]> {
        self.storage(record.offset, record.length)
    }

    /// `langTagCount`: the number of language tag records (0 in format 0).
    pub fn lang_tag_count(&self) -> u16 {
        u16::try_from(self.lang_tags.len()).unwrap_or(u16::MAX)
    }

    /// Language tag record `index` (format 1).
    pub fn lang_tag(&self, index: usize) -> Option<LangTagRecord> {
        self.lang_tags.get(index).map(|t| LangTagRecord {
            length: u16::from_be_bytes([t[0], t[1]]),
            offset: u16::from_be_bytes([t[2], t[3]]),
        })
    }

    /// The bytes of a language tag (UTF-16BE). `None` if they are not
    /// all in the table.
    pub fn lang_tag_string(&self, tag: &LangTagRecord) -> Option<&'a [u8]> {
        self.storage(tag.offset, tag.length)
    }

    fn storage(&self, offset: u16, length: u16) -> Option<&'a [u8]> {
        let start = usize::from(self.storage_offset).checked_add(usize::from(offset))?;
        be::slice_at(self.data, start, usize::from(length))
    }
}

/// Checks a `name` table: an unknown format and truncated records (and
/// nothing else then), strings and language tags outside the table,
/// records not in the order of platform, encoding, language and name id,
/// and UTF-16BE strings (platforms 0 and 3) of odd length. Offsets are
/// those of the records.
pub fn validate(data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    let mut emit = |offset: usize, kind: Kind| report(Diagnostic::new(TAG, to_u32(offset), kind));
    let name = match Name::new(data) {
        Ok(n) => n,
        Err(ReadError::UnknownFormat(f)) => {
            return emit(
                0,
                Kind::UnknownFormat {
                    found: u32::from(f),
                },
            );
        }
        Err(_) => {
            return emit(
                0,
                Kind::Truncated {
                    needed: to_u32(needed(data)),
                    available: to_u32(data.len()),
                },
            );
        }
    };
    let out_of_bounds = |offset: u16, length: u16| Kind::OffsetOutOfBounds {
        offset: u32::from(name.storage_offset()).saturating_add(u32::from(offset)),
        length: u32::from(length),
        available: to_u32(data.len()),
    };
    let mut previous: Option<NameRecord> = None;
    for (i, r) in name.records().enumerate() {
        let at = HEADER.saturating_add(i.saturating_mul(RECORD));
        let key = |r: &NameRecord| (r.platform_id, r.encoding_id, r.language_id, r.name_id);
        if previous.is_some_and(|p| key(&r) < key(&p)) {
            emit(at, Kind::RecordsNotSorted { index: to_u32(i) });
        }
        previous = Some(r);
        if name.string(&r).is_none() {
            emit(at, out_of_bounds(r.offset, r.length));
        }
        if matches!(r.platform_id, 0 | 3) && r.length % 2 != 0 {
            emit(
                at,
                Kind::OddLength {
                    index: to_u32(i),
                    length: u32::from(r.length),
                },
            );
        }
    }
    let tags_at = HEADER
        .saturating_add(to_usize(u32::from(name.count())).saturating_mul(RECORD))
        .saturating_add(2);
    for i in 0..usize::from(name.lang_tag_count()) {
        if let Some(t) = name.lang_tag(i)
            && name.lang_tag_string(&t).is_none()
        {
            emit(
                tags_at.saturating_add(i.saturating_mul(4)),
                out_of_bounds(t.offset, t.length),
            );
        }
    }
}
