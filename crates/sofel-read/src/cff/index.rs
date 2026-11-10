//! The CFF INDEX: a count, an offset size and `count + 1` offsets into
//! the object data that follows them.

use super::Version;
use crate::ReadError;
use crate::be::{self, to_usize};

/// A view over an INDEX. The count is 16 bits wide in CFF and 32 bits
/// wide in CFF2.
#[derive(Clone, Copy, Debug)]
pub struct Index<'a> {
    /// From the start of the INDEX to the end of the data given.
    data: &'a [u8],
    count: u32,
    off_size: u8,
    /// Where offset 0 starts: after the count and the offset size.
    offsets_at: usize,
}

impl Index<'static> {
    /// An empty INDEX.
    pub const fn empty(version: Version) -> Index<'static> {
        Index {
            data: &[],
            count: 0,
            off_size: 0,
            offsets_at: version.count_len(),
        }
    }
}

impl<'a> Index<'a> {
    /// A view over the INDEX at the start of `data`.
    ///
    /// Fails with [`ReadError::Truncated`] if the count, the offset size
    /// or the offsets do not fit, and with [`ReadError::InvalidValue`] if
    /// the offset size is not 1 to 4. The object data is not checked;
    /// [`Index::get`] returns `None` for objects outside `data`.
    pub fn new(data: &'a [u8], version: Version) -> Result<Index<'a>, ReadError> {
        let count_len = version.count_len();
        let count = match version {
            Version::Cff => be::u16_at(data, 0).map(u32::from),
            Version::Cff2 => be::u32_at(data, 0),
        }
        .ok_or(ReadError::truncated(count_len, data.len()))?;
        if count == 0 {
            return Ok(Index {
                data,
                count,
                off_size: 0,
                offsets_at: count_len,
            });
        }
        let off_size = be::u8_at(data, count_len).ok_or(ReadError::truncated(
            count_len.saturating_add(1),
            data.len(),
        ))?;
        if !(1..=4).contains(&off_size) {
            return Err(ReadError::InvalidValue);
        }
        let offsets_at = count_len.saturating_add(1);
        let needed = to_usize(count)
            .saturating_add(1)
            .saturating_mul(usize::from(off_size))
            .saturating_add(offsets_at);
        if needed > data.len() {
            return Err(ReadError::truncated(needed, data.len()));
        }
        Ok(Index {
            data,
            count,
            off_size,
            offsets_at,
        })
    }

    /// The number of objects.
    pub fn len(&self) -> u32 {
        self.count
    }

    /// Whether there are no objects.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The offset size, 1 to 4; 0 for an empty INDEX.
    pub fn off_size(&self) -> u8 {
        self.off_size
    }

    /// The bytes from the start of the INDEX to the end of the data the
    /// view was built over.
    pub fn bytes(&self) -> &'a [u8] {
        self.data
    }

    /// Offset `i` (0 to [`Index::len`]) as stored: 1 is the first byte
    /// of the object data.
    pub fn offset(&self, i: u32) -> Option<u32> {
        if i > self.count || self.count == 0 {
            return None;
        }
        let at = to_usize(i)
            .saturating_mul(usize::from(self.off_size))
            .saturating_add(self.offsets_at);
        let bytes = be::slice_at(self.data, at, usize::from(self.off_size))?;
        Some(bytes.iter().fold(0u32, |v, &b| (v << 8) | u32::from(b)))
    }

    /// Where the object data starts, from the start of the INDEX: the
    /// position of offset 1.
    pub fn data_offset(&self) -> usize {
        to_usize(self.count)
            .saturating_add(1)
            .saturating_mul(usize::from(self.off_size))
            .saturating_add(self.offsets_at)
    }

    /// The position just after the INDEX, from its start: after the last
    /// object. `None` when the last offset is 0.
    pub fn end(&self) -> Option<usize> {
        if self.count == 0 {
            return Some(self.offsets_at);
        }
        let last = self.offset(self.count)?;
        let last = to_usize(last.checked_sub(1)?);
        Some(self.data_offset().saturating_add(last))
    }

    /// Object `i`: the bytes between offsets `i` and `i + 1`, when the
    /// first offset is at least 1, not above the second, and the bytes
    /// are inside the data.
    pub fn get(&self, i: u32) -> Option<&'a [u8]> {
        let start = self.offset(i)?.checked_sub(1)?;
        let end = self.offset(i.checked_add(1)?)?.checked_sub(1)?;
        if start > end {
            return None;
        }
        let base = self.data_offset();
        self.data
            .get(base.saturating_add(to_usize(start))..base.saturating_add(to_usize(end)))
    }

    /// Object `i` as FreeType's `cff_index_access_element` reads it
    /// (CharStrings, Top DICTs, Font DICTs): the end is the next non-zero
    /// offset, cut at the end of the data; an object that starts at
    /// offset 0 or does not end after its start is empty. `None` for
    /// `i` not below the count (FreeType's `Invalid_Argument`).
    pub fn ft_element(&self, i: u32) -> Option<&'a [u8]> {
        if i >= self.count {
            return None;
        }
        let off1 = self.offset(i)?;
        let mut off2 = 0u32;
        if off1 != 0 {
            let mut e = i;
            loop {
                e = e.saturating_add(1);
                off2 = self.offset(e).unwrap_or(0);
                if off2 != 0 || e >= self.count {
                    break;
                }
            }
        }
        let size = self.data.len();
        let base = self.data_offset();
        // off2 > size + 1 || data_offset > size - off2 + 1
        let limit = size.saturating_sub(base).saturating_add(1);
        let off2 = if to_usize(off2) > limit {
            u32::try_from(limit).unwrap_or(u32::MAX)
        } else {
            off2
        };
        if off1 != 0 && off2 > off1 {
            let start = base.saturating_add(to_usize(off1)).saturating_sub(1);
            let end = base.saturating_add(to_usize(off2)).saturating_sub(1);
            Some(self.data.get(start..end).unwrap_or_default())
        } else {
            Some(&[])
        }
    }

    /// FreeType's `cff_index_get_pointers` offset `n` (0 to the count),
    /// from the start of the object data: offset 0 is 0, and each later
    /// one is the stored offset minus 1, raised to the one before it and
    /// cut at the size of the object data.
    fn ft_clamped(&self, n: u32, data_size: u32) -> u32 {
        (1..=n).fold(0u32, |cur, k| {
            let next = self
                .offset(k)
                .and_then(|o| o.checked_sub(1))
                .unwrap_or(u32::MAX);
            cur.max(next).min(data_size)
        })
    }

    /// Whether the offsets need none of FreeType's corrections in
    /// `cff_index_get_pointers`: the first is 1 and each later one is not
    /// below the one before it. Takes time in proportion to the count;
    /// pass the result to [`Index::ft_pointer`].
    pub fn ft_offsets_sorted(&self) -> bool {
        if self.count == 0 {
            return true;
        }
        let mut previous = 1u32;
        if self.offset(0) != Some(1) {
            return false;
        }
        (1..=self.count).all(|k| {
            let o = self.offset(k).unwrap_or(0);
            let ok = o >= previous;
            previous = o;
            ok
        })
    }

    /// Object `i` as FreeType reads it through `cff_index_get_pointers`
    /// (subroutines and strings): offsets corrected as described at
    /// [`Index::ft_offsets_sorted`]. With `sorted` from that function
    /// (true), it takes constant time; otherwise time in proportion to
    /// `i`. `None` for `i` not below the count or when the object data
    /// is not inside the data.
    pub fn ft_pointer(&self, i: u32, sorted: bool) -> Option<&'a [u8]> {
        if i >= self.count {
            return None;
        }
        let data_size = self.offset(self.count)?.checked_sub(1)?;
        let (start, end) = if sorted {
            let s = self.offset(i)?.saturating_sub(1).min(data_size);
            let e = self
                .offset(i.saturating_add(1))?
                .saturating_sub(1)
                .min(data_size);
            (s, e.max(s))
        } else {
            let s = self.ft_clamped(i, data_size);
            (s, self.ft_clamped(i.saturating_add(1), data_size))
        };
        let base = self.data_offset();
        self.data
            .get(base.saturating_add(to_usize(start))..base.saturating_add(to_usize(end)))
    }
}
