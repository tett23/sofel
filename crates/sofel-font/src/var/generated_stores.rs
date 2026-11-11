//! ADR 0029 compares cached and uncached row evaluation on generated stores.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::{
    Arith, Fixed, Ft, HbF32, HbF64, ItemVariationStore, OnTheFly, StoreScalars, item_delta,
};
use alloc::vec::Vec;

struct Generator(u32);

impl Generator {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0
    }

    fn below(&mut self, limit: u16) -> u16 {
        u16::try_from((self.next() >> 8) % u32::from(limit)).unwrap()
    }

    fn store(&mut self, axes: u16, regions: u16, groups: u16) -> Vec<u8> {
        let region_offset = 8 + 4 * usize::from(groups);
        let mut bytes = Vec::from(1u16.to_be_bytes());
        bytes.extend(u32::try_from(region_offset).unwrap().to_be_bytes());
        bytes.extend(groups.to_be_bytes());
        bytes.resize(region_offset, 0);
        bytes.extend(axes.to_be_bytes());
        bytes.extend(regions.to_be_bytes());
        for _ in 0..regions {
            for _ in 0..axes {
                let axis = match self.below(8) {
                    0 => [1i16, -1, 0],       // out of order
                    1 => [-8192, 4096, 8192], // crosses zero
                    2 => [0, 8192, 16384],
                    3 => [-16384, -8192, 0],
                    _ => [0, 0, 0],
                };
                for coordinate in axis {
                    bytes.extend(coordinate.to_be_bytes());
                }
            }
        }
        for group in 0..groups {
            let slot = 8 + 4 * usize::from(group);
            let offset = u32::try_from(bytes.len()).unwrap();
            bytes[slot..slot + 4].copy_from_slice(&offset.to_be_bytes());
            let columns = self.below(9);
            let words = self.below(columns + 1);
            let long = self.below(2) == 1;
            bytes.extend(2u16.to_be_bytes());
            bytes.extend((words | if long { 0x8000 } else { 0 }).to_be_bytes());
            bytes.extend(columns.to_be_bytes());
            for _ in 0..columns {
                bytes.extend(self.below(regions + 1).to_be_bytes());
            }
            for _ in 0..2 {
                for column in 0..columns {
                    let width = match (long, column < words) {
                        (true, true) => 4,
                        (false, false) => 1,
                        _ => 2,
                    };
                    bytes.extend(&self.next().to_be_bytes()[4 - width..]);
                }
            }
        }
        bytes
    }
}

fn compare<A: Arith, B: Eq + core::fmt::Debug>(
    view: &ItemVariationStore<'_>,
    coords: &[A::Coord],
    bits: impl Fn(A::Delta) -> B,
) {
    let cached = StoreScalars::<A::Scalar>::from_store::<A>(view, coords, true);
    let live = OnTheFly::<A>::new(view, coords, true);
    for outer in 0..view.data_count() {
        for inner in 0..2 {
            assert_eq!(
                bits(item_delta::<A>(view, &cached, outer, inner)),
                bits(item_delta::<A>(view, &live, outer, inner)),
                "outer={outer}, inner={inner}"
            );
        }
    }
}

#[test]
fn adr0029_i02_cached_and_live_generated_rows_match_bitwise() {
    let mut generator = Generator(29);
    for case in 0..1000 {
        let axes = 1 + generator.below(8);
        let regions = generator.below(65);
        let groups = 1 + generator.below(8);
        let bytes = generator.store(axes, regions, groups);
        let view = ItemVariationStore::new(&bytes).unwrap();
        let mut hb = Vec::new();
        let mut ft = Vec::new();
        for _ in 0..axes {
            let (h, f) = match generator.below(6) {
                0 => (-16384, -65536),
                1 => (0, 0),
                2 => (16384, 65536),
                _ => (
                    i32::from(generator.below(32769)) - 16384,
                    i32::try_from(generator.next() % 131073).unwrap() - 65536,
                ),
            };
            hb.push(h);
            ft.push(Fixed(f));
        }
        assert_eq!(view.data_count(), groups, "case {case}");
        compare::<HbF32, _>(&view, &hb, f32::to_bits);
        compare::<HbF64, _>(&view, &hb, f64::to_bits);
        compare::<Ft, _>(&view, &ft, |value| value);
    }
}

#[test]
fn adr0029_ft_item_delta_is_independent_of_column_order() {
    let mut generator = Generator(29);
    for _ in 0..1000 {
        let mut columns = Vec::new();
        for _ in 0..(1 + generator.below(12)) {
            let index = generator.below(3);
            let delta = i32::from_be_bytes(generator.next().to_be_bytes());
            columns.push((index, delta));
        }
        let coord = Fixed(i32::from(generator.below(32769)) - 16384);
        let evaluate = |columns: &[(u16, i32)]| {
            let mut bytes = Vec::from([
                0, 1, 0, 0, 0, 12, 0, 1, 0, 0, 0, 34, // store and data offsets
                0, 1, 0, 3, // one axis, three regions
                0, 0, 0x10, 0, 0x40, 0, // positive peak
                0xc0, 0, 0xf0, 0, 0, 0, // negative peak
                0, 0, 0, 0, 0, 0, // neutral axis
            ]);
            let count = u16::try_from(columns.len()).unwrap();
            bytes.extend(1u16.to_be_bytes());
            bytes.extend((0x8000 | count).to_be_bytes());
            bytes.extend(count.to_be_bytes());
            for (index, _) in columns {
                bytes.extend(index.to_be_bytes());
            }
            for (_, delta) in columns {
                bytes.extend(delta.to_be_bytes());
            }
            let view = ItemVariationStore::new(&bytes).unwrap();
            let coords = [coord];
            let source = OnTheFly::<Ft>::new(&view, &coords, true);
            item_delta::<Ft>(&view, &source, 0, 0)
        };
        let forward = evaluate(&columns);
        columns.reverse();
        assert_eq!(forward, evaluate(&columns));
    }
}
