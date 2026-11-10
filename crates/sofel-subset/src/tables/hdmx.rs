//! hdmx (`src/hb-ot-hdmx-table.hh`).

use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::be::{u8_at, u16_at, u32_at};
use sofel_write::Serializer;

use crate::error::SubsetError;
use crate::plan::Plan;
use crate::tables::Tables;

/// `hdmx::subset`.
pub(crate) fn subset(
    plan: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let bad = SubsetError::Malformed { tag };
    let version = u16_at(src, 0).ok_or(bad)?;
    let num_records = u16_at(src, 2).ok_or(bad)?;
    let size = u32_at(src, 4).ok_or(bad)?;
    let total = u64::from(num_records).saturating_mul(u64::from(size));
    if size < 2 || (src.len() as u64) < total.saturating_add(8) {
        return Err(bad);
    }
    let num_input_glyphs = (size as usize).saturating_sub(2);
    let n = plan.num_output_glyphs as usize;
    let rec_size = n.saturating_add(2).next_multiple_of(4);
    s.u16(version);
    s.u16(num_records);
    s.u32(u32::try_from(rec_size).map_err(|_| bad)?);
    for r in 0..usize::from(num_records) {
        let base = r.saturating_mul(size as usize).saturating_add(8);
        let width = |old: u32| -> u8 {
            if (old as usize) < num_input_glyphs {
                u8_at(src, base.saturating_add(2).saturating_add(old as usize)).unwrap_or(0)
            } else {
                0
            }
        };
        let row: Vec<u8> = plan
            .glyphs
            .new_to_old
            .iter()
            .map(|&(_, o)| width(o))
            .collect();
        let at = s.zeros(rec_size);
        s.write_at(
            at,
            &[
                u8_at(src, base).unwrap_or(0),
                row.iter().copied().max().unwrap_or(0),
            ],
        );
        for (&(new, _), w) in plan.glyphs.new_to_old.iter().zip(row) {
            s.write_at(at.saturating_add(2).saturating_add(new as usize), &[w]);
        }
    }
    Ok(true)
}
