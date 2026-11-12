//! Rewrites fvar's axes and instances when an axis is limited (ADR 0061).

use alloc::vec::Vec;

use sofel_font::read::{Tag, be, fvar::Fvar};
use sofel_geom::float::hb_roundf32;

use crate::error::SubsetError;
use crate::instance::{f64_to_i32, fvar_instance_kept};
use crate::plan::Plan;

const TAG: Tag = Tag::new(b"fvar");

fn fixed(value: f32) -> [u8; 4] {
    f64_to_i32(hb_roundf32(value * 65536.0)).to_be_bytes()
}

fn write_at(out: &mut [u8], at: usize, bytes: &[u8]) -> Result<(), SubsetError> {
    let end = at.saturating_add(bytes.len());
    let slot = out
        .get_mut(at..end)
        .ok_or(SubsetError::Malformed { tag: TAG })?;
    slot.copy_from_slice(bytes);
    Ok(())
}

/// `fvar::subset` for a plan whose variation data can be left unchanged.
pub(crate) fn subset(plan: &Plan, src: &[u8]) -> Result<Vec<u8>, SubsetError> {
    let fvar = Fvar::new(src).map_err(|_| SubsetError::Malformed { tag: TAG })?;
    let first = usize::from(be::u16_at(src, 4).ok_or(SubsetError::Malformed { tag: TAG })?);
    let kept = &plan.var.n.retained_axis_indices;
    let has_postscript =
        fvar.instance_size() >= fvar.axis_count().saturating_mul(4).saturating_add(6);
    let instance_size = kept
        .len()
        .saturating_mul(4)
        .saturating_add(if has_postscript { 6 } else { 4 });
    let axis_count = u16::try_from(kept.len()).map_err(|_| SubsetError::Failed { tag: TAG })?;
    let instance_size_u16 =
        u16::try_from(instance_size).map_err(|_| SubsetError::Failed { tag: TAG })?;
    let mut out = src
        .get(..16)
        .ok_or(SubsetError::Malformed { tag: TAG })?
        .to_vec();
    write_at(&mut out, 4, &16u16.to_be_bytes())?;
    write_at(&mut out, 8, &axis_count.to_be_bytes())?;
    write_at(&mut out, 14, &instance_size_u16.to_be_bytes())?;
    for &old in kept {
        let at = first.saturating_add(old.saturating_mul(20));
        let axis = src
            .get(at..at.saturating_add(20))
            .ok_or(SubsetError::Malformed { tag: TAG })?;
        let tag = fvar
            .axis(old)
            .ok_or(SubsetError::Malformed { tag: TAG })?
            .tag;
        let start = out.len();
        out.extend_from_slice(axis);
        if let Some(range) = plan.var.user().get(&tag) {
            for (offset, value) in [(4, range.min), (8, range.default), (12, range.max)] {
                write_at(&mut out, start.saturating_add(offset), &fixed(value))?;
            }
        }
    }
    let mut count = 0u16;
    let old_instance_size = fvar.instance_size();
    let instances_at = first.saturating_add(fvar.axis_count().saturating_mul(20));
    for index in 0..fvar.instance_count() {
        let at = instances_at.saturating_add(index.saturating_mul(old_instance_size));
        let record = src
            .get(at..at.saturating_add(old_instance_size))
            .ok_or(SubsetError::Malformed { tag: TAG })?;
        if !fvar_instance_kept(&fvar, index, plan.var.user()) {
            continue;
        }
        out.extend_from_slice(record.get(..4).ok_or(SubsetError::Malformed { tag: TAG })?);
        for &old in kept {
            let at = 4usize.saturating_add(old.saturating_mul(4));
            out.extend_from_slice(
                record
                    .get(at..at.saturating_add(4))
                    .ok_or(SubsetError::Malformed { tag: TAG })?,
            );
        }
        if has_postscript {
            let at = 4usize.saturating_add(fvar.axis_count().saturating_mul(4));
            out.extend_from_slice(
                record
                    .get(at..at.saturating_add(2))
                    .ok_or(SubsetError::Malformed { tag: TAG })?,
            );
        }
        count = count
            .checked_add(1)
            .ok_or(SubsetError::Failed { tag: TAG })?;
    }
    write_at(&mut out, 12, &count.to_be_bytes())?;
    Ok(out)
}
