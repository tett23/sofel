//! Shaping-time admission of OpenType Layout tables (ADR 0059).
mod children;
mod gdef;
mod lists;
mod lookups;
mod variations;
use super::{ClassDef, LayoutTable};
use crate::{ReadError, Tag, gpos, gsub};

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

fn classes(data: &[u8]) -> bool {
    matches!(
        ClassDef::new(data),
        Ok(_) | Err(ReadError::UnknownFormat { .. })
    )
}

fn layout_ok(data: &[u8], positioning: bool) -> bool {
    let Ok(layout) = LayoutTable::new(data) else {
        return false;
    };
    let mut budget = children::Budget::new(data.len());
    target(data, layout.script_list_offset(), |d| {
        lists::scripts(d, &mut budget)
    }) && target(data, layout.feature_list_offset(), |d| {
        lists::features(d, &mut budget)
    }) && layout
        .lookup_list()
        .is_ok_and(|list| list.is_none_or(|list| lookups::inspect(list, positioning, &mut budget)))
        && variations::inspect(&layout, &mut budget)
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
