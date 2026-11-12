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
    font: &Font<'_>,
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
        return pin_records(plan, font, tag, &table, &records, serializer);
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
    points: &'a [u8],
    deltas: Cow<'a, [u8]>,
}

// Match the run boundaries chosen by HarfBuzz TupleValues::compile_unsafe.
fn pack_deltas(mut values: &[i16], output: &mut Vec<u8>) {
    while let Some(&first) = values.first() {
        let byte = |value: i16| i8::try_from(value).is_ok();
        let flag = if first == 0 {
            0x80
        } else if byte(first) {
            0
        } else {
            0x40
        };
        let length = values
            .iter()
            .enumerate()
            .take_while(|&(i, &value)| {
                let next = values.get(i.saturating_add(1)).copied();
                match flag {
                    0x80 => value == 0,
                    0 => byte(value) && !(value == 0 && next == Some(0)),
                    _ => value != 0 && !(byte(value) && next.is_some_and(byte)),
                }
            })
            .count();
        let (run, rest) = values.split_at(length);
        for chunk in run.chunks(64) {
            output.push(flag | u8::try_from(chunk.len().saturating_sub(1)).unwrap_or(63));
            match flag {
                0 => output.extend(chunk.iter().map(|v| v.to_be_bytes()[1])),
                0x40 => output.extend(chunk.iter().flat_map(|v| v.to_be_bytes())),
                _ => {}
            }
        }
        values = rest;
    }
}

fn pin_records(
    plan: &Plan,
    font: &Font<'_>,
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
    let pinned_coord = *plan.var.n.normalized_coords.get(pin).ok_or(bad)?;
    let project = |tuple: Tuple<'_>| -> Result<Vec<u8>, SubsetError> {
        let mut coords = Vec::new();
        for &i in kept {
            coords.extend(tuple.axis(i).ok_or(bad)?.to_be_bytes());
        }
        Ok(coords)
    };
    let outlines = super::glyf::GlyfAccel::new(font);
    let mut glyphs = Vec::new();
    let mut frequencies = BTreeMap::<Vec<u8>, usize>::new();
    for ((gid, raw), &(_, old)) in records.iter().zip(plan.new_to_old_glyphs()) {
        if raw.is_empty() {
            continue;
        }
        let glyph = table.glyph_data(old).ok_or(bad)?;
        let mut points = &[0u8][..];
        if glyph.has_shared_points() {
            let offset = sofel_font::read::be::u16_at(raw, 2).ok_or(bad)?;
            let bytes = raw.get(usize::from(offset)..).ok_or(bad)?;
            let (_, length) = PackedPoints::parse(bytes).ok_or(bad)?;
            points = bytes.get(..length).ok_or(bad)?;
        }
        let mut tuples = Vec::new();
        let mut regions = BTreeMap::new();
        let mut groups: Vec<(_, _, Vec<_>)> = Vec::new();
        for tuple in glyph.tuples() {
            let tuple = tuple.ok_or(bad)?;
            let peak = tuple
                .embedded_peak()
                .or_else(|| table.shared_tuple(tuple.shared_peak_index()?))
                .ok_or(bad)?;
            let projected = project(peak)?;
            let pin_peak = i32::from(peak.axis(pin).ok_or(bad)?);
            let mut scalar = 1.0;
            if pin_peak != 0 {
                let (start, end) = if let Some((start, end)) = tuple.intermediate() {
                    (
                        i32::from(start.axis(pin).ok_or(bad)?),
                        i32::from(end.axis(pin).ok_or(bad)?),
                    )
                } else {
                    (pin_peak.min(0), pin_peak.max(0))
                };
                let valid_region = start <= pin_peak && pin_peak <= end && !(start < 0 && end > 0);
                let active = !valid_region
                    || pinned_coord == pin_peak
                    || (start < pinned_coord && pinned_coord < end);
                if !active || projected.iter().all(|&b| b == 0) {
                    continue;
                }
                if valid_region && pinned_coord != pin_peak {
                    let edge = if pinned_coord < pin_peak { start } else { end };
                    scalar = sofel_geom::float::i32_to_f32(pinned_coord.saturating_sub(edge))
                        / sofel_geom::float::i32_to_f32(pin_peak.saturating_sub(edge));
                }
            }
            let intermediate = tuple
                .intermediate()
                .map(|(start, end)| -> Result<_, SubsetError> {
                    let mut region = project(start)?;
                    region.extend(project(end)?);
                    Ok(region)
                })
                .transpose()?
                .filter(|region| {
                    let (starts, ends) = region.split_at(projected.len());
                    projected
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .zip(
                            starts
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .zip(ends.as_chunks::<2>().0.iter()),
                        )
                        .any(|(peak, (start, end))| {
                            let peak = i16::from_be_bytes(*peak);
                            *start != peak.min(0).to_be_bytes() || *end != peak.max(0).to_be_bytes()
                        })
                });
            let index = *regions
                .entry((projected.clone(), intermediate.clone()))
                .or_insert_with(|| {
                    let index = groups.len();
                    groups.push((projected, intermediate, Vec::new()));
                    index
                });
            groups.get_mut(index).ok_or(bad)?.2.push((tuple, scalar));
        }
        for (projected, intermediate, contributors) in groups {
            let &(tuple, scalar) = contributors.first().ok_or(bad)?;
            let count = frequencies.entry(projected.clone()).or_default();
            *count = count.saturating_add(1);
            let (points, deltas) = if tuple.private_points() {
                let (_, used) = PackedPoints::parse(tuple.data()).ok_or(bad)?;
                tuple.data().split_at_checked(used).ok_or(bad)?
            } else {
                (points, tuple.data())
            };
            let (points, deltas) = if contributors.len() == 1
                && scalar == 1.0
                && PackedPoints::parse(points).ok_or(bad)?.0.is_all()
            {
                (points, Cow::Borrowed(deltas))
            } else {
                let outline = outlines.glyph(old, false);
                let simple = (outline.kind == super::glyf::Kind::Simple)
                    .then(|| {
                        sofel_font::read::glyf::Glyph::new(outline.bytes)
                            .and_then(|glyph| glyph.simple())
                    })
                    .transpose()
                    .map_err(|_| bad)?;
                let count = if let Some(ref simple) = simple {
                    usize::try_from(simple.num_points()).map_err(|_| bad)?
                } else {
                    outline.components().count()
                };
                let n = count.checked_add(4).ok_or(bad)?;
                let mut sums = [alloc::vec![0.0; n], alloc::vec![0.0; n]];
                let mut original = Vec::new();
                for (part, scale) in contributors {
                    let (x, y) = part.deltas(n, glyph.shared_points()).ok_or(bad)?;
                    if let Some(indices) = part
                        .points()
                        .or(glyph.shared_points())
                        .filter(|p| !p.is_all())
                    {
                        use sofel_font::var::gvar::{Point, infer_hb};
                        if let Some(ref simple) = simple
                            && original.is_empty()
                        {
                            original.extend(simple.points(outline.extended).map(|p| Point {
                                x: sofel_geom::float::i32_to_f32(p.x),
                                y: sofel_geom::float::i32_to_f32(p.y),
                                end: false,
                            }));
                            if original.len().checked_add(4) != Some(n) {
                                return Err(bad);
                            }
                            for end in simple.end_points() {
                                original.get_mut(usize::from(end)).ok_or(bad)?.end = true;
                            }
                            original.resize(n, Point::default());
                        }
                        let mut expanded = alloc::vec![Point::default(); n];
                        let mut explicit = alloc::vec![false; n];
                        for ((i, dx), dy) in indices.iter().zip(x.iter()).zip(y.iter()) {
                            let i = usize::try_from(i).map_err(|_| bad)?;
                            let point = expanded.get_mut(i).ok_or(bad)?;
                            point.x = f32::from(dx) * scale;
                            point.y = f32::from(dy) * scale;
                            *explicit.get_mut(i).ok_or(bad)? = true;
                        }
                        if simple.is_some() {
                            infer_hb(&original, &mut expanded, &explicit).map_err(|_| bad)?;
                        }
                        let [xs, ys] = &mut sums;
                        for ((x, y), d) in xs.iter_mut().zip(ys).zip(expanded) {
                            *x += d.x;
                            *y += d.y;
                        }
                        continue;
                    }
                    for (sum, axis) in sums.iter_mut().zip([x, y]) {
                        for (value, delta) in sum.iter_mut().zip(axis.iter()) {
                            *value += f32::from(delta) * scale;
                        }
                    }
                }
                let mut encoded = Vec::new();
                for sum in sums {
                    let values = sum
                        .into_iter()
                        .map(|value| {
                            let rounded = sofel_geom::float::hb_roundf32(value);
                            i16::try_from(crate::instance::f64_to_i32(rounded))
                                .map_err(|_| SubsetError::Limit { tag })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    pack_deltas(&values, &mut encoded);
                }
                (&[0u8][..], Cow::Owned(encoded))
            };
            tuples.push(PinnedTuple {
                peak: projected,
                intermediate,
                points,
                deltas,
            });
        }
        if tuples.is_empty() {
            continue;
        }
        glyphs.push((*gid, tuples));
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
    for (gid, tuples) in glyphs {
        let mut point_counts = BTreeMap::<&[u8], usize>::new();
        for tuple in &tuples {
            let n = point_counts.entry(tuple.points).or_default();
            *n = n.saturating_add(1);
        }
        let shared_points = if point_counts.values().all(|&n| n > 1) {
            point_counts
                .into_iter()
                .max_by_key(|(points, n)| points.len().saturating_mul(n.saturating_sub(1)))
                .map(|(points, _)| points)
        } else {
            None
        };
        let count = u16::try_from(tuples.len()).map_err(|_| bad)?;
        let mut headers = Vec::new();
        let mut payload = shared_points.unwrap_or_default().to_vec();
        for tuple in tuples {
            let index = indices.get(tuple.peak.as_slice());
            let private = Some(tuple.points) != shared_points;
            let flags = index.copied().unwrap_or(0x8000)
                | if tuple.intermediate.is_some() {
                    0x4000
                } else {
                    0
                }
                | if private { 0x2000 } else { 0 };
            headers.extend(
                u16::try_from(
                    tuple
                        .deltas
                        .len()
                        .checked_add(if private { tuple.points.len() } else { 0 })
                        .ok_or(bad)?,
                )
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
            if private {
                payload.extend(tuple.points);
            }
            payload.extend_from_slice(&tuple.deltas);
        }
        let mut record = Vec::new();
        record.extend((count | if shared_points.is_some() { 0x8000 } else { 0 }).to_be_bytes());
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
