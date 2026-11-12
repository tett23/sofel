//! post (`src/hb-ot-post-table.hh`, `src/hb-ot-post-table-v2subset.hh`).

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::be::u16_at;
use sofel_write::{Errors, Serializer};

use crate::error::SubsetError;
use crate::input::Flags;
use crate::plan::Plan;
use crate::tables::Tables;

/// The 258 standard Macintosh glyph names (`format1_names`).
pub(crate) const STANDARD_NAMES: [&str; 258] = include!("post_names.inc");

/// `post::accelerator_t`'s glyph names of a version 2 table.
pub(crate) struct PostNames<'a> {
    data: &'a [u8],
    num_indices: usize,
    offsets: Vec<usize>,
}

impl<'a> PostNames<'a> {
    pub fn new(data: &'a [u8]) -> Option<PostNames<'a>> {
        if data.len() < 34 || sofel_font::read::be::u32_at(data, 0)? != 0x0002_0000 {
            return None;
        }
        let num_indices = usize::from(u16_at(data, 32)?);
        let pool = num_indices.saturating_mul(2).saturating_add(34);
        if data.len() < pool {
            return None;
        }
        let mut offsets = Vec::new();
        let mut p = pool;
        while offsets.len() < 65535
            && p < data.len()
            && p.saturating_add(usize::from(data.get(p).copied().unwrap_or(0))) < data.len()
        {
            offsets.push(p.saturating_sub(pool));
            p = p
                .saturating_add(1)
                .saturating_add(usize::from(data.get(p).copied().unwrap_or(0)));
        }
        Some(PostNames {
            data,
            num_indices,
            offsets,
        })
    }

    /// The name index of a glyph (`glyphNameIndex[gid]`, 0 outside).
    pub fn index(&self, gid: u32) -> u16 {
        if (gid as usize) < self.num_indices {
            u16_at(
                self.data,
                (gid as usize).saturating_mul(2).saturating_add(34),
            )
            .unwrap_or(0)
        } else {
            0
        }
    }

    /// `find_glyph_name`.
    pub fn name(&self, gid: u32) -> &'a [u8] {
        if gid as usize >= self.num_indices {
            return &[];
        }
        let index = usize::from(self.index(gid));
        if let Some(n) = STANDARD_NAMES.get(index) {
            return n.as_bytes();
        }
        let Some(&off) = index.checked_sub(258).and_then(|i| self.offsets.get(i)) else {
            return &[];
        };
        let at = self
            .num_indices
            .saturating_mul(2)
            .saturating_add(34)
            .saturating_add(off);
        let len = usize::from(self.data.get(at).copied().unwrap_or(0));
        self.data
            .get(at.saturating_add(1)..at.saturating_add(1).saturating_add(len))
            .unwrap_or(&[])
    }
}

/// `post::subset`.
pub(crate) fn subset(
    plan: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    if src.len() < 32 {
        return Err(SubsetError::Malformed { tag });
    }
    let glyph_names = plan.flags.has(Flags::GLYPH_NAMES);
    s.extend(src.get(..32).unwrap_or(&[]));
    if !glyph_names {
        s.set_u16(0, 3);
    }
    if let Some(r) = plan.var.user().get(&Tag::new(b"slnt")) {
        // hb_max (-90.f, hb_min (middle, 90.f))
        let m = if r.default <= 90.0 { r.default } else { 90.0 };
        let angle = if -90.0 >= m { -90.0 } else { m };
        let current = sofel_font::read::be::i32_at(src, 4).unwrap_or(0);
        if crate::instance::fixed_to_f32(sofel_font::read::Fixed(current)) != angle {
            let v = crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(
                angle / (1.0 / 65536.0),
            ));
            s.set_u32(4, u32::from_be_bytes(v.to_be_bytes()));
        }
    }
    if !glyph_names {
        return Ok(true);
    }
    if u16_at(src, 0) != Some(2) {
        return Ok(true);
    }
    let names = PostNames::new(src).ok_or(SubsetError::Malformed { tag })?;
    // postV2Tail::subset
    let mut old_new_index: BTreeMap<u16, u32> = BTreeMap::new();
    let mut old_gid_new_index: BTreeMap<u32, u32> = BTreeMap::new();
    let mut name_to_new: BTreeMap<&[u8], u32> = BTreeMap::new();
    let mut i = 0u32;
    for &(_, old) in &plan.glyphs.new_to_old {
        let old_index = names.index(old);
        let new_index = if old_index <= 257 {
            u32::from(old_index)
        } else if let Some(&n) = old_new_index.get(&old_index) {
            n
        } else {
            let name = names.name(old);
            let n = match name_to_new.get(name) {
                Some(&n) => n,
                None => {
                    let n = match STANDARD_NAMES.iter().position(|s| s.as_bytes() == name) {
                        Some(std) => u32::try_from(std).unwrap_or(0),
                        None => {
                            i = i.saturating_add(1);
                            i.saturating_add(258 - 1)
                        }
                    };
                    name_to_new.insert(name, n);
                    n
                }
            };
            old_new_index.insert(old_index, n);
            n
        };
        old_gid_new_index.insert(old, new_index);
    }
    let pairs: Vec<(u32, u32)> = (0..plan.num_output_glyphs)
        .map(|new| match plan.glyphs.old_gid(new) {
            Some(old) => (old, old_gid_new_index.get(&old).copied().unwrap_or(0)),
            None => (new, 0),
        })
        .collect();
    let at = s.u16(0);
    if !s.check_assign_u16(at, pairs.len() as u64, Errors::INT_OVERFLOW) {
        return Ok(false);
    }
    pairs.iter().for_each(|&(_, idx)| {
        s.u16((idx & 0xFFFF) as u16);
    });
    let mut copied = BTreeSet::new();
    for &(gid, idx) in &pairs {
        if idx < 258 || !copied.insert(idx) {
            continue;
        }
        let name = names.name(gid);
        let len = name.len();
        if len > 255 {
            s.err(Errors::INT_OVERFLOW);
            return Ok(false);
        }
        s.u8(u8::try_from(len & 0xFF).unwrap_or(0));
        s.extend(name);
    }
    Ok(true)
}
