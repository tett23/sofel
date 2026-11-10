//! HarfBuzz 15.1.0's `hb_hashmap_t<unsigned, unsigned>` (`src/hb-map.hh`).
//!
//! The repacker iterates some maps (the parents of a vertex, the subgraph
//! being isolated), and the order of that iteration decides which vertex
//! is cloned first and which parent is followed. This is the same open
//! addressing table with the same hash (Knuth's multiplicative hash, which
//! has no seed), sizes, probing and tombstones, so the iteration order is
//! HarfBuzz's.

use alloc::vec::Vec;

#[derive(Clone, Copy, Default, Debug)]
struct Item {
    key: u32,
    value: u32,
    hash: u32,
    used: bool,
    real: bool,
}

/// `hb_hashmap_t<unsigned, unsigned>`.
#[derive(Clone, Debug)]
pub(crate) struct HbMap {
    successful: bool,
    max_chain_length: u32,
    population: u32,
    occupancy: u32,
    mask: u32,
    prime: u32,
    items: Vec<Item>,
}

impl Default for HbMap {
    fn default() -> Self {
        HbMap::new()
    }
}

const NONE: u32 = u32::MAX;

/// `hb_hash` of an unsigned integer.
fn hb_hash(k: u32) -> u32 {
    k.wrapping_mul(2_654_435_761)
}

/// `hb_bit_storage`: the number of bits needed for `v`.
fn bit_storage(v: u32) -> u32 {
    32u32.wrapping_sub(v.leading_zeros())
}

const PRIMES: [u32; 32] = [
    1,
    2,
    3,
    7,
    13,
    31,
    61,
    127,
    251,
    509,
    1021,
    2039,
    4093,
    8191,
    16381,
    32749,
    65521,
    131_071,
    262_139,
    524_287,
    1_048_573,
    2_097_143,
    4_194_301,
    8_388_593,
    16_777_213,
    33_554_393,
    67_108_859,
    134_217_689,
    268_435_399,
    536_870_909,
    1_073_741_789,
    2_147_483_647,
];

fn prime_for(shift: u32) -> u32 {
    PRIMES.get(shift as usize).copied().unwrap_or(2_147_483_647)
}

impl HbMap {
    /// An empty map without storage.
    pub(crate) fn new() -> HbMap {
        HbMap {
            successful: true,
            max_chain_length: 0,
            population: 0,
            occupancy: 0,
            mask: 0,
            prime: 0,
            items: Vec::new(),
        }
    }

    /// The number of keys.
    pub(crate) fn population(&self) -> u32 {
        self.population
    }

    /// Whether there are no keys.
    pub(crate) fn is_empty(&self) -> bool {
        self.population == 0
    }

    /// `alloc`.
    fn alloc(&mut self, new_population: u32) -> bool {
        if !self.successful {
            return false;
        }
        if self.population > 0x3FFF_FFFF || new_population > 0x3FFF_FFFF {
            self.successful = false;
            return false;
        }
        if new_population != 0 && new_population.wrapping_add(new_population / 2) < self.mask {
            return true;
        }
        let power = bit_storage(self.population.max(new_population).wrapping_mul(2).max(4));
        let new_size = 1usize.checked_shl(power).unwrap_or(0);
        if new_size == 0 {
            self.successful = false;
            return false;
        }
        let old = core::mem::replace(&mut self.items, alloc::vec![Item::default(); new_size]);
        self.population = 0;
        self.occupancy = 0;
        self.mask = u32::try_from(new_size.wrapping_sub(1)).unwrap_or(u32::MAX);
        self.prime = prime_for(power);
        self.max_chain_length = power.wrapping_mul(2);
        for it in old.into_iter().filter(|it| it.real) {
            self.set_with_hash(it.key, it.hash, it.value, true);
        }
        true
    }

    fn item(&self, i: u32) -> Item {
        self.items.get(i as usize).copied().unwrap_or_default()
    }

    /// `set_with_hash`.
    fn set_with_hash(&mut self, key: u32, hash: u32, value: u32, overwrite: bool) -> bool {
        if !self.successful {
            return false;
        }
        if self.occupancy.wrapping_add(self.occupancy / 2) >= self.mask && !self.alloc(0) {
            return false;
        }
        let hash = hash & 0x3FFF_FFFF;
        let mut tombstone = NONE;
        let mut i = hash.checked_rem(self.prime).unwrap_or(0);
        let mut length = 0u32;
        let mut step = 0u32;
        loop {
            let it = self.item(i);
            if !it.used {
                break;
            }
            if it.key == key {
                if !overwrite {
                    return false;
                }
                break;
            }
            if !it.real && tombstone == NONE {
                tombstone = i;
            }
            step = step.wrapping_add(1);
            i = i.wrapping_add(step) & self.mask;
            length = length.wrapping_add(1);
        }
        let at = if tombstone == NONE { i } else { tombstone };
        let Some(item) = self.items.get_mut(at as usize) else {
            return false;
        };
        if item.used {
            self.occupancy = self.occupancy.wrapping_sub(1);
            self.population = self.population.wrapping_sub(u32::from(item.real));
        }
        *item = Item {
            key,
            value,
            hash,
            used: true,
            real: true,
        };
        self.occupancy = self.occupancy.wrapping_add(1);
        self.population = self.population.wrapping_add(1);
        if length > self.max_chain_length && self.occupancy > self.mask / 8 {
            self.alloc(self.mask.wrapping_sub(8));
        }
        true
    }

    /// `set`: inserts or overwrites; false when allocation failed.
    pub(crate) fn set(&mut self, key: u32, value: u32) -> bool {
        self.set_with_hash(key, hb_hash(key), value, true)
    }

    /// `fetch_item`: the slot of a present key.
    fn fetch(&self, key: u32) -> Option<usize> {
        if self.items.is_empty() {
            return None;
        }
        let hash = hb_hash(key) & 0x3FFF_FFFF;
        let mut i = hash.checked_rem(self.prime).unwrap_or(0);
        let mut step = 0u32;
        loop {
            let it = self.item(i);
            if !it.used {
                return None;
            }
            if it.key == key {
                return it.real.then_some(i as usize);
            }
            step = step.wrapping_add(1);
            i = i.wrapping_add(step) & self.mask;
        }
    }

    /// The value of `key`.
    pub(crate) fn get(&self, key: u32) -> Option<u32> {
        self.fetch(key)
            .and_then(|i| self.items.get(i))
            .map(|it| it.value)
    }

    /// The value of `key`, in place.
    pub(crate) fn get_mut(&mut self, key: u32) -> Option<&mut u32> {
        let i = self.fetch(key)?;
        self.items.get_mut(i).map(|it| &mut it.value)
    }

    /// Whether `key` is present.
    pub(crate) fn has(&self, key: u32) -> bool {
        self.fetch(key).is_some()
    }

    /// `del`: leaves a tombstone.
    pub(crate) fn del(&mut self, key: u32) {
        if let Some(it) = self.fetch(key).and_then(|i| self.items.get_mut(i)) {
            it.real = false;
            self.population = self.population.wrapping_sub(1);
        }
    }

    /// `clear` (and `reset`): drops every key but keeps the storage.
    pub(crate) fn clear(&mut self) {
        self.successful = true;
        if self.population == 0 && self.occupancy == 0 {
            return;
        }
        self.items.iter_mut().for_each(|it| *it = Item::default());
        self.population = 0;
        self.occupancy = 0;
    }

    /// The keys and values in storage order (`iter`).
    pub(crate) fn iter(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.items
            .iter()
            .filter(|it| it.real)
            .map(|it| (it.key, it.value))
    }

    /// The keys in storage order (`keys`).
    pub(crate) fn keys(&self) -> impl Iterator<Item = u32> + '_ {
        self.iter().map(|(k, _)| k)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )]
    use super::*;

    #[test]
    fn iteration_follows_the_hash_slots() {
        let mut m = HbMap::new();
        for k in [5u32, 1, 9, 3] {
            m.set(k, k * 10);
        }
        // Size 8, prime 7: slot = (k * 2654435761 & 0x3FFFFFFF) % 7.
        let slots: Vec<(u32, u32)> = [5u32, 1, 9, 3]
            .iter()
            .map(|&k| ((hb_hash(k) & 0x3FFF_FFFF) % 7, k))
            .collect();
        let mut expected = slots.clone();
        expected.sort();
        // No collisions among these keys, so storage order is slot order.
        let keys: Vec<u32> = m.keys().collect();
        assert_eq!(keys, expected.iter().map(|&(_, k)| k).collect::<Vec<_>>());
        assert_eq!(m.get(9), Some(90));
        m.del(9);
        assert!(!m.has(9));
        assert_eq!(m.population(), 3);
        *m.get_mut(5).unwrap() += 1;
        assert_eq!(m.get(5), Some(51));
    }

    #[test]
    fn grows_and_keeps_keys() {
        let mut m = HbMap::new();
        for k in 0..1000u32 {
            assert!(m.set(k, k + 1));
        }
        assert_eq!(m.population(), 1000);
        assert!((0..1000u32).all(|k| m.get(k) == Some(k + 1)));
        m.clear();
        assert!(m.is_empty());
        assert_eq!(m.keys().count(), 0);
    }
}
