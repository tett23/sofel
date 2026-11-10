//! Writing the tables (`hb_subset_plan_execute_or_fail` and the tables'
//! `subset` functions).

pub(crate) mod cff;
pub(crate) mod cmap;
pub(crate) mod glyf;
mod hdmx;
mod hmtx;
mod maxp;
mod name;
mod os2;
mod post;
mod vorg;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_write::{Errors, Serializer};

use crate::error::SubsetError;
use crate::input::Flags;
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
            b"CFF " => Some(cff::subset),
            b"VORG" => Some(vorg::subset),
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
        if s.in_error() && s.errors() != Errors::OFFSET_OVERFLOW {
            return Err(SubsetError::Failed { tag });
        }
        if !needed {
            return Ok(());
        }
        let packed = s.end().map_err(|_| SubsetError::Failed { tag })?;
        let bytes = packed.resolve().map_err(|e| {
            if e == Errors::OFFSET_OVERFLOW {
                SubsetError::Unsupported {
                    tag,
                    reason: "repacking offset overflows (ADR 0022 stage 4)",
                }
            } else {
                SubsetError::Failed { tag }
            }
        })?;
        tables.insert(tag, bytes);
        Ok(())
    }
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
