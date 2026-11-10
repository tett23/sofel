//! hmtx and vmtx (and HMTX and VMTX) with their headers
//! (`src/hb-ot-hmtx-table.hh`).

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::be::{i16_at, u16_at, u32_at};
use sofel_write::{Errors, Serializer};

use crate::error::SubsetError;
use crate::plan::Plan;
use crate::tables::Tables;

/// The header tag of a metrics table, and whether its count is 32-bit.
fn header_of(tag: Tag) -> (Tag, bool) {
    match &tag.0 {
        b"hmtx" => (Tag::new(b"hhea"), false),
        b"HMTX" => (Tag::new(b"HHEA"), true),
        b"vmtx" => (Tag::new(b"vhea"), false),
        _ => (Tag::new(b"VHEA"), true),
    }
}

/// HarfBuzz's view of a header: the table when it passes the sanitizer
/// (version 1 and the fixed size).
fn header<'a>(font: &Font<'a>, tag: Tag, wide: bool) -> Option<&'a [u8]> {
    let size = if wide { 38 } else { 36 };
    font.table(tag)
        .filter(|h| h.len() >= size && u16_at(h, 0) == Some(1))
}

/// `hmtxvmtx::accelerator_t`, without variations.
pub(crate) struct MetricsAccel<'a> {
    data: &'a [u8],
    num_long_metrics: u32,
    num_bearings: u32,
    num_advances: u32,
    default_advance: u32,
}

impl<'a> MetricsAccel<'a> {
    pub fn new(font: &Font<'a>, tag: Tag) -> MetricsAccel<'a> {
        let (htag, wide) = header_of(tag);
        let nlm = header(font, htag, wide).map_or(0, |h| {
            if wide {
                u32_at(h, 34).unwrap_or(0)
            } else {
                u32::from(u16_at(h, 34).unwrap_or(0))
            }
        });
        let data = font.table(tag).unwrap_or(&[]);
        let mut len = u32::try_from(data.len()).unwrap_or(u32::MAX);
        if len & 1 != 0 {
            len = len.saturating_sub(1);
        }
        let mut num_long_metrics = nlm.min(len / 4);
        len = len.saturating_sub(num_long_metrics.saturating_mul(4));
        let mut num_bearings = maxp_num_glyphs(font).max(num_long_metrics);
        if u64::from(num_bearings.saturating_sub(num_long_metrics)).saturating_mul(2)
            > u64::from(len)
        {
            num_bearings = num_long_metrics.saturating_add(len / 2);
        }
        len = len.saturating_sub(
            num_bearings
                .saturating_sub(num_long_metrics)
                .saturating_mul(2),
        );
        if num_long_metrics == 0 {
            num_bearings = 0;
            num_long_metrics = 0;
        }
        let upem = crate::tables::hb_upem(font);
        let horizontal = matches!(&tag.0, b"hmtx" | b"HMTX");
        MetricsAccel {
            data,
            num_long_metrics,
            num_bearings,
            num_advances: num_bearings.saturating_add(len / 2),
            default_advance: if horizontal { upem / 2 } else { upem },
        }
    }

    /// `has_data`: whether the table has metrics.
    pub fn has_data(&self) -> bool {
        self.num_bearings != 0
    }

    /// `hmtx_accelerator_t` (or `vmtx_accelerator_t` with `vertical`):
    /// HMTX (VMTX) when it has metrics, else hmtx (vmtx), as
    /// `hb_dual_accelerator_t`.
    pub fn dual(font: &Font<'a>, vertical: bool) -> MetricsAccel<'a> {
        let (upper, lower) = if vertical {
            (Tag::new(b"VMTX"), Tag::new(b"vmtx"))
        } else {
            (Tag::new(b"HMTX"), Tag::new(b"hmtx"))
        };
        let u = MetricsAccel::new(font, upper);
        if u.has_data() {
            u
        } else {
            MetricsAccel::new(font, lower)
        }
    }

    /// `get_leading_bearing_without_var_unscaled`.
    pub fn side_bearing(&self, g: u32) -> i32 {
        if g < self.num_long_metrics {
            return i32::from(
                i16_at(self.data, (g as usize).saturating_mul(4).saturating_add(2)).unwrap_or(0),
            );
        }
        if g >= self.num_bearings {
            return 0;
        }
        let at = (self.num_long_metrics as usize)
            .saturating_mul(4)
            .saturating_add((g.saturating_sub(self.num_long_metrics) as usize).saturating_mul(2));
        i32::from(i16_at(self.data, at).unwrap_or(0))
    }

    /// `get_advance_without_var_unscaled`.
    pub fn advance(&self, g: u32) -> u32 {
        if g < self.num_bearings {
            let i = g.min(self.num_long_metrics.saturating_sub(1)) as usize;
            return u32::from(u16_at(self.data, 4 * i).unwrap_or(0));
        }
        if self.num_advances == 0 {
            return self.default_advance;
        }
        0
    }
}

/// maxp's (or MAXP's) glyph count, as `face->table.maxp`.
fn maxp_num_glyphs(font: &Font<'_>) -> u32 {
    font.num_glyphs()
}

/// `hmtxvmtx::subset`.
pub(crate) fn subset(
    plan: &Plan,
    font: &Font<'_>,
    tag: Tag,
    _src: &[u8],
    s: &mut Serializer,
    tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let (htag, wide) = header_of(tag);
    let mtx = MetricsAccel::new(font, tag);
    let horizontal = matches!(&tag.0, b"hmtx" | b"HMTX");
    let (map, bounds) = if horizontal {
        (&plan.var.metrics.hmtx, &plan.var.metrics.bounds_width)
    } else {
        (&plan.var.metrics.vmtx, &plan.var.metrics.bounds_height)
    };
    let max = if wide { u32::MAX } else { 0xFFFF };
    // `get_new_gid_advance_unscaled`.
    let new_adv = |new: u32| {
        if map.is_empty() {
            plan.glyphs.old_gid(new).map_or(0, |o| mtx.advance(o))
        } else {
            map.get(&new).map_or(0, |m| m.0)
        }
    };
    let mut nlm = plan.num_output_glyphs.min(max);
    let last = new_adv(nlm.wrapping_sub(1));
    while nlm > 1 && last == new_adv(nlm.saturating_sub(2)) {
        nlm = nlm.saturating_sub(1);
    }
    let total = plan.num_output_glyphs;
    let start = s.zeros(
        (nlm as usize)
            .saturating_mul(4)
            .saturating_add((total.saturating_sub(nlm) as usize).saturating_mul(2)),
    );
    for &(new, old) in &plan.glyphs.new_to_old {
        let (adv, sb) = map
            .get(&new)
            .copied()
            .unwrap_or_else(|| (mtx.advance(old), mtx.side_bearing(old)));
        if new < nlm {
            let at = start.saturating_add((new as usize).saturating_mul(4));
            s.set_u16(at, (adv & 0xFFFF) as u16);
            s.write_at(
                at.saturating_add(2),
                &u16::try_from(sb & 0xFFFF).unwrap_or(0).to_be_bytes(),
            );
        } else {
            let at = start
                .saturating_add((nlm as usize).saturating_mul(4))
                .saturating_add((new.saturating_sub(nlm) as usize).saturating_mul(2));
            s.write_at(at, &u16::try_from(sb & 0xFFFF).unwrap_or(0).to_be_bytes());
        }
    }
    let mut h = header(font, htag, wide)
        .ok_or(SubsetError::Failed { tag: htag })?
        .to_vec();
    if wide {
        if let Some(f) = h.get_mut(34..38) {
            f.copy_from_slice(&nlm.to_be_bytes());
        }
    } else {
        if nlm > 0xFFFF {
            s.err(Errors::INT_OVERFLOW);
            return Ok(false);
        }
        if let Some(f) = h.get_mut(34..36) {
            f.copy_from_slice(&((nlm & 0xFFFF) as u16).to_be_bytes());
        }
    }
    if plan.var.instancing() {
        update_header(&mut h, map, bounds);
    }
    tables.insert(htag, h);
    Ok(true)
}

/// The instancing part of `subset_update_header`: the maximum advance
/// and, from the glyphs with bounds, the minimum side bearings and the
/// maximum extent. (The MVAR deltas and the ascender, descender and line
/// gap from OS/2 do not change at the default location.)
fn update_header(
    h: &mut [u8],
    map: &alloc::collections::BTreeMap<u32, (u32, i32)>,
    bounds: &[u32],
) {
    let mut empty = true;
    let (mut min_lsb, mut min_rsb, mut max_extent) = (0x7FFFi32, 0x7FFFi32, -0x7FFFi32);
    let mut max_adv = 0u32;
    for (&gid, &(adv, lsb)) in map {
        max_adv = max_adv.max(adv);
        let Some(&width) = bounds.get(gid as usize).filter(|&&w| w != u32::MAX) else {
            continue;
        };
        empty = false;
        // `int rsb = adv - lsb - bound_width` in unsigned arithmetic.
        let rsb = adv
            .wrapping_sub(lsb.cast_unsigned())
            .wrapping_sub(width)
            .cast_signed();
        let extent = lsb.cast_unsigned().wrapping_add(width).cast_signed();
        min_lsb = min_lsb.min(lsb);
        min_rsb = min_rsb.min(rsb);
        max_extent = max_extent.max(extent);
    }
    let mut put = |at: usize, v: u16| {
        if let Some(f) = h.get_mut(at..at.saturating_add(2)) {
            f.copy_from_slice(&v.to_be_bytes());
        }
    };
    put(10, (max_adv & 0xFFFF) as u16);
    if !empty {
        put(12, crate::instance::lo16(min_lsb));
        put(14, crate::instance::lo16(min_rsb));
        put(16, crate::instance::lo16(max_extent));
    }
}
