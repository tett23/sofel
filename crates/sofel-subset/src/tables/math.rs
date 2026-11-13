//! MATH closure and subsetting (`hb-ot-math-table.hh`: `closure_glyphs`
//! and `subset`), without a variation store.
//!
//! Subtables are read as HarfBuzz reads them after sanitizing: an
//! offset to a table that does not fit reads as null (HarfBuzz neuters
//! it), and a table whose header does not sanitize fails the subset.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_write::{Errors, Serializer};

use super::Tables;
use crate::error::SubsetError;
use crate::layout::common::Sub;
use crate::layout::data::{Cov, D, n16};
use crate::plan::Plan;
use crate::set::{INVALID, RangeSet};

fn collect_variation_index(
    src: &[u8],
    dev: D<'_>,
    keep: bool,
    formats: &mut BTreeSet<usize>,
    indices: &mut BTreeSet<u32>,
) -> Option<()> {
    if dev.is_null() || dev.u16(4) != 0x8000 {
        return Some(());
    }
    let at = src.len().checked_sub(dev.0.len())?.checked_add(4)?;
    formats.insert(at);
    if keep {
        let index = (u32::from(dev.u16(0)) << 16) | u32::from(dev.u16(2));
        if index != u32::MAX {
            indices.insert(index);
        }
    }
    Some(())
}

/// VariationIndex references in MathConstants, retained glyph values and
/// MathKern records, and retained MathVariants assemblies.
pub(crate) fn retained_variation_indices(
    src: &[u8],
    glyphs: &RangeSet,
    cmap_glyphs: &RangeSet,
) -> Option<BTreeSet<u32>> {
    let m = header(src)?;
    let k = constants(m.off16(4));
    let mut formats = BTreeSet::new();
    let mut indices = BTreeSet::new();
    for i in 0..51usize {
        let record = i.saturating_mul(4).saturating_add(8);
        collect_variation_index(
            src,
            device(k.off16(record.saturating_add(2))),
            true,
            &mut formats,
            &mut indices,
        )?;
    }
    let gi = glyph_info(m.off16(6));
    for offset in [0, 2] {
        let table = value_table(gi.off16(offset));
        for (i, glyph) in coverage(table.off16(0))
            .glyphs()
            .into_iter()
            .take(usize::from(table.u16(2)))
            .enumerate()
        {
            let record = i.saturating_mul(4).saturating_add(4);
            collect_variation_index(
                src,
                device(table.off16(record.saturating_add(2))),
                glyphs.contains(glyph),
                &mut formats,
                &mut indices,
            )?;
        }
    }
    let ki = kern_info(gi.off16(6));
    let mut visited_kerns = BTreeSet::new();
    for (i, glyph) in coverage(ki.off16(0))
        .glyphs()
        .into_iter()
        .take(usize::from(ki.u16(2)))
        .enumerate()
    {
        let keep = glyphs.contains(glyph);
        let record = i.saturating_mul(8).saturating_add(4);
        for corner in 0..4usize {
            let k = kern(ki.off16(record.saturating_add(corner.saturating_mul(2))));
            if k.is_null() {
                continue;
            }
            let at = src.len().checked_sub(k.0.len())?;
            if !visited_kerns.insert((at, keep)) {
                continue;
            }
            let records = usize::from(k.u16(0)).saturating_mul(2).saturating_add(1);
            for j in 0..records {
                collect_variation_index(
                    src,
                    device(k.off16(j.saturating_mul(4).saturating_add(4))),
                    keep,
                    &mut formats,
                    &mut indices,
                )?;
            }
        }
    }
    let v = variants(m.off16(8));
    let vert = usize::from(v.u16(6));
    let horiz = usize::from(v.u16(8));
    for (coverage_at, first, count) in [(2, 0, vert), (4, vert, horiz)] {
        for (i, glyph) in coverage(v.off16(coverage_at))
            .glyphs()
            .into_iter()
            .take(count)
            .enumerate()
        {
            let at = first.saturating_add(i).saturating_mul(2).saturating_add(10);
            let c = construction(v.off16(at));
            let a = assembly(c.off16(0));
            collect_variation_index(
                src,
                device(a.off16(2)),
                cmap_glyphs.contains(glyph),
                &mut formats,
                &mut indices,
            )?;
        }
    }
    if src
        .windows(2)
        .enumerate()
        .any(|(at, word)| word == [0x80, 0x00] && !formats.contains(&at))
    {
        return None;
    }
    Some(indices)
}

/// `d`, or the Null table when it has fewer than `n` bytes.
fn sized(d: D<'_>, n: usize) -> D<'_> {
    if d.0.len() < n { D(&[]) } else { d }
}

/// The table, when its header sanitizes (`MATH::sanitize`).
fn header(src: &[u8]) -> Option<D<'_>> {
    let d = D(src);
    (src.len() >= 10 && d.u16(0) == 1).then_some(d)
}

fn constants(d: D<'_>) -> D<'_> {
    sized(d, 214)
}

fn glyph_info(d: D<'_>) -> D<'_> {
    sized(d, 8)
}

/// MathItalicsCorrectionInfo or MathTopAccentAttachment.
fn value_table(d: D<'_>) -> D<'_> {
    sized(
        d,
        4usize.saturating_add(usize::from(d.u16(2)).saturating_mul(4)),
    )
}

fn kern_info(d: D<'_>) -> D<'_> {
    sized(
        d,
        4usize.saturating_add(usize::from(d.u16(2)).saturating_mul(8)),
    )
}

fn kern(d: D<'_>) -> D<'_> {
    let records = usize::from(d.u16(0)).saturating_mul(2).saturating_add(1);
    sized(d, records.saturating_mul(4).saturating_add(2))
}

fn variants(d: D<'_>) -> D<'_> {
    let d = sized(d, 10);
    let n = usize::from(d.u16(6)).saturating_add(usize::from(d.u16(8)));
    sized(d, n.saturating_mul(2).saturating_add(10))
}

fn construction(d: D<'_>) -> D<'_> {
    sized(
        d,
        4usize.saturating_add(usize::from(d.u16(2)).saturating_mul(4)),
    )
}

fn assembly(d: D<'_>) -> D<'_> {
    sized(
        d,
        6usize.saturating_add(usize::from(d.u16(4)).saturating_mul(10)),
    )
}

fn coverage(d: D<'_>) -> Cov<'_> {
    let n = usize::from(d.u16(2));
    let size = match d.u16(0) {
        1 => n.saturating_mul(2),
        2 => n.saturating_mul(6),
        _ => 0,
    };
    Cov(sized(d, size.saturating_add(4)))
}

/// The size of a Device table (`HintingDevice::get_size`; 6 for the
/// other formats).
fn device_size(d: D<'_>) -> usize {
    match d.u16(4) {
        f @ 1..=3 => {
            let (start, end) = (d.u16(0), d.u16(2));
            if start > end {
                6
            } else {
                let words = usize::from(end.wrapping_sub(start) >> 4u16.wrapping_sub(f));
                words.saturating_add(4).saturating_mul(2)
            }
        }
        _ => 6,
    }
}

/// A Device table, or the Null table when it does not fit
/// (`Device::sanitize`).
fn device(d: D<'_>) -> D<'_> {
    sized(d, device_size(d))
}

/// `MathGlyphConstruction::closure_glyphs`.
fn closure_construction(c: D<'_>, out: &mut RangeSet) {
    let a = assembly(c.off16(0));
    (0..usize::from(a.u16(4))).for_each(|i| {
        out.add(u32::from(
            a.u16(6usize.saturating_add(i.saturating_mul(10))),
        ));
    });
    (0..usize::from(c.u16(2))).for_each(|i| {
        out.add(u32::from(c.u16(4usize.saturating_add(i.saturating_mul(4)))));
    });
}

/// `MATH::closure_glyphs`: adds the variants and parts of the
/// constructions of the glyphs of `glyphs`.
pub(crate) fn closure(src: &[u8], glyphs: &mut RangeSet) {
    let Some(m) = header(src) else { return };
    let v = variants(m.off16(8));
    if v.is_null() {
        return;
    }
    let vert = usize::from(v.u16(6));
    let horiz = usize::from(v.u16(8));
    let mut out = RangeSet::new();
    for (cov_at, first, count) in [(2, 0, vert), (4, vert, horiz)] {
        let cov = v.off16(cov_at);
        if cov.is_null() {
            continue;
        }
        for (i, g) in coverage(cov).glyphs().into_iter().take(count).enumerate() {
            if glyphs.contains(g) {
                let at = first.saturating_add(i).saturating_mul(2).saturating_add(10);
                closure_construction(construction(v.off16(at)), &mut out);
            }
        }
    }
    glyphs.union(&out);
}

/// The new id of a glyph, [`INVALID`] when it is not kept.
fn map(s: &Sub<'_, '_>, g: u32) -> u32 {
    s.p.glyphs.new_gid(g).unwrap_or(INVALID)
}

/// `Device::copy`: keep hinting devices and remap retained VariationIndex
/// devices through the GDEF store's subset map.
fn device_copy(s: &mut Sub<'_, '_>, dev: D<'_>) -> bool {
    if dev.u16(4) == 0x8000 {
        let old = (u32::from(dev.u16(0)) << 16) | u32::from(dev.u16(2));
        let remap = if s.p.var.n.avar2 && !s.p.var.all_axes_pinned() {
            &s.p.layout.math_var_map
        } else {
            &s.p.layout.gdef_var_map
        };
        if let Some(&new) = remap.get(&old) {
            s.s.u16((new >> 16) as u16);
            s.s.u16(u16::try_from(new & 0xffff).unwrap_or_default());
            s.s.u16(0x8000);
            return true;
        }
        return false;
    }
    if !matches!(dev.u16(4), 1..=3) {
        return false;
    }
    s.s.extend(dev.slice(0, device_size(dev)));
    true
}

/// `MathValueRecord::copy` of the record at `at` of `base`.
fn value_record(s: &mut Sub<'_, '_>, base: D<'_>, at: usize) {
    let dev = device(base.off16(at.saturating_add(2)));
    let value = base.u16(at);
    let pos = s.s.u16(value);
    if !crate::layout::gdef::adjust_coordinate(s, pos, value, dev) {
        return;
    }
    let field = s.s.u16(0);
    s.serialize_copy(field, 2, dev, |s| device_copy(s, dev));
}

/// `MathConstants::copy`.
fn constants_copy(s: &mut Sub<'_, '_>, k: D<'_>) -> bool {
    s.s.extend(k.slice(0, 8));
    for i in 0..51usize {
        value_record(s, k, i.saturating_mul(4).saturating_add(8));
    }
    s.s.extend(k.slice(212, 2));
    true
}

/// `MathItalicsCorrectionInfo::subset` and
/// `MathTopAccentAttachment::subset`.
fn value_table_subset(s: &mut Sub<'_, '_>, t: D<'_>) -> bool {
    s.s.u16(0);
    let count_at = s.s.u16(0);
    let glyphs = coverage(t.off16(0)).glyphs();
    let mut new = Vec::new();
    for (i, g) in glyphs.into_iter().take(usize::from(t.u16(2))).enumerate() {
        if !s.p.glyphset_mathed.contains(g) {
            continue;
        }
        value_record(s, t, i.saturating_mul(4).saturating_add(4));
        s.bump_u16(count_at, 1);
        new.push(map(s, g));
    }
    s.serialize_serialize(0, 2, |s| s.coverage(&new));
    true
}

/// `MathKern::copy`.
fn kern_copy(s: &mut Sub<'_, '_>, k: D<'_>) -> bool {
    let n = k.u16(0);
    s.s.u16(n);
    let records = usize::from(n).saturating_mul(2).saturating_add(1);
    for i in 0..records {
        value_record(s, k, i.saturating_mul(4).saturating_add(2));
    }
    true
}

/// `MathKernInfo::subset`.
fn kern_info_subset(s: &mut Sub<'_, '_>, ki: D<'_>) -> bool {
    s.s.u16(0);
    let count_at = s.s.u16(0);
    let glyphs = coverage(ki.off16(0)).glyphs();
    let mut new = Vec::new();
    for (i, g) in glyphs.into_iter().take(usize::from(ki.u16(2))).enumerate() {
        if !s.p.glyphset_mathed.contains(g) {
            continue;
        }
        let record = i.saturating_mul(8).saturating_add(4);
        let at = s.s.zeros(8);
        for corner in 0..4usize {
            let k = kern(ki.off16(record.saturating_add(corner.saturating_mul(2))));
            let field = at.saturating_add(corner.saturating_mul(2));
            s.serialize_copy(field, 2, k, |s| kern_copy(s, k));
        }
        s.bump_u16(count_at, 1);
        new.push(map(s, g));
    }
    s.serialize_serialize(0, 2, |s| s.coverage(&new));
    true
}

/// `MathGlyphInfo::subset`.
fn glyph_info_subset(s: &mut Sub<'_, '_>, gi: D<'_>) -> bool {
    s.s.zeros(8);
    let it = value_table(gi.off16(0));
    s.serialize_subset_opt(0, 2, it, |s| value_table_subset(s, it));
    let ta = value_table(gi.off16(2));
    s.serialize_subset_opt(2, 2, ta, |s| value_table_subset(s, ta));
    let extended: Vec<u32> = coverage(gi.off16(4))
        .glyphs()
        .into_iter()
        .take(s.p.source_glyphs as usize)
        .filter(|&g| s.p.glyphset_mathed.contains(g))
        .map(|g| map(s, g))
        .collect();
    if !extended.is_empty() {
        s.serialize_serialize(4, 2, |s| s.coverage(&extended));
    }
    let ki = kern_info(gi.off16(6));
    s.serialize_subset_opt(6, 2, ki, |s| kern_info_subset(s, ki));
    true
}

/// A glyph id field at `at` of the current object, set to the new id of
/// `g` (`check_assign` of `glyph_map.get`).
fn glyph_field(s: &mut Sub<'_, '_>, at: usize, g: u16) -> bool {
    let new = map(s, u32::from(g));
    s.s.check_assign_u16(at, u64::from(new), Errors::INT_OVERFLOW)
}

/// `MathGlyphAssembly::subset`.
fn assembly_subset(s: &mut Sub<'_, '_>, a: D<'_>) -> bool {
    value_record(s, a, 0);
    let n = a.u16(4);
    s.s.u16(n);
    for i in 0..usize::from(n) {
        let record = i.saturating_mul(10).saturating_add(6);
        let at = s.s.extend(a.slice(record, 10));
        if !glyph_field(s, at, a.u16(record)) {
            return false;
        }
    }
    true
}

/// `MathGlyphConstruction::subset`.
fn construction_subset(s: &mut Sub<'_, '_>, c: D<'_>) -> bool {
    s.s.u16(0);
    let n = c.u16(2);
    s.s.u16(n);
    let a = assembly(c.off16(0));
    s.serialize_subset_opt(0, 2, a, |s| assembly_subset(s, a));
    for i in 0..usize::from(n) {
        let record = i.saturating_mul(4).saturating_add(4);
        let at = s.s.extend(c.slice(record, 4));
        if !glyph_field(s, at, c.u16(record)) {
            return false;
        }
    }
    true
}

/// `MathVariants::collect_coverage_and_indices`: the new ids of the
/// covered glyphs of `_glyphset_cmaped` among indices `first..end`, and
/// their indices.
fn coverage_and_indices(
    s: &Sub<'_, '_>,
    v: D<'_>,
    cov_at: usize,
    first: usize,
    end: usize,
    indices: &mut Vec<usize>,
) -> Vec<u32> {
    let cov = v.off16(cov_at);
    if cov.is_null() {
        return Vec::new();
    }
    let mut new = Vec::new();
    for (i, g) in (first..end).zip(coverage(cov).glyphs()) {
        if s.p.glyphset_cmaped.contains(g) {
            new.push(map(s, g));
            indices.push(i);
        }
    }
    new
}

/// `MathVariants::subset`.
fn variants_subset(s: &mut Sub<'_, '_>, v: D<'_>) -> bool {
    s.s.u16(v.u16(0));
    s.s.zeros(4);
    let vert_at = s.s.u16(0);
    let horiz_at = s.s.u16(0);
    let vert = usize::from(v.u16(6));
    let end = vert.saturating_add(usize::from(v.u16(8)));
    let mut indices = Vec::new();
    let new_vert = coverage_and_indices(s, v, 2, 0, vert, &mut indices);
    let new_horiz = coverage_and_indices(s, v, 4, vert, end, &mut indices);
    s.s.set_u16(vert_at, n16(new_vert.len()));
    s.s.set_u16(horiz_at, n16(new_horiz.len()));
    for i in indices {
        let field = s.s.u16(0);
        let c = construction(v.off16(i.saturating_mul(2).saturating_add(10)));
        s.serialize_subset_opt(field, 2, c, |s| construction_subset(s, c));
    }
    if !new_vert.is_empty() {
        s.serialize_serialize(2, 2, |s| s.coverage(&new_vert));
    }
    if !new_horiz.is_empty() {
        s.serialize_serialize(4, 2, |s| s.coverage(&new_horiz));
    }
    true
}

/// `MATH::subset`.
pub(crate) fn subset(
    p: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _out: &mut Tables,
) -> Result<bool, SubsetError> {
    let Some(m) = header(src) else {
        return Err(SubsetError::Malformed { tag });
    };
    let mut c = Sub::new(p, s, false);
    c.s.extend(m.slice(0, 4));
    c.s.zeros(6);
    let k = constants(m.off16(4));
    c.serialize_copy(4, 2, k, |s| constants_copy(s, k));
    let gi = glyph_info(m.off16(6));
    c.serialize_subset_opt(6, 2, gi, |s| glyph_info_subset(s, gi));
    let v = variants(m.off16(8));
    c.serialize_subset_opt(8, 2, v, |s| variants_subset(s, v));
    if let Some(reason) = c.unsupported {
        return Err(SubsetError::Unsupported { tag, reason });
    }
    Ok(true)
}
