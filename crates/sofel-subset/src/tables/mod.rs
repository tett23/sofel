//! Writing the tables (`hb_subset_plan_execute_or_fail` and the tables'
//! `subset` functions).

pub(crate) mod avar;
mod avar2;
pub(crate) mod base;
mod cblc;
pub(crate) mod cff;
pub(crate) mod cmap;
pub(crate) mod colr;
pub(crate) mod cpal;
mod fvar;
pub(crate) mod glyf;
mod gvar;
mod hdmx;
pub(crate) mod hmtx;
pub(crate) mod hvar;
pub(crate) mod math;
mod maxp;
mod mvar;
mod name;
mod os2;
mod post;
mod sbix;
pub(crate) mod stat;
pub(crate) mod varc;
pub(crate) mod varc_complex;
mod variation_region;
mod vorg;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_write::{Errors, Serializer, WriteError};

use crate::error::SubsetError;
use crate::input::Flags;
use crate::layout::Gsubgpos;
use crate::layout::common::Sub;
use crate::plan::Plan;
use crate::room;

/// `hb_face_get_upem`.
pub(crate) fn hb_upem(font: &Font<'_>) -> u32 {
    u32::from(font.hb_units_per_em())
}

/// The tables of the subset font (`plan->add_table`; a later add replaces).
pub(crate) type Tables = BTreeMap<Tag, Vec<u8>>;

const fn t(b: &[u8; 4]) -> Tag {
    Tag::new(b)
}

impl Plan {
    /// `_should_drop_table`.
    pub(crate) fn should_drop(&self, tag: Tag) -> bool {
        if self.drop_tables.contains(tag.to_u32()) {
            return true;
        }
        let pinned = self.var.all_axes_pinned();
        match &tag.0 {
            b"cvar" => pinned || self.flags.has(Flags::NO_HINTING),
            b"cvt " | b"fpgm" | b"prep" | b"hdmx" | b"VDMX" => self.flags.has(Flags::NO_HINTING),
            b"gvar" | b"GVAR" => pinned || (self.var.instancing() && self.inert_gvar),
            b"avar"
                if self.var.n.avar2
                    && (self.var.all_axes_pinned()
                        || (self.var.n.old_intermediates.is_empty()
                            && !self.var.user().is_empty()
                            && self.var.user().values().all(|range| range.min == range.max))) =>
            {
                true
            }
            b"avar" | b"fvar" | b"HVAR" | b"VVAR" | b"MVAR" => pinned,
            _ => false,
        }
    }

    /// Writes the tables and assembles the font
    /// (`hb_subset_plan_execute_or_fail` and `hb_face_reference_blob`).
    ///
    /// # Errors
    ///
    /// [`SubsetError::Malformed`] for a table that cannot be read,
    /// [`SubsetError::Failed`] where HarfBuzz fails too,
    /// [`SubsetError::OffsetOverflow`] for offsets that do not fit and
    /// [`SubsetError::Unsupported`] for what is not subset yet.
    pub fn execute(&self, font: &Font<'_>) -> Result<Vec<u8>, SubsetError> {
        let tags: Vec<Tag> = font.sfnt().records().map(|r| r.tag).collect();
        let mut pending: BTreeSet<u32> = tags
            .iter()
            .filter(|&&tag| !self.should_drop(tag))
            .map(|t| t.to_u32())
            .collect();
        let mut tables = Tables::new();
        let mut room = room::initial();
        // Delay the tables whose dependencies are pending, as
        // `hb_subset_plan_execute_or_fail` does.
        let mut changed = true;
        while changed && !pending.is_empty() {
            changed = false;
            for tag in pending.clone() {
                if !self.dependencies_satisfied(Tag::from_u32(tag), &pending) {
                    continue;
                }
                pending.remove(&tag);
                changed = true;
                room = self.subset_table(font, Tag::from_u32(tag), room, &mut tables)?;
            }
        }
        if let Some(&tag) = pending.iter().next() {
            // "Table dependencies unable to be satisfied. Subset failed."
            return Err(SubsetError::Failed {
                tag: Tag::from_u32(tag),
            });
        }
        let list: Vec<(Tag, &[u8])> = tables.iter().map(|(t, d)| (*t, d.as_slice())).collect();
        sofel_write::build_sfnt(&list).map_err(|_| SubsetError::Failed { tag: t(b"sfnt") })
    }

    /// `_dependencies_satisfied`: with instancing, the metrics tables
    /// wait for the outlines; GSUB and GPOS wait for GDEF unless every
    /// axis is pinned.
    fn dependencies_satisfied(&self, tag: Tag, pending: &BTreeSet<u32>) -> bool {
        let has = |t: &[u8; 4]| pending.contains(&Tag::new(t).to_u32());
        match &tag.0 {
            b"hmtx" | b"vmtx" | b"maxp" | b"HMTX" | b"VMTX" | b"MAXP" | b"OS/2" => {
                !self.var.instancing() || !(has(b"glyf") || has(b"GLYF"))
            }
            b"GPOS" | b"GSUB" => self.var.all_axes_pinned() || !has(b"GDEF"),
            _ => true,
        }
    }

    fn passthrough(&self, font: &Font<'_>, tag: Tag, tables: &mut Tables) {
        tables.insert(tag, font.table(tag).unwrap_or(&[]).to_vec());
    }

    /// The room of HarfBuzz's buffer after subsetting `tag` from `src`
    /// with a serializer that held at most `peak` bytes;
    /// [`SubsetError::Limit`] when HarfBuzz's buffer cannot grow enough
    /// (see [`room`]).
    fn room_after(
        &self,
        font: &Font<'_>,
        tag: Tag,
        src: &[u8],
        room: u32,
        peak: usize,
    ) -> Result<u32, SubsetError> {
        let len = u32::try_from(src.len()).unwrap_or(u32::MAX);
        let kept = u32::try_from(self.glyph_set().len()).unwrap_or(u32::MAX);
        let estimate = room::estimate(self.flags, font.num_glyphs(), kept, len, tag);
        match room::fit(room, estimate, peak, len) {
            (room, true) => Ok(room),
            (_, false) => Err(SubsetError::Limit { tag }),
        }
    }

    /// `_subset_table`; returns the room of HarfBuzz's buffer after the
    /// table.
    fn subset_table(
        &self,
        font: &Font<'_>,
        tag: Tag,
        room: u32,
        tables: &mut Tables,
    ) -> Result<u32, SubsetError> {
        if self.no_subset_tables.contains(tag.to_u32()) {
            self.passthrough(font, tag, tables);
            return Ok(room);
        }
        type F = fn(
            &Plan,
            &Font<'_>,
            Tag,
            &[u8],
            &mut Serializer,
            &mut Tables,
        ) -> Result<bool, SubsetError>;
        let f: Option<F> = match &tag.0 {
            b"glyf" | b"GLYF" => Some(|p, f, tag, _, s, out| glyf::subset(p, f, tag, s, out)),
            b"gvar" | b"GVAR" => Some(gvar::subset),
            b"hdmx" => Some(hdmx::subset),
            b"HVAR" | b"VVAR" => Some(hvar::subset),
            b"name" => Some(name::subset),
            b"hmtx" | b"HMTX" | b"vmtx" | b"VMTX" => Some(hmtx::subset),
            b"maxp" | b"MAXP" => Some(maxp::subset),
            b"cmap" => Some(|p, f, _, d, s, out| cmap::subset(p, f, d, false, s, out)),
            b"DMAP" => Some(|p, f, _, d, s, out| cmap::subset(p, f, d, true, s, out)),
            b"OS/2" => Some(os2::subset),
            b"post" => Some(post::subset),
            b"GDEF" => Some(gdef),
            b"GSUB" | b"GPOS" => Some(gsubgpos),
            b"BASE" => Some(base::subset),
            b"MATH" => Some(math::subset),
            b"CFF " => Some(cff::subset),
            b"CFF2" => Some(cff::subset2),
            b"VORG" => Some(vorg::subset),
            b"VARC" => Some(varc::subset),
            b"COLR" => Some(colr::subset),
            b"CPAL" => Some(cpal::subset),
            b"CBLC" => Some(cblc::subset),
            b"sbix" => Some(sbix::subset),
            // Written by CBLC.
            b"CBDT" => return Ok(room),
            b"hhea" | b"HHEA" | b"vhea" | b"VHEA" | b"loca" | b"LOCA" => return Ok(room),
            b"fvar" | b"avar" | b"cvar" | b"MVAR" | b"STAT" if self.var.user().is_empty() => {
                self.passthrough(font, tag, tables);
                return Ok(room);
            }
            b"MVAR" if !self.var.instancing() => {
                self.passthrough(font, tag, tables);
                return Ok(room);
            }
            b"MVAR" => Some(mvar::subset),
            b"cvar" => Some(gvar::cvar::subset),
            b"STAT" => Some(stat::subset),
            b"fvar" if !self.var.user().is_empty() => {
                let src = font.table(tag).unwrap_or(&[]);
                let bytes = fvar::subset(self, src)?;
                let room = self.room_after(font, tag, src, room, bytes.len())?;
                tables.insert(tag, bytes);
                return Ok(room);
            }
            b"avar" if !self.var.user().is_empty() => {
                let src = font.table(tag).unwrap_or(&[]);
                let bytes = avar::subset(self, font, src)?;
                if bytes.is_empty() {
                    return Ok(room);
                }
                let room = self.room_after(font, tag, src, room, bytes.len())?;
                tables.insert(tag, bytes);
                return Ok(room);
            }
            b"head" => {
                let present = |t: Tag| font.table(t).is_some() && !self.should_drop(t);
                if present(Tag::new(b"glyf")) || present(Tag::new(b"GLYF")) {
                    return Ok(room);
                }
                let bytes = head(self, font)?;
                // `head::subset` embeds the 54 bytes.
                let room =
                    self.room_after(font, tag, font.table(tag).unwrap_or(&[]), room, bytes.len())?;
                tables.insert(tag, bytes);
                return Ok(room);
            }
            b"cvt "
                if !self.var.n.pinned_at_default
                    && !self.var.n.normalized_coords.is_empty()
                    && font.table(Tag::new(b"cvar")).is_some() =>
            {
                let cvar = font.table(Tag::new(b"cvar")).unwrap_or_default();
                let mut bytes = font.table(tag).unwrap_or_default().to_vec();
                sofel_font::var::gvar::apply_cvar_hb(
                    &mut bytes,
                    cvar,
                    &self.var.n.normalized_coords,
                )
                .map_err(|_| SubsetError::Malformed {
                    tag: Tag::new(b"cvar"),
                })?;
                tables.insert(tag, bytes);
                return Ok(room);
            }
            b"cvt " => {
                self.passthrough(font, tag, tables);
                return Ok(room);
            }
            _ => None,
        };
        let Some(f) = f else {
            if self.flags.has(Flags::PASSTHROUGH_UNRECOGNIZED) {
                self.passthrough(font, tag, tables);
            }
            return Ok(room);
        };
        let src = font.table(tag).unwrap_or(&[]);
        let mut s = Serializer::new();
        let needed = f(self, font, tag, src, &mut s, tables)?;
        let room = self.room_after(font, tag, src, room, s.peak_len())?;
        if let Some(mut bytes) = finish_table(tag, s, needed)? {
            if matches!(&tag.0, b"OS/2" | b"post") {
                mvar::apply(self, font, tag, &mut bytes)?;
            }
            tables.insert(tag, bytes);
        }
        Ok(room)
    }
}

/// GDEF (`GDEF::subset`).
fn gdef(
    p: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _out: &mut Tables,
) -> Result<bool, SubsetError> {
    let mut c = Sub::new(p, s, false);
    let needed = crate::layout::gdef::subset(&mut c, src);
    match c.unsupported {
        Some(reason) => Err(SubsetError::Unsupported { tag, reason }),
        None => Ok(needed),
    }
}

/// GSUB and GPOS (`GSUBGPOS::subset`).
fn gsubgpos(
    p: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _out: &mut Tables,
) -> Result<bool, SubsetError> {
    let gsub = tag == t(b"GSUB");
    let mut c = Sub::new(p, s, gsub);
    let needed = c.gsubgpos(Gsubgpos::new(src, gsub));
    match c.unsupported {
        Some(reason) => Err(SubsetError::Unsupported { tag, reason }),
        None => Ok(needed),
    }
}

/// The end of `_hb_subset_table`: the table's bytes from the serializer
/// (none when the writer found it not needed), repacked as
/// `_hb_subset_repack` does when an offset overflowed.
fn finish_table(tag: Tag, s: Serializer, needed: bool) -> Result<Option<Vec<u8>>, SubsetError> {
    if s.in_error() && s.errors() != Errors::OFFSET_OVERFLOW {
        return Err(SubsetError::Failed { tag });
    }
    if !needed {
        return Ok(None);
    }
    let packed = s.end().map_err(|_| SubsetError::Failed { tag })?;
    let bytes = packed.repack(tag).map_err(|e| match e {
        WriteError::Serialize(_) => SubsetError::Failed { tag },
        _ => SubsetError::OffsetOverflow { tag },
    })?;
    Ok(Some(bytes))
}

/// head without glyf (`head::subset`): copied, with the bounds of the
/// instanced outlines when instancing.
fn head(plan: &Plan, font: &Font<'_>) -> Result<Vec<u8>, SubsetError> {
    let h = font
        .table(t(b"head"))
        .ok_or(SubsetError::Failed { tag: t(b"head") })?;
    let ok = h.len() >= 54
        && sofel_font::read::be::u16_at(h, 0) == Some(1)
        && sofel_font::read::be::u32_at(h, 12) == Some(0x5F0F_3CF5);
    if !ok {
        return Err(SubsetError::Malformed { tag: t(b"head") });
    }
    let mut out = h.get(..54).unwrap_or(h).to_vec();
    if plan.var.instancing() {
        let m = &plan.var.metrics.head_maxp;
        for (at, v) in [
            (36usize, m.x_min),
            (38, m.y_min),
            (40, m.x_max),
            (42, m.y_max),
        ] {
            let v = i16::try_from(v).map_err(|_| SubsetError::Failed { tag: t(b"head") })?;
            if let Some(f) = out.get_mut(at..at.saturating_add(2)) {
                f.copy_from_slice(&v.to_be_bytes());
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]
    use super::*;

    fn object(s: &mut Serializer, bytes: &[u8], children: &[u32]) -> u32 {
        s.push();
        s.extend(bytes);
        for &c in children {
            let at = s.zeros(2);
            s.link16(at, c);
        }
        s.pop_pack(false)
    }

    #[test]
    fn overflowing_tables_are_repacked() {
        // test-repacker.cc's dedup overflow: resolved by duplicating the
        // shared leaf.
        let mut s = Serializer::new();
        let leaf = object(&mut s, b"def", &[]);
        let mid = object(&mut s, &[b'a'; 60000], &[leaf]);
        s.extend(&[b'a'; 10000]);
        let at = s.zeros(4);
        s.link16(at, mid);
        s.link16(at + 2, leaf);
        let out = finish_table(Tag::new(b"test"), s, true).unwrap().unwrap();
        assert_eq!(out.len(), 10000 + 2 * 2 + 60000 + 2 + 3 * 2);
    }

    #[test]
    fn unresolved_overflows_are_reported() {
        // A 16-bit offset past a 70000-byte object cannot be resolved.
        let mut s = Serializer::new();
        let leaf = object(&mut s, b"x", &[]);
        let large = object(&mut s, &[b'a'; 70000], &[leaf]);
        let small = object(&mut s, b"s", &[leaf]);
        let at = s.zeros(4);
        s.link16(at, small);
        s.link16(at + 2, large);
        let tag = Tag::new(b"test");
        assert_eq!(
            finish_table(tag, s, true),
            Err(SubsetError::OffsetOverflow { tag })
        );
    }
}
