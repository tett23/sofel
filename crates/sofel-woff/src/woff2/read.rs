//! WOFF2 directory and untransformed sfnt table reconstruction.
use super::{DecodeError, Vec, read_entry};
use crate::decode::word;

struct Bits<'a> {
    data: &'a [u8],
    bit: usize,
}

impl Bits<'_> {
    fn take(&mut self, count: u32) -> Option<usize> {
        let mut value = 0;
        for shift in 0..count {
            let byte = self.data.get(self.bit / 8)?;
            value |= usize::from((byte >> (self.bit % 8)) & 1) << shift;
            self.bit = self.bit.checked_add(1)?;
        }
        Some(value)
    }

    fn align(&mut self) -> Option<()> {
        let padding = self.bit.wrapping_neg() & 7;
        (self.take(u32::try_from(padding).ok()?)? == 0).then_some(())
    }
}

fn stored_stream(data: &[u8], expected: usize) -> Option<Vec<u8>> {
    let mut bits = Bits { data, bit: 0 };
    if bits.take(1)? != 0 && bits.take(3)? == 0 && bits.take(3)? == 1 {
        return None;
    }
    let mut out = Vec::new();
    loop {
        if bits.take(1)? != 0 {
            if bits.take(1)? != 1 {
                return None;
            }
            bits.align()?;
            return (out.len() == expected && bits.bit / 8 == data.len()).then_some(out);
        }
        let nibbles = u32::try_from(bits.take(2)?).ok()?.checked_add(4)?;
        if nibbles == 7 {
            return None;
        }
        let encoded = bits.take(nibbles << 2)?;
        if nibbles > 4 && encoded >> (nibbles.saturating_sub(1) << 2) == 0 {
            return None;
        }
        let length = encoded.checked_add(1)?;
        if bits.take(1)? != 1 || out.len().checked_add(length)? > expected {
            return None;
        }
        bits.align()?;
        let start = bits.bit / 8;
        let end = start.checked_add(length)?;
        out.extend_from_slice(data.get(start..end)?);
        bits.bit = end.checked_mul(8)?;
    }
}

/// Reconstructs a single sfnt with untransformed tables in uncompressed Brotli blocks.
///
/// # Errors
/// Reports invalid input, unsupported transformations or compression, and output limit violations.
pub fn decode_woff2(input: &[u8], limit: usize) -> Result<Vec<u8>, DecodeError> {
    let error = |offset, reason| DecodeError { offset, reason };
    if input.get(..4) != Some(b"wOF2") {
        return Err(error(0, "invalid WOFF2 signature"));
    }
    let flavor = word(input, 4)?;
    if !matches!(flavor, 0x10000 | 0x4f54544f) {
        return Err(error(4, "unsupported sfnt flavor"));
    }
    if word(input, 8)? as usize != input.len() {
        return Err(error(8, "file length mismatch"));
    }
    let count = usize::from(u16::try_from(word(input, 12)? >> 16).unwrap_or_default());
    if !(1..4096).contains(&count) {
        return Err(error(12, "invalid table count"));
    }
    let total = word(input, 16)? as usize;
    if total > limit {
        return Err(error(16, "output limit exceeded"));
    }
    let compressed = word(input, 20)? as usize;
    let mut remaining = input.get(48..).ok_or(error(48, "truncated WOFF2 header"))?;
    let mut entries = Vec::with_capacity(count);
    let mut sfnt_size = 12usize.saturating_add(count << 4);
    let mut stored_size = 0usize;
    for _ in 0..count {
        let at = input.len().saturating_sub(remaining.len());
        let entry = read_entry(&mut remaining).ok_or(error(at, "invalid WOFF2 table entry"))?;
        if entry.transformed {
            return Err(error(at, "unsupported transformed table"));
        }
        let length = (entry.original as usize)
            .checked_next_multiple_of(4)
            .ok_or(error(at, "table length overflow"))?;
        sfnt_size = sfnt_size
            .checked_add(length)
            .filter(|n| *n <= limit)
            .ok_or(error(at, "sfnt exceeds limit"))?;
        stored_size = stored_size
            .checked_add(entry.stored as usize)
            .ok_or(error(at, "stream length overflow"))?;
        entries.push(entry);
    }
    if sfnt_size != total {
        return Err(error(16, "sfnt size mismatch"));
    }
    let at = input.len().saturating_sub(remaining.len());
    let stream = remaining
        .get(..compressed)
        .ok_or(error(at, "truncated Brotli data"))?;
    let mut end = at
        .checked_add(compressed)
        .ok_or(error(at, "compressed size overflow"))?;
    for (offset_field, size_field) in [(28, 32), (40, 44)] {
        let offset = word(input, offset_field)? as usize;
        let size = word(input, size_field)? as usize;
        if (offset == 0) != (size == 0)
            || (offset_field == 28 && (offset == 0) != (word(input, 36)? == 0))
        {
            return Err(error(offset_field, "inconsistent optional block fields"));
        }
        if offset != 0 {
            if Some(offset) != end.checked_next_multiple_of(4) {
                return Err(error(offset_field, "invalid optional block offset"));
            }
            end = offset
                .checked_add(size)
                .filter(|n| *n <= input.len())
                .ok_or(error(size_field, "optional block outside file"))?;
        }
    }
    if input.len()
        > end
            .checked_next_multiple_of(4)
            .ok_or(error(end, "padding overflow"))?
    {
        return Err(error(end, "unclaimed data after final block"));
    }
    let decoded = stored_stream(stream, stored_size)
        .ok_or(error(at, "unsupported or malformed Brotli stream"))?;
    let mut data = decoded.as_slice();
    let mut tables = Vec::with_capacity(count);
    for entry in entries {
        let (table, tail) = data
            .split_at_checked(entry.original as usize)
            .ok_or(error(at, "truncated table data"))?;
        tables.push((entry.tag, table));
        data = tail;
    }
    let sfnt = sofel_write::build_sfnt(&tables).map_err(|_| error(at, "invalid sfnt directory"))?;
    if word(&sfnt, 0)? != flavor {
        return Err(error(4, "flavor does not match outline tables"));
    }
    Ok(sfnt)
}
