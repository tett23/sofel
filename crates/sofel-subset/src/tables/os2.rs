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
    if plan.var.instancing() {
        let avg = avg_char_width(&plan.var.metrics.hmtx);
        let v = i16::try_from(avg).map_err(|_| SubsetError::Failed { tag })?;
        s.set_u16(2, v.cast_unsigned());
    }
    let user = plan.var.user();
    if let Some(r) = user.get(&Tag::new(b"wght")) {
        let w = crate::instance::clamp_f32(r.default, 1.0, 1000.0);
        s.set_u16(4, to_u16(sofel_geom::float::hb_roundf32(w)));
    }
    if let Some(r) = user.get(&Tag::new(b"wdth")) {
        let w = wdth_to_width_class(r.default);
        s.set_u16(6, to_u16(sofel_geom::float::hb_roundf32(w)));
    }
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

/// `static_cast<unsigned> (v)` stored in a 16-bit field.
fn to_u16(v: f64) -> u16 {
    crate::instance::lo16(crate::instance::f64_to_i32(v))
}

/// `calc_avg_char_width`: the rounded mean of the nonzero advances.
fn avg_char_width(hmtx: &alloc::collections::BTreeMap<u32, (u32, i32)>) -> u32 {
    let (num, total) = hmtx
        .values()
        .filter(|v| v.0 != 0)
        .fold((0u32, 0u32), |(n, t), v| {
            (n.wrapping_add(1), t.wrapping_add(v.0))
        });
    if num == 0 {
        return 0;
    }
    let avg = sofel_geom::float::hb_roundf(f64::from(total) / f64::from(num));
    // `(unsigned)` of a value in the u32 range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let u = avg.clamp(0.0, f64::from(u32::MAX)) as u32;
    u
}

/// `map_wdth_to_widthclass`.
fn wdth_to_width_class(width: f32) -> f32 {
    use sofel_geom::float::{ceilf, floorf, i32_to_f32};
    if width < 50.0 {
        return 1.0;
    }
    if width > 200.0 {
        return 9.0;
    }
    let ratio = (width - 50.0) / 12.5;
    let int = |v: f32| crate::instance::f64_to_i32(f64::from(v));
    let mut a = int(floorf(ratio));
    let mut b = int(ceilf(ratio));
    if b <= 6 {
        if a == b {
            return i32_to_f32(a) + 1.0;
        }
    } else if b == 7 {
        a = 6;
        b = 8;
    } else if b == 8 {
        if a == b {
            return 8.0;
        }
        a = 6;
    } else {
        if a == b && a == 12 {
            return 9.0;
        }
        b = 12;
        a = 8;
    }
    let va = 50.0 + i32_to_f32(a) * 12.5;
    let vb = 50.0 + i32_to_f32(b) * 12.5;
    let mut ret = i32_to_f32(a) + (width - va) / (vb - va);
    if a <= 6 {
        ret += 1.0;
    }
    ret
}
