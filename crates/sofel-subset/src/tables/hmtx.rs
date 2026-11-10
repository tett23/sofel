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
    let max = if wide { u32::MAX } else { 0xFFFF };
    let new_adv = |new: u32| plan.glyphs.old_gid(new).map_or(0, |o| mtx.advance(o));
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
        let (adv, sb) = (mtx.advance(old), mtx.side_bearing(old));
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
    tables.insert(htag, h);
    Ok(true)
}
