//! name (`src/OT/name/name.hh`).

use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::be::u16_at;
use sofel_write::{Errors, Serializer, Whence};

use crate::error::SubsetError;
use crate::input::Flags;
use crate::plan::Plan;
use crate::tables::Tables;

/// `name::subset`.
pub(crate) fn subset(
    plan: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let bad = SubsetError::Malformed { tag };
    let format = u16_at(src, 0).ok_or(bad)?;
    let count = usize::from(u16_at(src, 2).ok_or(bad)?);
    let string_offset = usize::from(u16_at(src, 4).ok_or(bad)?);
    if format > 1
        || src.len() < count.saturating_mul(12).saturating_add(6)
        || src.len() < string_offset
    {
        return Err(bad);
    }
    let field = |i: usize, f: usize| {
        u16_at(
            src,
            i.saturating_mul(12)
                .saturating_add(6)
                .saturating_add(f.saturating_mul(2)),
        )
        .unwrap_or(0)
    };
    let mut records: Vec<[u16; 6]> = (0..count)
        .map(|i| {
            [
                field(i, 0),
                field(i, 1),
                field(i, 2),
                field(i, 3),
                field(i, 4),
                field(i, 5),
            ]
        })
        .filter(|r| plan.name_ids.contains(u32::from(r[3])))
        .filter(|r| plan.name_languages.contains(u32::from(r[2])))
        .filter(|r| {
            plan.flags.has(Flags::NAME_LEGACY)
                || r[0] == 0
                || (r[0] == 3 && matches!(r[1], 0 | 1 | 10))
        })
        .collect();
    for r in &records {
        let start = string_offset.saturating_add(usize::from(r[5]));
        if start.saturating_add(usize::from(r[4])) > src.len() {
            return Err(bad);
        }
    }
    records.sort_by_key(|r| (r[0], r[1], r[2], r[3], r[4]));
    s.u16(0);
    let at = s.u16(0);
    if !s.check_assign_u16(at, records.len() as u64, Errors::INT_OVERFLOW) {
        return Ok(false);
    }
    let so = s.u16(0);
    for r in &records {
        let rec = s.len();
        r.iter().take(5).for_each(|&v| {
            s.u16(v);
        });
        s.u16(0);
        let start = string_offset.saturating_add(usize::from(r[5]));
        s.push();
        s.extend(
            src.get(start..start.saturating_add(usize::from(r[4])))
                .unwrap_or(&[]),
        );
        let idx = s.pop_pack(true);
        s.add_link(rec.saturating_add(10), 2, false, idx, Whence::Tail, 0);
    }
    let len = s.len();
    s.set_u16(so, u16::try_from(len & 0xFFFF).unwrap_or(0));
    Ok(true)
}
