//! Reading of each subtable format. `data` always runs from the start
//! of the subtable to the end of the `cmap` table.

use crate::be::{self, to_usize};
use crate::cmap::VariantGlyph;
use crate::{GlyphId, ReadError};

fn u16_of(a: [u8; 2]) -> u16 {
    u16::from_be_bytes(a)
}

/// `(a + b) mod 65536` for a glyph id and a signed delta.
fn add_delta(a: u32, delta: i16) -> u32 {
    a.wrapping_add_signed(i32::from(delta)) & 0xFFFF
}

// ---------------------------------------------------------------- format 0

/// Format 0: byte encoding table.
#[derive(Clone, Copy, Debug)]
pub struct Format0<'a> {
    language: u16,
    ids: &'a [u8; 256],
}

pub(crate) const F0_LEN: usize = 6 + 256;

impl<'a> Format0<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Result<Format0<'a>, ReadError> {
        let h = data
            .first_chunk::<4>()
            .ok_or(ReadError::truncated(4, data.len()))?;
        let length = usize::from(u16::from_be_bytes([h[2], h[3]]));
        if length < F0_LEN {
            return Err(ReadError::truncated(F0_LEN, length));
        }
        if length > data.len() {
            return Err(ReadError::truncated(length, data.len()));
        }
        let language = be::u16_at(data, 4).ok_or(ReadError::truncated(F0_LEN, data.len()))?;
        let ids = be::array_at::<256>(data, 6).ok_or(ReadError::truncated(F0_LEN, data.len()))?;
        Ok(Format0 { language, ids })
    }

    /// The `language` field.
    pub fn language(&self) -> u32 {
        u32::from(self.language)
    }

    /// The raw glyph id for `cp`: 0 when unmapped, `None` when `cp` is
    /// outside the table or the data runs out.
    pub fn glyph(&self, cp: u32) -> Option<u32> {
        self.ids.get(to_usize(cp)).map(|&g| u32::from(g))
    }
}

// ---------------------------------------------------------------- format 2

/// Format 2: high-byte mapping through table.
#[derive(Clone, Copy, Debug)]
pub struct Format2<'a> {
    data: &'a [u8],
    language: u16,
    keys: &'a [[u8; 2]],
}

pub(crate) const F2_KEYS: usize = 6;
pub(crate) const F2_SUBS: usize = 6 + 512;

/// The number of sub-headers that the keys refer to.
pub(crate) fn f2_num_subs(keys: &[[u8; 2]]) -> usize {
    keys.iter()
        .map(|k| usize::from(u16_of(*k) >> 3))
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

impl<'a> Format2<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Result<Format2<'a>, ReadError> {
        let h = data
            .first_chunk::<4>()
            .ok_or(ReadError::truncated(4, data.len()))?;
        let length = usize::from(u16::from_be_bytes([h[2], h[3]]));
        if length < F2_SUBS {
            return Err(ReadError::truncated(F2_SUBS, length));
        }
        if length > data.len() {
            return Err(ReadError::truncated(length, data.len()));
        }
        let language = be::u16_at(data, 4).ok_or(ReadError::truncated(F2_SUBS, data.len()))?;
        let keys = be::chunks_at::<2>(data, F2_KEYS, 256)
            .ok_or(ReadError::truncated(F2_SUBS, data.len()))?;
        let subs_end = f2_num_subs(keys).saturating_mul(8).saturating_add(F2_SUBS);
        if subs_end > data.len() {
            return Err(ReadError::truncated(subs_end, data.len()));
        }
        Ok(Format2 {
            data,
            language,
            keys,
        })
    }

    /// The `language` field.
    pub fn language(&self) -> u32 {
        u32::from(self.language)
    }

    fn key(&self, byte: u32) -> Option<u16> {
        self.keys.get(to_usize(byte)).map(|k| u16_of(*k))
    }

    /// The offset of the sub-header for `cp`, following FreeType's
    /// `tt_cmap2_get_subheader`.
    fn subheader(&self, cp: u32) -> Option<usize> {
        if cp > 0xFFFF {
            return None;
        }
        let hi = cp >> 8;
        let lo = cp & 0xFF;
        if hi == 0 {
            // A one-byte code is valid only if its key is sub-header 0.
            (self.key(lo)? == 0).then_some(F2_SUBS)
        } else {
            let k = usize::from(self.key(hi)? & !7);
            (k != 0).then(|| F2_SUBS.saturating_add(k))
        }
    }

    /// The raw glyph id for `cp`: 0 when unmapped, `None` when `cp` is
    /// outside the table or the data runs out.
    pub fn glyph(&self, cp: u32) -> Option<u32> {
        let sub = self.subheader(cp)?;
        let first = u32::from(be::u16_at(self.data, sub)?);
        let count = u32::from(be::u16_at(self.data, sub.checked_add(2)?)?);
        let delta = be::i16_at(self.data, sub.checked_add(4)?)?;
        let range_offset = usize::from(be::u16_at(self.data, sub.checked_add(6)?)?);
        let idx = (cp & 0xFF).checked_sub(first)?;
        if idx >= count || range_offset == 0 {
            return Some(0);
        }
        let pos = sub
            .checked_add(6)?
            .checked_add(range_offset)?
            .checked_add(to_usize(idx).checked_mul(2)?)?;
        let g = u32::from(be::u16_at(self.data, pos)?);
        Some(if g == 0 { 0 } else { add_delta(g, delta) })
    }

    pub(crate) fn range(&self, index: usize) -> Option<(u32, u32)> {
        if index == 0 {
            return Some((0, 0xFF));
        }
        // index 1.. are the lead bytes 1..=255 that have a sub-header.
        let hi = (1u32..256)
            .filter(|&hi| self.key(hi).is_some_and(|k| k & !7 != 0))
            .nth(index.checked_sub(1)?)?;
        let start = hi.checked_shl(8)?;
        Some((start, start | 0xFF))
    }
}

// ---------------------------------------------------------------- format 4

/// Format 4: segment mapping to delta values.
#[derive(Clone, Copy, Debug)]
pub struct Format4<'a> {
    data: &'a [u8],
    language: u16,
    ends: &'a [[u8; 2]],
    starts: &'a [[u8; 2]],
    deltas: &'a [[u8; 2]],
    range_offsets: &'a [[u8; 2]],
    /// Where idRangeOffset[0] is.
    range_offsets_at: usize,
}

pub(crate) struct F4Layout {
    pub segs: usize,
    pub ends: usize,
    pub starts: usize,
    pub deltas: usize,
    pub range_offsets: usize,
    pub glyph_ids: usize,
}

pub(crate) fn f4_layout(seg_count_x2: u16) -> F4Layout {
    let segs = usize::from(seg_count_x2 / 2);
    let two = segs.saturating_mul(2);
    F4Layout {
        segs,
        ends: 14,
        starts: two.saturating_add(16),
        deltas: two.saturating_mul(2).saturating_add(16),
        range_offsets: two.saturating_mul(3).saturating_add(16),
        glyph_ids: two.saturating_mul(4).saturating_add(16),
    }
}

impl<'a> Format4<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Result<Format4<'a>, ReadError> {
        let h = data
            .first_chunk::<16>()
            .ok_or(ReadError::truncated(16, data.len()))?;
        let language = u16::from_be_bytes([h[4], h[5]]);
        let l = f4_layout(u16::from_be_bytes([h[6], h[7]]));
        let short = ReadError::truncated(l.glyph_ids, data.len());
        let ends = be::chunks_at::<2>(data, l.ends, l.segs).ok_or(short)?;
        let starts = be::chunks_at::<2>(data, l.starts, l.segs).ok_or(short)?;
        let deltas = be::chunks_at::<2>(data, l.deltas, l.segs).ok_or(short)?;
        let range_offsets = be::chunks_at::<2>(data, l.range_offsets, l.segs).ok_or(short)?;
        Ok(Format4 {
            data,
            language,
            ends,
            starts,
            deltas,
            range_offsets,
            range_offsets_at: l.range_offsets,
        })
    }

    /// The `language` field.
    pub fn language(&self) -> u32 {
        u32::from(self.language)
    }

    fn segment(&self, i: usize) -> Option<(u32, u32)> {
        Some((
            u32::from(u16_of(*self.starts.get(i)?)),
            u32::from(u16_of(*self.ends.get(i)?)),
        ))
    }

    /// Binary search on both ends of the segments, as FreeType does.
    fn find(&self, cp: u32) -> Option<usize> {
        let (mut min, mut max) = (0usize, self.ends.len());
        while min < max {
            let mid = min.midpoint(max);
            let (start, end) = self.segment(mid)?;
            if cp < start {
                max = mid;
            } else if cp > end {
                min = mid.checked_add(1)?;
            } else {
                return Some(mid);
            }
        }
        None
    }

    /// The raw glyph id for `cp`: 0 when unmapped, `None` when `cp` is
    /// outside the table or the data runs out.
    pub fn glyph(&self, cp: u32) -> Option<u32> {
        if cp > 0xFFFF {
            return None;
        }
        let Some(i) = self.find(cp) else {
            return Some(0);
        };
        let (start, end) = self.segment(i)?;
        let mut delta = i16::from_be_bytes(*self.deltas.get(i)?);
        let mut range_offset = usize::from(u16_of(*self.range_offsets.get(i)?));
        let field = self.range_offsets_at.checked_add(i.checked_mul(2)?)?;
        let last = i.checked_add(1)? == self.ends.len();
        // FreeType: a final 0xFFFF segment whose glyph id lies outside the
        // table maps 0xFFFF to glyph 0.
        if last && start == 0xFFFF && end == 0xFFFF && range_offset != 0 {
            let reach = field.saturating_add(range_offset).saturating_add(2);
            if reach > self.data.len() {
                delta = 1;
                range_offset = 0;
            }
        }
        if range_offset == 0xFFFF {
            return Some(0);
        }
        if range_offset == 0 {
            return Some(add_delta(cp, delta));
        }
        let pos = field
            .checked_add(range_offset)?
            .checked_add(to_usize(cp.checked_sub(start)?).checked_mul(2)?)?;
        let g = u32::from(be::u16_at(self.data, pos)?);
        Some(if g == 0 { 0 } else { add_delta(g, delta) })
    }

    pub(crate) fn range(&self, index: usize) -> Option<(u32, u32)> {
        self.segment(index)
    }
}

// ---------------------------------------------------------------- format 6

/// Format 6: trimmed table mapping.
#[derive(Clone, Copy, Debug)]
pub struct Format6<'a> {
    language: u16,
    first: u16,
    ids: &'a [[u8; 2]],
}

impl<'a> Format6<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Result<Format6<'a>, ReadError> {
        let h = data
            .first_chunk::<10>()
            .ok_or(ReadError::truncated(10, data.len()))?;
        let length = usize::from(u16::from_be_bytes([h[2], h[3]]));
        let count = usize::from(u16::from_be_bytes([h[8], h[9]]));
        let need = count.saturating_mul(2).saturating_add(10);
        if length > data.len() {
            return Err(ReadError::truncated(length, data.len()));
        }
        if length < need {
            return Err(ReadError::truncated(need, length));
        }
        let ids =
            be::chunks_at::<2>(data, 10, count).ok_or(ReadError::truncated(need, data.len()))?;
        Ok(Format6 {
            language: u16::from_be_bytes([h[4], h[5]]),
            first: u16::from_be_bytes([h[6], h[7]]),
            ids,
        })
    }

    /// The `language` field.
    pub fn language(&self) -> u32 {
        u32::from(self.language)
    }

    /// The raw glyph id for `cp`: 0 when unmapped, `None` when `cp` is
    /// outside the table or the data runs out.
    pub fn glyph(&self, cp: u32) -> Option<u32> {
        let i = cp.checked_sub(u32::from(self.first))?;
        Some(
            self.ids
                .get(to_usize(i))
                .map_or(0, |g| u32::from(u16_of(*g))),
        )
    }

    pub(crate) fn range(&self) -> Option<(u32, u32)> {
        let n = u32::try_from(self.ids.len()).ok()?.checked_sub(1)?;
        let first = u32::from(self.first);
        Some((first, first.checked_add(n)?))
    }
}

// ---------------------------------------------------------------- groups

/// `(start, end, glyph)` triples, sorted by code.
fn group(g: &[u8; 12]) -> (u32, u32, u32) {
    (
        u32::from_be_bytes([g[0], g[1], g[2], g[3]]),
        u32::from_be_bytes([g[4], g[5], g[6], g[7]]),
        u32::from_be_bytes([g[8], g[9], g[10], g[11]]),
    )
}

fn find_group(groups: &[[u8; 12]], cp: u32) -> Option<(u32, u32, u32)> {
    let (mut min, mut max) = (0usize, groups.len());
    while min < max {
        let mid = min.midpoint(max);
        let g = group(groups.get(mid)?);
        if cp < g.0 {
            max = mid;
        } else if cp > g.1 {
            min = mid.checked_add(1)?;
        } else {
            return Some(g);
        }
    }
    None
}

// ---------------------------------------------------------------- format 8

/// Format 8: mixed 16-bit and 32-bit coverage.
#[derive(Clone, Copy, Debug)]
pub struct Format8<'a> {
    language: u32,
    groups: &'a [[u8; 12]],
}

pub(crate) const F8_GROUPS: usize = 12 + 8192 + 4;

impl<'a> Format8<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Result<Format8<'a>, ReadError> {
        if data.len() < F8_GROUPS {
            return Err(ReadError::truncated(F8_GROUPS, data.len()));
        }
        let length =
            to_usize(be::u32_at(data, 4).ok_or(ReadError::truncated(F8_GROUPS, data.len()))?);
        if length > data.len() {
            return Err(ReadError::truncated(length, data.len()));
        }
        if length < F8_GROUPS {
            return Err(ReadError::truncated(F8_GROUPS, length));
        }
        let n = to_usize(
            be::u32_at(data, F8_GROUPS.saturating_sub(4))
                .ok_or(ReadError::truncated(F8_GROUPS, data.len()))?,
        );
        let groups = be::chunks_at::<12>(data, F8_GROUPS, n).ok_or(ReadError::truncated(
            n.saturating_mul(12).saturating_add(F8_GROUPS),
            data.len(),
        ))?;
        let language = be::u32_at(data, 8).ok_or(ReadError::truncated(F8_GROUPS, data.len()))?;
        Ok(Format8 { language, groups })
    }

    /// The `language` field.
    pub fn language(&self) -> u32 {
        self.language
    }

    /// The raw glyph id for `cp`: 0 when unmapped, `None` when `cp` is
    /// outside the table or the data runs out.
    pub fn glyph(&self, cp: u32) -> Option<u32> {
        let Some((start, _, id)) = find_group(self.groups, cp) else {
            return Some(0);
        };
        id.checked_add(cp.checked_sub(start)?)
    }

    pub(crate) fn range(&self, index: usize) -> Option<(u32, u32)> {
        let (s, e, _) = group(self.groups.get(index)?);
        Some((s, e))
    }
}

// ---------------------------------------------------------------- format 10

/// Format 10: trimmed array.
#[derive(Clone, Copy, Debug)]
pub struct Format10<'a> {
    language: u32,
    start: u32,
    ids: &'a [[u8; 2]],
}

impl<'a> Format10<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Result<Format10<'a>, ReadError> {
        let h = data
            .first_chunk::<20>()
            .ok_or(ReadError::truncated(20, data.len()))?;
        let length = to_usize(u32::from_be_bytes([h[4], h[5], h[6], h[7]]));
        let count = to_usize(u32::from_be_bytes([h[16], h[17], h[18], h[19]]));
        if length > data.len() {
            return Err(ReadError::truncated(length, data.len()));
        }
        let need = count.saturating_mul(2).saturating_add(20);
        if length < need {
            return Err(ReadError::truncated(need, length));
        }
        let ids =
            be::chunks_at::<2>(data, 20, count).ok_or(ReadError::truncated(need, data.len()))?;
        Ok(Format10 {
            language: u32::from_be_bytes([h[8], h[9], h[10], h[11]]),
            start: u32::from_be_bytes([h[12], h[13], h[14], h[15]]),
            ids,
        })
    }

    /// The `language` field.
    pub fn language(&self) -> u32 {
        self.language
    }

    /// The raw glyph id for `cp`: 0 when unmapped, `None` when `cp` is
    /// outside the table or the data runs out.
    pub fn glyph(&self, cp: u32) -> Option<u32> {
        let i = cp.checked_sub(self.start)?;
        Some(
            self.ids
                .get(to_usize(i))
                .map_or(0, |g| u32::from(u16_of(*g))),
        )
    }

    pub(crate) fn range(&self) -> Option<(u32, u32)> {
        let n = u32::try_from(self.ids.len()).ok()?.checked_sub(1)?;
        Some((self.start, self.start.checked_add(n)?))
    }
}

// ---------------------------------------------------------------- formats 12, 13

/// Formats 12 (segmented coverage) and 13 (many-to-one).
#[derive(Clone, Copy, Debug)]
pub struct Groups<'a> {
    language: u32,
    groups: &'a [[u8; 12]],
    many_to_one: bool,
}

impl<'a> Groups<'a> {
    pub(crate) fn new(data: &'a [u8], many_to_one: bool) -> Result<Groups<'a>, ReadError> {
        let h = data
            .first_chunk::<16>()
            .ok_or(ReadError::truncated(16, data.len()))?;
        let length = to_usize(u32::from_be_bytes([h[4], h[5], h[6], h[7]]));
        let n = to_usize(u32::from_be_bytes([h[12], h[13], h[14], h[15]]));
        if length > data.len() {
            return Err(ReadError::truncated(length, data.len()));
        }
        let need = n.saturating_mul(12).saturating_add(16);
        if length < need {
            return Err(ReadError::truncated(need, length));
        }
        let groups =
            be::chunks_at::<12>(data, 16, n).ok_or(ReadError::truncated(need, data.len()))?;
        Ok(Groups {
            language: u32::from_be_bytes([h[8], h[9], h[10], h[11]]),
            groups,
            many_to_one,
        })
    }

    /// The `language` field.
    pub fn language(&self) -> u32 {
        self.language
    }

    /// The raw glyph id for `cp`: 0 when unmapped, `None` when `cp` is
    /// outside the table or the data runs out.
    pub fn glyph(&self, cp: u32) -> Option<u32> {
        let Some((start, _, id)) = find_group(self.groups, cp) else {
            return Some(0);
        };
        if self.many_to_one {
            Some(id)
        } else {
            id.checked_add(cp.checked_sub(start)?)
        }
    }

    pub(crate) fn range(&self, index: usize) -> Option<(u32, u32)> {
        let (s, e, _) = group(self.groups.get(index)?);
        Some((s, e))
    }
}

// ---------------------------------------------------------------- formats 14, 15

/// Formats 14 and 15 (variation sequences; 15 has 24-bit glyph ids).
#[derive(Clone, Copy, Debug)]
pub struct Uvs<'a> {
    data: &'a [u8],
    records: &'a [[u8; 11]],
    wide: bool,
}

impl<'a> Uvs<'a> {
    pub(crate) fn new(data: &'a [u8], wide: bool) -> Result<Uvs<'a>, ReadError> {
        let h = data
            .first_chunk::<10>()
            .ok_or(ReadError::truncated(10, data.len()))?;
        let length = to_usize(u32::from_be_bytes([h[2], h[3], h[4], h[5]]));
        let n = to_usize(u32::from_be_bytes([h[6], h[7], h[8], h[9]]));
        if length > data.len() {
            return Err(ReadError::truncated(length, data.len()));
        }
        let need = n.saturating_mul(11).saturating_add(10);
        if length < need {
            return Err(ReadError::truncated(need, length));
        }
        let records =
            be::chunks_at::<11>(data, 10, n).ok_or(ReadError::truncated(need, data.len()))?;
        Ok(Uvs {
            data,
            records,
            wide,
        })
    }

    fn record(&self, selector: u32) -> Option<(usize, usize)> {
        let i = self
            .records
            .binary_search_by_key(&selector, |r| be::u24([r[0], r[1], r[2]]))
            .ok()?;
        let r = self.records.get(i)?;
        Some((
            to_usize(u32::from_be_bytes([r[3], r[4], r[5], r[6]])),
            to_usize(u32::from_be_bytes([r[7], r[8], r[9], r[10]])),
        ))
    }

    /// The glyph for `cp` + `selector`; see [`super::Subtable::variant`].
    pub fn variant(&self, cp: u32, selector: u32) -> Option<VariantGlyph> {
        let (default, non_default) = self.record(selector)?;
        if default != 0 && self.in_default(default, cp) {
            return Some(VariantGlyph::Default);
        }
        if non_default != 0 {
            return self
                .non_default(non_default, cp)
                .map(|g| VariantGlyph::Glyph(GlyphId(g)));
        }
        None
    }

    fn in_default(&self, at: usize, cp: u32) -> bool {
        let Some(n) = be::u32_at(self.data, at) else {
            return false;
        };
        let Some(ranges) = be::chunks_at::<4>(self.data, at.saturating_add(4), to_usize(n)) else {
            return false;
        };
        // The last range starting at or before cp.
        let i = ranges.partition_point(|r| be::u24([r[0], r[1], r[2]]) <= cp);
        let Some(r) = i.checked_sub(1).and_then(|i| ranges.get(i)) else {
            return false;
        };
        let start = be::u24([r[0], r[1], r[2]]);
        cp.checked_sub(start).is_some_and(|d| d <= u32::from(r[3]))
    }

    fn non_default(&self, at: usize, cp: u32) -> Option<u32> {
        let n = to_usize(be::u32_at(self.data, at)?);
        let base = at.checked_add(4)?;
        if self.wide {
            let maps = be::chunks_at::<6>(self.data, base, n)?;
            let i = maps
                .binary_search_by_key(&cp, |m| be::u24([m[0], m[1], m[2]]))
                .ok()?;
            maps.get(i).map(|m| be::u24([m[3], m[4], m[5]]))
        } else {
            let maps = be::chunks_at::<5>(self.data, base, n)?;
            let i = maps
                .binary_search_by_key(&cp, |m| be::u24([m[0], m[1], m[2]]))
                .ok()?;
            maps.get(i)
                .map(|m| u32::from(u16::from_be_bytes([m[3], m[4]])))
        }
    }
}
