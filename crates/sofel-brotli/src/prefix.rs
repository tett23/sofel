//! Canonical prefix codes from the Brotli bitstream.
use super::Bits;
use alloc::{vec, vec::Vec};

pub(super) enum Prefix {
    Single(usize),
    Tree {
        counts: [usize; 16],
        symbols: Vec<usize>,
    },
}

impl Prefix {
    fn from_lengths(lengths: &[u8]) -> Option<Self> {
        let mut counts = [0usize; 16];
        for &len in lengths.iter().filter(|&&n| n != 0) {
            let count = counts.get_mut(usize::from(len))?;
            *count = count.checked_add(1)?;
        }
        let mut space = 1usize;
        let mut symbols = Vec::new();
        for (depth, &count) in counts.iter().enumerate().skip(1) {
            space = space.checked_mul(2)?.checked_sub(count)?;
            symbols.extend(
                lengths
                    .iter()
                    .enumerate()
                    .filter_map(|(s, &n)| (usize::from(n) == depth).then_some(s)),
            );
        }
        (space == 0).then_some(Self::Tree { counts, symbols })
    }

    pub(super) fn symbol(&self, bits: &mut Bits<'_>) -> Option<usize> {
        match self {
            Self::Single(s) => Some(*s),
            Self::Tree { counts, symbols } => {
                let mut code = 0usize;
                let mut offset = 0usize;
                for &count in counts.iter().skip(1) {
                    code = code.checked_mul(2)?.checked_add(bits.take(1)?)?;
                    if code < count {
                        return symbols.get(offset.checked_add(code)?).copied();
                    }
                    offset = offset.checked_add(count)?;
                    code = code.checked_sub(count)?;
                }
                None
            }
        }
    }

    pub(super) fn read(bits: &mut Bits<'_>, alphabet: usize) -> Option<Self> {
        let skip = bits.take(2)?;
        if skip != 1 {
            return Self::complex(bits, alphabet, skip);
        }
        let n = bits.take(2)?.checked_add(1)?;
        let width = usize::BITS.checked_sub(alphabet.checked_sub(1)?.leading_zeros())?;
        let mut symbols = Vec::with_capacity(n);
        for _ in 0..n {
            let s = bits.take(width)?;
            if s >= alphabet || symbols.contains(&s) {
                return None;
            }
            symbols.push(s);
        }
        if let [one] = symbols.as_slice() {
            return Some(Self::Single(*one));
        }
        let lengths: &[u8] = match n {
            2 => &[1, 1],
            3 => &[1, 2, 2],
            4 if bits.take(1)? == 0 => &[2, 2, 2, 2],
            4 => &[1, 2, 3, 3],
            _ => return None,
        };
        let mut full = vec![0; alphabet];
        for (s, &len) in symbols.into_iter().zip(lengths) {
            *full.get_mut(s)? = len;
        }
        Self::from_lengths(&full)
    }

    fn complex(bits: &mut Bits<'_>, alphabet: usize, skip: usize) -> Option<Self> {
        let mut lengths = [0u8; 18];
        let mut space = 32usize;
        for s in [1, 2, 3, 4, 0, 5, 17, 6, 16, 7, 8, 9, 10, 11, 12, 13, 14, 15]
            .into_iter()
            .skip(skip)
        {
            let len = match bits.take(2)? {
                0 => 0,
                1 => 4,
                2 => 3,
                3 if bits.take(1)? == 0 => 2,
                3 if bits.take(1)? == 0 => 1,
                3 => 5,
                _ => return None,
            };
            *lengths.get_mut(s)? = len;
            if len != 0 {
                space = space.checked_sub(32 >> len)?;
            }
            if space == 0 {
                break;
            }
        }
        let codes = if lengths.iter().filter(|&&n| n != 0).count() == 1 {
            Self::Single(lengths.iter().position(|&n| n != 0)?)
        } else {
            Self::from_lengths(&lengths)?
        };
        let mut output = Vec::with_capacity(alphabet);
        let mut previous = 8;
        let mut repeat = 0usize;
        let mut repeated = None;
        let mut left = 32768usize;
        while output.len() < alphabet && left > 0 {
            let s = codes.symbol(bits)?;
            let (length, n) = if s < 16 {
                repeat = 0;
                repeated = None;
                let len = u8::try_from(s).ok()?;
                if len != 0 {
                    previous = len;
                }
                (len, 1)
            } else {
                let (len, extra) = if s == 16 { (previous, 2) } else { (0, 3) };
                if repeated != Some(len) {
                    repeat = 0;
                }
                repeated = Some(len);
                let old = repeat;
                repeat = if old == 0 {
                    0
                } else {
                    old.checked_sub(2)?.checked_shl(extra)?
                };
                repeat = repeat.checked_add(3)?.checked_add(bits.take(extra)?)?;
                (len, repeat.checked_sub(old)?)
            };
            let end = output.len().checked_add(n)?;
            if end > alphabet {
                return None;
            }
            output.resize(end, length);
            if length != 0 {
                left = left.checked_sub(n.checked_mul(32768 >> length)?)?;
            }
        }
        Self::from_lengths(&output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adr0065_i2_prefix_truncation_and_canonical_order() {
        let tree = Prefix::from_lengths(&[2, 1, 2]);
        let mut bits = Bits {
            data: &[0b00011101],
            bit: 0,
        };
        let decoded = tree.and_then(|t| {
            Some([
                t.symbol(&mut bits)?,
                t.symbol(&mut bits)?,
                t.symbol(&mut bits)?,
            ])
        });
        assert_eq!(decoded, Some([0, 2, 0]));
        assert!(Prefix::read(&mut Bits { data: &[], bit: 0 }, 256).is_none());
        assert!(Prefix::from_lengths(&[1, 1, 1]).is_none());
    }
}
