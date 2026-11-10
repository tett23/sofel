//! The bitmap location tables `CBLC`, `EBLC` and Apple's `bloc`, which
//! share one layout (ADR 0023): BitmapSize records, IndexSubTableArrays
//! and IndexSubTables of formats 1 to 5.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/eblc>,
//! <https://learn.microsoft.com/typography/opentype/spec/cblc>

use crate::be::{self, to_u32, to_usize};
use crate::{Diagnostic, Kind, ReadError, Tag};

/// The `CBLC` tag.
pub const TAG_CBLC: Tag = Tag::new(b"CBLC");
/// The `EBLC` tag.
pub const TAG_EBLC: Tag = Tag::new(b"EBLC");
/// Apple's `bloc` tag.
pub const TAG_BLOC: Tag = Tag::new(b"bloc");

/// `SbitLineMetrics`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct LineMetrics {
    /// `ascender`.
    pub ascender: i8,
    /// `descender`.
    pub descender: i8,
    /// `widthMax`.
    pub width_max: u8,
    /// `caretSlopeNumerator`.
    pub caret_slope_numerator: i8,
    /// `caretSlopeDenominator`.
    pub caret_slope_denominator: i8,
    /// `caretOffset`.
    pub caret_offset: i8,
    /// `minOriginSB`.
    pub min_origin_sb: i8,
    /// `minAdvanceSB`.
    pub min_advance_sb: i8,
    /// `maxBeforeBL`.
    pub max_before_bl: i8,
    /// `minAfterBL`.
    pub min_after_bl: i8,
}

impl LineMetrics {
    fn read(b: &[u8]) -> LineMetrics {
        let i = |k: usize| i8::from_be_bytes([b.get(k).copied().unwrap_or(0)]);
        LineMetrics {
            ascender: i(0),
            descender: i(1),
            width_max: b.get(2).copied().unwrap_or(0),
            caret_slope_numerator: i(3),
            caret_slope_denominator: i(4),
            caret_offset: i(5),
            min_origin_sb: i(6),
            min_advance_sb: i(7),
            max_before_bl: i(8),
            min_after_bl: i(9),
        }
    }
}

/// A `BitmapSize` record: one strike.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BitmapSize {
    /// `indexSubTableArrayOffset`, from the start of the table.
    pub index_subtable_array_offset: u32,
    /// `indexTablesSize`.
    pub index_tables_size: u32,
    /// `numberofIndexSubTables`.
    pub number_of_index_subtables: u32,
    /// `colorRef`.
    pub color_ref: u32,
    /// Horizontal line metrics.
    pub hori: LineMetrics,
    /// Vertical line metrics.
    pub vert: LineMetrics,
    /// `startGlyphIndex`.
    pub start_glyph: u16,
    /// `endGlyphIndex`.
    pub end_glyph: u16,
    /// `ppemX`.
    pub ppem_x: u8,
    /// `ppemY`.
    pub ppem_y: u8,
    /// `bitDepth`: 1, 2, 4, 8 or 32.
    pub bit_depth: u8,
    /// `flags`.
    pub flags: i8,
}

/// An IndexSubTableArray record.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct IndexRecord {
    /// `firstGlyphIndex`.
    pub first_glyph: u16,
    /// `lastGlyphIndex`.
    pub last_glyph: u16,
    /// The IndexSubTable's offset from the start of the table.
    pub offset: usize,
}

/// Where a glyph's image is in the bitmap data table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ImageLocation {
    /// `imageFormat` of the IndexSubTable.
    pub image_format: u16,
    /// The offset of the image in the data table.
    pub offset: u32,
    /// The length of the image.
    pub length: u32,
    /// The metrics of index formats 2 and 5 (eight bytes of
    /// `BigGlyphMetrics`).
    pub metrics: Option<[u8; 8]>,
}

/// A view over a `CBLC`, `EBLC` or `bloc` table. Building it checks the
/// header and that the BitmapSize records fit.
#[derive(Clone, Copy, Debug)]
pub struct Cblc<'a> {
    data: &'a [u8],
    major: u16,
    minor: u16,
    sizes: &'a [[u8; 48]],
}

impl<'a> Cblc<'a> {
    /// A view over `data`.
    ///
    /// Fails with [`ReadError::Truncated`] when the header or the
    /// BitmapSize records do not fit.
    pub fn new(data: &'a [u8]) -> Result<Cblc<'a>, ReadError> {
        let h = data
            .first_chunk::<8>()
            .ok_or(ReadError::truncated(8, data.len()))?;
        let n = to_usize(u32::from_be_bytes([h[4], h[5], h[6], h[7]]));
        let sizes = be::chunks_at::<48>(data, 8, n).ok_or(ReadError::truncated(
            8usize.saturating_add(n.saturating_mul(48)),
            data.len(),
        ))?;
        Ok(Cblc {
            data,
            major: u16::from_be_bytes([h[0], h[1]]),
            minor: u16::from_be_bytes([h[2], h[3]]),
            sizes,
        })
    }

    /// A view over `data` with as many BitmapSize records as fit, at
    /// most `numSizes` (FreeType's `tt_face_load_sbit`). Fails with
    /// [`ReadError::Truncated`] when the 8-byte header does not fit.
    pub fn new_clamped(data: &'a [u8]) -> Result<Cblc<'a>, ReadError> {
        let h = data
            .first_chunk::<8>()
            .ok_or(ReadError::truncated(8, data.len()))?;
        let n = to_usize(u32::from_be_bytes([h[4], h[5], h[6], h[7]]))
            .min(data.len().saturating_sub(8) / 48);
        let sizes = be::chunks_at::<48>(data, 8, n).ok_or(ReadError::truncated(8, data.len()))?;
        Ok(Cblc {
            data,
            major: u16::from_be_bytes([h[0], h[1]]),
            minor: u16::from_be_bytes([h[2], h[3]]),
            sizes,
        })
    }

    /// The table's bytes.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `majorVersion`.
    pub fn major_version(&self) -> u16 {
        self.major
    }
    /// `minorVersion`.
    pub fn minor_version(&self) -> u16 {
        self.minor
    }
    /// `numSizes`.
    pub fn num_sizes(&self) -> usize {
        self.sizes.len()
    }
    /// BitmapSize record `i`.
    pub fn size(&self, i: usize) -> Option<BitmapSize> {
        let r = self.sizes.get(i)?;
        let u32_ = |k: usize| be::u32_at(r, k).unwrap_or(0);
        let u16_ = |k: usize| be::u16_at(r, k).unwrap_or(0);
        let byte = |k: usize| r.get(k).copied().unwrap_or(0);
        Some(BitmapSize {
            index_subtable_array_offset: u32_(0),
            index_tables_size: u32_(4),
            number_of_index_subtables: u32_(8),
            color_ref: u32_(12),
            hori: LineMetrics::read(r.get(16..28).unwrap_or_default()),
            vert: LineMetrics::read(r.get(28..40).unwrap_or_default()),
            start_glyph: u16_(40),
            end_glyph: u16_(42),
            ppem_x: byte(44),
            ppem_y: byte(45),
            bit_depth: byte(46),
            flags: i8::from_be_bytes([byte(47)]),
        })
    }
    /// Record `j` of the IndexSubTableArray of `size`.
    pub fn index_record(&self, size: &BitmapSize, j: u32) -> Option<IndexRecord> {
        if j >= size.number_of_index_subtables {
            return None;
        }
        let array = to_usize(size.index_subtable_array_offset);
        let at = array.checked_add(to_usize(j).checked_mul(8)?)?;
        let r = be::array_at::<8>(self.data, at)?;
        Some(IndexRecord {
            first_glyph: u16::from_be_bytes([r[0], r[1]]),
            last_glyph: u16::from_be_bytes([r[2], r[3]]),
            offset: array.saturating_add(to_usize(u32::from_be_bytes([r[4], r[5], r[6], r[7]]))),
        })
    }
    /// The IndexSubTable header at `record`: `indexFormat`,
    /// `imageFormat` and `imageDataOffset`.
    pub fn index_header(&self, record: &IndexRecord) -> Option<(u16, u16, u32)> {
        let h = be::array_at::<8>(self.data, record.offset)?;
        Some((
            u16::from_be_bytes([h[0], h[1]]),
            u16::from_be_bytes([h[2], h[3]]),
            u32::from_be_bytes([h[4], h[5], h[6], h[7]]),
        ))
    }

    /// Where `glyph`'s image is according to `record`, as the
    /// specification reads the index formats 1 to 5; `None` when the
    /// glyph is not in the record, the subtable cannot be read or the
    /// image is empty.
    pub fn locate(&self, record: &IndexRecord, glyph: u32) -> Option<ImageLocation> {
        let first = u32::from(record.first_glyph);
        if glyph < first || glyph > u32::from(record.last_glyph) {
            return None;
        }
        let (index_format, image_format, base) = self.index_header(record)?;
        let at = record.offset.saturating_add(8);
        let k = to_usize(glyph.saturating_sub(first));
        let d = self.data;
        let (start, end, metrics) = match index_format {
            1 => {
                let p = at.checked_add(k.checked_mul(4)?)?;
                (be::u32_at(d, p)?, be::u32_at(d, p.saturating_add(4))?, None)
            }
            3 => {
                let p = at.checked_add(k.checked_mul(2)?)?;
                (
                    u32::from(be::u16_at(d, p)?),
                    u32::from(be::u16_at(d, p.saturating_add(2))?),
                    None,
                )
            }
            2 => {
                let size = be::u32_at(d, at)?;
                let start = size.checked_mul(u32::try_from(k).ok()?)?;
                (
                    start,
                    start.checked_add(size)?,
                    Some(*be::array_at::<8>(d, at.saturating_add(4))?),
                )
            }
            4 => {
                let n = to_usize(be::u32_at(d, at)?);
                let pairs = be::chunks_at::<4>(d, at.saturating_add(4), n.checked_add(1)?)?;
                let i = pairs
                    .iter()
                    .take(n)
                    .position(|p| u32::from(u16::from_be_bytes([p[0], p[1]])) == glyph)?;
                let off = |p: &[u8; 4]| u32::from(u16::from_be_bytes([p[2], p[3]]));
                (
                    off(pairs.get(i)?),
                    off(pairs.get(i.saturating_add(1))?),
                    None,
                )
            }
            5 => {
                let size = be::u32_at(d, at)?;
                let metrics = *be::array_at::<8>(d, at.saturating_add(4))?;
                let n = to_usize(be::u32_at(d, at.saturating_add(12))?);
                let ids = be::chunks_at::<2>(d, at.saturating_add(16), n)?;
                let i = ids
                    .iter()
                    .position(|g| u32::from(u16::from_be_bytes(*g)) == glyph)?;
                let start = size.checked_mul(u32::try_from(i).ok()?)?;
                (start, start.checked_add(size)?, Some(metrics))
            }
            _ => return None,
        };
        if end <= start {
            return None;
        }
        Some(ImageLocation {
            image_format,
            offset: base.checked_add(start)?,
            length: end.saturating_sub(start),
            metrics,
        })
    }
}

/// Checks a `CBLC`, `EBLC` or `bloc` table (`tag`): truncation (and
/// nothing else then); a major version other than 2 or 3; IndexSubTableArrays
/// and IndexSubTables out of bounds; glyph ranges reversed or not
/// ascending; unknown index formats; and offsets of formats 1 and 3
/// that do not ascend.
pub fn validate(tag: Tag, data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    let mut emit = |offset: usize, kind: Kind| report(Diagnostic::new(tag, to_u32(offset), kind));
    let cblc = match Cblc::new(data) {
        Ok(c) => c,
        Err(_) => {
            let n = be::u32_at(data, 4).map_or(0, to_usize);
            return emit(
                0,
                Kind::Truncated {
                    needed: to_u32(8usize.saturating_add(n.saturating_mul(48))),
                    available: to_u32(data.len()),
                },
            );
        }
    };
    if !matches!(cblc.major_version(), 2 | 3) {
        emit(
            0,
            Kind::UnknownVersion {
                found: be::u32_at(data, 0).unwrap_or(0),
            },
        );
    }
    for i in 0..cblc.num_sizes() {
        let Some(size) = cblc.size(i) else { continue };
        let at = 8usize.saturating_add(i.saturating_mul(48));
        let array = to_usize(size.index_subtable_array_offset);
        let len = to_usize(size.number_of_index_subtables).saturating_mul(8);
        if be::slice_at(data, array, len).is_none() {
            emit(
                at,
                Kind::OffsetOutOfBounds {
                    offset: size.index_subtable_array_offset,
                    length: to_u32(len),
                    available: to_u32(data.len()),
                },
            );
            continue;
        }
        if size.start_glyph > size.end_glyph {
            emit(
                at,
                Kind::StartAfterEnd {
                    index: to_u32(i),
                    start: u32::from(size.start_glyph),
                    end: u32::from(size.end_glyph),
                },
            );
        }
        let mut prev_last: Option<u16> = None;
        for j in 0..size.number_of_index_subtables {
            let Some(r) = cblc.index_record(&size, j) else {
                continue;
            };
            let rat = array.saturating_add(to_usize(j).saturating_mul(8));
            if r.first_glyph > r.last_glyph {
                emit(
                    rat,
                    Kind::StartAfterEnd {
                        index: j,
                        start: u32::from(r.first_glyph),
                        end: u32::from(r.last_glyph),
                    },
                );
            }
            if let Some(p) = prev_last
                && r.first_glyph <= p
            {
                emit(rat, Kind::RecordsNotSorted { index: j });
            }
            prev_last = Some(r.last_glyph);
            let Some((index_format, _, _)) = cblc.index_header(&r) else {
                emit(
                    rat,
                    Kind::OffsetOutOfBounds {
                        offset: to_u32(r.offset),
                        length: 8,
                        available: to_u32(data.len()),
                    },
                );
                continue;
            };
            let n = to_usize(u32::from(r.last_glyph).saturating_sub(u32::from(r.first_glyph)))
                .saturating_add(1);
            let body = r.offset.saturating_add(8);
            let (needed, elem) = match index_format {
                1 => (n.saturating_add(1).saturating_mul(4), 4),
                3 => (n.saturating_add(1).saturating_mul(2), 2),
                2 => (12, 0),
                4 => {
                    let k = be::u32_at(data, body).map_or(0, to_usize);
                    (k.saturating_add(1).saturating_mul(4).saturating_add(4), 0)
                }
                5 => {
                    let k = be::u32_at(data, body.saturating_add(12)).map_or(0, to_usize);
                    (k.saturating_mul(2).saturating_add(16), 0)
                }
                f => {
                    emit(
                        r.offset,
                        Kind::UnknownFormat {
                            found: u32::from(f),
                        },
                    );
                    continue;
                }
            };
            if be::slice_at(data, body, needed).is_none() {
                emit(
                    r.offset,
                    Kind::OffsetOutOfBounds {
                        offset: to_u32(body),
                        length: to_u32(needed),
                        available: to_u32(data.len()),
                    },
                );
                continue;
            }
            if elem != 0 {
                let read = |k: usize| {
                    let p = body.saturating_add(k.saturating_mul(elem));
                    if elem == 4 {
                        be::u32_at(data, p)
                    } else {
                        be::u16_at(data, p).map(u32::from)
                    }
                };
                let bad = (1..=n).find(|&k| read(k) < read(k.saturating_sub(1)));
                if let Some(k) = bad {
                    emit(
                        body.saturating_add(k.saturating_mul(elem)),
                        Kind::OffsetsNotSorted { index: to_u32(k) },
                    );
                }
            }
        }
    }
}
