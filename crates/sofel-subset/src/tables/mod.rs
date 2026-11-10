//! Writing the tables (`hb_subset_plan_execute_or_fail` and the tables'
//! `subset` functions).

mod base;
pub(crate) mod cmap;
pub(crate) mod glyf;
mod hdmx;
mod hmtx;
mod maxp;
mod name;
mod os2;
mod post;

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
    fn should_drop(&self, tag: Tag) -> bool {
        if self.drop_tables.contains(tag.to_u32()) {
            return true;
        }
        let pinned = false;
        match &tag.0 {
            b"cvar" => pinned || self.flags.has(Flags::NO_HINTING),
            b"cvt " | b"fpgm" | b"prep" | b"hdmx" | b"VDMX" => self.flags.has(Flags::NO_HINTING),
            b"avar" | b"fvar" | b"gvar" | b"GVAR" | b"HVAR" | b"VVAR" | b"MVAR" => pinned,
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
        // No dependency delays without instancing (`_dependencies_satisfied`).
        while let Some(&tag) = pending.iter().next() {
            pending.remove(&tag);
            self.subset_table(font, Tag::from_u32(tag), &mut tables)?;
        }
        let list: Vec<(Tag, &[u8])> = tables.iter().map(|(t, d)| (*t, d.as_slice())).collect();
        sofel_write::build_sfnt(&list).map_err(|_| SubsetError::Failed { tag: t(b"sfnt") })
    }

    fn passthrough(&self, font: &Font<'_>, tag: Tag, tables: &mut Tables) {
        tables.insert(tag, font.table(tag).unwrap_or(&[]).to_vec());
    }

    /// `_subset_table`.
    fn subset_table(
        &self,
        font: &Font<'_>,
        tag: Tag,
        tables: &mut Tables,
    ) -> Result<(), SubsetError> {
        if self.no_subset_tables.contains(tag.to_u32()) {
            self.passthrough(font, tag, tables);
            return Ok(());
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
            b"hdmx" => Some(hdmx::subset),
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
            b"hhea" | b"HHEA" | b"vhea" | b"VHEA" | b"loca" | b"LOCA" => return Ok(()),
            b"fvar" | b"avar" | b"cvar" | b"MVAR" | b"STAT" if self.axes.is_empty() => {
                self.passthrough(font, tag, tables);
                return Ok(());
            }
            b"head" => {
                let present = |t: Tag| font.table(t).is_some() && !self.should_drop(t);
                if present(Tag::new(b"glyf")) || present(Tag::new(b"GLYF")) {
                    return Ok(());
                }
                tables.insert(tag, head(font)?);
                return Ok(());
            }
            b"cvt " => {
                self.passthrough(font, tag, tables);
                return Ok(());
            }
            _ => None,
        };
        let Some(f) = f else {
            if self.flags.has(Flags::PASSTHROUGH_UNRECOGNIZED) {
                self.passthrough(font, tag, tables);
            }
            return Ok(());
        };
        let src = font.table(tag).unwrap_or(&[]);
        let mut s = Serializer::new();
        let needed = f(self, font, tag, src, &mut s, tables)?;
        if let Some(bytes) = finish_table(tag, s, needed)? {
            tables.insert(tag, bytes);
        }
        Ok(())
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

/// head without glyf: copied (`head::subset` embeds the table).
fn head(font: &Font<'_>) -> Result<Vec<u8>, SubsetError> {
    let h = font
        .table(t(b"head"))
        .ok_or(SubsetError::Failed { tag: t(b"head") })?;
    let ok = h.len() >= 54
        && sofel_font::read::be::u16_at(h, 0) == Some(1)
        && sofel_font::read::be::u32_at(h, 12) == Some(0x5F0F_3CF5);
    if !ok {
        return Err(SubsetError::Malformed { tag: t(b"head") });
    }
    Ok(h.get(..54).unwrap_or(h).to_vec())
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
