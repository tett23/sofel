//! Removes pinned axes from an unchanged avar version 1 map (ADR 0061).

use alloc::vec::Vec;

use sofel_font::read::{Tag, avar::Avar};

use crate::error::SubsetError;
use crate::plan::Plan;

const TAG: Tag = Tag::new(b"avar");

/// `avar::subset` for a plan that only pins axes at their defaults.
pub(crate) fn subset(plan: &Plan, src: &[u8]) -> Result<Vec<u8>, SubsetError> {
    let avar = Avar::new(src).map_err(|_| SubsetError::Malformed { tag: TAG })?;
    if avar.major_version() != 1 {
        return Err(SubsetError::Unsupported {
            tag: TAG,
            reason: "avar version 2 instancing",
        });
    }
    let count = u16::try_from(plan.var.n.retained_axis_indices.len())
        .map_err(|_| SubsetError::Failed { tag: TAG })?;
    let mut out = Vec::from([0, 1, 0, 0, 0, 0]);
    out.extend_from_slice(&count.to_be_bytes());
    for &old in &plan.var.n.retained_axis_indices {
        let map = avar
            .segment_map(old)
            .ok_or(SubsetError::Malformed { tag: TAG })?;
        let len = u16::try_from(map.len()).map_err(|_| SubsetError::Failed { tag: TAG })?;
        out.extend_from_slice(&len.to_be_bytes());
        for index in 0..map.len() {
            let (from, to) = map.pair(index).ok_or(SubsetError::Malformed { tag: TAG })?;
            out.extend_from_slice(&from.0.to_be_bytes());
            out.extend_from_slice(&to.0.to_be_bytes());
        }
    }
    Ok(out)
}
