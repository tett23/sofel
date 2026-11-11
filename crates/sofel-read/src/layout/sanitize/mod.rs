//! Shaping-time admission of OpenType Layout tables (ADR 0059).
mod children;
mod gdef;
mod lists;
mod lookups;
mod variations;
use super::{LayoutTable, LookupList, VERSION_1_2, header_size};
use crate::{Tag, gpos, gsub};

/// Whether a face has a usable copy of a layout table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// No table record was present.
    Absent,
    /// Shaping may use this table.
    Accepted,
    /// The table failed its load-time checks.
    Discarded,
}

fn target(data: &[u8], offset: u32, check: impl FnOnce(&[u8]) -> bool) -> bool {
    offset == 0
        || usize::try_from(offset)
            .ok()
            .and_then(|n| data.get(n..))
            .is_some_and(check)
}

fn layout_ok(data: &[u8], positioning: bool) -> bool {
    let Ok(layout) = LayoutTable::new(data) else {
        return false;
    };
    let mut budget = children::Budget::new(data.len());
    if !budget.take(header_size(layout.version())) {
        return false;
    }
    // HarfBuzz visits all three narrow lists first. In 1.2 a non-null
    // wide list replaces that result, sharing the same operation budget.
    let mut visited = 0;
    let mut list = |field, wide| {
        let width = if wide { 4 } else { 2 };
        let offset = super::uint(data, field, width).unwrap_or(0);
        target(data, offset, |d| match field {
            4 | 14 => lists::scripts(d, &mut budget),
            6 | 18 => lists::features(d, &mut budget),
            _ => LookupList::new(d, wide)
                .is_ok_and(|l| lookups::inspect(l, positioning, &mut budget, &mut visited)),
        })
    };
    let mut scripts = list(4, false);
    let mut features = list(6, false);
    let mut lookups = list(8, false);
    if layout.version() >= VERSION_1_2 {
        for (field, result) in [(14, &mut scripts), (18, &mut features), (22, &mut lookups)] {
            if crate::be::u32_at(data, field).is_some_and(|offset| offset != 0) {
                *result = list(field, true);
            }
        }
    }
    scripts && features && lookups && variations::inspect(&layout, &mut budget)
}

/// Determines the admission state before applying the GDEF blocklist.
pub fn assess(tag: Tag, data: Option<&[u8]>) -> Admission {
    let Some(data) = data else {
        return Admission::Absent;
    };
    let accepted = if tag == gsub::TAG || tag == gpos::TAG {
        layout_ok(data, tag == gpos::TAG)
    } else if tag == crate::gdef::TAG {
        gdef::assess(data)
    } else {
        false
    };
    if accepted {
        Admission::Accepted
    } else {
        Admission::Discarded
    }
}
