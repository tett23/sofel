//! HarfBuzz's item_variations_t::as_item_varstore for explicit index maps.
use alloc::collections::{BTreeMap, BinaryHeap};
use alloc::vec::Vec;
use core::cmp::Reverse;

use super::instance::{Block, Region};
use super::{SubsetError, Tag};

struct Encoding {
    widths: Vec<u8>,
    rows: Vec<Vec<i32>>,
}

impl Encoding {
    fn new(rows: Vec<Vec<i32>>, columns: usize) -> Self {
        let mut widths = alloc::vec![0;columns];
        for row in &rows {
            for (width, &value) in widths.iter_mut().zip(row) {
                let size = match value {
                    0 => 0,
                    -128..=127 => 1,
                    -32768..=32767 => 2,
                    _ => 4,
                };
                *width = (*width).max(size);
            }
        }
        if widths.contains(&4) {
            for width in &mut widths {
                if *width == 1 {
                    *width = 2;
                }
            }
        }
        Self { widths, rows }
    }

    fn width(&self) -> usize {
        self.widths.iter().map(|&n| usize::from(n)).sum()
    }
    fn overhead(&self) -> usize {
        10usize
            .saturating_add(2usize.saturating_mul(self.widths.iter().filter(|&&w| w != 0).count()))
    }
    fn gain(&self, other: &Self) -> usize {
        let mut combined = 0usize;
        let mut overhead = 10usize;
        for (&a, &b) in self.widths.iter().zip(&other.widths) {
            combined = combined.saturating_add(usize::from(a.max(b)));
            if a != 0 || b != 0 {
                overhead = overhead.saturating_add(2);
            }
        }
        self.overhead()
            .saturating_add(other.overhead())
            .saturating_sub(overhead)
            .saturating_sub(
                combined
                    .saturating_sub(self.width())
                    .saturating_mul(self.rows.len()),
            )
            .saturating_sub(
                combined
                    .saturating_sub(other.width())
                    .saturating_mul(other.rows.len()),
            )
    }
    fn merge(&mut self, other: Self) {
        for (a, b) in self.widths.iter_mut().zip(other.widths) {
            *a = (*a).max(b);
        }
        self.rows.extend(other.rows);
    }
}

fn combine(mut encodings: Vec<Encoding>) -> Vec<Encoding> {
    encodings.sort_by(|a, b| {
        a.width()
            .cmp(&b.width())
            .then_with(|| b.widths.cmp(&a.widths))
    });
    let mut queue = BinaryHeap::new();
    for (j, second) in encodings.iter().enumerate() {
        for (i, first) in encodings.iter().take(j).enumerate() {
            let gain = first.gain(second);
            if gain > 0 {
                queue.push((gain, Reverse((i, j))));
            }
        }
    }
    let mut slots: Vec<_> = encodings.into_iter().map(Some).collect();
    while let Some((_, Reverse((i, j)))) = queue.pop() {
        if !slots.get(i).is_some_and(Option::is_some) || !slots.get(j).is_some_and(Option::is_some)
        {
            continue;
        }
        let Some(mut merged) = slots.get_mut(i).and_then(Option::take) else {
            continue;
        };
        let Some(other) = slots.get_mut(j).and_then(Option::take) else {
            continue;
        };
        merged.merge(other);
        let next = slots.len();
        for (k, slot) in slots.iter_mut().enumerate() {
            let Some(existing) = slot.as_ref() else {
                continue;
            };
            if existing.widths == merged.widths {
                if let Some(existing) = slot.take() {
                    merged.rows.extend(existing.rows);
                }
            } else {
                let gain = merged.gain(existing);
                if gain > 0 {
                    queue.push((gain, Reverse((k, next))));
                }
            }
        }
        slots.push(Some(merged));
    }
    slots.into_iter().flatten().collect()
}

pub(super) fn optimize(
    blocks: &mut Vec<Block>,
    regions: &[Region],
    tag: Tag,
) -> Result<BTreeMap<u32, u32>, SubsetError> {
    let limit = SubsetError::Limit { tag };
    let ids: BTreeMap<_, _> = regions.iter().enumerate().map(|(i, r)| (r, i)).collect();
    let mut sources = BTreeMap::<Vec<i32>, Vec<u32>>::new();
    let mut encodings = Vec::new();
    for (outer, (count, columns)) in blocks.drain(..).enumerate() {
        let mut rows = alloc::vec![alloc::vec![0;regions.len()];count];
        for (region, values) in columns {
            let col = *ids.get(&region).ok_or(limit)?;
            for (row, v) in rows.iter_mut().zip(values) {
                *row.get_mut(col).ok_or(limit)? = v;
            }
        }
        let mut unique = Vec::new();
        for (inner, row) in rows.into_iter().enumerate() {
            if tag == Tag::new(b"MVAR") && row.iter().all(|&delta| delta == 0) {
                continue;
            }
            let index = variation_index(outer, inner, tag)?;
            let source = sources.entry(row.clone()).or_default();
            if source.is_empty() {
                unique.push(row);
            }
            source.push(index);
        }
        if !unique.is_empty() {
            encodings.push(Encoding::new(unique, regions.len()));
        }
    }
    let mut remap = BTreeMap::new();
    for mut encoding in combine(encodings) {
        encoding.rows.sort_unstable();
        for chunk in encoding.rows.chunks(usize::from(u16::MAX)) {
            for (inner, row) in chunk.iter().enumerate() {
                let index = variation_index(blocks.len(), inner, tag)?;
                for old in sources.remove(row).ok_or(limit)? {
                    remap.insert(old, index);
                }
            }
            let mut columns = Vec::new();
            for (i, region) in regions.iter().enumerate() {
                let values = chunk
                    .iter()
                    .map(|r| r.get(i).copied().ok_or(limit))
                    .collect::<Result<Vec<_>, _>>()?;
                if values.iter().any(|&v| v != 0) {
                    columns.push((region.clone(), values));
                }
            }
            blocks.push((chunk.len(), columns));
        }
    }
    Ok(remap)
}

fn variation_index(outer: usize, inner: usize, tag: Tag) -> Result<u32, SubsetError> {
    let limit = SubsetError::Limit { tag };
    Ok((u32::from(u16::try_from(outer).map_err(|_| limit)?) << 16)
        | u32::from(u16::try_from(inner).map_err(|_| limit)?))
}
