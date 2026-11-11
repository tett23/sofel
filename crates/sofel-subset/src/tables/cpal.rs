//! CPAL (`src/OT/Color/CPAL/CPAL.hh`): the palette entries COLR uses,
//! palettes with the same kept colors sharing their records, and the
//! name ids of version 1 for the plan's name closure.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::cpal::Cpal;
use sofel_write::Serializer;

use super::Tables;
use crate::error::SubsetError;
use crate::layout::data::{D, n16};
use crate::plan::Plan;
use crate::set::RangeSet;

/// The version 1 offsets (flags, palette labels, color labels).
fn v1_offsets(t: D<'_>) -> [u32; 3] {
    let tail = 12usize.saturating_add(usize::from(t.u16(4)).saturating_mul(2));
    [0, 4, 8].map(|k| t.u32(tail.saturating_add(k)))
}

/// `CPAL::collect_name_ids`: the palette labels and the labels of the
/// kept palette entries (`palettes` keys are old entry indices).
pub(crate) fn collect_name_ids(data: &[u8], palettes: &BTreeMap<u32, u32>, names: &mut RangeSet) {
    if Cpal::new(data).is_err() {
        return;
    }
    let t = D(data);
    if t.u16(0) != 1 {
        return;
    }
    let [_, labels, color_labels] = v1_offsets(t);
    if labels != 0 {
        let l = t.at(labels as usize);
        (0..usize::from(t.u16(4))).for_each(|i| names.add(u32::from(l.u16(i.saturating_mul(2)))));
    }
    if color_labels != 0 {
        let l = t.at(color_labels as usize);
        (0..t.u16(2))
            .filter(|i| palettes.contains_key(&u32::from(*i)))
            .for_each(|i| names.add(u32::from(l.u16(usize::from(i).saturating_mul(2)))));
    }
}

/// `CPAL::subset`.
pub(crate) fn subset(
    plan: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    if Cpal::new(src).is_err() {
        return Err(SubsetError::Failed { tag });
    }
    let t = D(src);
    let (version, num_colors, num_palettes) = (t.u16(0), t.u16(2), t.u16(4));
    let palettes = &plan.color.palettes;
    if num_palettes == 0 || palettes.is_empty() {
        return Ok(false);
    }
    let retained: Vec<u32> = palettes.keys().copied().filter(|&k| k != 0xFFFF).collect();
    if retained.is_empty() {
        return Ok(false);
    }
    let pop = retained.len();
    let records = t.follow(t.u32(8));
    let num_records = usize::from(t.u16(6));
    let record = |i: usize| -> u32 {
        if i < num_records {
            records.u32(i.saturating_mul(4))
        } else {
            0
        }
    };
    let indices: Vec<u16> = (0..usize::from(num_palettes))
        .map(|i| t.u16(12usize.saturating_add(i.saturating_mul(2))))
        .collect();
    let mut first_for_layer: Vec<usize> = Vec::new();
    let mut first_to_layer: BTreeMap<u16, usize> = BTreeMap::new();
    let mut colors_to_layer: BTreeMap<Vec<u32>, usize> = BTreeMap::new();
    for &first in &indices {
        if first_to_layer.contains_key(&first) {
            continue;
        }
        let colors: Vec<u32> = retained
            .iter()
            .map(|&c| record(usize::from(first).saturating_add(c as usize)))
            .collect();
        let layer = match colors_to_layer.get(&colors) {
            Some(&l) => l,
            None => {
                let l = first_for_layer.len();
                first_for_layer.push(usize::from(first));
                colors_to_layer.insert(colors, l);
                l
            }
        };
        first_to_layer.insert(first, layer);
    }
    s.u16(version);
    s.u16(n16(pop));
    s.u16(num_palettes);
    s.u16(n16(first_for_layer.len().saturating_mul(pop)));
    s.u32(0);
    for first in &indices {
        let layer = first_to_layer.get(first).copied().unwrap_or(0);
        s.u16(n16(layer.saturating_mul(pop)));
    }
    s.push();
    for &first in &first_for_layer {
        for &c in &retained {
            s.u32(record(first.saturating_add(c as usize)));
        }
    }
    let idx = s.pop_pack(true);
    s.link32(8, idx);
    if version != 1 {
        return Ok(true);
    }
    let tail = s.zeros(12);
    let [flags, labels, color_labels] = v1_offsets(t);
    let copy = |s: &mut Serializer, at: usize, off: u32, items: usize, size: usize| {
        if off == 0 {
            return;
        }
        let d = t.at(off as usize);
        s.push();
        let bytes: Vec<u8> = (0..items.saturating_mul(size)).map(|i| d.u8(i)).collect();
        s.extend(&bytes);
        let idx = s.pop_pack(true);
        s.link32(at, idx);
    };
    copy(s, tail, flags, usize::from(num_palettes), 4);
    copy(
        s,
        tail.saturating_add(4),
        labels,
        usize::from(num_palettes),
        2,
    );
    if color_labels != 0 {
        let d = t.at(color_labels as usize);
        s.push();
        (0..num_colors)
            .filter(|i| palettes.contains_key(&u32::from(*i)))
            .for_each(|i| {
                s.u16(d.u16(usize::from(i).saturating_mul(2)));
            });
        let idx = s.pop_pack(true);
        s.link32(tail.saturating_add(8), idx);
    }
    Ok(true)
}
