//! OS/2 (`src/hb-ot-os2-table.hh`).

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::be::{u16_at, u32_at};
use sofel_write::Serializer;

use crate::error::SubsetError;
use crate::input::Flags;
use crate::plan::Plan;
use crate::set::RangeSet;
use crate::tables::Tables;

/// `_hb_os2_unicode_ranges`: (first, last, bit).
const RANGES: [(u32, u32, u32); 169] = include!("os2_ranges.inc");

/// `_hb_ot_os2_get_unicode_range_bit`.
fn range_bit(cp: u32) -> Option<u32> {
    RANGES
        .binary_search_by(|&(a, b, _)| {
            if cp < a {
                core::cmp::Ordering::Greater
            } else if cp > b {
                core::cmp::Ordering::Less
            } else {
                core::cmp::Ordering::Equal
            }
        })
        .ok()
        .and_then(|i| RANGES.get(i))
        .map(|r| r.2)
}

/// The table's size for its version (`OS2::get_size`).
fn size(version: u16) -> usize {
    match version {
        0 => 78,
        1 => 86,
        2..=4 => 96,
        _ => 100,
    }
}

/// `_update_unicode_ranges`: the bits of the kept characters.
fn unicode_ranges(unicodes: &RangeSet) -> [u32; 4] {
    let mut bits = [0u32; 4];
    let mut set = |bit: u32| {
        if let Some(b) = bits.get_mut((bit / 32) as usize) {
            *b |= 1 << (bit % 32);
        }
    };
    for cp in unicodes.iter() {
        if let Some(bit) = range_bit(cp).filter(|&b| b < 128) {
            set(bit);
        }
        if (0x10000..=0x110000).contains(&cp) {
            set(57);
        }
    }
    bits
}

/// `OS2::subset`.
pub(crate) fn subset(
    plan: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let version = u16_at(src, 0).ok_or(SubsetError::Malformed { tag })?;
    let n = size(version);
    if src.len() < n {
        return Err(SubsetError::Malformed { tag });
    }
    s.extend(src.get(..n).unwrap_or(&[]));
    s.set_u16(64, (plan.os2_min_cp.min(0xFFFF) & 0xFFFF) as u16);
    s.set_u16(66, (plan.os2_max_cp.min(0xFFFF) & 0xFFFF) as u16);
    if plan.flags.has(Flags::NO_PRUNE_UNICODE_RANGES) {
        return Ok(true);
    }
    let bits = unicode_ranges(&plan.unicodes);
    for (i, b) in bits.iter().enumerate() {
        let at = i.saturating_mul(4).saturating_add(42);
        let v = u32_at(src, at).unwrap_or(0) & b;
        s.set_u32(at, v);
    }
    Ok(true)
}
