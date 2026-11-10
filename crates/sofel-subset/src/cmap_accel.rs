//! HarfBuzz's cmap reading as the subsetter uses it
//! (`src/hb-ot-cmap-table.hh`: the subtables' `get_glyph`,
//! `collect_unicodes` and `collect_mapping`, `cmap::accelerator_t` and
//! `cmap_accelerator_t`, which consults `DMAP` before `cmap`).

use alloc::collections::BTreeMap;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::be::{u8_at, u16_at, u24_at, u32_at};
use sofel_font::remap;

use crate::set::RangeSet;

/// `HB_UNICODE_MAX`.
const UNICODE_MAX: u32 = 0x10FFFF;

/// A cmap or DMAP table.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CmapTable<'a> {
    pub data: &'a [u8],
}

/// An encoding record.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Record {
    pub platform: u16,
    pub encoding: u16,
    pub offset: u32,
}

/// A subtable: its bytes from its start to the table's end.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Subtable<'a> {
    pub data: &'a [u8],
    /// `table_length - offset` (`get_subtable_data_size`).
    pub size: usize,
}

fn u16a(d: &[u8], at: usize) -> u32 {
    u16_at(d, at).map_or(0, u32::from)
}
fn u32a(d: &[u8], at: usize) -> u32 {
    u32_at(d, at).unwrap_or(0)
}

/// A segment index as HarfBuzz's `unsigned int` (segment counts are at
/// most 0x7FFF, so the value is kept).
fn c_uint(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

impl<'a> CmapTable<'a> {
    /// The table if it passes HarfBuzz's header check (version 0 and the
    /// records in range).
    pub fn new(data: &'a [u8]) -> Option<CmapTable<'a>> {
        let n = usize::from(u16_at(data, 2)?);
        (u16_at(data, 0)? == 0 && data.len() >= 4usize.saturating_add(n.saturating_mul(8)))
            .then_some(CmapTable { data })
    }

    pub fn records(&self) -> impl Iterator<Item = Record> + 'a {
        let d = self.data;
        let n = u16a(d, 2) as usize;
        (0..n).map(move |i| {
            let at = 4usize.saturating_add(i.saturating_mul(8));
            Record {
                platform: u16_at(d, at).unwrap_or(0),
                encoding: u16_at(d, at.saturating_add(2)).unwrap_or(0),
                offset: u32a(d, at.saturating_add(4)),
            }
        })
    }

    /// The subtable of a record; a subtable HarfBuzz's sanitizer rejects is
    /// read as the null subtable.
    pub fn subtable(&self, r: &Record) -> Subtable<'a> {
        let off = r.offset as usize;
        match self.data.get(off..) {
            Some(d) if sane(d) => Subtable {
                data: d,
                size: d.len(),
            },
            _ => Subtable { data: &[], size: 0 },
        }
    }

    /// `find_subtable`: binary search of the sorted records; an encoding
    /// of 0xFFFF matches any.
    pub fn find(&self, platform: u16, encoding: u16) -> Option<Subtable<'a>> {
        let recs: alloc::vec::Vec<Record> = self.records().collect();
        let cmp = |r: &Record| -> core::cmp::Ordering {
            r.platform.cmp(&platform).then(if encoding == 0xFFFF {
                core::cmp::Ordering::Equal
            } else {
                r.encoding.cmp(&encoding)
            })
        };
        let mut lo: i64 = 0;
        let mut hi: i64 = i64::try_from(recs.len()).unwrap_or(0).saturating_sub(1);
        while lo <= hi {
            let mid = (lo.saturating_add(hi)) / 2;
            let r = recs.get(usize::try_from(mid).unwrap_or(0))?;
            match cmp(r) {
                core::cmp::Ordering::Greater => hi = mid.saturating_sub(1),
                core::cmp::Ordering::Less => lo = mid.saturating_add(1),
                core::cmp::Ordering::Equal => {
                    return (r.offset != 0).then(|| self.subtable(r));
                }
            }
        }
        None
    }

    /// `find_best_subtable`, with its symbol, mac and macroman flags.
    pub fn best(&self) -> (Subtable<'a>, bool, bool, bool) {
        if let Some(s) = self.find(3, 0) {
            return (s, true, false, false);
        }
        for (p, e) in [
            (3, 10),
            (0, 6),
            (0, 4),
            (3, 1),
            (0, 3),
            (0, 2),
            (0, 1),
            (0, 0),
        ] {
            if let Some(s) = self.find(p, e) {
                return (s, false, false, false);
            }
        }
        if let Some(s) = self.find(1, 0) {
            return (s, false, true, true);
        }
        if let Some(s) = self.find(1, 0xFFFF) {
            return (s, false, true, false);
        }
        (Subtable { data: &[], size: 0 }, false, false, false)
    }

    /// `find_variation_selector_subtable`: the last (0, 5) format 14, or
    /// the first format 15.
    pub fn uvs(&self) -> Option<Subtable<'a>> {
        let mut f14 = None;
        for r in self
            .records()
            .filter(|r| r.platform == 0 && r.encoding == 5)
        {
            let s = self.subtable(&r);
            match s.format() {
                15 => return Some(s),
                14 => f14 = Some(s),
                _ => {}
            }
        }
        f14
    }
}

/// HarfBuzz's subtable sanitizer, per format.
fn sane(d: &[u8]) -> bool {
    let Some(format) = u16_at(d, 0) else {
        return false;
    };
    let fits = |n: u64| (d.len() as u64) >= n;
    match format {
        0 => fits(262),
        4 => u16_at(d, 6)
            .is_some_and(|x2| fits(u64::from(x2).saturating_mul(4).saturating_add(14 + 2))),
        6 => u16_at(d, 8).is_some_and(|n| fits(u64::from(n).saturating_mul(2).saturating_add(10))),
        10 => {
            u32_at(d, 16).is_some_and(|n| fits(u64::from(n).saturating_mul(2).saturating_add(20)))
        }
        12 | 13 => {
            u32_at(d, 12).is_some_and(|n| fits(u64::from(n).saturating_mul(12).saturating_add(16)))
        }
        14 | 15 => {
            u32_at(d, 6).is_some_and(|n| fits(u64::from(n).saturating_mul(11).saturating_add(10)))
        }
        _ => true,
    }
}

impl<'a> Subtable<'a> {
    pub fn format(&self) -> u16 {
        u16_at(self.data, 0).unwrap_or(0)
    }

    pub fn is_variation_selector(&self) -> bool {
        matches!(self.format(), 14 | 15)
    }

    pub fn language(&self) -> u32 {
        match self.format() {
            0 | 4 | 6 => u16a(self.data, 4),
            10 | 12 | 13 => u32a(self.data, 8),
            _ => 0,
        }
    }

    /// `get_glyph` (with the data size for format 4).
    pub fn glyph(&self, cp: u32) -> Option<u32> {
        let d = self.data;
        let g = match self.format() {
            0 => (cp < 256)
                .then(|| u8_at(d, 6usize.saturating_add(cp as usize)))
                .flatten()
                .map_or(0, u32::from),
            4 => return F4::new(self).glyph(cp),
            6 => {
                let first = u16a(d, 6);
                let n = u16a(d, 8);
                let i = cp.wrapping_sub(first);
                if i < n {
                    u16a(d, 10usize.saturating_add(2 * i as usize))
                } else {
                    0
                }
            }
            10 => {
                let first = u32a(d, 12);
                let n = u32a(d, 16);
                let i = cp.wrapping_sub(first);
                if i < n {
                    u16a(d, 20usize.saturating_add(2usize.saturating_mul(i as usize)))
                } else {
                    0
                }
            }
            f @ (12 | 13) => {
                let n = u32a(d, 12) as usize;
                let gr = |i: usize| -> (u32, u32, u32) {
                    let at = 16usize.saturating_add(i.saturating_mul(12));
                    (
                        u32a(d, at),
                        u32a(d, at.saturating_add(4)),
                        u32a(d, at.saturating_add(8)),
                    )
                };
                match bsearch(n, |i| {
                    let (s, e, _) = gr(i);
                    if cp < s {
                        core::cmp::Ordering::Greater
                    } else if cp > e {
                        core::cmp::Ordering::Less
                    } else {
                        core::cmp::Ordering::Equal
                    }
                }) {
                    Some(i) => {
                        let (s, e, g) = gr(i);
                        if s > e {
                            0
                        } else if f == 12 {
                            g.wrapping_add(cp.wrapping_sub(s))
                        } else {
                            g
                        }
                    }
                    None => 0,
                }
            }
            _ => 0,
        };
        (g != 0).then_some(g)
    }

    /// `collect_unicodes` (`num_glyphs` for formats 12 and 13).
    pub fn collect_unicodes(&self, out: &mut RangeSet, num_glyphs: u32) {
        let d = self.data;
        match self.format() {
            0 => (0..256u32)
                .filter(|&i| u8_at(d, 6 + i as usize).is_some_and(|g| g != 0))
                .for_each(|i| out.add(i)),
            4 => F4::new(self).collect_unicodes(out),
            6 | 10 => {
                let (first, n, at) = if self.format() == 6 {
                    (u16a(d, 6), u16a(d, 8), 10usize)
                } else {
                    (u32a(d, 12), u32a(d, 16), 20usize)
                };
                (0..n)
                    .filter(|&i| u16a(d, at.saturating_add(2usize.saturating_mul(i as usize))) != 0)
                    .for_each(|i| out.add(first.wrapping_add(i)));
            }
            f @ (12 | 13) => {
                let n = u32a(d, 12) as usize;
                for i in 0..n {
                    let at = 16usize.saturating_add(i.saturating_mul(12));
                    let mut start = u32a(d, at);
                    let mut end = u32a(d, at.saturating_add(4)).min(UNICODE_MAX);
                    let mut gid = u32a(d, at.saturating_add(8));
                    if gid == 0 {
                        // group_get_glyph (group, end) for format 13 is the
                        // group's glyph, 0 here.
                        let at_end = if f == 12 && start <= u32a(d, at.saturating_add(4)) {
                            gid.wrapping_add(end.wrapping_sub(start))
                        } else {
                            0
                        };
                        if at_end == 0 {
                            continue;
                        }
                        start = start.wrapping_add(1);
                        gid = gid.wrapping_add(1);
                    }
                    if gid >= num_glyphs {
                        continue;
                    }
                    if gid.wrapping_add(end).wrapping_sub(start) >= num_glyphs {
                        end = start.wrapping_add(num_glyphs).wrapping_sub(gid);
                    }
                    out.add_range(start, end.min(0x10FFFF));
                }
            }
            _ => {}
        }
    }

    /// `collect_mapping` (`num_glyphs` for formats 12 and 13).
    pub fn collect_mapping(
        &self,
        unicodes: &mut RangeSet,
        mapping: &mut BTreeMap<u32, u32>,
        num_glyphs: u32,
    ) {
        let d = self.data;
        match self.format() {
            0 => (0..256u32).for_each(|i| {
                if let Some(g) = u8_at(d, 6 + i as usize).filter(|&g| g != 0) {
                    unicodes.add(i);
                    mapping.insert(i, u32::from(g));
                }
            }),
            4 => F4::new(self).collect_mapping(unicodes, mapping),
            6 | 10 => {
                let (first, n, at) = if self.format() == 6 {
                    (u16a(d, 6), u16a(d, 8), 10usize)
                } else {
                    (u32a(d, 12), u32a(d, 16), 20usize)
                };
                for i in 0..n {
                    let g = u16a(d, at.saturating_add(2usize.saturating_mul(i as usize)));
                    if g != 0 {
                        let u = first.wrapping_add(i);
                        unicodes.add(u);
                        mapping.insert(u, g);
                    }
                }
            }
            f @ (12 | 13) => {
                let n = u32a(d, 12) as usize;
                let mut last_end = 0u32;
                for i in 0..n {
                    let at = 16usize.saturating_add(i.saturating_mul(12));
                    let mut start = u32a(d, at);
                    let mut end = u32a(d, at.saturating_add(4)).min(UNICODE_MAX);
                    if start > end || start < last_end {
                        continue;
                    }
                    last_end = end;
                    let mut gid = u32a(d, at.saturating_add(8));
                    if gid == 0 {
                        if f == 13 {
                            continue;
                        }
                        start = start.wrapping_add(1);
                        gid = gid.wrapping_add(1);
                    }
                    if gid >= num_glyphs {
                        continue;
                    }
                    if gid.wrapping_add(end).wrapping_sub(start) >= num_glyphs {
                        end = start.wrapping_add(num_glyphs).wrapping_sub(gid);
                    }
                    unicodes.add_range(start, end);
                    let mut cp = start;
                    while cp <= end {
                        mapping.insert(cp, gid);
                        if f == 12 {
                            gid = gid.wrapping_add(1);
                        }
                        if cp == u32::MAX {
                            break;
                        }
                        cp = cp.wrapping_add(1);
                    }
                }
            }
            _ => {}
        }
    }

    /// The variation selector records (formats 14 and 15): (selector,
    /// default UVS offset, non-default UVS offset, where the record is).
    pub fn uvs_records(&self) -> impl Iterator<Item = (u32, u32, u32, usize)> + 'a {
        let d = self.data;
        let n = if self.is_variation_selector() {
            u32a(d, 6) as usize
        } else {
            0
        };
        (0..n).map(move |i| {
            let at = 10usize.saturating_add(i.saturating_mul(11));
            (
                u24_at(d, at).unwrap_or(0),
                u32a(d, at.saturating_add(3)),
                u32a(d, at.saturating_add(7)),
                at,
            )
        })
    }

    /// The glyph id width of a format 14 (2) or 15 (3) subtable.
    pub fn uvs_glyph_size(&self) -> usize {
        if self.format() == 15 { 3 } else { 2 }
    }

    /// The (unicode, glyph) mappings of a non-default UVS table at `off`.
    pub fn non_default(&self, off: u32) -> impl Iterator<Item = (u32, u32)> + 'a {
        let d = self.data;
        let w = self.uvs_glyph_size();
        let base = off as usize;
        let n = if off == 0 { 0 } else { u32a(d, base) as usize };
        let rec = 3usize.saturating_add(w);
        let n = n.min(
            d.len()
                .saturating_sub(base.saturating_add(4))
                .checked_div(rec)
                .unwrap_or(0),
        );
        (0..n).map(move |i| {
            let at = base.saturating_add(4).saturating_add(i.saturating_mul(rec));
            let g = if w == 3 {
                u24_at(d, at.saturating_add(3)).unwrap_or(0)
            } else {
                u16a(d, at.saturating_add(3))
            };
            (u24_at(d, at).unwrap_or(0), g)
        })
    }

    /// The (start, additional count) ranges of a default UVS table at `off`.
    pub fn default_ranges(&self, off: u32) -> impl Iterator<Item = (u32, u32)> + 'a {
        let d = self.data;
        let base = off as usize;
        let n = if off == 0 { 0 } else { u32a(d, base) as usize };
        let n = n.min(d.len().saturating_sub(base.saturating_add(4)) / 4);
        (0..n).map(move |i| {
            let at = base.saturating_add(4).saturating_add(i.saturating_mul(4));
            (
                u24_at(d, at).unwrap_or(0),
                u8_at(d, at.saturating_add(3)).map_or(0, u32::from),
            )
        })
    }
}

/// `hb_bsearch` over `n` items: `cmp(i)` is the item's order relative to
/// the key.
pub(crate) fn bsearch(n: usize, cmp: impl Fn(usize) -> core::cmp::Ordering) -> Option<usize> {
    let mut lo: i64 = 0;
    let mut hi: i64 = i64::try_from(n).unwrap_or(0).saturating_sub(1);
    while lo <= hi {
        let mid = (lo.saturating_add(hi)) / 2;
        let m = usize::try_from(mid).ok()?;
        match cmp(m) {
            core::cmp::Ordering::Greater => hi = mid.saturating_sub(1),
            core::cmp::Ordering::Less => lo = mid.saturating_add(1),
            core::cmp::Ordering::Equal => return Some(m),
        }
    }
    None
}

/// `CmapSubtableFormat4::accelerator_t`.
struct F4<'a> {
    d: &'a [u8],
    seg_count: usize,
    glyph_id_array_len: usize,
}

impl<'a> F4<'a> {
    fn new(s: &Subtable<'a>) -> F4<'a> {
        let seg_count = (u16a(s.data, 6) / 2) as usize;
        let values_offset = 16usize.saturating_add(8usize.saturating_mul(seg_count));
        F4 {
            d: s.data,
            seg_count,
            glyph_id_array_len: if s.size > values_offset {
                s.size.saturating_sub(values_offset) / 2
            } else {
                0
            },
        }
    }
    fn end(&self, i: usize) -> u32 {
        u16a(self.d, 14usize.saturating_add(i.saturating_mul(2)))
    }
    fn start(&self, i: usize) -> u32 {
        u16a(
            self.d,
            16usize.saturating_add(self.seg_count.saturating_add(i).saturating_mul(2)),
        )
    }
    fn delta(&self, i: usize) -> u32 {
        u16a(
            self.d,
            16usize.saturating_add(
                self.seg_count
                    .saturating_mul(2)
                    .saturating_add(i)
                    .saturating_mul(2),
            ),
        )
    }
    fn range_offset(&self, i: usize) -> u32 {
        u16a(
            self.d,
            16usize.saturating_add(
                self.seg_count
                    .saturating_mul(3)
                    .saturating_add(i)
                    .saturating_mul(2),
            ),
        )
    }
    fn glyph_id(&self, index: usize) -> u32 {
        u16a(
            self.d,
            16usize.saturating_add(
                self.seg_count
                    .saturating_mul(4)
                    .saturating_add(index)
                    .saturating_mul(2),
            ),
        )
    }

    fn glyph(&self, cp: u32) -> Option<u32> {
        if self.d.is_empty() {
            return None;
        }
        let i = bsearch(self.seg_count, |i| {
            if cp > self.end(i) {
                core::cmp::Ordering::Less
            } else if cp < self.start(i) {
                core::cmp::Ordering::Greater
            } else {
                core::cmp::Ordering::Equal
            }
        })?;
        let ro = self.range_offset(i);
        let gid = if ro == 0 {
            cp.wrapping_add(self.delta(i))
        } else {
            let index = (ro / 2)
                .wrapping_add(cp.wrapping_sub(self.start(i)))
                .wrapping_add(c_uint(i))
                .wrapping_sub(c_uint(self.seg_count)) as usize;
            if index >= self.glyph_id_array_len {
                return None;
            }
            let g = self.glyph_id(index);
            if g == 0 {
                return None;
            }
            g.wrapping_add(self.delta(i))
        } & 0xFFFF;
        (gid != 0).then_some(gid)
    }

    fn count(&self) -> usize {
        let n = self.seg_count;
        if n > 0 && self.start(n.saturating_sub(1)) == 0xFFFF {
            n.saturating_sub(1)
        } else {
            n
        }
    }

    fn collect_unicodes(&self, out: &mut RangeSet) {
        for i in 0..self.count() {
            let (start, end, ro) = (self.start(i), self.end(i), self.range_offset(i));
            out.add_range(start, end);
            for cp in start..=end {
                if ro == 0 {
                    if cp.wrapping_add(self.delta(i)) & 0xFFFF == 0 {
                        out.remove(cp);
                    }
                } else {
                    let index = (ro / 2)
                        .wrapping_add(cp.wrapping_sub(start))
                        .wrapping_add(c_uint(i))
                        .wrapping_sub(c_uint(self.seg_count))
                        as usize;
                    if index >= self.glyph_id_array_len {
                        out.remove_range(cp, end);
                        break;
                    }
                    if self.glyph_id(index) == 0 {
                        out.remove(cp);
                    }
                }
            }
        }
    }

    fn collect_mapping(&self, unicodes: &mut RangeSet, mapping: &mut BTreeMap<u32, u32>) {
        for i in 0..self.count() {
            let (start, end, ro) = (self.start(i), self.end(i), self.range_offset(i));
            for cp in start..=end {
                let gid = if ro == 0 {
                    cp.wrapping_add(self.delta(i)) & 0xFFFF
                } else {
                    let index = (ro / 2)
                        .wrapping_add(cp.wrapping_sub(start))
                        .wrapping_add(c_uint(i))
                        .wrapping_sub(c_uint(self.seg_count))
                        as usize;
                    if index >= self.glyph_id_array_len {
                        break;
                    }
                    let g = self.glyph_id(index);
                    if g == 0 {
                        continue;
                    }
                    g.wrapping_add(self.delta(i)) & 0xFFFF
                };
                if gid == 0 {
                    continue;
                }
                unicodes.add(cp);
                mapping.insert(cp, gid);
            }
        }
    }
}

/// How the best subtable is looked up (`cmap::accelerator_t`'s getter).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Getter {
    Plain,
    Symbol(u8),
    MacRoman,
    Ascii,
}

/// `cmap::accelerator_t` of one table.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Accel<'a> {
    pub table: Option<CmapTable<'a>>,
    subtable: Subtable<'a>,
    uvs: Option<Subtable<'a>>,
    getter: Getter,
}

impl<'a> Accel<'a> {
    pub fn new(data: Option<&'a [u8]>, font_page: u16) -> Accel<'a> {
        let table = data.filter(|d| !d.is_empty()).and_then(CmapTable::new);
        let empty = Subtable { data: &[], size: 0 };
        let Some(t) = table else {
            return Accel {
                table: None,
                subtable: empty,
                uvs: None,
                getter: Getter::Plain,
            };
        };
        let (subtable, symbol, mac, macroman) = t.best();
        let getter = if symbol {
            match font_page {
                0 => Getter::Symbol(0),
                0xB200 => Getter::Symbol(1),
                0xB300 => Getter::Symbol(2),
                _ => Getter::Plain,
            }
        } else if macroman {
            Getter::MacRoman
        } else if mac {
            Getter::Ascii
        } else {
            Getter::Plain
        };
        Accel {
            table,
            subtable,
            uvs: t.uvs(),
            getter,
        }
    }

    pub fn has_data(&self) -> bool {
        self.table.is_some()
    }

    pub fn nominal_glyph(&self, cp: u32) -> Option<u32> {
        self.table?;
        let s = &self.subtable;
        match self.getter {
            Getter::Plain => s.glyph(cp),
            Getter::Symbol(page) => s.glyph(cp).or_else(|| {
                let m = match page {
                    0 => {
                        if cp <= 0xFF {
                            cp.wrapping_add(0xF000)
                        } else {
                            0
                        }
                    }
                    1 => remap::arabic_pua_simp_map(cp),
                    _ => remap::arabic_pua_trad_map(cp),
                };
                (m != 0).then(|| s.glyph(m)).flatten()
            }),
            Getter::Ascii => (cp < 0x80).then(|| s.glyph(cp)).flatten(),
            Getter::MacRoman => (cp < 0x80).then(|| s.glyph(cp)).flatten().or_else(|| {
                remap::unicode_to_macroman(cp)
                    .filter(|&m| m != 0)
                    .and_then(|m| s.glyph(m))
            }),
        }
    }

    pub fn collect_mapping(&self, unicodes: &mut RangeSet, mapping: &mut BTreeMap<u32, u32>) {
        if self.table.is_some() {
            self.subtable.collect_mapping(unicodes, mapping, u32::MAX);
        }
    }

    pub fn collect_variation_selectors(&self, out: &mut RangeSet) {
        if let Some(u) = &self.uvs {
            u.uvs_records().for_each(|r| out.add(r.0));
        }
    }

    /// `cmap::closure_glyphs`: the non-default UVS glyphs of every variation
    /// selector subtable for the selectors in `unicodes`.
    pub fn closure_glyphs(&self, unicodes: &RangeSet, glyphset: &mut RangeSet) {
        let Some(t) = self.table else {
            return;
        };
        for r in t.records() {
            let s = t.subtable(&r);
            if !s.is_variation_selector() {
                continue;
            }
            for (sel, _, nd, _) in s.uvs_records() {
                if nd == 0 || !unicodes.contains(sel) {
                    continue;
                }
                s.non_default(nd)
                    .filter(|(u, _)| unicodes.contains(*u))
                    .for_each(|(_, g)| glyphset.add(g));
            }
        }
    }
}

/// `cmap_accelerator_t`: DMAP first, then cmap.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CmapAccel<'a> {
    pub dmap: Accel<'a>,
    pub base: Accel<'a>,
}

impl<'a> CmapAccel<'a> {
    pub fn new(font: &Font<'a>) -> CmapAccel<'a> {
        let page = font_page(font);
        CmapAccel {
            dmap: Accel::new(font.table(Tag::new(b"DMAP")), page),
            base: Accel::new(font.table(Tag::new(b"cmap")), page),
        }
    }

    pub fn nominal_glyph(&self, cp: u32) -> Option<u32> {
        self.dmap
            .nominal_glyph(cp)
            .or_else(|| self.base.nominal_glyph(cp))
    }

    pub fn collect_mapping(&self, unicodes: &mut RangeSet, mapping: &mut BTreeMap<u32, u32>) {
        self.base.collect_mapping(unicodes, mapping);
        self.dmap.collect_mapping(unicodes, mapping);
    }

    pub fn collect_variation_selectors(&self, out: &mut RangeSet) {
        self.base.collect_variation_selectors(out);
        self.dmap.collect_variation_selectors(out);
    }

    pub fn closure_glyphs(&self, unicodes: &RangeSet, glyphset: &mut RangeSet) {
        if !self.dmap.has_data() {
            self.base.closure_glyphs(unicodes, glyphset);
            return;
        }
        // With DMAP, HarfBuzz closes over the variation selectors of both
        // tables: each base character's variation glyph.
        let mut selectors = RangeSet::new();
        self.collect_variation_selectors(&mut selectors);
        for sel in selectors.iter().filter(|s| unicodes.contains(*s)) {
            let mut bases = RangeSet::new();
            self.collect_variation_unicodes(sel, &mut bases);
            for u in bases.iter().filter(|u| unicodes.contains(*u)) {
                if let Some(g) = self.variation_glyph(u, sel) {
                    glyphset.add(g);
                }
            }
        }
    }

    fn collect_variation_unicodes(&self, sel: u32, out: &mut RangeSet) {
        for a in [&self.base, &self.dmap] {
            if let Some(u) = &a.uvs
                && let Some((_, def, nd, _)) = u.uvs_records().find(|r| r.0 == sel)
            {
                u.default_ranges(def)
                    .for_each(|(s, c)| out.add_range(s, s.saturating_add(c).min(UNICODE_MAX)));
                u.non_default(nd).for_each(|(c, _)| out.add(c));
            }
        }
    }

    fn variant(a: &Accel<'_>, cp: u32, sel: u32) -> Option<Option<u32>> {
        let u = a.uvs.as_ref()?;
        let (_, def, nd, _) = u.uvs_records().find(|r| r.0 == sel)?;
        if u.default_ranges(def)
            .any(|(s, c)| cp >= s && cp <= s.saturating_add(c))
        {
            return Some(None);
        }
        u.non_default(nd)
            .find(|(c, _)| *c == cp)
            .filter(|(_, g)| *g != 0)
            .map(|(_, g)| Some(g))
    }

    /// `get_variation_glyph`.
    pub fn variation_glyph(&self, cp: u32, sel: u32) -> Option<u32> {
        match CmapAccel::variant(&self.dmap, cp, sel)
            .or_else(|| CmapAccel::variant(&self.base, cp, sel))
        {
            Some(Some(g)) => Some(g),
            Some(None) => self.nominal_glyph(cp),
            None => None,
        }
    }
}

/// OS/2's font page (`get_font_page`): the high byte of `fsSelection` for
/// version 0, else 0.
fn font_page(font: &Font<'_>) -> u16 {
    font.table(Tag::new(b"OS/2"))
        .filter(|d| d.len() >= 78)
        .and_then(|d| (u16_at(d, 0)? == 0).then(|| u16_at(d, 62)).flatten())
        .map_or(0, |f| f & 0xFF00)
}
