//! GDEF load-time checks, following HarfBuzz's GDEF sanitizer (ADR 0059).

use super::{children::Budget, classes, target};
use crate::ReadError;
use crate::be;
use crate::device::Device;
use crate::gdef::{Gdef, VERSION_1_2, VERSION_1_3, VERSION_1_4};
use crate::layout::{Coverage, counted, uint};

fn coverage(data: &[u8]) -> bool {
    matches!(
        Coverage::new(data),
        Ok(_) | Err(ReadError::UnknownFormat(_))
    )
}

fn offsets(
    data: &[u8],
    at: usize,
    count: usize,
    width: usize,
) -> Option<impl Iterator<Item = u32> + '_> {
    let end = at.checked_add(count.checked_mul(width)?)?;
    data.get(at..end)?;
    Some((0..count).filter_map(move |i| uint(data, at.checked_add(i.checked_mul(width)?)?, width)))
}

fn attachment_points(data: &[u8]) -> bool {
    counted(data, 0, 2, 2).is_ok()
}

fn attach_list(data: &[u8], budget: &mut Budget) -> bool {
    let Ok((_, records)) = counted(data, 2, 2, 2) else {
        return false;
    };
    let count = records.len() / 2;
    target(data, u32::from(be::u16_at(data, 0).unwrap_or(0)), coverage)
        && budget.take(count)
        && offsets(data, 4, count, 2)
            .is_some_and(|mut items| items.all(|off| target(data, off, attachment_points)))
}

fn caret(data: &[u8]) -> bool {
    let Some(format) = be::u16_at(data, 0) else {
        return false;
    };
    match format {
        1 | 2 => data.get(..4).is_some(),
        3 => {
            let Some(offset) = be::u16_at(data, 4) else {
                return false;
            };
            target(data, u32::from(offset), |d| Device::new(d).is_ok())
        }
        _ => true,
    }
}

fn lig_glyph(data: &[u8], budget: &mut Budget) -> bool {
    let Ok((_, records)) = counted(data, 0, 2, 2) else {
        return false;
    };
    let count = records.len() / 2;
    budget.take(count)
        && offsets(data, 2, count, 2)
            .is_some_and(|mut items| items.all(|off| target(data, off, caret)))
}

fn lig_caret_list(data: &[u8], wide: bool, budget: &mut Budget) -> bool {
    let (count_at, width, first) = if wide { (4, 3, 7) } else { (2, 2, 4) };
    let Ok((_, records)) = counted(data, count_at, width, width) else {
        return false;
    };
    let count = records.len().checked_div(width).unwrap_or(0);
    let Some(coverage_offset) = uint(data, 0, if wide { 4 } else { 2 }) else {
        return false;
    };
    target(data, coverage_offset, coverage)
        && budget.take(count)
        && offsets(data, first, count, width)
            .is_some_and(|mut items| items.all(|off| target(data, off, |d| lig_glyph(d, budget))))
}

fn mark_sets(data: &[u8], budget: &mut Budget) -> bool {
    let Some(format) = be::u16_at(data, 0) else {
        return false;
    };
    if format != 1 {
        return true;
    }
    let Ok((_, records)) = counted(data, 2, 2, 4) else {
        return false;
    };
    let count = records.len() / 4;
    budget.take(count)
        && offsets(data, 4, count, 4)
            .is_some_and(|mut items| items.all(|off| target(data, off, coverage)))
}

fn var_region_list(data: &[u8]) -> bool {
    let (Some(axes), Some(regions)) = (be::u16_at(data, 0), be::u16_at(data, 2)) else {
        return false;
    };
    let size = usize::from(axes)
        .checked_mul(usize::from(regions))
        .and_then(|n| n.checked_mul(6))
        .and_then(|n| n.checked_add(4));
    size.is_some_and(|size| data.get(..size).is_some())
}

fn var_data(data: &[u8]) -> bool {
    let (Some(rows), Some(word_count), Some(regions)) = (
        be::u16_at(data, 0),
        be::u16_at(data, 2),
        be::u16_at(data, 4),
    ) else {
        return false;
    };
    let words = usize::from(word_count & 0x7fff);
    let regions = usize::from(regions);
    if words > regions {
        return false;
    }
    let (large, small) = if word_count & 0x8000 == 0 {
        (2, 1)
    } else {
        (4, 2)
    };
    let size = regions
        .checked_mul(2)
        .and_then(|n| n.checked_add(6))
        .and_then(|header| {
            words
                .checked_mul(large)
                .and_then(|n| {
                    regions
                        .checked_sub(words)
                        .and_then(|remaining| remaining.checked_mul(small))
                        .and_then(|m| n.checked_add(m))
                })
                .and_then(|row| row.checked_mul(usize::from(rows)))
                .and_then(|all| header.checked_add(all))
        });
    size.is_some_and(|size| data.get(..size).is_some())
}

fn variation_store(data: &[u8], budget: &mut Budget) -> bool {
    if be::u16_at(data, 0) != Some(1) {
        return false;
    }
    let (Some(regions), Some(count)) = (be::u32_at(data, 2), be::u16_at(data, 6)) else {
        return false;
    };
    let count = usize::from(count);
    if !budget.take(count) || !target(data, regions, var_region_list) {
        return false;
    }
    offsets(data, 8, count, 4).is_some_and(|mut items| items.all(|off| target(data, off, var_data)))
}

fn reference(
    data: &[u8],
    narrow: usize,
    wide: usize,
    version: u32,
    mut check: impl FnMut(&[u8]) -> bool,
) -> bool {
    let narrow_offset = u32::from(be::u16_at(data, narrow).unwrap_or(0));
    target(data, narrow_offset, &mut check)
        && (version < VERSION_1_4 || target(data, be::u32_at(data, wide).unwrap_or(0), check))
}

/// Whether HarfBuzz can use this GDEF table during shaping.
pub(super) fn assess(data: &[u8]) -> bool {
    let Ok(header) = Gdef::new(data) else {
        return false;
    };
    let version = header.version();
    let mut budget = Budget::new(data.len());
    reference(data, 4, 18, version, classes)
        && reference(data, 6, 22, version, |d| attach_list(d, &mut budget))
        && target(data, u32::from(be::u16_at(data, 8).unwrap_or(0)), |d| {
            lig_caret_list(d, false, &mut budget)
        })
        && (version < VERSION_1_4
            || target(data, be::u32_at(data, 26).unwrap_or(0), |d| {
                lig_caret_list(d, true, &mut budget)
            }))
        && reference(data, 10, 30, version, classes)
        && (version < VERSION_1_2
            || reference(data, 12, 34, version, |d| mark_sets(d, &mut budget)))
        && (version < VERSION_1_3
            || target(data, header.item_var_store_offset().unwrap_or(0), |d| {
                variation_store(d, &mut budget)
            }))
}
