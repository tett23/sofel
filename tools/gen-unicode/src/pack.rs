//! Multi-level tables (ADR 0019, "データ構造").
//!
//! A table maps a code point to a value. Values outside `lo..=hi` are the
//! default. Inside, with `c = cp - lo`:
//!
//! ```text
//! i1 = S1[c >> (k2 + k3)]
//! i2 = S2[(i1 << k2) + ((c >> k3) & ((1 << k2) - 1))]
//! v  = S3[(i2 << k3) + (c & ((1 << k3) - 1))]
//! ```
//!
//! With `k2 = 0` the table has two levels and `S2` is omitted:
//! `v = S3[(S1[c >> k3] << k3) + (c & ((1 << k3) - 1))]`.

use std::collections::HashMap;

/// The widths an element of a level may have, in bits.
pub const WIDTHS: [u32; 4] = [1, 4, 8, 16];

/// The largest `k2` and `k3` tried.
pub const MAX_K: u32 = 8;

/// One level of a table: its elements and their width in bits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Level {
    /// Width of one element in bits, one of [`WIDTHS`].
    pub bits: u32,
    /// The elements.
    pub items: Vec<u32>,
}

impl Level {
    fn new(items: Vec<u32>) -> Option<Level> {
        let max = items.iter().copied().max().unwrap_or(0);
        WIDTHS
            .iter()
            .copied()
            .find(|&b| u64::from(max) < (1u64 << b))
            .map(|bits| Level { bits, items })
    }

    /// Bytes taken by the level: 1- and 4-bit elements are packed into bytes.
    pub fn bytes(&self) -> usize {
        (self.items.len() * self.bits as usize).div_ceil(8)
    }
}

/// A packed table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packed {
    /// The value outside `lo..=hi`.
    pub default: u32,
    /// The first code point with a value other than the default.
    pub lo: u32,
    /// The last code point with a value other than the default.
    pub hi: u32,
    /// Bits of the index into the second level (0: two levels).
    pub k2: u32,
    /// Bits of the index into a block of values.
    pub k3: u32,
    /// Indices of blocks of `S2` (or of `S3` with two levels).
    pub s1: Level,
    /// Indices of blocks of `S3`; `None` with two levels.
    pub s2: Option<Level>,
    /// Blocks of values.
    pub s3: Level,
}

impl Packed {
    /// Total bytes of the levels.
    pub fn bytes(&self) -> usize {
        self.s1.bytes() + self.s2.as_ref().map_or(0, Level::bytes) + self.s3.bytes()
    }

    /// Number of levels (2 or 3).
    pub fn levels(&self) -> u32 {
        if self.s2.is_some() { 3 } else { 2 }
    }

    /// Look up `cp` the way the generated code does.
    pub fn get(&self, cp: u32) -> u32 {
        self.get_inside(cp).unwrap_or(self.default)
    }

    fn get_inside(&self, cp: u32) -> Option<u32> {
        let c = cp.checked_sub(self.lo)?;
        if c > self.hi - self.lo {
            return None;
        }
        let i1 = *self.s1.items.get((c >> (self.k2 + self.k3)) as usize)?;
        let i2 = match &self.s2 {
            Some(s2) => *s2
                .items
                .get(((i1 << self.k2) | ((c >> self.k3) & mask(self.k2))) as usize)?,
            None => i1,
        };
        self.s3
            .items
            .get(((i2 << self.k3) | (c & mask(self.k3))) as usize)
            .copied()
    }
}

fn mask(k: u32) -> u32 {
    (1u32 << k) - 1
}

/// Split `items` into blocks of `size`, padding the last with `pad`, and
/// remove duplicate blocks. Returns the distinct blocks concatenated in the
/// order of their first appearance, and the block number of each block.
fn dedup(items: &[u32], size: usize, pad: u32) -> (Vec<u32>, Vec<u32>) {
    let mut seen: HashMap<Vec<u32>, u32> = HashMap::new();
    let mut blocks = Vec::new();
    let ids = items
        .chunks(size)
        .map(|chunk| {
            let mut block = chunk.to_vec();
            block.resize(size, pad);
            let next = seen.len() as u32;
            *seen.entry(block.clone()).or_insert_with(|| {
                blocks.extend_from_slice(&block);
                next
            })
        })
        .collect();
    (blocks, ids)
}

/// Pack `values[lo..=hi]` with the given `k2` and `k3`. `None` if an
/// element does not fit in 16 bits.
pub fn pack_with(values: &[u32], default: u32, lo: u32, k2: u32, k3: u32) -> Option<Packed> {
    let hi = lo + values.len() as u32 - 1;
    let (s3, ids3) = dedup(values, 1 << k3, default);
    let (s1, s2) = if k2 == 0 {
        (ids3, None)
    } else {
        let (s2, ids2) = dedup(&ids3, 1 << k2, 0);
        (ids2, Some(Level::new(s2)?))
    };
    Some(Packed {
        default,
        lo,
        hi,
        k2,
        k3,
        s1: Level::new(s1)?,
        s2,
        s3: Level::new(s3)?,
    })
}

/// Pack a table of every code point (`values[cp]`), trying every `k2` and
/// `k3` in `0..=MAX_K` and keeping the smallest; ties go to fewer levels,
/// then to the smaller `k3`, then to the smaller `k2`. `None` if every
/// value is the default.
pub fn pack(values: &[u32], default: u32) -> Option<Packed> {
    let lo = values.iter().position(|&v| v != default)?;
    let hi = values.iter().rposition(|&v| v != default)?;
    let inside = &values[lo..=hi];
    (0..=MAX_K)
        .flat_map(|k2| (0..=MAX_K).map(move |k3| (k2, k3)))
        .filter_map(|(k2, k3)| pack_with(inside, default, lo as u32, k2, k3))
        .min_by_key(|p| (p.bytes(), p.levels(), p.k3, p.k2))
}

/// Check that the packed table returns `values[cp]` for every `cp` in
/// `values`, and the default beyond.
pub fn verify(name: &str, packed: &Packed, values: &[u32], default: u32) -> Result<(), String> {
    let beyond = [values.len() as u32, 0x7FFF_FFFF, u32::MAX];
    let expected = |cp: u32| values.get(cp as usize).copied().unwrap_or(default);
    (0..values.len() as u32)
        .chain(beyond)
        .find(|&cp| packed.get(cp) != expected(cp))
        .map_or(Ok(()), |cp| {
            Err(format!(
                "{name}: U+{cp:04X} packs to {} but is {}",
                packed.get(cp),
                expected(cp)
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u32> {
        let mut v = vec![0u32; 3000];
        v[100] = 3;
        v[101..140].iter_mut().for_each(|x| *x = 7);
        v[600..900]
            .iter_mut()
            .enumerate()
            .for_each(|(i, x)| *x = (i % 5) as u32);
        v[2000] = 300;
        v[2001] = 1;
        v
    }

    #[test]
    fn every_split_returns_the_values() {
        let v = sample();
        let (lo, hi) = (100, 2001);
        for k2 in 0..=MAX_K {
            for k3 in 0..=MAX_K {
                let p = pack_with(&v[lo..=hi], 0, lo as u32, k2, k3).unwrap();
                verify("sample", &p, &v, 0).unwrap();
                assert_eq!(p.levels(), if k2 == 0 { 2 } else { 3 });
            }
        }
    }

    #[test]
    fn the_chosen_split_is_the_smallest() {
        let v = sample();
        let best = pack(&v, 0).unwrap();
        assert_eq!((best.lo, best.hi), (100, 2001));
        let all: Vec<Packed> = (0..=MAX_K)
            .flat_map(|k2| (0..=MAX_K).map(move |k3| (k2, k3)))
            .map(|(k2, k3)| pack_with(&v[100..=2001], 0, 100, k2, k3).unwrap())
            .collect();
        let min = all.iter().map(Packed::bytes).min().unwrap();
        assert_eq!(best.bytes(), min);
        let ties: Vec<&Packed> = all.iter().filter(|p| p.bytes() == min).collect();
        let fewest = ties.iter().map(|p| p.levels()).min().unwrap();
        assert_eq!(best.levels(), fewest);
        let smallest_k3 = ties
            .iter()
            .filter(|p| p.levels() == fewest)
            .map(|p| p.k3)
            .min()
            .unwrap();
        assert_eq!(best.k3, smallest_k3);
    }

    #[test]
    fn values_outside_the_bounds_are_the_default() {
        let mut v = vec![9u32; 50];
        v[10] = 1;
        v[20] = 2;
        let p = pack(&v, 9).unwrap();
        assert_eq!((p.lo, p.hi), (10, 20));
        assert_eq!(p.get(9), 9);
        assert_eq!(p.get(21), 9);
        assert_eq!(p.get(u32::MAX), 9);
        verify("small", &p, &v, 9).unwrap();
    }

    #[test]
    fn all_default_has_no_table() {
        assert_eq!(pack(&[4, 4, 4], 4), None);
    }

    #[test]
    fn widths_are_the_smallest_that_fit() {
        assert_eq!(Level::new(vec![0, 1]).unwrap().bits, 1);
        assert_eq!(Level::new(vec![2, 15]).unwrap().bits, 4);
        assert_eq!(Level::new(vec![16, 255]).unwrap().bits, 8);
        assert_eq!(Level::new(vec![256, 65535]).unwrap().bits, 16);
        assert_eq!(Level::new(vec![65536]), None);
        assert_eq!(Level::new(vec![1; 9]).unwrap().bytes(), 2);
        assert_eq!(Level::new(vec![5; 3]).unwrap().bytes(), 2);
    }

    #[test]
    fn a_wrong_table_is_caught() {
        let v = sample();
        let mut p = pack(&v, 0).unwrap();
        p.s3.items
            .iter_mut()
            .filter(|x| **x == 300)
            .for_each(|x| *x = 301);
        assert!(verify("broken", &p, &v, 0).is_err());
    }
}
