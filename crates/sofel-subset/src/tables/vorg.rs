//! VORG (`src/hb-ot-vorg-table.hh`): the records of the kept glyphs, in
//! the source order, with their new glyph ids.

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::be::{u16_at, u24_at};
use sofel_write::Serializer;

use crate::error::SubsetError;
use crate::plan::Plan;
use crate::tables::Tables;

/// `VORG::subset`: version 1 has 16-bit glyph ids and count, version 2
/// (beyond 64K) 24-bit ones. A table HarfBuzz's sanitizer rejects fails
/// the subset.
pub(crate) fn subset(
    plan: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let bad = SubsetError::Failed { tag };
    let major = u16_at(src, 0).ok_or(bad)?;
    let wide = match major {
        1 => false,
        2 => true,
        _ => return Err(bad),
    };
    let default_y = u16_at(src, 4).ok_or(bad)?;
    let (count, records_at, gid_size) = if wide {
        (u24_at(src, 6).ok_or(bad)?, 9usize, 3usize)
    } else {
        (u32::from(u16_at(src, 6).ok_or(bad)?), 8, 2)
    };
    let rec = gid_size.saturating_add(2);
    let end = (count as usize)
        .saturating_mul(rec)
        .saturating_add(records_at);
    if src.len() < end {
        return Err(bad);
    }
    s.u16(major);
    s.u16(0);
    s.u16(default_y);
    let count_at = if wide { s.u24(0) } else { s.u16(0) };
    let mut n = 0u64;
    for i in 0..count as usize {
        let at = records_at.saturating_add(i.saturating_mul(rec));
        let glyph = if wide {
            u24_at(src, at)
        } else {
            u16_at(src, at).map(u32::from)
        }
        .ok_or(bad)?;
        if !plan.glyphset.contains(glyph) {
            continue;
        }
        let y = u16_at(src, at.saturating_add(gid_size)).ok_or(bad)?;
        let new = plan.glyphs.new_gid(glyph).unwrap_or(crate::set::INVALID);
        if wide {
            s.u24(new & 0xFF_FFFF);
        } else {
            s.u16(u16::try_from(new & 0xFFFF).unwrap_or(0));
        }
        s.u16(y);
        n = n.saturating_add(1);
    }
    if wide {
        s.set_u24(count_at, u32::try_from(n & 0xFF_FFFF).unwrap_or(0));
    } else {
        s.set_u16(count_at, u16::try_from(n & 0xFFFF).unwrap_or(0));
    }
    Ok(true)
}
