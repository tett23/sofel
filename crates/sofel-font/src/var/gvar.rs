//! Applying gvar tuples with HarfBuzz and FreeType arithmetic (ADR 0030).

#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::too_many_arguments
)]
// Point indexes are checked against the input length when each tuple is decoded.

use alloc::vec::Vec;
use sofel_geom::{div_fix, mul_div, mul_fix};
use sofel_read::glyf;
use sofel_read::gvar::{self, Gvar, Tuple, TupleVariation};

use crate::{Font, var::Instance};

/// Opens the variation table matching the selected glyf table.
pub(crate) fn table<'a>(font: &Font<'a>) -> Option<Gvar<'a>> {
    let tag = if font.table(glyf::TAG_WIDE).is_some() {
        gvar::GVAR_TAG
    } else {
        gvar::TAG
    };
    font.table(tag).and_then(|bytes| Gvar::new(bytes, tag).ok())
}

fn peak_values(tuple: Tuple<'_>) -> impl Iterator<Item = i16> + '_ {
    (0..tuple.len()).map(move |axis| tuple.axis(axis).unwrap_or(0))
}

pub(crate) fn shared_hb(table: &Gvar<'_>, coords: &[i32]) -> Vec<f32> {
    (0..table.shared_tuple_count())
        .map(|index| {
            table
                .shared_tuple(index)
                .map(|peak| tuple_scalar_hb(coords, &peak_values(peak).collect::<Vec<_>>(), None))
                .unwrap_or(0.0)
        })
        .collect()
}

pub(crate) fn shared_ft(table: &Gvar<'_>, coords: &[i32]) -> Vec<i32> {
    (0..table.shared_tuple_count())
        .map(|index| {
            table
                .shared_tuple(index)
                .map(|peak| tuple_scalar_ft(coords, &peak_values(peak).collect::<Vec<_>>(), None))
                .unwrap_or(0)
        })
        .collect()
}

/// `TupleVariationHeader::calculate_scalar`, folding in axis order.
pub(crate) fn tuple_scalar_hb(
    coords: &[i32],
    peak: &[i16],
    intermediate: Option<(&[i16], &[i16])>,
) -> f32 {
    let mut scalar = 1.0f32;
    for (axis, &peak) in peak.iter().enumerate() {
        let peak = i32::from(peak);
        if peak == 0 {
            continue;
        }
        let coord = coords.get(axis).copied().unwrap_or(0);
        if coord == 0 {
            return 0.0;
        }
        if coord == peak {
            continue;
        }
        if let Some((starts, ends)) = intermediate {
            let (Some(&start), Some(&end)) = (starts.get(axis), ends.get(axis)) else {
                return 0.0;
            };
            let (start, end) = (i32::from(start), i32::from(end));
            if start > peak || peak > end || (start < 0 && end > 0) {
                continue;
            }
            if coord < start || coord > end {
                return 0.0;
            }
            if coord < peak && peak != start {
                scalar *= (coord - start) as f32 / (peak - start) as f32;
            } else if coord > peak && peak != end {
                scalar *= (end - coord) as f32 / (end - peak) as f32;
            }
        } else {
            if coord < peak.min(0) || coord > peak.max(0) {
                return 0.0;
            }
            scalar *= coord as f32 / peak as f32;
        }
    }
    scalar
}

/// `ft_var_apply_tuple`, including cumulative `FT_MulDiv` rounding.
pub(crate) fn tuple_scalar_ft(
    coords: &[i32],
    peak: &[i16],
    intermediate: Option<(&[i16], &[i16])>,
) -> i32 {
    let mut scalar = 0x10000;
    for (axis, &peak) in peak.iter().enumerate() {
        let peak = i32::from(peak) * 4;
        if peak == 0 {
            continue;
        }
        let coord = coords.get(axis).copied().unwrap_or(0);
        if coord == 0 {
            return 0;
        }
        if coord == peak {
            continue;
        }
        if let Some((starts, ends)) = intermediate {
            let (Some(&start), Some(&end)) = (starts.get(axis), ends.get(axis)) else {
                return 0;
            };
            let (start, end) = (i32::from(start) * 4, i32::from(end) * 4);
            if coord <= start || coord >= end {
                return 0;
            }
            scalar = if coord < peak {
                mul_div(scalar, coord - start, peak - start)
            } else {
                mul_div(scalar, end - coord, end - peak)
            };
        } else if (peak > coord && coord > 0) || (peak < coord && coord < 0) {
            scalar = mul_div(scalar, coord, peak);
        } else {
            return 0;
        }
    }
    scalar
}

/// An outline point in the arithmetic type of the selected engine.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point<T> {
    /// Horizontal coordinate.
    pub x: T,
    /// Vertical coordinate.
    pub y: T,
    /// The final point of an outline contour. Phantom points leave this false.
    pub end: bool,
}

/// Errors that prevent an entire glyph from being returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GvarError {
    /// A tuple or its serialized deltas cannot be read.
    InvalidData,
    /// HarfBuzz's per-glyph calculation budget ran out.
    BudgetExceeded,
    /// The location was rejected by FreeType.
    InvalidLocation,
}

/// Reused HarfBuzz work area.
#[derive(Default)]
pub struct HbScratch {
    working: Vec<Point<f32>>,
    original: Vec<Point<f32>>,
    pending: Vec<Point<f32>>,
    explicit: Vec<bool>,
}

/// Reused FreeType work area.
#[derive(Clone, Debug, Default)]
pub struct FtScratch {
    coords: Vec<i32>,
    original: Vec<Point<i32>>,
    out: Vec<Point<i32>>,
    deltas: Vec<Point<i32>>,
    explicit: Vec<bool>,
}

impl Instance<'_, '_, '_, '_> {
    /// Apply HarfBuzz gvar deltas to floating point outline and phantom points.
    pub fn apply_gvar_hb(
        &self,
        glyph: u32,
        points: &mut [Point<f32>],
        scratch: &mut HbScratch,
        phantom_only: bool,
        budget: &mut i64,
    ) -> Result<bool, GvarError> {
        if !self.location.hb_nonzero {
            return Ok(false);
        }
        let Some(table) = table(self.font) else {
            return Ok(false);
        };
        let shared = self
            .location
            .source
            .filter(|source| source.matches(self.font.sfnt.file(), self.font.face_index))
            .map_or(&[][..], |_| self.location.gvar_hb.as_slice());
        apply_hb(
            &table,
            glyph,
            &self.location.hb_coords,
            shared,
            points,
            scratch,
            phantom_only,
            budget,
        )
    }

    /// Apply FreeType gvar deltas to 16.16 outline and phantom points.
    pub fn apply_gvar_ft(
        &self,
        glyph: u32,
        points: &mut [Point<i32>],
        scratch: &mut FtScratch,
    ) -> Result<bool, GvarError> {
        if self.location.ft_error.is_some() {
            return Err(GvarError::InvalidLocation);
        }
        if !self.location.ft_coords_supplied
            || self.location.ft_coords.iter().all(|coord| coord.0 == 0)
        {
            return Ok(false);
        }
        let Some(table) = table(self.font) else {
            return Ok(false);
        };
        let shared = self
            .location
            .source
            .filter(|source| source.matches(self.font.sfnt.file(), self.font.face_index))
            .map_or(&[][..], |_| self.location.gvar_ft.as_slice());
        let mut coords = core::mem::take(&mut scratch.coords);
        coords.clear();
        coords.extend(self.location.ft_coords.iter().map(|coord| coord.0));
        let result = apply_ft(&table, glyph, &coords, shared, points, scratch);
        scratch.coords = coords;
        result
    }

    /// Apply only FreeType's four gvar phantom-point deltas.
    pub(crate) fn apply_gvar_ft_phantoms(
        &self,
        glyph: u32,
        outline_count: usize,
        phantoms: &mut [Point<i32>; 4],
        scratch: &mut FtScratch,
    ) -> Result<bool, GvarError> {
        if self.location.ft_error.is_some() {
            return Err(GvarError::InvalidLocation);
        }
        if !self.location.ft_coords_supplied
            || self.location.ft_coords.iter().all(|coord| coord.0 == 0)
        {
            return Ok(false);
        }
        let Some(table) = table(self.font) else {
            return Ok(false);
        };
        let shared = self
            .location
            .source
            .filter(|source| source.matches(self.font.sfnt.file(), self.font.face_index))
            .map_or(&[][..], |_| self.location.gvar_ft.as_slice());
        let mut coords = core::mem::take(&mut scratch.coords);
        coords.clear();
        coords.extend(self.location.ft_coords.iter().map(|coord| coord.0));
        let result = apply_ft_phantoms(&table, glyph, &coords, shared, outline_count, phantoms);
        scratch.coords = coords;
        result
    }
}

fn apply_ft_phantoms(
    table: &Gvar<'_>,
    glyph: u32,
    coords: &[i32],
    shared: &[i32],
    outline_count: usize,
    phantoms: &mut [Point<i32>; 4],
) -> Result<bool, GvarError> {
    let Some(data) = glyph_data(table, glyph)? else {
        return Ok(false);
    };
    let total = outline_count.checked_add(4).ok_or(GvarError::InvalidData)?;
    let mut deltas = [Point::<i32>::default(); 4];
    for tuple in data.tuples() {
        let tuple = tuple.ok_or(GvarError::InvalidData)?;
        let scalar = scalar_ft(table, &tuple, coords, shared)?;
        if scalar == 0 {
            continue;
        }
        let shared_points = data.shared_points();
        let (x, y) = tuple
            .deltas(total, shared_points)
            .ok_or(GvarError::InvalidData)?;
        let points = tuple.points().or(shared_points);
        if let Some(points) = points.filter(|points| !points.is_all()) {
            for ((index, dx), dy) in points.iter().zip(x.iter()).zip(y.iter()) {
                let index = usize::try_from(index).map_err(|_| GvarError::InvalidData)?;
                if index >= outline_count {
                    let delta = deltas
                        .get_mut(index - outline_count)
                        .ok_or(GvarError::InvalidData)?;
                    delta.x = delta.x.wrapping_add(mul_fix(i32::from(dx) << 16, scalar));
                    delta.y = delta.y.wrapping_add(mul_fix(i32::from(dy) << 16, scalar));
                }
            }
        } else {
            for ((index, dx), dy) in x.iter().enumerate().zip(y.iter()) {
                if index >= outline_count {
                    let delta = deltas
                        .get_mut(index - outline_count)
                        .ok_or(GvarError::InvalidData)?;
                    delta.x = delta.x.wrapping_add(mul_fix(i32::from(dx) << 16, scalar));
                    delta.y = delta.y.wrapping_add(mul_fix(i32::from(dy) << 16, scalar));
                }
            }
        }
    }
    for (point, delta) in phantoms.iter_mut().zip(deltas) {
        point.x = point.x.wrapping_add(delta.x);
        point.y = point.y.wrapping_add(delta.y);
    }
    Ok(true)
}

fn tuple_peak(table: &Gvar<'_>, tuple: &TupleVariation<'_>) -> Result<Vec<i16>, GvarError> {
    let peak = tuple
        .embedded_peak()
        .or_else(|| {
            tuple
                .shared_peak_index()
                .and_then(|index| table.shared_tuple(index))
        })
        .ok_or(GvarError::InvalidData)?;
    Ok(peak_values(peak).collect())
}

fn tuple_intermediate(tuple: &TupleVariation<'_>) -> Option<(Vec<i16>, Vec<i16>)> {
    tuple
        .intermediate()
        .map(|(start, end)| (peak_values(start).collect(), peak_values(end).collect()))
}

fn scalar_hb(
    table: &Gvar<'_>,
    tuple: &TupleVariation<'_>,
    coords: &[i32],
    shared: &[f32],
) -> Result<f32, GvarError> {
    if tuple.embedded_peak().is_none()
        && tuple.intermediate().is_none()
        && let Some(value) = tuple
            .shared_peak_index()
            .and_then(|index| shared.get(usize::from(index)))
    {
        return Ok(*value);
    }
    let peak = tuple_peak(table, tuple)?;
    let middle = tuple_intermediate(tuple);
    Ok(tuple_scalar_hb(
        coords,
        &peak,
        middle
            .as_ref()
            .map(|(start, end)| (start.as_slice(), end.as_slice())),
    ))
}

fn scalar_ft(
    table: &Gvar<'_>,
    tuple: &TupleVariation<'_>,
    coords: &[i32],
    shared: &[i32],
) -> Result<i32, GvarError> {
    if tuple.embedded_peak().is_none()
        && tuple.intermediate().is_none()
        && let Some(value) = tuple
            .shared_peak_index()
            .and_then(|index| shared.get(usize::from(index)))
    {
        return Ok(*value);
    }
    let peak = tuple_peak(table, tuple)?;
    let middle = tuple_intermediate(tuple);
    Ok(tuple_scalar_ft(
        coords,
        &peak,
        middle
            .as_ref()
            .map(|(start, end)| (start.as_slice(), end.as_slice())),
    ))
}

fn glyph_data<'a>(
    table: &'a Gvar<'a>,
    glyph: u32,
) -> Result<Option<gvar::GlyphVariationData<'a>>, GvarError> {
    if glyph >= table.glyph_count() {
        return Ok(None);
    }
    let first = table.glyph_offset(glyph).ok_or(GvarError::InvalidData)?;
    let last = table
        .glyph_offset(glyph + 1)
        .ok_or(GvarError::InvalidData)?;
    if first == last {
        return Ok(None);
    }
    table
        .glyph_data(glyph)
        .map(Some)
        .ok_or(GvarError::InvalidData)
}

fn point_indices(
    tuple: &TupleVariation<'_>,
    shared: Option<gvar::PackedPoints<'_>>,
    count: usize,
) -> (Vec<usize>, bool) {
    let points = tuple.points().or(shared);
    match points {
        Some(points) if !points.is_all() => (
            points
                .iter()
                .filter_map(|i| usize::try_from(i).ok())
                .collect(),
            false,
        ),
        _ => ((0..count).collect(), true),
    }
}

fn flush_hb(points: &mut [Point<f32>], deltas: &[Point<f32>], start: usize) {
    for (point, delta) in points.iter_mut().zip(deltas).skip(start) {
        point.x += delta.x;
        point.y += delta.y;
    }
}

/// Apply HarfBuzz's gvar tuples. On error, the caller's points stay intact.
pub fn apply_hb(
    table: &Gvar<'_>,
    glyph: u32,
    coords: &[i32],
    shared: &[f32],
    points: &mut [Point<f32>],
    scratch: &mut HbScratch,
    phantom_only: bool,
    budget: &mut i64,
) -> Result<bool, GvarError> {
    let Some(data) = glyph_data(table, glyph)? else {
        return Ok(false);
    };
    let count = points.len();
    if count < 4 {
        return Err(GvarError::InvalidData);
    }
    let start = if phantom_only { count - 4 } else { 0 };
    let private_any = data
        .tuples()
        .any(|tuple| tuple.is_some_and(|tuple| tuple.private_points()));
    scratch.working.clear();
    scratch.working.extend_from_slice(points);
    scratch.original.clear();
    scratch.pending.clear();
    scratch.pending.resize(count, Point::default());
    scratch.explicit.clear();
    scratch.explicit.resize(count, false);
    let mut flush = false;
    for tuple in data.tuples() {
        let tuple = tuple.ok_or(GvarError::InvalidData)?;
        let scalar = scalar_hb(table, &tuple, coords, shared)?;
        if scalar == 0.0 {
            continue;
        }
        let cost = i64::try_from(count - start).map_err(|_| GvarError::BudgetExceeded)?;
        if *budget < cost {
            return Err(GvarError::BudgetExceeded);
        }
        *budget -= cost;
        let shared_points = data.shared_points();
        let (x, y) = tuple
            .deltas(count, shared_points)
            .ok_or(GvarError::InvalidData)?;
        let (indices, all) = point_indices(&tuple, shared_points, count);
        if all && !private_any {
            for ((point_index, dx), dy) in indices.iter().zip(x.iter()).zip(y.iter()) {
                if *point_index >= start {
                    let point = scratch
                        .working
                        .get_mut(*point_index)
                        .ok_or(GvarError::InvalidData)?;
                    point.x += f32::from(dx) * scalar;
                    point.y += f32::from(dy) * scalar;
                }
            }
            continue;
        }
        if !all {
            if scratch.original.is_empty() && !phantom_only {
                scratch.original.extend_from_slice(&scratch.working);
            }
            if flush {
                flush_hb(&mut scratch.working, &scratch.pending, start);
            }
            scratch.pending.fill(Point::default());
            scratch.explicit.fill(false);
        }
        for ((point_index, dx), dy) in indices.iter().zip(x.iter()).zip(y.iter()) {
            if *point_index < start {
                continue;
            }
            let point = scratch
                .pending
                .get_mut(*point_index)
                .ok_or(GvarError::InvalidData)?;
            point.x += f32::from(dx) * scalar;
            point.y += f32::from(dy) * scalar;
            if !all {
                scratch.explicit[*point_index] = true;
            }
        }
        if !all && !phantom_only {
            infer_hb(&scratch.original, &mut scratch.pending, &scratch.explicit);
        }
        flush = true;
    }
    if flush {
        flush_hb(&mut scratch.working, &scratch.pending, start);
    }
    points.copy_from_slice(&scratch.working);
    Ok(true)
}

/// Apply FreeType's gvar tuples to 16.16 original points.
pub fn apply_ft(
    table: &Gvar<'_>,
    glyph: u32,
    coords: &[i32],
    shared: &[i32],
    points: &mut [Point<i32>],
    scratch: &mut FtScratch,
) -> Result<bool, GvarError> {
    let Some(data) = glyph_data(table, glyph)? else {
        return Ok(false);
    };
    let count = points.len();
    if count < 4 {
        return Err(GvarError::InvalidData);
    }
    scratch.original.clear();
    scratch.original.extend_from_slice(points);
    scratch.out.clear();
    scratch.out.resize(count, Point::default());
    scratch.deltas.clear();
    scratch.deltas.resize(count, Point::default());
    scratch.explicit.clear();
    scratch.explicit.resize(count, false);
    for tuple in data.tuples() {
        let tuple = tuple.ok_or(GvarError::InvalidData)?;
        let scalar = scalar_ft(table, &tuple, coords, shared)?;
        if scalar == 0 {
            continue;
        }
        let shared_points = data.shared_points();
        let (x, y) = tuple
            .deltas(count, shared_points)
            .ok_or(GvarError::InvalidData)?;
        let (indices, all) = point_indices(&tuple, shared_points, count);
        if all {
            for ((point_index, dx), dy) in indices.iter().zip(x.iter()).zip(y.iter()) {
                let point = scratch
                    .deltas
                    .get_mut(*point_index)
                    .ok_or(GvarError::InvalidData)?;
                point.x = point.x.wrapping_add(mul_fix(i32::from(dx) << 16, scalar));
                point.y = point.y.wrapping_add(mul_fix(i32::from(dy) << 16, scalar));
            }
        } else {
            scratch.out.copy_from_slice(&scratch.original);
            scratch.explicit.fill(false);
            for ((point_index, dx), dy) in indices.iter().zip(x.iter()).zip(y.iter()) {
                let point = scratch
                    .out
                    .get_mut(*point_index)
                    .ok_or(GvarError::InvalidData)?;
                point.x = point.x.wrapping_add(mul_fix(i32::from(dx) << 16, scalar));
                point.y = point.y.wrapping_add(mul_fix(i32::from(dy) << 16, scalar));
                scratch.explicit[*point_index] = true;
            }
            infer_ft(&scratch.original, &mut scratch.out, &scratch.explicit);
            for (delta, (out, original)) in scratch
                .deltas
                .iter_mut()
                .zip(scratch.out.iter().zip(&scratch.original))
            {
                delta.x = delta.x.wrapping_add(out.x.wrapping_sub(original.x));
                delta.y = delta.y.wrapping_add(out.y.wrapping_sub(original.y));
            }
        }
    }
    for (point, delta) in points.iter_mut().zip(&scratch.deltas) {
        point.x = point.x.wrapping_add(delta.x);
        point.y = point.y.wrapping_add(delta.y);
    }
    Ok(true)
}

fn contour_ranges<T>(
    points: &[Point<T>],
) -> impl Iterator<Item = core::ops::RangeInclusive<usize>> + '_ {
    let mut first = 0;
    points.iter().enumerate().filter_map(move |(last, point)| {
        if point.end {
            let range = first..=last;
            first = last + 1;
            Some(range)
        } else {
            None
        }
    })
}

fn infer_hb(original: &[Point<f32>], deltas: &mut [Point<f32>], explicit: &[bool]) {
    for range in contour_ranges(original) {
        let indices: Vec<usize> = range.clone().filter(|&index| explicit[index]).collect();
        if indices.len() == 1 {
            let one = deltas[indices[0]];
            for index in range {
                if !explicit[index] {
                    deltas[index].x = one.x;
                    deltas[index].y = one.y;
                }
            }
        } else if indices.len() > 1 {
            for pair in 0..indices.len() {
                let prev = indices[pair];
                let next = indices[(pair + 1) % indices.len()];
                let mut index = if prev == *range.end() {
                    *range.start()
                } else {
                    prev + 1
                };
                while index != next {
                    deltas[index].x = infer_hb_axis(
                        original[index].x,
                        original[prev].x,
                        original[next].x,
                        deltas[prev].x,
                        deltas[next].x,
                    );
                    deltas[index].y = infer_hb_axis(
                        original[index].y,
                        original[prev].y,
                        original[next].y,
                        deltas[prev].y,
                        deltas[next].y,
                    );
                    index = if index == *range.end() {
                        *range.start()
                    } else {
                        index + 1
                    };
                }
            }
        }
    }
}

fn infer_hb_axis(value: f32, prev: f32, next: f32, prev_delta: f32, next_delta: f32) -> f32 {
    if prev == next {
        return if prev_delta == next_delta {
            prev_delta
        } else {
            0.0
        };
    }
    if value <= prev.min(next) {
        return if prev < next { prev_delta } else { next_delta };
    }
    if value >= prev.max(next) {
        return if prev > next { prev_delta } else { next_delta };
    }
    prev_delta + ((value - prev) / (next - prev)) * (next_delta - prev_delta)
}

fn infer_ft(original: &[Point<i32>], out: &mut [Point<i32>], explicit: &[bool]) {
    for range in contour_ranges(original) {
        let indices: Vec<usize> = range.clone().filter(|&index| explicit[index]).collect();
        if indices.len() == 1 {
            let one = indices[0];
            let dx = out[one].x.wrapping_sub(original[one].x);
            let dy = out[one].y.wrapping_sub(original[one].y);
            for index in range {
                if !explicit[index] {
                    out[index].x = original[index].x.wrapping_add(dx);
                    out[index].y = original[index].y.wrapping_add(dy);
                }
            }
        } else if indices.len() > 1 {
            for pair in 0..indices.len() {
                let prev = indices[pair];
                let next = indices[(pair + 1) % indices.len()];
                let mut index = if prev == *range.end() {
                    *range.start()
                } else {
                    prev + 1
                };
                while index != next {
                    out[index].x = infer_ft_axis(
                        original[index].x,
                        original[prev].x,
                        original[next].x,
                        out[prev].x,
                        out[next].x,
                        out[index].x,
                    );
                    out[index].y = infer_ft_axis(
                        original[index].y,
                        original[prev].y,
                        original[next].y,
                        out[prev].y,
                        out[next].y,
                        out[index].y,
                    );
                    index = if index == *range.end() {
                        *range.start()
                    } else {
                        index + 1
                    };
                }
            }
        }
    }
}

fn infer_ft_axis(
    value: i32,
    prev: i32,
    next: i32,
    prev_out: i32,
    next_out: i32,
    current: i32,
) -> i32 {
    let (first, last, first_out, last_out) = if prev <= next {
        (prev, next, prev_out, next_out)
    } else {
        (next, prev, next_out, prev_out)
    };
    if first == last && first_out != last_out {
        return current;
    }
    let d1 = first_out.wrapping_sub(first);
    let d2 = last_out.wrapping_sub(last);
    if value <= first {
        return value.wrapping_add(d1);
    }
    if value >= last {
        return value.wrapping_add(d2);
    }
    let scale = if first == last {
        0
    } else {
        div_fix(last_out.wrapping_sub(first_out), last.wrapping_sub(first))
    };
    first_out.wrapping_add(mul_fix(value.wrapping_sub(first), scale))
}

#[cfg(test)]
mod tests {
    use super::{
        FtScratch, HbScratch, Point, apply_ft, apply_ft_phantoms, apply_hb, infer_ft, infer_hb,
        tuple_scalar_ft, tuple_scalar_hb,
    };
    use alloc::{vec, vec::Vec};
    use sofel_read::gvar::{Gvar, TAG};

    fn one_all_points_tuple() -> Vec<u8> {
        let mut data = vec![
            0, 1, 0, 0, // version 1
            0, 1, 0, 0, // one axis, no shared tuple
            0, 0, 0, 24, // shared tuple offset
            0, 1, 0, 0, // one glyph, short offsets
            0, 0, 0, 24, // glyph data offset
            0, 0, 0, 8, // glyph bytes are 16 bytes
            0, 1, 0, 10, // one tuple, serialized data at 10
            0, 6, 0x80, 0, 0x40, 0, // embedded peak, one axis
            3, 10, 20, 30, 40, 0x83, // x deltas and zero y deltas
        ];
        assert_eq!(data.len(), 40);
        data.shrink_to_fit();
        data
    }

    #[test]
    fn adr0047_i03_active_private_and_zero_scalar_tuples_preserve_deltas() {
        let bytes = [
            0, 1, 0, 0, 0, 1, 0, 0, // version and one axis
            0, 0, 0, 24, 0, 1, 0, 0, // shared tuple offset and glyph count
            0, 0, 0, 24, 0, 0, 0, 21, // glyph data offset and 42-byte glyph
            0, 3, 0, 22, // three tuples, data after 18 header bytes
            0, 6, 0x80, 0, 0x40, 0, // all points, positive peak
            0, 7, 0xa0, 0, 0x40, 0, // private points, positive peak
            0, 7, 0xa0, 0, 0xc0, 0, // private points, negative peak
            3, 10, 20, 30, 40, 0x83, // first x and zero y deltas
            0, 3, 1, 1, 1, 1, 0x83, // private all points, positive deltas
            0, 3, 99, 99, 99, 99, 0x83, // zero-scalar tuple must not apply
            0,    // short-offset padding
        ];
        let parsed = Gvar::new(&bytes, TAG);
        assert!(parsed.is_ok());
        let Some(table) = parsed.ok() else { return };
        let mut hb = [Point::default(); 4];
        let mut budget = 8;
        assert_eq!(
            apply_hb(
                &table,
                0,
                &[0x2000],
                &[],
                &mut hb,
                &mut HbScratch::default(),
                false,
                &mut budget,
            ),
            Ok(true)
        );
        assert_eq!(hb.map(|point| point.x), [5.5, 10.5, 15.5, 20.5]);
        let mut ft = [Point::default(); 4];
        assert_eq!(
            apply_ft(
                &table,
                0,
                &[0x8000],
                &[],
                &mut ft,
                &mut FtScratch::default()
            ),
            Ok(true)
        );
        assert_eq!(
            ft.map(|point| point.x),
            [5, 10, 15, 20].map(|x| (x << 16) + 0x8000)
        );
    }

    #[test]
    fn adr0030_i01_ft_cumulative_tuple_rounding() {
        let peak = [0x4000, 0x6000];
        assert_eq!(tuple_scalar_ft(&[0x8000, 0x4000], &peak, None), 5461);
        assert_eq!(
            tuple_scalar_hb(&[0x2000, 0x1000], &peak, None).to_bits(),
            (1.0f32 / 12.0).to_bits()
        );
    }

    #[test]
    fn adr0030_i07_tuple_boundaries() {
        let peak = [0x2000];
        let middle = Some((&[0x2000][..], &[0x4000][..]));
        assert_eq!(tuple_scalar_hb(&[0x2000], &peak, middle), 1.0);
        assert_eq!(tuple_scalar_ft(&[0x8000], &peak, middle), 0x10000);
        assert_eq!(tuple_scalar_hb(&[0], &peak, None), 0.0);
        assert_eq!(tuple_scalar_ft(&[0], &peak, None), 0);
    }

    #[test]
    fn adr0030_i07_all_points_and_phantom_only_match() {
        let bytes = one_all_points_tuple();
        let parsed = Gvar::new(&bytes, TAG);
        assert!(parsed.is_ok());
        let Some(gvar) = parsed.ok() else { return };
        let mut all = [Point::default(); 4];
        let mut only = all;
        let mut scratch = HbScratch::default();
        let mut budget = 4;
        assert_eq!(
            apply_hb(
                &gvar,
                0,
                &[0x2000],
                &[],
                &mut all,
                &mut scratch,
                false,
                &mut budget,
            ),
            Ok(true)
        );
        let mut budget = 4;
        assert_eq!(
            apply_hb(
                &gvar,
                0,
                &[0x2000],
                &[],
                &mut only,
                &mut scratch,
                true,
                &mut budget,
            ),
            Ok(true)
        );
        assert_eq!(all, only);
        assert_eq!(all.map(|point| point.x), [5.0, 10.0, 15.0, 20.0]);

        let mut ft = [Point::default(); 4];
        assert_eq!(
            apply_ft(&gvar, 0, &[0x8000], &[], &mut ft, &mut FtScratch::default()),
            Ok(true)
        );
        assert_eq!(ft.map(|point| point.x), [5, 10, 15, 20].map(|x| x << 16));
    }

    #[test]
    fn adr0048_i02_ft_phantom_only_matches_full_at_zero_outline_points() {
        let bytes = one_all_points_tuple();
        let Ok(gvar) = Gvar::new(&bytes, TAG) else {
            return;
        };
        let mut full = [Point::default(); 4];
        let mut only = full;
        assert_eq!(
            apply_ft(
                &gvar,
                0,
                &[0x8000],
                &[],
                &mut full,
                &mut FtScratch::default()
            ),
            Ok(true)
        );
        assert_eq!(
            apply_ft_phantoms(&gvar, 0, &[0x8000], &[], 0, &mut only),
            Ok(true)
        );
        assert_eq!(full, only);
    }

    #[test]
    fn adr0030_i10_gvar_budget_is_exact() {
        let bytes = one_all_points_tuple();
        let parsed = Gvar::new(&bytes, TAG);
        assert!(parsed.is_ok());
        let Some(gvar) = parsed.ok() else { return };
        let mut points = [Point::default(); 4];
        let mut budget = 3;
        assert!(
            apply_hb(
                &gvar,
                0,
                &[0x2000],
                &[],
                &mut points,
                &mut HbScratch::default(),
                false,
                &mut budget
            )
            .is_err()
        );
        assert_eq!(points, [Point::default(); 4]);
    }

    #[test]
    fn adr0030_i07_iup_two_references_and_single_reference() {
        let hb_original = [
            Point {
                x: 0.0,
                y: 0.0,
                end: false,
            },
            Point {
                x: 50.0,
                y: 0.0,
                end: false,
            },
            Point {
                x: 100.0,
                y: 0.0,
                end: true,
            },
        ];
        let mut hb_deltas = [
            Point {
                x: 10.0,
                ..Point::default()
            },
            Point::default(),
            Point {
                x: 20.0,
                ..Point::default()
            },
        ];
        infer_hb(&hb_original, &mut hb_deltas, &[true, false, true]);
        assert_eq!(hb_deltas[1].x, 15.0);
        hb_deltas.fill(Point::default());
        hb_deltas[1].x = 7.0;
        infer_hb(&hb_original, &mut hb_deltas, &[false, true, false]);
        assert_eq!(hb_deltas.map(|point| point.x), [7.0; 3]);

        let ft_original = [
            Point {
                x: 0,
                y: 0,
                end: false,
            },
            Point {
                x: 50 << 16,
                y: 0,
                end: false,
            },
            Point {
                x: 100 << 16,
                y: 0,
                end: true,
            },
        ];
        let mut ft_out = ft_original;
        ft_out[0].x += 10 << 16;
        ft_out[2].x += 20 << 16;
        infer_ft(&ft_original, &mut ft_out, &[true, false, true]);
        // FT_DivFix rounds the 1.1 slope to 16.16 before multiplying.
        assert_eq!(ft_out[1].x, (65 << 16) + 20);
        ft_out = ft_original;
        ft_out[1].x += 7 << 16;
        infer_ft(&ft_original, &mut ft_out, &[false, true, false]);
        assert_eq!(ft_out.map(|point| point.x), [7 << 16, 57 << 16, 107 << 16]);
    }
}
