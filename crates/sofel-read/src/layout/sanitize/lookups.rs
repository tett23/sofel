//! Traverse the LookupList before building shaping accelerators.
use super::super::{Coverage, Lookup, LookupList, uint};
use super::{
    children::{self, Budget},
    target,
};
use crate::{ReadError, be, gpos, gsub};

fn coverage(data: &[u8]) -> bool {
    matches!(
        Coverage::new(data),
        Ok(_) | Err(ReadError::UnknownFormat(_))
    )
}

fn known(ty: u16, positioning: bool) -> bool {
    (1..=if positioning { 9 } else { 8 }).contains(&ty)
}

fn subtable(data: &[u8], ty: u16, positioning: bool, extension: u16, budget: &mut Budget) -> bool {
    let Some(fmt) = be::u16_at(data, 0) else {
        return false;
    };
    if ty == extension {
        if fmt != 1 {
            return true;
        }
        let (Some(inner), Some(offset)) = (be::u16_at(data, 2), be::u32_at(data, 4)) else {
            return false;
        };
        return inner != extension
            && (!known(inner, positioning)
                || target(data, offset, |t| {
                    subtable(t, inner, positioning, extension, budget)
                }));
    }
    let parsed = if positioning {
        gpos::parse(ty, data).map(|view| children::pos(view, data, fmt, budget))
    } else {
        gsub::parse(ty, data).map(|view| children::subst(view, budget))
    };
    match parsed {
        Err(ReadError::UnknownFormat(_)) => return true,
        Err(_) => return false,
        Ok(false) => return false,
        Ok(true) => (),
    }
    let context = if positioning { (7, 8) } else { (5, 6) };
    if (ty == context.0 || ty == context.1) && matches!(fmt, 3 | 6) {
        return true;
    }
    let wide = match (positioning, ty) {
        (false, 1) | (true, 1 | 2) => fmt >= 3,
        (_, t) if t == context.0 || t == context.1 => fmt >= 4,
        _ => fmt == 2,
    };
    let width: usize = if wide { 4 } else { 2 };
    let reference = |field: usize, check: fn(&[u8]) -> bool| {
        uint(data, field, width).is_some_and(|off| target(data, off, check))
    };
    if !reference(2, coverage) {
        return false;
    }
    if matches!(fmt, 2 | 5) && (ty == context.0 || ty == context.1) {
        let count = if ty == context.0 { 1 } else { 3 };
        return (1usize..=count).all(|i| {
            reference(
                2usize.saturating_add(i.saturating_mul(width)),
                super::classes,
            )
        });
    }
    if positioning && matches!(ty, 4..=6) {
        return reference(2usize.saturating_add(width), coverage) && marks(data, ty, wide, budget);
    }
    true
}

fn lookup(value: Lookup<'_>, positioning: bool, visited: &mut usize, budget: &mut Budget) -> bool {
    let extension = if positioning {
        gpos::EXTENSION
    } else {
        gsub::EXTENSION
    };
    *visited = visited.saturating_add(value.num_subtables());
    *visited < 0x4000
        && value.extension_types_agree(extension)
        && (0..value.num_subtables()).all(|i| {
            !known(value.lookup_type(), positioning)
                || value.subtable_offset(i) == Some(0)
                || value.subtable_data(i).is_some_and(|data| {
                    subtable(data, value.lookup_type(), positioning, extension, budget)
                })
        })
}

pub(super) fn inspect(list: LookupList<'_>, positioning: bool, budget: &mut Budget) -> bool {
    let mut visited = 0;
    (0..list.len()).all(|i| match list.lookup(i) {
        Ok(Some(value)) => lookup(value, positioning, &mut visited, budget),
        Ok(None) => true,
        Err(_) => false,
    })
}

fn marks(data: &[u8], ty: u16, wide: bool, budget: &mut Budget) -> bool {
    let (cw, w, count_at, marks_at, second_at) = if wide {
        (4, 3, 10, 12, 16)
    } else {
        (2, 2, 6, 8, 10)
    };
    let Some(count) = be::u16_at(data, count_at) else {
        return false;
    };
    uint(data, marks_at, cw).is_some_and(|off| {
        target(data, off, |m| {
            let Some(n) = uint(m, 0, w).and_then(|n| usize::try_from(n).ok()) else {
                return false;
            };
            let stride = w.saturating_add(2);
            let Some(rows) = be::slice_at(m, w, n.saturating_mul(stride)) else {
                return false;
            };
            budget.take(rows.len())
                && rows
                    .chunks_exact(stride)
                    .all(|r| uint(r, 2, w).is_some_and(|off| target(m, off, anchor)))
        })
    }) && uint(data, second_at, cw).is_some_and(|off| {
        target(data, off, |t| match ty {
            gpos::MARK_LIG => gpos::LigatureArray::new(t, count, w).is_ok(),
            gpos::MARK_BASE => gpos::AnchorMatrix::new(t, count, w, w).is_ok(),
            _ => gpos::AnchorMatrix::new(t, count, 2, w).is_ok(),
        })
    })
}

fn anchor(data: &[u8]) -> bool {
    match gpos::Anchor::new(data) {
        Ok(gpos::Anchor::Format3 { .. }) => [6, 8].into_iter().all(|at| {
            be::u16_at(data, at).is_some_and(|off| {
                target(data, u32::from(off), |d| {
                    crate::device::Device::new(d).is_ok()
                })
            })
        }),
        Ok(_) | Err(ReadError::UnknownFormat(_)) => true,
        Err(_) => false,
    }
}
