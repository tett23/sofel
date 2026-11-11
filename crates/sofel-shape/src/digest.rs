//! Approximate glyph sets (`hb_set_digest_t`): three 64-bit masks of
//! glyph id bits that never miss a member, so that lookups and
//! subtables that cannot match a glyph are skipped cheaply (ADR 0020,
//! データ構造).
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-set-digest.hh`, MIT; see
//! THIRD_PARTY_NOTICES.md.

/// The shifts of the three masks.
const SHIFTS: [u32; 3] = [4, 0, 6];

/// A set digest.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Digest {
    masks: [u64; 3],
}

#[allow(dead_code)] // Some items serve the shapers of ADR 0024.
impl Digest {
    /// A digest that may have every glyph.
    pub(crate) fn full() -> Digest {
        Digest {
            masks: [u64::MAX; 3],
        }
    }
    /// Adds a glyph.
    pub(crate) fn add(&mut self, g: u32) {
        for (m, s) in self.masks.iter_mut().zip(SHIFTS) {
            *m |= 1u64 << ((g >> s) & 63);
        }
    }
    /// Adds the glyphs `a..=b`.
    pub(crate) fn add_range(&mut self, a: u32, b: u32) {
        for (m, s) in self.masks.iter_mut().zip(SHIFTS) {
            if (b >> s).wrapping_sub(a >> s) >= 63 {
                *m = u64::MAX;
            } else {
                let ma = 1u64 << ((a >> s) & 63);
                let mb = 1u64 << ((b >> s) & 63);
                *m |= mb
                    .wrapping_add(mb.wrapping_sub(ma))
                    .wrapping_sub(u64::from(mb < ma));
            }
        }
    }
    /// Stops fetching coverage records once every glyph may match.
    pub(crate) fn add_ranges(&mut self, ranges: impl IntoIterator<Item = (u32, u32)>) {
        if *self == Self::full() {
            return;
        }
        for (first, last) in ranges {
            if first == last {
                self.add(first);
            } else {
                self.add_range(first, last);
            }
            if *self == Self::full() {
                break;
            }
        }
    }
    /// The union.
    pub(crate) fn union(&mut self, o: &Digest) {
        for (m, n) in self.masks.iter_mut().zip(o.masks) {
            *m |= n;
        }
    }
    /// Whether the set may have `g`.
    pub(crate) fn may_have(&self, g: u32) -> bool {
        self.masks
            .iter()
            .zip(SHIFTS)
            .all(|(&m, s)| m & (1u64 << ((g >> s) & 63)) != 0)
    }
    /// Whether the sets may share a glyph.
    pub(crate) fn may_intersect(&self, o: &Digest) -> bool {
        self.masks.iter().zip(o.masks).all(|(&m, n)| m & n != 0)
    }
}

#[cfg(test)]
mod saturation_tests {
    use super::Digest;

    #[test]
    fn adr0042_i1_ranges_match_full_traversal() {
        for end in [0, 1, 62, 63, 64, 1023, 4031, 4095, 65535] {
            let ranges = [(0, end), (63, 63), (64, 65), (end, end)];
            let mut expected = Digest::default();
            for (a, b) in ranges {
                expected.add_range(a, b);
            }
            let mut actual = Digest::default();
            actual.add_ranges(ranges);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn adr0042_i2_stop_before_reading_after_saturation() {
        let mut d = Digest::default();
        let mut n = 0;
        d.add_ranges(
            [(0, 4031), (4032, 4095), (4096, 4096)]
                .into_iter()
                .inspect(|_| n += 1),
        );
        assert_eq!(d, Digest::full());
        assert_eq!(n, 2);
        d.add_ranges([(0, 0)].into_iter().inspect(|_| n += 1));
        assert_eq!(n, 2);
    }
}
