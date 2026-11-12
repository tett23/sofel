//! VARC subsetting for component records without auxiliary references.
//! The remaining VARC forms stay unsupported until sofel ADR 0067 is complete.

use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::varc::Varc;
use sofel_font::read::{GlyphId, Tag};
use sofel_write::{Errors, Serializer};

use crate::error::SubsetError;
use crate::input::Flags;
use crate::layout::common::Sub;
use crate::plan::Plan;

use super::{Tables, cff};

/// The currently supported auxiliary form: one format-1 axis-range
/// condition with no nested references. An absent list yields an empty slice.
pub(crate) fn simple_condition_bytes(src: &[u8]) -> Option<&[u8]> {
    let offset = u32::from_be_bytes(src.get(12..16)?.try_into().ok()?) as usize;
    if offset == 0 {
        return Some(&[]);
    }
    let bytes = src.get(offset..offset.checked_add(16)?)?;
    (u32::from_be_bytes(bytes.get(0..4)?.try_into().ok()?) == 1
        && u32::from_be_bytes(bytes.get(4..8)?.try_into().ok()?) == 8
        && u16::from_be_bytes(bytes.get(8..10)?.try_into().ok()?) == 1)
        .then_some(bytes)
}

#[allow(clippy::cast_possible_truncation)] // Variable-length bytes deliberately keep low octets.
fn varint(out: &mut Vec<u8>, value: u32) {
    if value < 0x80 {
        out.push(value as u8);
    } else if value < 0x4000 {
        out.extend_from_slice(&[(0x80 | (value >> 8)) as u8, value as u8]);
    } else if value < 0x20_0000 {
        out.extend_from_slice(&[
            (0xc0 | (value >> 16)) as u8,
            (value >> 8) as u8,
            value as u8,
        ]);
    } else if value < 0x1000_0000 {
        out.extend_from_slice(&[
            (0xe0 | (value >> 24)) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ]);
    } else {
        out.push(0xf0);
        out.extend_from_slice(&value.to_be_bytes());
    }
}

fn rewrite_record(varc: &Varc<'_>, old_gid: u32, plan: &Plan) -> Result<Vec<u8>, SubsetError> {
    let tag = Tag::new(b"VARC");
    let source = varc
        .record(GlyphId(old_gid))
        .ok_or(SubsetError::Malformed { tag })?;
    if plan.flags.has(Flags::RETAIN_GIDS) {
        return Ok(source.to_vec());
    }
    let mut result = Vec::with_capacity(source.len());
    let mut position = 0usize;
    for part in varc
        .components(GlyphId(old_gid))
        .ok_or(SubsetError::Malformed { tag })?
    {
        let part = part.ok_or(SubsetError::Malformed { tag })?;
        let child = plan
            .glyphs
            .new_gid(part.glyph.0)
            .ok_or(SubsetError::Failed { tag })?;
        let use_wide = part.glyph_size == 3 || child > 0xffff;
        let flags = if use_wide {
            part.flags | (1 << 12)
        } else {
            part.flags
        };
        varint(&mut result, flags);
        let encoded = child.to_be_bytes();
        result.extend_from_slice(if use_wide {
            &encoded[1..]
        } else {
            &encoded[2..]
        });
        let end = position
            .checked_add(part.size)
            .ok_or(SubsetError::Failed { tag })?;
        let remainder = position
            .checked_add(part.glyph_offset)
            .and_then(|at| at.checked_add(part.glyph_size))
            .ok_or(SubsetError::Failed { tag })?;
        result.extend_from_slice(
            source
                .get(remainder..end)
                .ok_or(SubsetError::Malformed { tag })?,
        );
        position = end;
    }
    Ok(result)
}

/// Rewrites the covered glyphs, remapping component glyph IDs and preserving
/// the order of each component record.
pub(crate) fn subset(
    plan: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _out: &mut Tables,
) -> Result<bool, SubsetError> {
    let varc = Varc::new(src).map_err(|_| SubsetError::Malformed { tag })?;
    let conditions = simple_condition_bytes(src).ok_or(SubsetError::Unsupported {
        tag,
        reason: "VARC condition format",
    })?;
    let mut gids = Vec::new();
    let mut records = Vec::new();
    let mut uses_condition = false;
    for (first, last) in varc.coverage().ranges() {
        for old in first.0..=last.0 {
            if plan.glyphset_varced.contains(old) {
                gids.push(
                    plan.glyphs
                        .new_gid(old)
                        .ok_or(SubsetError::Failed { tag })?,
                );
                records.push(rewrite_record(&varc, old, plan)?);
                uses_condition |= varc
                    .components(GlyphId(old))
                    .ok_or(SubsetError::Malformed { tag })?
                    .any(|part| part.is_some_and(|part| part.condition_index.is_some()));
            }
        }
    }
    if records.is_empty() {
        return Ok(false);
    }
    s.extend(src.get(..4).ok_or(SubsetError::Malformed { tag })?);
    let coverage_field = s.zeros(4);
    s.zeros(4); // varStore absent
    let condition_field = s.zeros(4);
    s.zeros(4); // axisIndicesList absent
    let records_field = s.u32(24);
    if records_field != 20 || !cff::index_n(s, &records, 1, true) {
        return Err(SubsetError::Failed { tag });
    }
    if uses_condition {
        if conditions.is_empty() {
            return Err(SubsetError::Malformed { tag });
        }
        let at = s.len();
        if !s.check_assign_u32(condition_field, at as u64, Errors::INT_OVERFLOW) {
            return Err(SubsetError::OffsetOverflow { tag });
        }
        s.extend(conditions);
    }
    let at = s.len();
    if !s.check_assign_u32(coverage_field, at as u64, Errors::INT_OVERFLOW) {
        return Err(SubsetError::OffsetOverflow { tag });
    }
    let mut context = Sub::new(plan, s, false);
    if !context.coverage(&gids) {
        return match context.unsupported {
            Some(reason) => Err(SubsetError::Unsupported { tag, reason }),
            None => Err(SubsetError::Failed { tag }),
        };
    }
    Ok(true)
}
