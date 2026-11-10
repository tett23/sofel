//! CBLC and CBDT (`src/OT/Color/CBDT/CBDT.hh`): the strikes of the kept
//! glyphs, with index subtables of formats 1 and 3, the image data copied
//! into a new CBDT (identical blocks shared, at most
//! `HB_SUBSET_CBDT_MAX_COPY_FACTOR` times the source table copied).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::cblc::Cblc;
use sofel_write::Serializer;

use super::Tables;
use crate::error::SubsetError;
use crate::layout::data::{D, n32};
use crate::limits;
use crate::plan::Plan;
use crate::set::{INVALID, RangeSet};

/// `cbdt_dedup_context_t`: data blocks already in the new CBDT, keyed by
/// image format, data and per-glyph lengths, and the copy budget.
struct Dedup {
    map: BTreeMap<(u16, Vec<u8>, Vec<u32>), usize>,
    budget: i64,
}

/// The state shared by the strikes (`cblc_bitmap_size_subset_context_t`
/// and the new CBDT).
struct Ctx<'a, 'p> {
    p: &'p Plan,
    cblc: D<'a>,
    cbdt: &'a [u8],
    prime: Vec<u8>,
    dedup: Dedup,
    size: u32,
    num_tables: u32,
    start_glyph: u32,
    end_glyph: u32,
}

/// `IndexSubtable::get_image_data` of the subtable at `sub`: the data
/// offset in CBDT and its length (formats 1 and 3).
fn image_data(sub: D<'_>, idx: u32) -> Option<(u32, u32)> {
    let (a, b) = match sub.u16(0) {
        1 => {
            let at = 8usize.saturating_add((idx as usize).saturating_mul(4));
            (sub.u32(at), sub.u32(at.saturating_add(4)))
        }
        3 => {
            let at = 8usize.saturating_add((idx as usize).saturating_mul(2));
            (
                u32::from(sub.u16(at)),
                u32::from(sub.u16(at.saturating_add(2))),
            )
        }
        _ => return None,
    };
    if b <= a {
        return None;
    }
    Some((sub.u32(4).wrapping_add(a), b.wrapping_sub(a)))
}

/// An IndexSubtableRecord of the array at `array`: first and last glyph
/// and the subtable's offset from the array.
fn record(array: D<'_>, i: usize) -> (u32, u32, u32) {
    let at = i.saturating_mul(8);
    (
        u32::from(array.u16(at)),
        u32::from(array.u16(at.saturating_add(2))),
        array.u32(at.saturating_add(4)),
    )
}

impl Ctx<'_, '_> {
    /// `IndexSubtableArray::build_lookup`: the kept glyphs with image
    /// data as `(new glyph, record index)`.
    fn build_lookup(&mut self, array: D<'_>) -> Vec<(u32, usize)> {
        let mut glyphs: RangeSet = self.p.glyphset.clone();
        let mut record_map: BTreeMap<u32, usize> = BTreeMap::new();
        for i in 0..self.num_tables as usize {
            let (first, last, _) = record(array, i);
            let mut g = first.wrapping_sub(1);
            loop {
                g = glyphs.next(g);
                if g == INVALID || g > last {
                    break;
                }
                record_map.insert(g, i);
            }
            if first <= last {
                glyphs.remove_range(first, last);
            }
        }
        let mut lookup = Vec::new();
        let mut start_set = false;
        for new in 0..self.p.num_output_glyphs {
            let Some(old) = self.p.glyphs.old_gid(new) else {
                continue;
            };
            let Some(&i) = record_map.get(&old) else {
                continue;
            };
            let (first, last, off) = record(array, i);
            if old < first || old > last {
                continue;
            }
            if image_data(array.at(off as usize), old.saturating_sub(first)).is_none() {
                continue;
            }
            lookup.push((new, i));
            if !start_set {
                self.start_glyph = new;
                start_set = true;
            }
            self.end_glyph = new;
        }
        lookup
    }

    /// `IndexSubtableRecord::add_new_subtable` into the pushed object.
    fn add_new_subtable(
        &mut self,
        s: &mut Serializer,
        array: D<'_>,
        lookup: &[(u32, usize)],
        rec: &mut (u32, u32),
        this: usize,
        start: &mut usize,
    ) -> bool {
        s.zeros(8);
        let (_, _, this_off) = record(array, this);
        let old_sub = array.at(this_off as usize);
        let image_format = old_sub.u16(2);
        let mut data: Vec<u8> = Vec::new();
        let mut lengths: Vec<u32> = Vec::new();
        let mut num_glyphs = 0u32;
        let mut early = false;
        for (i, &(new, ri)) in lookup.iter().enumerate().skip(*start) {
            let (first, _, off) = record(array, ri);
            if off != this_off {
                *start = i;
                early = true;
                break;
            }
            // add_glyph_for_subset
            let missing = if rec.0 > rec.1 {
                *rec = (new, new);
                0
            } else if rec.1 > new {
                0
            } else {
                let m = new.wrapping_sub(rec.1).wrapping_sub(1);
                rec.1 = new;
                m
            };
            lengths.extend(core::iter::repeat_n(0, missing as usize));
            let old = self.p.glyphs.old_gid(new).unwrap_or(0);
            if old < first {
                return false;
            }
            let Some((offset, length)) =
                image_data(array.at(off as usize), old.wrapping_sub(first))
            else {
                return false;
            };
            let cbdt_len = n32(self.cbdt.len());
            if offset > cbdt_len || cbdt_len.wrapping_sub(offset) < length {
                return false;
            }
            self.dedup.budget = self.dedup.budget.saturating_sub(i64::from(length));
            if self.dedup.budget < 0 {
                return false;
            }
            let block = self
                .cbdt
                .get(offset as usize..offset.saturating_add(length) as usize)
                .unwrap_or(&[]);
            data.extend_from_slice(block);
            lengths.push(length);
            num_glyphs = num_glyphs.saturating_add(1).saturating_add(missing);
        }
        if !early {
            *start = lookup.len();
        }
        let key = (image_format, data, lengths);
        let shared = if key.1.is_empty() {
            None
        } else {
            self.dedup.map.get(&key).copied().filter(|&base| {
                self.prime.get(base..base.saturating_add(key.1.len())) == Some(key.1.as_slice())
            })
        };
        let base = match shared {
            Some(b) => b,
            None => {
                let b = self.prime.len();
                self.prime.extend_from_slice(&key.1);
                if !key.1.is_empty() {
                    self.dedup.map.insert(key.clone(), b);
                }
                b
            }
        };
        let index_format = old_sub.u16(0);
        s.set_u16(0, index_format);
        s.set_u16(2, image_format);
        s.set_u32(4, n32(base));
        if matches!(index_format, 1 | 3) {
            self.size = self.size.wrapping_add(8);
        }
        let add_offset = |s: &mut Serializer, size: &mut u32, v: u32| match index_format {
            1 => {
                s.u32(v);
                *size = size.wrapping_add(4);
                true
            }
            3 => {
                s.u16((v & 0xFFFF) as u16);
                *size = size.wrapping_add(2);
                true
            }
            _ => false,
        };
        let mut cumulative = 0u32;
        for &length in &key.2 {
            if !add_offset(s, &mut self.size, cumulative) {
                return false;
            }
            cumulative = cumulative.wrapping_add(length);
        }
        // finish_subtable
        if !add_offset(s, &mut self.size, cumulative) {
            return false;
        }
        if index_format == 3 && num_glyphs & 1 == 0 {
            return add_offset(s, &mut self.size, 0);
        }
        true
    }

    /// `IndexSubtableArray::subset` into the pushed object.
    fn index_subtable_array(&mut self, s: &mut Serializer, array_at: usize) -> bool {
        let array = self.cblc.at(array_at);
        let lookup = self.build_lookup(array);
        self.size = 0;
        self.num_tables = 0;
        let mut records: Vec<(u32, u32)> = Vec::new();
        let mut start = 0usize;
        while start < lookup.len() {
            let this = lookup.get(start).map_or(0, |l| l.1);
            // add_new_record
            let snap = s.snapshot();
            let old_size = self.size;
            let old_prime = self.prime.len();
            let mut rec = (1u32, 0u32);
            self.size = self.size.wrapping_add(8);
            s.push();
            if self.add_new_subtable(s, array, &lookup, &mut rec, this, &mut start) {
                records.push(rec);
                self.num_tables = self.num_tables.wrapping_add(1);
            } else {
                s.pop_discard();
                s.revert(snap);
                self.prime.truncate(old_prime);
                self.size = old_size;
                records.iter().for_each(|_| s.pop_discard());
                return false;
            }
        }
        let objidxs: Vec<u32> = records.iter().map(|_| s.pop_pack(true)).collect();
        for (i, &(first, last)) in records.iter().enumerate() {
            let at = s.u16((first & 0xFFFF) as u16);
            s.u16((last & 0xFFFF) as u16);
            s.u32(0);
            let idx = objidxs
                .get(records.len().saturating_sub(1).saturating_sub(i))
                .copied()
                .unwrap_or(0);
            s.link32(at.saturating_add(4), idx);
        }
        true
    }

    /// `BitmapSizeTable::subset` of the size table at `at` into the CBLC
    /// object.
    fn size_table(&mut self, s: &mut Serializer, at: usize) -> bool {
        let table = self.cblc.at(at);
        let out = s.extend(&(0..48).map(|i| table.u8(i)).collect::<Vec<u8>>());
        self.size = table.u32(4);
        self.num_tables = table.u32(8);
        self.start_glyph = 1;
        self.end_glyph = 0;
        // NNOffset32To::serialize_subset: packed and linked even when it
        // fails.
        s.set_u32(out, 0);
        s.push();
        let ret = self.index_subtable_array(s, table.u32(0) as usize);
        let idx = s.pop_pack(true);
        s.link32(out, idx);
        if !ret || self.size == 0 || self.num_tables == 0 || self.start_glyph > self.end_glyph {
            return false;
        }
        s.set_u32(out.saturating_add(4), self.size);
        s.set_u32(out.saturating_add(8), self.num_tables);
        s.set_u16(out.saturating_add(40), (self.start_glyph & 0xFFFF) as u16);
        s.set_u16(out.saturating_add(42), (self.end_glyph & 0xFFFF) as u16);
        true
    }
}

/// `CBLC::subset`: writes CBLC and adds the new CBDT to `tables`.
pub(crate) fn subset(
    plan: &Plan,
    font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let ok = Cblc::new(src).is_ok_and(|c| matches!(c.major_version(), 2 | 3));
    if !ok {
        return Err(SubsetError::Failed { tag });
    }
    let t = D(src);
    s.u32(t.u32(0));
    s.u32(0);
    let cbdt = font
        .table(Tag::new(b"CBDT"))
        .filter(|d| d.len() >= 4 && matches!(D(d).u16(0), 2 | 3))
        .unwrap_or(&[]);
    if cbdt.len() < 4 {
        return Ok(false);
    }
    let budget = i64::try_from(cbdt.len())
        .unwrap_or(i64::MAX)
        .saturating_mul(limits::SUBSET_CBDT_MAX_COPY_FACTOR);
    let mut c = Ctx {
        p: plan,
        cblc: t,
        cbdt,
        prime: cbdt.get(..4).unwrap_or(&[]).to_vec(),
        dedup: Dedup {
            map: BTreeMap::new(),
            budget,
        },
        size: 0,
        num_tables: 0,
        start_glyph: 1,
        end_glyph: 0,
    };
    let mut count = 0u32;
    for i in 0..t.u32(4) as usize {
        count = count.wrapping_add(1);
        s.set_u32(4, count);
        let snap = s.snapshot();
        let prime_len = c.prime.len();
        if !c.size_table(s, 8usize.saturating_add(i.saturating_mul(48))) {
            count = count.wrapping_sub(1);
            s.set_u32(4, count);
            s.revert(snap);
            c.prime.truncate(prime_len);
        }
    }
    tables.insert(Tag::new(b"CBDT"), c.prime);
    Ok(true)
}
