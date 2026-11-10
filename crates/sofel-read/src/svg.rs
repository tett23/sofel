//! The `SVG ` table: the SVG document index (ADR 0023).
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/svg>

use core::cmp::Ordering;

use crate::be::{self, to_u32, to_usize};
use crate::search::hb_bsearch;
use crate::{Diagnostic, Kind, ReadError, Tag};

/// The `SVG ` tag.
pub const TAG: Tag = Tag::new(b"SVG ");

/// An SVG Document Index entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DocumentRecord {
    /// `startGlyphID`.
    pub start_glyph: u16,
    /// `endGlyphID`.
    pub end_glyph: u16,
    /// The document's offset from the start of the table.
    pub offset: usize,
    /// `svgDocLength`.
    pub length: u32,
}

/// A view over the `SVG ` table. Building it checks the header and that
/// the document index entries fit.
#[derive(Clone, Copy, Debug)]
pub struct Svg<'a> {
    data: &'a [u8],
    version: u16,
    list: usize,
    entries: &'a [[u8; 12]],
}

impl<'a> Svg<'a> {
    /// A view over `data`.
    ///
    /// Fails with [`ReadError::Truncated`] when the header does not fit
    /// and with [`ReadError::OutOfBounds`] when the document index does
    /// not.
    pub fn new(data: &'a [u8]) -> Result<Svg<'a>, ReadError> {
        let h = data
            .first_chunk::<10>()
            .ok_or(ReadError::truncated(10, data.len()))?;
        let list = to_usize(u32::from_be_bytes([h[2], h[3], h[4], h[5]]));
        let entries = if list == 0 {
            &[][..]
        } else {
            let n = be::u16_at(data, list).ok_or(ReadError::OutOfBounds)?;
            be::chunks_at::<12>(data, list.saturating_add(2), usize::from(n))
                .ok_or(ReadError::OutOfBounds)?
        };
        Ok(Svg {
            data,
            version: u16::from_be_bytes([h[0], h[1]]),
            list,
            entries,
        })
    }
    /// The table version.
    pub fn version(&self) -> u16 {
        self.version
    }
    /// The number of documents (`numEntries`).
    pub fn num_documents(&self) -> usize {
        self.entries.len()
    }
    /// Document index entry `i`.
    pub fn record(&self, i: usize) -> Option<DocumentRecord> {
        self.entries.get(i).map(|e| DocumentRecord {
            start_glyph: u16::from_be_bytes([e[0], e[1]]),
            end_glyph: u16::from_be_bytes([e[2], e[3]]),
            offset: self
                .list
                .saturating_add(to_usize(u32::from_be_bytes([e[4], e[5], e[6], e[7]]))),
            length: u32::from_be_bytes([e[8], e[9], e[10], e[11]]),
        })
    }
    /// The index of the document that covers `glyph`, by HarfBuzz's
    /// binary search.
    pub fn find(&self, glyph: u32) -> Option<usize> {
        hb_bsearch(self.entries.len(), |i| {
            self.record(i).map(|r| {
                if glyph < u32::from(r.start_glyph) {
                    Ordering::Less
                } else if glyph > u32::from(r.end_glyph) {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            })
        })
    }
    /// The bytes of document `i` as HarfBuzz cuts them from the table
    /// (`hb_blob_create_sub_blob`): cut at the end of the table; `None`
    /// when empty.
    pub fn document(&self, i: usize) -> Option<&'a [u8]> {
        let r = self.record(i)?;
        let rest = self.data.get(r.offset..)?;
        let doc = rest.get(..to_usize(r.length).min(rest.len()))?;
        (!doc.is_empty()).then_some(doc)
    }
}

/// Checks an `SVG ` table: truncation and an index out of bounds (and
/// nothing else then); entries whose range is reversed or that do not
/// ascend; and documents out of bounds or empty.
pub fn validate(data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    let mut emit = |offset: usize, kind: Kind| report(Diagnostic::new(TAG, to_u32(offset), kind));
    let svg = match Svg::new(data) {
        Ok(s) => s,
        Err(ReadError::Truncated { needed, available }) => {
            return emit(
                0,
                Kind::Truncated {
                    needed: to_u32(needed),
                    available: to_u32(available),
                },
            );
        }
        Err(_) => {
            return emit(
                2,
                Kind::OffsetOutOfBounds {
                    offset: be::u32_at(data, 2).unwrap_or(0),
                    length: 2,
                    available: to_u32(data.len()),
                },
            );
        }
    };
    let mut prev_end: Option<u16> = None;
    for i in 0..svg.num_documents() {
        let Some(r) = svg.record(i) else { continue };
        let at = svg
            .list
            .saturating_add(2)
            .saturating_add(i.saturating_mul(12));
        if r.start_glyph > r.end_glyph {
            emit(
                at,
                Kind::StartAfterEnd {
                    index: to_u32(i),
                    start: u32::from(r.start_glyph),
                    end: u32::from(r.end_glyph),
                },
            );
        }
        if let Some(p) = prev_end
            && r.start_glyph <= p
        {
            emit(at, Kind::RecordsNotSorted { index: to_u32(i) });
        }
        prev_end = Some(r.end_glyph);
        if r.length == 0 || be::slice_at(data, r.offset, to_usize(r.length)).is_none() {
            emit(
                at,
                Kind::OffsetOutOfBounds {
                    offset: to_u32(r.offset),
                    length: r.length,
                    available: to_u32(data.len()),
                },
            );
        }
    }
}
