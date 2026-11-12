//! `gvar` and `GVAR` subsetting without instancing (ADR 0022 stage 10).

use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
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
        records.push((new_gid, Cow::Borrowed(bytes)));
    }
    if plan.var.instancing()
        && plan.var.n.retained_axis_indices.len() < usize::from(table.axis_count())
    {
        return pin_records(plan, tag, &table, &records, serializer);
    }
    let shared_len = usize::from(table.axis_count())
        .checked_mul(usize::from(table.shared_tuple_count()))
        .and_then(|n| n.checked_mul(2))
        .ok_or(SubsetError::Limit { tag })?;
    let source_shared_at = usize::try_from(u32::from_be_bytes(
        src.get(8..12)
            .and_then(|data| data.try_into().ok())
            .ok_or(SubsetError::Malformed { tag })?,
    ))
    .map_err(|_| SubsetError::Malformed { tag })?;
    // HarfBuzz preserves the count but emits no shared tuple bytes for
    // a zero offset. Copying from offset zero would copy the gvar header.
    let shared = if source_shared_at == 0 {
        &[][..]
    } else {
        src.get(source_shared_at..source_shared_at.saturating_add(shared_len))
            .ok_or(SubsetError::Malformed { tag })?
    };
    serialize_records(
        plan,
        tag,
        table.axis_count(),
        table.shared_tuple_count(),
        shared,
        &records,
        serializer,
    )
}

/// Writes the shared peak tuples and the output glyph records, choosing
/// 16-bit or 32-bit offsets from the compiled size.
fn serialize_records(
    plan: &Plan,
    tag: Tag,
    axes: u16,
    shared_count: u16,
    shared: &[u8],
    records: &[(u32, Cow<'_, [u8]>)],
    serializer: &mut Serializer,
) -> Result<bool, SubsetError> {
    let glyph_count = plan.num_output_glyphs();
    let large = tag == Tag::new(b"GVAR");
    let padded_len = records
        .iter()
        .try_fold(0usize, |total, (_, bytes)| {
            total.checked_add(bytes.len())?.checked_add(bytes.len() % 2)
        })
        .ok_or(SubsetError::Limit { tag })?;
    let long = padded_len > 0x1fffe || plan.flags.has(Flags::IFTB_REQUIREMENTS);
    let offset_width = if long { 4usize } else { 2 };
    let offsets_len = usize::try_from(glyph_count)
        .ok()
        .and_then(|n| n.checked_add(1))
        .and_then(|n| n.checked_mul(offset_width))
        .ok_or(SubsetError::Limit { tag })?;
    let header_len = if large { 21usize } else { 20 };
    let shared_at = header_len
        .checked_add(offsets_len)
        .ok_or(SubsetError::Limit { tag })?;
    let data_at = shared_at
        .checked_add(shared.len())
        .ok_or(SubsetError::Limit { tag })?;

    serializer.u32(0x0001_0000);
    serializer.u16(axes);
    serializer.u16(shared_count);
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

struct PinnedTuple<'a> {
    peak: Vec<u8>,
    intermediate: Option<Vec<u8>>,
    private: bool,
    deltas: &'a [u8],
}

fn pin_records(
    plan: &Plan,
    tag: Tag,
    table: &Gvar<'_>,
    records: &[(u32, Cow<'_, [u8]>)],
    serializer: &mut Serializer,
) -> Result<bool, SubsetError> {
    use sofel_font::read::gvar::{PackedPoints, Tuple};
    let bad = SubsetError::Malformed { tag };
    let kept = &plan.var.n.retained_axis_indices;
    let axes = usize::from(table.axis_count());
    let pin = (0..axes).find(|i| !kept.contains(i)).ok_or(bad)?;
    if kept.len().checked_add(1) != Some(axes) {
        return Err(SubsetError::Unsupported {
            tag,
            reason: "gvar instancing across multiple pinned axes",
        });
    }
    let project = |tuple: Tuple<'_>| -> Result<Vec<u8>, SubsetError> {
        let mut coords = Vec::new();
        for &i in kept {
            coords.extend(tuple.axis(i).ok_or(bad)?.to_be_bytes());
        }
        Ok(coords)
    };
    let mut glyphs = Vec::new();
    let mut frequencies = BTreeMap::<Vec<u8>, usize>::new();
    for ((gid, raw), &(_, old)) in records.iter().zip(plan.new_to_old_glyphs()) {
        if raw.is_empty() {
            continue;
        }
        let glyph = table.glyph_data(old).ok_or(bad)?;
        let mut tuples = Vec::new();
        for tuple in glyph.tuples() {
            let tuple = tuple.ok_or(bad)?;
            let peak = tuple
                .embedded_peak()
                .or_else(|| table.shared_tuple(tuple.shared_peak_index()?))
                .ok_or(bad)?;
            let projected = project(peak)?;
            if peak.axis(pin).ok_or(bad)? != 0 {
                if projected.iter().any(|&b| b != 0) {
                    return Err(SubsetError::Unsupported {
                        tag,
                        reason: "rebasing a gvar tuple shared by pinned and retained axes",
                    });
                }
                // The pinned-only contribution is already in glyf and the phantom points.
                continue;
            }
            let intermediate = tuple
                .intermediate()
                .map(|(start, end)| -> Result<_, SubsetError> {
                    let mut region = project(start)?;
                    region.extend(project(end)?);
                    Ok(region)
                })
                .transpose()?;
            let count = frequencies.entry(projected.clone()).or_default();
            *count = count.saturating_add(1);
            tuples.push(PinnedTuple {
                peak: projected,
                intermediate,
                private: tuple.private_points(),
                deltas: tuple.data(),
            });
        }
        if tuples.is_empty() {
            continue;
        }
        let mut points = &[][..];
        if glyph.has_shared_points() {
            let offset = sofel_font::read::be::u16_at(raw, 2).ok_or(bad)?;
            let bytes = raw.get(usize::from(offset)..).ok_or(bad)?;
            let (_, length) = PackedPoints::parse(bytes).ok_or(bad)?;
            points = bytes.get(..length).ok_or(bad)?;
        }
        glyphs.push((*gid, points, tuples));
    }
    if glyphs.is_empty() {
        return Ok(false);
    }
    let mut peaks: Vec<_> = frequencies.into_iter().filter(|(_, n)| *n > 1).collect();
    peaks.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    peaks.truncate(4096);
    let mut indices = BTreeMap::new();
    let mut shared = Vec::new();
    for (index, (peak, _)) in peaks.iter().enumerate() {
        indices.insert(peak.as_slice(), u16::try_from(index).map_err(|_| bad)?);
        shared.extend(peak);
    }
    let mut output = Vec::new();
    for (gid, points, tuples) in glyphs {
        let count = u16::try_from(tuples.len()).map_err(|_| bad)?;
        let mut headers = Vec::new();
        let mut payload = points.to_vec();
        for tuple in tuples {
            let index = indices.get(tuple.peak.as_slice());
            let flags = index.copied().unwrap_or(0x8000)
                | if tuple.intermediate.is_some() {
                    0x4000
                } else {
                    0
                }
                | if tuple.private { 0x2000 } else { 0 };
            headers.extend(
                u16::try_from(tuple.deltas.len())
                    .map_err(|_| bad)?
                    .to_be_bytes(),
            );
            headers.extend(flags.to_be_bytes());
            if index.is_none() {
                headers.extend(tuple.peak);
            }
            if let Some(region) = tuple.intermediate {
                headers.extend(region);
            }
            payload.extend(tuple.deltas);
        }
        let mut record = Vec::new();
        record.extend((count | if points.is_empty() { 0 } else { 0x8000 }).to_be_bytes());
        record.extend(
            u16::try_from(headers.len().checked_add(4).ok_or(bad)?)
                .map_err(|_| bad)?
                .to_be_bytes(),
        );
        record.extend(headers);
        record.extend(payload);
        output.push((gid, Cow::Owned(record)));
    }
    serialize_records(
        plan,
        tag,
        u16::try_from(kept.len()).map_err(|_| bad)?,
        u16::try_from(peaks.len()).map_err(|_| bad)?,
        &shared,
        &output,
        serializer,
    )
}
