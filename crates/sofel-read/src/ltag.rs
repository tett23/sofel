//! Apple language tag ranges, shared by name lookup and AAT shaping.
use crate::be;

/// A language table with all string ranges checked against the table length.
#[derive(Clone, Copy)]
pub struct Ltag<'a> {
    data: &'a [u8],
    ranges: &'a [[u8; 4]],
}

fn string<'a>(data: &'a [u8], range: &[u8; 4]) -> Option<&'a [u8]> {
    let [a, b, c, d] = *range;
    be::slice_at(
        data,
        usize::from(u16::from_be_bytes([a, b])),
        usize::from(u16::from_be_bytes([c, d])),
    )
}

impl<'a> Ltag<'a> {
    /// A malformed range invalidates the whole table in HarfBuzz.
    pub fn new(data: &'a [u8]) -> Option<Self> {
        if be::u32_at(data, 0)? < 1 {
            return None;
        }
        let ranges = be::chunks_at::<4>(data, 12, be::to_usize(be::u32_at(data, 8)?))?;
        if !ranges.iter().all(|r| string(data, r).is_some()) {
            return None;
        }
        Some(Self { data, ranges })
    }

    /// The raw tag at the given zero-based index.
    pub fn tag(self, index: u16) -> Option<&'a [u8]> {
        string(self.data, self.ranges.get(usize::from(index))?)
    }
}

#[cfg(test)]
mod tests {
    use super::Ltag;

    #[test]
    fn i2_last_range_must_end_within_the_table() {
        let data = [
            0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 20, 0, 2, 0, 22, 0, 2, b's', b'r', b'z', b'h',
        ];
        let tags = Ltag::new(&data);
        assert_eq!(tags.and_then(|tags| tags.tag(1)), Some(&b"zh"[..]));
        assert_eq!(tags.and_then(|tags| tags.tag(2)), None);
        assert!(Ltag::new(&data[..23]).is_none());
    }
}
