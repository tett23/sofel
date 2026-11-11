//! Structural damage found while validating a layout table (ADR 0059).
//!
//! This adapter preserves every existing validator diagnostic and adds a
//! marker when a referenced branch could not be inspected. It does not
//! allocate or deduplicate: callers count distinct `(table, source)`
//! locations after collecting the events for one face.

use crate::be::{self, to_u32, to_usize};
use crate::layout::{Feature, LayoutTable, Lookup, Script, uint};
use crate::{Diagnostic, Kind, ReadError, Tag, gdef, gpos, gsub};

const EXTRA_LIMIT: usize = 1000;

/// An original layout diagnostic or a limit on the completed walk.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DamageEvent {
    /// The same diagnostic emitted by the existing table validator.
    Diagnostic(Diagnostic),
    /// A reference at `source` could not be followed far enough to check
    /// its descendants. `reason` is the original diagnostic kind.
    Unchecked {
        /// The layout table containing the reference.
        table: Tag,
        /// Byte position of the reference field in the table.
        source: u32,
        /// Why the descendants could not be checked.
        reason: Kind,
    },
    /// The table walk reached its operation or diagnostic limit.
    WorkLimit {
        /// The layout table whose walk stopped.
        table: Tag,
    },
}

/// Validates one GSUB, GPOS, or GDEF table and reports its structural
/// damage. A caller using this function should not also call the
/// corresponding `validate` function for the same table: the
/// [`DamageEvent::Diagnostic`] events already contain those diagnostics.
///
/// Unsupported tags emit no events. `mark_sets` is passed to GSUB and
/// GPOS exactly as in their ordinary validators.
pub fn inspect_damage(
    tag: Tag,
    data: &[u8],
    mark_sets: Option<u16>,
    report: &mut dyn FnMut(DamageEvent),
) {
    let mut forward = |diagnostic: Diagnostic| forward(diagnostic, report);
    if tag == gsub::TAG {
        gsub::validate(data, mark_sets, &mut forward);
    } else if tag == gpos::TAG {
        gpos::validate(data, mark_sets, &mut forward);
    } else if tag == gdef::TAG {
        gdef::validate(data, &mut forward);
    }
    if tag == gsub::TAG || tag == gpos::TAG {
        scan_partial_lists(tag, data, report);
    }
}

fn forward(diagnostic: Diagnostic, report: &mut dyn FnMut(DamageEvent)) {
    let kind = diagnostic.kind;
    let source = diagnostic.offset;
    report(DamageEvent::Diagnostic(diagnostic));
    match kind {
        Kind::OffsetOutOfBounds { .. }
        | Kind::Truncated { .. }
        | Kind::UnknownFormat { .. }
        | Kind::UnknownVersion { .. } => {
            report(DamageEvent::Unchecked {
                table: diagnostic.table,
                source,
                reason: kind,
            });
        }
        Kind::WorkLimitExceeded => report(DamageEvent::WorkLimit {
            table: diagnostic.table,
        }),
        _ => {}
    }
}

#[derive(Clone, Copy)]
enum PartialList {
    Scripts,
    Features,
    Lookups { wide: bool },
}

impl PartialList {
    fn size(self) -> usize {
        match self {
            Self::Scripts | Self::Features => 6,
            Self::Lookups { wide: true } => 4,
            Self::Lookups { wide: false } => 2,
        }
    }

    fn field(self) -> usize {
        match self {
            Self::Scripts | Self::Features => 4,
            Self::Lookups { .. } => 0,
        }
    }

    fn child(self, data: &[u8]) -> Result<(), ReadError> {
        match self {
            Self::Scripts => Script::new(data).map(|_| ()),
            Self::Features => Feature::new(data).map(|_| ()),
            Self::Lookups { .. } => Lookup::new(data).map(|_| ()),
        }
    }
}

fn scan_partial_lists(tag: Tag, data: &[u8], report: &mut dyn FnMut(DamageEvent)) {
    let Ok(header) = LayoutTable::new(data) else {
        return;
    };
    for (offset, kind) in [
        (header.script_list_offset(), PartialList::Scripts),
        (header.feature_list_offset(), PartialList::Features),
        (
            header.lookup_list_offset(),
            PartialList::Lookups {
                wide: header.has_wide_lookup_list(),
            },
        ),
    ] {
        if offset != 0 {
            scan_partial_list(tag, data, to_usize(offset), kind, report);
        }
    }
}

fn scan_partial_list(
    tag: Tag,
    data: &[u8],
    at: usize,
    kind: PartialList,
    report: &mut dyn FnMut(DamageEvent),
) {
    let Some(count) = be::u16_at(data, at).map(usize::from) else {
        return;
    };
    let first = at.saturating_add(2);
    let width = kind.size();
    let needed = first.saturating_add(count.saturating_mul(width));
    if needed <= data.len() {
        return;
    }
    let readable = data.len().saturating_sub(first) / width;
    let scanned = readable.min(count).min(EXTRA_LIMIT);
    for i in 0..scanned {
        let field = first
            .saturating_add(i.saturating_mul(width))
            .saturating_add(kind.field());
        let value_width = if matches!(kind, PartialList::Lookups { wide: true }) {
            4
        } else {
            2
        };
        let Some(offset) = uint(data, field, value_width) else {
            continue;
        };
        if offset == 0 {
            continue;
        }
        let child_at = at.saturating_add(to_usize(offset));
        let result = data
            .get(child_at..)
            .ok_or(ReadError::OutOfBounds)
            .and_then(|tail| kind.child(tail));
        if let Err(error) = result {
            let problem = match error {
                ReadError::Truncated { needed, .. } => Kind::OffsetOutOfBounds {
                    offset: to_u32(child_at),
                    length: to_u32(needed),
                    available: to_u32(data.len()),
                },
                ReadError::UnknownFormat(found) => Kind::UnknownFormat {
                    found: u32::from(found),
                },
                ReadError::UnknownVersion(found) => Kind::UnknownVersion { found },
                _ => Kind::OffsetOutOfBounds {
                    offset: to_u32(child_at),
                    length: 0,
                    available: to_u32(data.len()),
                },
            };
            forward(Diagnostic::new(tag, to_u32(field), problem), report);
        }
    }
    let unreadable = first.saturating_add(scanned.saturating_mul(width));
    let source = unreadable.saturating_add(kind.field());
    let source = if source < data.len() { source } else { at };
    report(DamageEvent::Unchecked {
        table: tag,
        source: to_u32(source),
        reason: Kind::Truncated {
            needed: to_u32(needed),
            available: to_u32(data.len()),
        },
    });
    if readable > EXTRA_LIMIT {
        forward(Diagnostic::new(tag, 0, Kind::WorkLimitExceeded), report);
    }
}
