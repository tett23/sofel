//! GDEF load-time checks, following HarfBuzz's GDEF sanitizer (ADR 0059).

use super::{children::Budget, lookups::class_bytes, target};
use crate::ReadError;
use crate::be;
use crate::device::Device;
use crate::gdef::{Gdef, VERSION_1_2, VERSION_1_3, VERSION_1_4};
use crate::layout::{Coverage, counted, uint};

fn coverage(data: &[u8], budget: &mut Budget) -> bool {
    match Coverage::new(data) {
        Ok(view) => budget.take(view.num_records().saturating_mul(match view.format() {
            1 => 2,
            2 => 6,
            3 => 3,
            _ => 9,
        })),
        Err(ReadError::UnknownFormat(_)) => true,
        Err(_) => false,
    }
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

fn attachment_points(data: &[u8], budget: &mut Budget) -> bool {
    counted(data, 0, 2, 2).is_ok_and(|(_, records)| budget.take(records.len()))
}

fn attach_list(data: &[u8], budget: &mut Budget) -> bool {
    let Ok((_, records)) = counted(data, 2, 2, 2) else {
        return false;
    };
    let count = records.len() / 2;
    target(data, u32::from(be::u16_at(data, 0).unwrap_or(0)), |d| {
        coverage(d, budget)
    }) && budget.take(records.len())
        && offsets(data, 4, count, 2).is_some_and(|mut items| {
            items.all(|off| target(data, off, |d| attachment_points(d, budget)))
        })
}

fn caret(data: &[u8], budget: &mut Budget) -> bool {
    let Some(format) = be::u16_at(data, 0) else {
        return false;
    };
    match format {
        1 | 2 => data.get(..4).is_some(),
        3 => {
            let Some(offset) = be::u16_at(data, 4) else {
                return false;
            };
            target(data, u32::from(offset), |d| {
                Device::new(d).is_ok_and(|view| {
                    !(1..=3).contains(&view.delta_format()) || budget.take(view.data().len())
                })
            })
        }
        _ => true,
    }
}

fn lig_glyph(data: &[u8], budget: &mut Budget) -> bool {
    let Ok((_, records)) = counted(data, 0, 2, 2) else {
        return false;
    };
    let count = records.len() / 2;
    budget.take(records.len())
        && offsets(data, 2, count, 2)
            .is_some_and(|mut items| items.all(|off| target(data, off, |d| caret(d, budget))))
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
    target(data, coverage_offset, |d| coverage(d, budget))
        && budget.take(records.len())
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
    budget.take(records.len())
        && offsets(data, 4, count, 4)
            .is_some_and(|mut items| items.all(|off| target(data, off, |d| coverage(d, budget))))
}

fn var_region_list(data: &[u8], budget: &mut Budget) -> bool {
    let (Some(axes), Some(regions)) = (be::u16_at(data, 0), be::u16_at(data, 2)) else {
        return false;
    };
    let size = usize::from(axes)
        .checked_mul(usize::from(regions))
        .and_then(|n| n.checked_mul(6))
        .and_then(|n| n.checked_add(4));
    size.is_some_and(|size| data.get(..size).is_some() && budget.take(size.saturating_sub(4)))
}

fn var_data(data: &[u8], budget: &mut Budget) -> bool {
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
    size.is_some_and(|size| {
        data.get(..size).is_some()
            && budget.take(regions.saturating_mul(2))
            && budget.take(
                size.saturating_sub(6)
                    .saturating_sub(regions.saturating_mul(2)),
            )
    })
}

fn variation_store(data: &[u8], budget: &mut Budget) -> bool {
    if be::u16_at(data, 0) != Some(1) {
        return false;
    }
    let (Some(regions), Some(count)) = (be::u32_at(data, 2), be::u16_at(data, 6)) else {
        return false;
    };
    let count = usize::from(count);
    if !target(data, regions, |d| var_region_list(d, budget))
        || !budget.take(count.saturating_mul(4))
    {
        return false;
    }
    offsets(data, 8, count, 4)
        .is_some_and(|mut items| items.all(|off| target(data, off, |d| var_data(d, budget))))
}

fn reference(
    data: &[u8],
    narrow: usize,
    wide: usize,
    version: u32,
    mut check: impl FnMut(&[u8]) -> bool,
) -> bool {
    let narrow_offset = u32::from(be::u16_at(data, narrow).unwrap_or(0));
    let narrow_ok = target(data, narrow_offset, &mut check);
    let wide_offset = if version >= VERSION_1_4 {
        be::u32_at(data, wide).unwrap_or(0)
    } else {
        0
    };
    if wide_offset != 0 {
        target(data, wide_offset, &mut check)
    } else {
        narrow_ok
    }
}

/// Whether HarfBuzz can use this GDEF table during shaping.
pub(super) fn assess(data: &[u8]) -> bool {
    let Ok(header) = Gdef::new(data) else {
        return false;
    };
    let version = header.version();
    let mut budget = Budget::new(data.len());
    let header_bytes = if version >= VERSION_1_4 {
        38
    } else if version >= VERSION_1_3 {
        18
    } else if version >= VERSION_1_2 {
        14
    } else {
        12
    };
    if !budget.take(header_bytes) {
        return false;
    }
    let glyph_class_def_ok = reference(data, 4, 18, version, |d| class_bytes(d, &mut budget));
    let attach_list_ok = reference(data, 6, 22, version, |d| attach_list(d, &mut budget));
    let narrow_lig = target(data, u32::from(be::u16_at(data, 8).unwrap_or(0)), |d| {
        lig_caret_list(d, false, &mut budget)
    });
    let wide_offset = if version >= VERSION_1_4 {
        be::u32_at(data, 26).unwrap_or(0)
    } else {
        0
    };
    let lig_caret_list_ok = if wide_offset != 0 {
        target(data, wide_offset, |d| lig_caret_list(d, true, &mut budget))
    } else {
        narrow_lig
    };
    let mark_attach_class_def_ok =
        reference(data, 10, 30, version, |d| class_bytes(d, &mut budget));
    let mark_sets_ok =
        version < VERSION_1_2 || reference(data, 12, 34, version, |d| mark_sets(d, &mut budget));
    glyph_class_def_ok
        && attach_list_ok
        && lig_caret_list_ok
        && mark_attach_class_def_ok
        && mark_sets_ok
        && (version < VERSION_1_3
            || target(data, header.item_var_store_offset().unwrap_or(0), |d| {
                variation_store(d, &mut budget)
            }))
}
