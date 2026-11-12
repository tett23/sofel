//! WOFF 1.0 directory validation and sfnt reconstruction.
use super::*;
use alloc::vec;

/// Invalid WOFF field or compressed table, with the byte offset of that field or table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeError {
    /// Position in the input container.
    pub offset: usize,
    /// Reason the container could not be decoded.
    pub reason: &'static str,
}
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WOFF byte {}: {}", self.offset, self.reason)
    }
}
impl core::error::Error for DecodeError {}
type Decoded<T> = core::result::Result<T, DecodeError>;
fn error(offset: usize, reason: &'static str) -> DecodeError {
    DecodeError { offset, reason }
}
fn word(bytes: &[u8], at: usize) -> Decoded<u32> {
    bytes
        .get(at..)
        .and_then(|s| s.first_chunk::<4>())
        .copied()
        .map(u32::from_be_bytes)
        .ok_or(error(at, "truncated field"))
}
#[derive(Clone, Copy)]
struct Entry {
    tag: u32,
    offset: usize,
    stored: usize,
    original: usize,
    sum: u32,
    at: usize,
}
fn padded(n: usize, at: usize) -> Decoded<usize> {
    n.checked_next_multiple_of(4)
        .ok_or(error(at, "length overflow"))
}

/// Decodes WOFF 1.0 tables into a single sfnt, preserving their physical order.
///
/// # Errors
/// Reports the input field or table for invalid lengths, checksums and
/// compressed data. The sfnt must fit in `limit` bytes.
pub fn decode_woff(input: &[u8], limit: usize) -> Decoded<Vec<u8>> {
    if input.get(..4) != Some(b"wOFF") {
        return Err(error(0, "invalid signature"));
    }
    let flavor = word(input, 4)?;
    if !matches!(flavor, 0x10000 | 0x4f54544f | 0x74727565 | 0x74797031) {
        return Err(error(4, "invalid sfnt flavor"));
    }
    if word(input, 8)? as usize != input.len() {
        return Err(error(8, "file length mismatch"));
    }
    let counts = word(input, 12)?;
    let count = u16::try_from(counts >> 16).map_err(|_| error(12, "table count overflow"))?;
    if count == 0 || count >= 4096 || counts & 0xffff != 0 {
        return Err(error(12, "invalid table count or reserved field"));
    }
    let total = word(input, 16)? as usize;
    if total > limit {
        return Err(error(16, "output limit exceeded"));
    }
    let end = 44usize.saturating_add(20 * usize::from(count));
    let directory = input
        .get(44..end)
        .ok_or(error(44, "truncated table directory"))?;
    let mut records = Vec::with_capacity(usize::from(count));
    let mut prev = None;
    for (i, row) in directory.as_chunks::<20>().0.iter().enumerate() {
        let at = 44usize.saturating_add(i.saturating_mul(20));
        let tag = word(row, 0)?;
        if prev.is_some_and(|p| tag <= p) {
            return Err(error(at, "tags not strictly ordered"));
        }
        records.push(Entry {
            tag,
            offset: word(row, 4)? as usize,
            stored: word(row, 8)? as usize,
            original: word(row, 12)? as usize,
            sum: word(row, 16)?,
            at,
        });
        prev = Some(tag);
    }
    records.sort_by_key(|r| (r.offset, r.stored));
    let mut source = end;
    let mut target = 12usize.saturating_add(16 * usize::from(count));
    for r in &records {
        if r.offset != source || r.stored > r.original {
            return Err(error(
                r.at.saturating_add(4),
                "invalid table offset or length",
            ));
        }
        source = source
            .checked_add(padded(r.stored, r.at)?)
            .filter(|&n| n <= input.len())
            .ok_or(error(r.at, "table outside input"))?;
        target = target
            .checked_add(padded(r.original, r.at)?)
            .filter(|&n| n <= total)
            .ok_or(error(r.at, "table outside sfnt"))?;
    }
    if target != total {
        return Err(error(16, "sfnt size mismatch"));
    }
    for (offset_field, len_field) in [(24, 28), (36, 40)] {
        let offset = word(input, offset_field)? as usize;
        let len = word(input, len_field)? as usize;
        if offset == 0 {
            if len != 0 || (offset_field == 24 && word(input, 32)? != 0) {
                return Err(error(offset_field, "missing optional block"));
            }
            continue;
        }
        if offset != padded(source, offset_field)?
            || len == 0
            || (offset_field == 24 && word(input, 32)? == 0)
        {
            return Err(error(offset_field, "invalid optional block"));
        }
        source = offset
            .checked_add(len)
            .filter(|&n| n <= input.len())
            .ok_or(error(offset_field, "optional block outside input"))?;
    }
    if source != input.len() {
        return Err(error(8, "unclaimed bytes after tables"));
    }
    let mut out = Vec::from(flavor.to_be_bytes());
    let power = u16::try_from(count.ilog2()).map_err(|_| error(12, "invalid table count"))?;
    let range = 16u16 << power;
    for n in [
        count,
        range,
        power,
        count.saturating_mul(16).saturating_sub(range),
    ] {
        out.extend(n.to_be_bytes());
    }
    let mut dir = vec![[0u8; 16]; usize::from(count)];
    out.resize(12usize.saturating_add(16 * usize::from(count)), 0);
    for r in records {
        let offset = u32::try_from(out.len()).map_err(|_| error(r.at, "sfnt offset overflow"))?;
        let block = input
            .get(r.offset..)
            .and_then(|d| d.get(..r.stored))
            .ok_or(error(r.at, "truncated table"))?;
        if r.stored == r.original {
            out.extend(block);
        } else {
            let data = sofel_inflate::zlib(block, r.original)
                .map_err(|_| error(r.offset, "invalid zlib stream"))?;
            if data.len() != r.original {
                return Err(error(r.offset, "expanded length mismatch"));
            }
            out.extend(data);
        }
        let data = out
            .get(offset as usize..)
            .ok_or(error(r.at, "invalid sfnt offset"))?;
        let mut sum = checksum(data);
        if r.tag == Tag::new(b"head").to_u32() {
            sum = sum.wrapping_sub(word(data, 8).map_err(|_| error(r.offset, "truncated head"))?);
        }
        if sum != r.sum {
            return Err(error(r.at.saturating_add(16), "table checksum mismatch"));
        }
        let index = r.at.saturating_sub(44) / 20;
        let row = dir
            .get_mut(index)
            .ok_or(error(r.at, "invalid directory index"))?;
        for (slot, value) in row.as_chunks_mut::<4>().0.iter_mut().zip([
            r.tag,
            r.sum,
            offset,
            u32::try_from(r.original).map_err(|_| error(r.at, "table length overflow"))?,
        ]) {
            slot.copy_from_slice(&value.to_be_bytes());
        }
        out.resize(padded(out.len(), r.at)?, 0);
    }
    out.get_mut(12..)
        .and_then(|d| d.get_mut(..dir.as_flattened().len()))
        .ok_or(error(12, "truncated sfnt directory"))?
        .copy_from_slice(dir.as_flattened());
    Ok(out)
}
