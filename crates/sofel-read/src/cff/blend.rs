//! FreeType's CFF2 blend vector (`cff_blend_build_vector` in
//! `src/cff/cffload.c`, FreeType 2.15.0. Copyright (C) 1996-2026 by David
//! Turner, Robert Wilhelm, and Werner Lemberg. Used under the FreeType
//! License (FTL); see THIRD_PARTY_NOTICES.md).

use super::dict::BlendWeights;
use super::ft;
use crate::item_variation::ItemVariationStore;

/// The weights FreeType gives the regions of a CFF2 Variation Store at
/// normalized coordinates `coords` (16.16): with no coordinates (a font
/// without `fvar`), every region weighs 0 (`BV = (1, 0, …, 0)`);
/// otherwise each region's scalar, where an axis with peak 0 or at the
/// coordinate counts 1, a coordinate at or outside the start or the end
/// makes the region 0, and others scale proportionally (`FT_MulDiv`).
/// Axes whose start is negative while the end is positive, or whose
/// start, peak and end are out of order, have their peak set to 0 when
/// FreeType loads them.
#[derive(Clone, Copy, Debug)]
pub struct FtBlend<'a> {
    store: Option<ItemVariationStore<'a>>,
    coords: &'a [i32],
}

impl<'a> FtBlend<'a> {
    /// The blend vectors of `store` (`None` for a font without a
    /// Variation Store) at `coords`.
    pub fn new(store: Option<ItemVariationStore<'a>>, coords: &'a [i32]) -> FtBlend<'a> {
        FtBlend { store, coords }
    }
}

/// `FT_fdot14ToFixed`.
fn to_fixed(v: i16) -> i64 {
    i64::from(v) << 2
}

impl BlendWeights for FtBlend<'_> {
    fn regions(&self, vsindex: u32) -> Option<u32> {
        let s = self.store?;
        if !self.coords.is_empty() && self.coords.len() != usize::from(s.axis_count()) {
            return None;
        }
        let d = s.item_data(u16::try_from(vsindex).ok()?)?;
        let n = d.region_index_count();
        (0..usize::from(n))
            .all(|k| d.region_index(k).is_some_and(|i| i < s.region_count()))
            .then_some(u32::from(n))
    }

    fn weight(&self, vsindex: u32, k: u32) -> i32 {
        if self.coords.is_empty() {
            return 0;
        }
        let Some(s) = self.store else { return 0 };
        let region = u16::try_from(vsindex)
            .ok()
            .and_then(|v| s.item_data(v))
            .and_then(|d| d.region_index(usize::try_from(k).ok()?))
            .and_then(|i| s.region(i));
        let Some(region) = region else { return 0 };
        let mut bv: i32 = 0x1_0000;
        for (j, &c) in self.coords.iter().enumerate() {
            let Some((start, peak, end)) = region.axis(j) else {
                break;
            };
            let peak = if (start < 0 && end > 0) || start > peak || peak > end {
                0
            } else {
                peak
            };
            let (start, peak, end, c) =
                (to_fixed(start), to_fixed(peak), to_fixed(end), i64::from(c));
            if peak == c || peak == 0 {
                continue;
            }
            if c <= start || c >= end {
                bv = 0;
                break;
            }
            bv = if c < peak {
                ft::to_i32(ft::mul_div(
                    i64::from(bv),
                    c.wrapping_sub(start),
                    peak.wrapping_sub(start),
                ))
            } else {
                ft::to_i32(ft::mul_div(
                    i64::from(bv),
                    end.wrapping_sub(c),
                    end.wrapping_sub(peak),
                ))
            };
        }
        bv
    }
}
