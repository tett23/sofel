//! The `gvar` and `GVAR` glyph variation tables (ADR 0027).
//!
//! Views borrow the table bytes and decode packed runs without allocation.
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/gvar>

use crate::be::{self, to_u32, to_usize};
use crate::{Diagnostic, Kind, ReadError, Tag};

/// The standard glyph variations table tag.
pub const TAG: Tag = Tag::new(b"gvar");
/// The 24-bit glyph-count extension used with `GLYF`.
pub const GVAR_TAG: Tag = Tag::new(b"GVAR");

/// A borrowed glyph variations table, in either glyph-count format.
#[derive(Clone, Copy, Debug)]
pub struct Gvar<'a> {
    data: &'a [u8],
    tag: Tag,
    axes: u16,
    glyphs: u32,
    shared_count: u16,
    shared_at: usize,
    offsets_at: usize,
    data_at: usize,
    long_offsets: bool,
}

impl<'a> Gvar<'a> {
    /// Opens a `gvar` or `GVAR` table, checking the complete header, offset
    /// array, and shared tuple array. Glyph data is checked when accessed.
    pub fn new(data: &'a [u8], tag: Tag) -> Result<Self, ReadError> {
        let large = tag == GVAR_TAG;
        let header = if large { 21usize } else { 20 };
        let trunc = |needed: usize| ReadError::truncated(needed, data.len());
        let version = be::u32_at(data, 0).ok_or(trunc(header))?;
        if version != 0x0001_0000 {
            return Err(ReadError::UnknownVersion(version));
        }
        let axes = be::u16_at(data, 4).ok_or(trunc(header))?;
        let shared_count = be::u16_at(data, 6).ok_or(trunc(header))?;
        let shared_at = to_usize(be::u32_at(data, 8).ok_or(trunc(header))?);
        let glyphs = if large {
            be::u24_at(data, 12).ok_or(trunc(header))?
        } else {
            u32::from(be::u16_at(data, 12).ok_or(trunc(header))?)
        };
        let flags_at = if large { 15 } else { 14 };
        let long_offsets = be::u16_at(data, flags_at).ok_or(trunc(header))? & 1 != 0;
        let data_at = to_usize(be::u32_at(data, flags_at.saturating_add(2)).ok_or(trunc(header))?);
        let offset_size = if long_offsets { 4usize } else { 2 };
        let offsets_end = header
            .checked_add(
                to_usize(glyphs)
                    .checked_add(1)
                    .and_then(|n| n.checked_mul(offset_size))
                    .ok_or(trunc(usize::MAX))?,
            )
            .ok_or(trunc(usize::MAX))?;
        be::slice_at(data, header, offsets_end.saturating_sub(header)).ok_or(trunc(offsets_end))?;
        let shared_len = usize::from(axes)
            .checked_mul(usize::from(shared_count))
            .and_then(|n| n.checked_mul(2))
            .ok_or(trunc(usize::MAX))?;
        be::slice_at(data, shared_at, shared_len)
            .ok_or(trunc(shared_at.saturating_add(shared_len)))?;
        be::tail_at(data, data_at).ok_or(trunc(data_at))?;
        Ok(Self {
            data,
            tag,
            axes,
            glyphs,
            shared_count,
            shared_at,
            offsets_at: header,
            data_at,
            long_offsets,
        })
    }

    /// The table tag (`gvar` or `GVAR`).
    pub fn tag(&self) -> Tag {
        self.tag
    }
    /// The number of variation axes.
    pub fn axis_count(&self) -> u16 {
        self.axes
    }
    /// The number of glyphs, up to 24 bits for `GVAR`.
    pub fn glyph_count(&self) -> u32 {
        self.glyphs
    }
    /// The number of shared peak tuples.
    pub fn shared_tuple_count(&self) -> u16 {
        self.shared_count
    }

    /// Shared peak tuple `i`.
    pub fn shared_tuple(&self, i: u16) -> Option<Tuple<'a>> {
        if i >= self.shared_count {
            return None;
        }
        let len = usize::from(self.axes).checked_mul(2)?;
        let at = self
            .shared_at
            .checked_add(usize::from(i).checked_mul(len)?)?;
        Some(Tuple {
            bytes: be::slice_at(self.data, at, len)?,
        })
    }

    /// Offset relative to the start of the glyph data array.
    pub fn glyph_offset(&self, glyph: u32) -> Option<usize> {
        if glyph > self.glyphs {
            return None;
        }
        let width = if self.long_offsets { 4usize } else { 2 };
        let at = self
            .offsets_at
            .checked_add(to_usize(glyph).checked_mul(width)?)?;
        if self.long_offsets {
            Some(to_usize(be::u32_at(self.data, at)?))
        } else {
            Some(usize::from(be::u16_at(self.data, at)?).checked_mul(2)?)
        }
    }

    /// Variation data for a glyph. `None` means an empty or unreadable entry.
    pub fn glyph_data(&self, glyph: u32) -> Option<GlyphVariationData<'a>> {
        if glyph >= self.glyphs {
            return None;
        }
        let first = self.glyph_offset(glyph)?;
        let last = self.glyph_offset(glyph.checked_add(1)?)?;
        if first >= last {
            return None;
        }
        let at = self.data_at.checked_add(first)?;
        let bytes = be::slice_at(self.data, at, last.checked_sub(first)?)?;
        GlyphVariationData::new(bytes, self.axes, self.tag == GVAR_TAG)
    }
}

/// A normalized 2.14 tuple, with one coordinate per axis.
#[derive(Clone, Copy, Debug)]
pub struct Tuple<'a> {
    bytes: &'a [u8],
}

impl Tuple<'_> {
    /// The number of axes.
    pub fn len(&self) -> usize {
        self.bytes.len() / 2
    }
    /// Whether there are no axes.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    /// Raw 2.14 coordinate of axis `i`.
    pub fn axis(&self, i: usize) -> Option<i16> {
        be::i16_at(self.bytes, i.checked_mul(2)?)
    }
}

/// One glyph's tuple headers and serialized variation data.
#[derive(Clone, Copy, Debug)]
pub struct GlyphVariationData<'a> {
    bytes: &'a [u8],
    axes: u16,
    count: u16,
    shared_points: bool,
    data_at: usize,
    header_at: usize,
}

impl<'a> GlyphVariationData<'a> {
    fn new(bytes: &'a [u8], axes: u16, large: bool) -> Option<Self> {
        let count_flags = be::u16_at(bytes, 0)?;
        let header_at = if large { 5 } else { 4 };
        let data_at = if large {
            to_usize(be::u24_at(bytes, 2)?)
        } else {
            usize::from(be::u16_at(bytes, 2)?)
        };
        be::tail_at(bytes, data_at)?;
        let this = Self {
            bytes,
            axes,
            count: count_flags & 0x0fff,
            shared_points: count_flags & 0x8000 != 0,
            data_at,
            header_at,
        };
        let mut header = header_at;
        let mut data = data_at;
        if this.shared_points {
            let (_, used) = PackedPoints::parse(be::tail_at(bytes, data)?)?;
            data = data.checked_add(used)?;
        }
        for _ in 0..this.count {
            let (next, size, _) = this.header(header)?;
            header = next;
            data = data.checked_add(size)?;
            be::tail_at(bytes, data)?;
        }
        (header <= data_at).then_some(this)
    }

    fn header(&self, at: usize) -> Option<(usize, usize, u16)> {
        let size = usize::from(be::u16_at(self.bytes, at)?);
        let index = be::u16_at(self.bytes, at.checked_add(2)?)?;
        let tuple_len = usize::from(self.axes).checked_mul(2)?;
        let count = usize::from(index & 0x8000 != 0)
            .saturating_add(2usize.saturating_mul(usize::from(index & 0x4000 != 0)));
        let next = at
            .checked_add(4)?
            .checked_add(count.checked_mul(tuple_len)?)?;
        be::slice_at(self.bytes, at, next.checked_sub(at)?)?;
        Some((next, size, index))
    }

    /// Number of tuple variation records.
    pub fn tuple_count(&self) -> u16 {
        self.count
    }
    /// Whether a shared point-number list starts the serialized data.
    pub fn has_shared_points(&self) -> bool {
        self.shared_points
    }
    /// Shared point numbers, if present.
    pub fn shared_points(&self) -> Option<PackedPoints<'a>> {
        self.shared_points
            .then(|| PackedPoints::parse(be::tail_at(self.bytes, self.data_at)?).map(|p| p.0))
            .flatten()
    }

    /// Tuple variation `i`, with its own serialized-data slice.
    pub fn tuple(&self, i: u16) -> Option<TupleVariation<'a>> {
        if i >= self.count {
            return None;
        }
        let mut header = self.header_at;
        let mut data = self.data_at;
        if self.shared_points {
            data = data.checked_add(PackedPoints::parse(be::tail_at(self.bytes, data)?)?.1)?;
        }
        for k in 0..=i {
            let (next, size, index) = self.header(header)?;
            if k == i {
                let bytes = be::slice_at(self.bytes, data, size)?;
                let coordinate_at = header.checked_add(4)?;
                let coordinate_len = usize::from(self.axes).checked_mul(2)?;
                let embedded = index & 0x8000 != 0;
                let peak = if embedded {
                    Some(Tuple {
                        bytes: be::slice_at(self.bytes, coordinate_at, coordinate_len)?,
                    })
                } else {
                    None
                };
                let mid = coordinate_at.checked_add(if embedded { coordinate_len } else { 0 })?;
                let intermediate = if index & 0x4000 != 0 {
                    Some((
                        Tuple {
                            bytes: be::slice_at(self.bytes, mid, coordinate_len)?,
                        },
                        Tuple {
                            bytes: be::slice_at(
                                self.bytes,
                                mid.checked_add(coordinate_len)?,
                                coordinate_len,
                            )?,
                        },
                    ))
                } else {
                    None
                };
                return Some(TupleVariation {
                    bytes,
                    index,
                    peak,
                    intermediate,
                });
            }
            header = next;
            data = data.checked_add(size)?;
        }
        None
    }
}

/// A glyph tuple's peak, optional intermediate region, point list and deltas.
#[derive(Clone, Copy, Debug)]
pub struct TupleVariation<'a> {
    bytes: &'a [u8],
    index: u16,
    peak: Option<Tuple<'a>>,
    intermediate: Option<(Tuple<'a>, Tuple<'a>)>,
}

impl<'a> TupleVariation<'a> {
    /// Index of a shared peak tuple, if no peak is embedded.
    pub fn shared_peak_index(&self) -> Option<u16> {
        self.peak.is_none().then_some(self.index & 0x0fff)
    }
    /// Embedded peak tuple.
    pub fn embedded_peak(&self) -> Option<Tuple<'a>> {
        self.peak
    }
    /// Optional start and end tuples of an intermediate region.
    pub fn intermediate(&self) -> Option<(Tuple<'a>, Tuple<'a>)> {
        self.intermediate
    }
    /// Whether point numbers are private to this tuple.
    pub fn private_points(&self) -> bool {
        self.index & 0x2000 != 0
    }
    /// Serialized point and delta bytes for this tuple.
    pub fn data(&self) -> &'a [u8] {
        self.bytes
    }
    /// The private point list, if present. Without one, use the glyph's
    /// shared point list; without either, deltas cover all points.
    pub fn points(&self) -> Option<PackedPoints<'a>> {
        self.private_points()
            .then(|| PackedPoints::parse(self.bytes).map(|p| p.0))
            .flatten()
    }

    /// Decode the x and y delta runs for a glyph with `point_count` outline
    /// and phantom points. The shared point list is ignored when this tuple
    /// has a private list. Returns `None` for an invalid point number, delta
    /// count, or serialized-data length.
    pub fn deltas(
        &self,
        point_count: usize,
        shared_points: Option<PackedPoints<'a>>,
    ) -> Option<(PackedDeltas<'a>, PackedDeltas<'a>)> {
        let (points, point_bytes) = if self.private_points() {
            let (points, bytes) = PackedPoints::parse(self.bytes)?;
            (Some(points), bytes)
        } else {
            (shared_points, 0)
        };
        let count = match points {
            Some(points) if !points.is_all() => {
                if points
                    .iter()
                    .any(|point| usize::try_from(point).ok().is_none_or(|i| i >= point_count))
                {
                    return None;
                }
                points.len()
            }
            _ => point_count,
        };
        let bytes = be::tail_at(self.bytes, point_bytes)?;
        let (x, used_x) = PackedDeltas::parse(bytes, count)?;
        let (y, used_y) = PackedDeltas::parse(be::tail_at(bytes, used_x)?, count)?;
        (point_bytes.checked_add(used_x)?.checked_add(used_y)? == self.bytes.len())
            .then_some((x, y))
    }
}

/// Packed point numbers, including the all-points sentinel.
#[derive(Clone, Copy, Debug)]
pub struct PackedPoints<'a> {
    runs: &'a [u8],
    count: u16,
    all: bool,
}

impl<'a> PackedPoints<'a> {
    /// Parses the complete point-number block, returning bytes consumed.
    pub fn parse(data: &'a [u8]) -> Option<(Self, usize)> {
        let first = be::u8_at(data, 0)?;
        let (count, mut at) = if first & 0x80 != 0 {
            (
                u16::from(first & 0x7f)
                    .checked_mul(256)?
                    .checked_add(u16::from(be::u8_at(data, 1)?))?,
                2usize,
            )
        } else {
            (u16::from(first), 1usize)
        };
        if count == 0 {
            return Some((
                Self {
                    runs: &[],
                    count,
                    all: true,
                },
                at,
            ));
        }
        let start = at;
        let mut read = 0usize;
        let mut point = 0u32;
        while read < usize::from(count) {
            let control = be::u8_at(data, at)?;
            at = at.checked_add(1)?;
            let n = usize::from(control & 0x7f) + 1;
            if n > usize::from(count).checked_sub(read)? {
                return None;
            }
            let width = if control & 0x80 != 0 { 2usize } else { 1 };
            for _ in 0..n {
                let delta = if width == 2 {
                    u32::from(be::u16_at(data, at)?)
                } else {
                    u32::from(be::u8_at(data, at)?)
                };
                point = point.checked_add(delta)?;
                at = at.checked_add(width)?;
            }
            read = read.checked_add(n)?;
        }
        Some((
            Self {
                runs: be::slice_at(data, start, at.checked_sub(start)?)?,
                count,
                all: false,
            },
            at,
        ))
    }
    /// Whether the count's zero sentinel denotes all points.
    pub fn is_all(&self) -> bool {
        self.all
    }
    /// Number of explicit point numbers.
    pub fn len(&self) -> usize {
        usize::from(self.count)
    }
    /// Whether the explicit list is empty (the all-points case).
    pub fn is_empty(&self) -> bool {
        self.all
    }
    /// Decodes points in order without allocating.
    pub fn iter(&self) -> PointIter<'a> {
        PointIter {
            data: self.runs,
            at: 0,
            remaining: usize::from(self.count),
            run: 0,
            words: false,
            point: 0,
        }
    }
}

/// Iterator over cumulative packed point numbers.
pub struct PointIter<'a> {
    data: &'a [u8],
    at: usize,
    remaining: usize,
    run: usize,
    words: bool,
    point: u32,
}
impl Iterator for PointIter<'_> {
    type Item = u32;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        if self.run == 0 {
            let control = be::u8_at(self.data, self.at)?;
            self.at = self.at.checked_add(1)?;
            self.run = usize::from(control & 0x7f).checked_add(1)?;
            self.words = control & 0x80 != 0;
        }
        let delta = if self.words {
            u32::from(be::u16_at(self.data, self.at)?)
        } else {
            u32::from(be::u8_at(self.data, self.at)?)
        };
        self.at = self.at.checked_add(if self.words { 2 } else { 1 })?;
        self.run = self.run.checked_sub(1)?;
        self.remaining = self.remaining.checked_sub(1)?;
        self.point = self.point.checked_add(delta)?;
        Some(self.point)
    }
}

/// Packed signed delta values (8-bit, 16-bit, or a run of zeroes).
#[derive(Clone, Copy, Debug)]
pub struct PackedDeltas<'a> {
    runs: &'a [u8],
    count: usize,
}
impl<'a> PackedDeltas<'a> {
    /// Parses exactly `count` logical deltas, returning bytes consumed.
    pub fn parse(data: &'a [u8], count: usize) -> Option<(Self, usize)> {
        let mut at = 0usize;
        let mut read = 0usize;
        while read < count {
            let control = be::u8_at(data, at)?;
            at = at.checked_add(1)?;
            let n = usize::from(control & 0x3f) + 1;
            if n > count.checked_sub(read)? {
                return None;
            }
            let width = if control & 0x80 != 0 {
                0usize
            } else if control & 0x40 != 0 {
                2
            } else {
                1
            };
            at = at.checked_add(n.checked_mul(width)?)?;
            be::tail_at(data, at)?;
            read = read.checked_add(n)?;
        }
        Some((
            Self {
                runs: be::slice_at(data, 0, at)?,
                count,
            },
            at,
        ))
    }
    /// Number of logical deltas.
    pub fn len(&self) -> usize {
        self.count
    }
    /// Whether there are no deltas.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    /// Decodes the signed deltas without allocating.
    pub fn iter(&self) -> DeltaIter<'a> {
        DeltaIter {
            data: self.runs,
            at: 0,
            remaining: self.count,
            run: 0,
            width: 0,
        }
    }
}

/// Iterator over packed signed deltas.
pub struct DeltaIter<'a> {
    data: &'a [u8],
    at: usize,
    remaining: usize,
    run: usize,
    width: usize,
}
impl Iterator for DeltaIter<'_> {
    type Item = i16;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        if self.run == 0 {
            let control = be::u8_at(self.data, self.at)?;
            self.at = self.at.checked_add(1)?;
            self.run = usize::from(control & 0x3f).checked_add(1)?;
            self.width = if control & 0x80 != 0 {
                0
            } else if control & 0x40 != 0 {
                2
            } else {
                1
            };
        }
        let value = match self.width {
            0 => 0,
            1 => {
                let v = i16::from(be::i8_at(self.data, self.at)?);
                self.at = self.at.checked_add(1)?;
                v
            }
            _ => {
                let v = be::i16_at(self.data, self.at)?;
                self.at = self.at.checked_add(2)?;
                v
            }
        };
        self.run = self.run.checked_sub(1)?;
        self.remaining = self.remaining.checked_sub(1)?;
        Some(value)
    }
}

/// Checks the header, shared tuples, glyph offsets, and tuple headers.
/// Use [`validate_with_points`] to check point numbers and delta counts too.
pub fn validate(
    data: &[u8],
    tag: Tag,
    fvar_axes: Option<u16>,
    maxp_glyphs: Option<u32>,
    report: &mut dyn FnMut(Diagnostic),
) {
    validate_with_points(data, tag, fvar_axes, maxp_glyphs, |_| None, report);
}

/// Validates packed point numbers and x/y deltas against the glyph's point
/// count, including four phantom points. A missing count skips that glyph's
/// packed-data checks while retaining table-structure checks.
pub fn validate_with_points(
    data: &[u8],
    tag: Tag,
    fvar_axes: Option<u16>,
    maxp_glyphs: Option<u32>,
    point_count: impl Fn(u32) -> Option<usize>,
    report: &mut dyn FnMut(Diagnostic),
) {
    let emit = |offset: usize, kind: Kind, report: &mut dyn FnMut(Diagnostic)| {
        report(Diagnostic::new(tag, to_u32(offset), kind))
    };
    let table = match Gvar::new(data, tag) {
        Ok(t) => t,
        Err(ReadError::UnknownVersion(found)) => {
            emit(0, Kind::UnknownVersion { found }, report);
            return;
        }
        Err(ReadError::Truncated { needed, available }) => {
            emit(
                0,
                Kind::Truncated {
                    needed: to_u32(needed),
                    available: to_u32(available),
                },
                report,
            );
            return;
        }
        Err(_) => {
            emit(
                0,
                Kind::Truncated {
                    needed: to_u32(data.len().saturating_add(1)),
                    available: to_u32(data.len()),
                },
                report,
            );
            return;
        }
    };
    if let Some(expected) = fvar_axes
        && expected != table.axes
    {
        emit(
            4,
            Kind::AxisCountMismatch {
                found: u32::from(table.axes),
                expected: u32::from(expected),
            },
            report,
        );
    }
    if let Some(expected) = maxp_glyphs
        && expected != table.glyphs
    {
        emit(
            12,
            Kind::GlyphCountMismatch {
                found: table.glyphs,
                expected,
            },
            report,
        );
    }
    for glyph in 0..table.glyphs {
        let first = table.glyph_offset(glyph).unwrap_or(usize::MAX);
        let last = table
            .glyph_offset(glyph.saturating_add(1))
            .unwrap_or(usize::MAX);
        if first > last
            || table
                .data_at
                .checked_add(last)
                .is_none_or(|end| end > data.len())
        {
            emit(
                table.offsets_at.saturating_add(
                    to_usize(glyph).saturating_mul(if table.long_offsets { 4 } else { 2 }),
                ),
                Kind::OffsetOutOfBounds {
                    offset: to_u32(first),
                    length: to_u32(last.saturating_sub(first)),
                    available: to_u32(data.len()),
                },
                report,
            );
            continue;
        }
        if first == last {
            continue;
        }
        let Some(glyph_data) = table.glyph_data(glyph) else {
            emit(
                table.data_at.saturating_add(first),
                Kind::Truncated {
                    needed: to_u32(last.saturating_sub(first)),
                    available: to_u32(
                        data.len()
                            .saturating_sub(table.data_at.saturating_add(first)),
                    ),
                },
                report,
            );
            continue;
        };
        let points = point_count(glyph);
        let shared_points = glyph_data.shared_points();
        for i in 0..glyph_data.tuple_count() {
            let Some(tuple) = glyph_data.tuple(i) else {
                break;
            };
            if let Some(index) = tuple.shared_peak_index()
                && index >= table.shared_count
            {
                emit(
                    table.data_at.saturating_add(first),
                    Kind::RegionIndexOutOfRange {
                        index: u32::from(index),
                        count: u32::from(table.shared_count),
                    },
                    report,
                );
            }
            if let Some(points) = points
                && tuple.deltas(points, shared_points).is_none()
            {
                emit(
                    table.data_at.saturating_add(first),
                    Kind::PointDeltaCountMismatch {
                        glyph,
                        tuple: u32::from(i),
                    },
                    report,
                );
            }
        }
    }
}
