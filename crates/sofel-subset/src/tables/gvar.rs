//! `gvar` and `GVAR` subsetting without instancing (ADR 0022 stage 10).

use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::{Tag, gvar::Gvar};
use sofel_write::Serializer;

use crate::error::SubsetError;
use crate::input::Flags;
use crate::plan::Plan;

use super::Tables;

pub(super) fn subset(
    plan: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    serializer: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let table = Gvar::new(src, tag).map_err(|_| SubsetError::Malformed { tag })?;
    let large = tag == Tag::new(b"GVAR");
    let glyph_count = plan.num_output_glyphs();
    if glyph_count > if large { 0x00ff_ffff } else { u16::MAX as u32 } {
        return Err(SubsetError::Limit { tag });
    }

    let data_offset_at = if large { 17..21 } else { 16..20 };
    let source_data_at = usize::try_from(u32::from_be_bytes(
        src.get(data_offset_at)
            .and_then(|data| data.try_into().ok())
            .ok_or(SubsetError::Malformed { tag })?,
    ))
    .map_err(|_| SubsetError::Malformed { tag })?;
    let mut records = Vec::with_capacity(plan.new_to_old_glyphs().len());
    let mut padded_len = 0usize;
    for &(new_gid, old_gid) in plan.new_to_old_glyphs() {
        let bytes = if (new_gid == 0 && !plan.flags.has(Flags::NOTDEF_OUTLINE))
            || old_gid >= table.glyph_count()
        {
            &[][..]
        } else {
            let first = table.glyph_offset(old_gid).unwrap_or(0);
            let last = table.glyph_offset(old_gid.saturating_add(1)).unwrap_or(0);
            source_data_at
                .checked_add(first)
                .zip(source_data_at.checked_add(last))
                .and_then(|(start, end)| src.get(start..end))
                .filter(|data| data.len() >= 4)
                .unwrap_or(&[])
        };
        padded_len = bytes
            .len()
            .checked_add(bytes.len() % 2)
            .and_then(|n| padded_len.checked_add(n))
            .ok_or(SubsetError::Limit { tag })?;
        records.push((new_gid, bytes));
    }

    let long = padded_len > 0x1fffe || plan.flags.has(Flags::IFTB_REQUIREMENTS);
    let offset_width = if long { 4usize } else { 2 };
    let count = usize::try_from(glyph_count).map_err(|_| SubsetError::Limit { tag })?;
    let offsets_len = count
        .checked_add(1)
        .and_then(|n| n.checked_mul(offset_width))
        .ok_or(SubsetError::Limit { tag })?;
    let header_len = if large { 21usize } else { 20 };
    let shared_len = usize::from(table.axis_count())
        .checked_mul(usize::from(table.shared_tuple_count()))
        .and_then(|n| n.checked_mul(2))
        .ok_or(SubsetError::Limit { tag })?;
    let shared_at = header_len
        .checked_add(offsets_len)
        .ok_or(SubsetError::Limit { tag })?;
    let data_at = shared_at
        .checked_add(shared_len)
        .ok_or(SubsetError::Limit { tag })?;
    let source_shared_at = usize::try_from(u32::from_be_bytes(
        src.get(8..12)
            .and_then(|data| data.try_into().ok())
            .ok_or(SubsetError::Malformed { tag })?,
    ))
    .map_err(|_| SubsetError::Malformed { tag })?;
    let shared = src
        .get(source_shared_at..source_shared_at.saturating_add(shared_len))
        .ok_or(SubsetError::Malformed { tag })?;

    serializer.u32(0x0001_0000);
    serializer.u16(table.axis_count());
    serializer.u16(table.shared_tuple_count());
    serializer.u32(u32::try_from(shared_at).map_err(|_| SubsetError::Limit { tag })?);
    if large {
        serializer.u24(glyph_count);
    } else {
        serializer.u16(u16::try_from(glyph_count).map_err(|_| SubsetError::Limit { tag })?);
    }
    serializer.u16(u16::from(long));
    serializer.u32(u32::try_from(data_at).map_err(|_| SubsetError::Limit { tag })?);
    let offsets_at = serializer.zeros(offsets_len);
    serializer.extend(shared);

    let mut data_offset = 0usize;
    let mut record = records.iter().peekable();
    for new_gid in 0..glyph_count {
        if let Some((gid, bytes)) = record.peek().copied()
            && *gid == new_gid
        {
            serializer.extend(bytes);
            data_offset = data_offset
                .checked_add(bytes.len())
                .ok_or(SubsetError::Limit { tag })?;
            if !long && bytes.len() % 2 != 0 {
                serializer.u8(0);
                data_offset = data_offset
                    .checked_add(1)
                    .ok_or(SubsetError::Limit { tag })?;
            }
            record.next();
        }
        let value = if long { data_offset } else { data_offset / 2 };
        let index = usize::try_from(new_gid).map_err(|_| SubsetError::Limit { tag })?;
        let at = index
            .checked_add(1)
            .and_then(|n| n.checked_mul(offset_width))
            .and_then(|n| offsets_at.checked_add(n))
            .ok_or(SubsetError::Limit { tag })?;
        if long {
            serializer.set_u32(
                at,
                u32::try_from(value).map_err(|_| SubsetError::Limit { tag })?,
            );
        } else {
            serializer.set_u16(
                at,
                u16::try_from(value).map_err(|_| SubsetError::Limit { tag })?,
            );
        }
    }
    Ok(true)
}
