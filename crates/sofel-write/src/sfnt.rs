//! Assembling an sfnt file from its tables, as HarfBuzz 15.1.0's face
//! builder does (`src/hb-face-builder.cc`, `src/hb-open-file.hh`).

use alloc::vec::Vec;

use sofel_read::Tag;

use crate::WriteError;

const HEAD: Tag = Tag::new(b"head");
const CFF: Tag = Tag::new(b"CFF ");
const CFF2: Tag = Tag::new(b"CFF2");
/// `head`'s fixed size in HarfBuzz (`head::static_size`).
const HEAD_SIZE: usize = 54;
const CHECKSUM_ADJUSTMENT: usize = 8;

/// Builds an sfnt file from `tables`, as HarfBuzz's face builder.
///
/// The version is `OTTO` when there is a `CFF ` or `CFF2` table and
/// `0x00010000` otherwise. The table data are laid out from the shortest
/// to the longest (equal lengths by tag), each padded with zeros to a
/// multiple of 4; the table records are sorted by tag. Each record's
/// checksum covers the padded data, with `head`'s `checkSumAdjustment`
/// taken as 0; `checkSumAdjustment` is then `0xB1B0AFBA` minus the sum
/// of the directory's and the tables' checksums.
///
/// # Errors
///
/// [`WriteError::DuplicateTable`] for a tag given twice,
/// [`WriteError::TooManyTables`] for more than 65535 tables and
/// [`WriteError::TooLarge`] for 2 GiB or more of output.
pub fn build_sfnt(tables: &[(Tag, &[u8])]) -> Result<Vec<u8>, WriteError> {
    build_sfnt_ordered(tables, &[])
}

/// [`build_sfnt`] with the tables of `order` first, in that order
/// (`hb_face_builder_sort_tables`); the other tables follow as in
/// [`build_sfnt`]. Tags of `order` without a table are ignored.
///
/// # Errors
///
/// As [`build_sfnt`].
pub fn build_sfnt_ordered(tables: &[(Tag, &[u8])], order: &[Tag]) -> Result<Vec<u8>, WriteError> {
    let mut sorted: Vec<(Tag, &[u8])> = tables.to_vec();
    sorted.sort_by_key(|&(tag, _)| tag);
    if let Some(w) = sorted
        .windows(2)
        .find(|w| w.first().map(|a| a.0) == w.get(1).map(|b| b.0))
    {
        return Err(WriteError::DuplicateTable(w.first().map_or(HEAD, |a| a.0)));
    }
    let count = u16::try_from(sorted.len()).map_err(|_| WriteError::TooManyTables)?;
    if count == u16::MAX {
        return Err(WriteError::TooManyTables);
    }
    let rank = |tag: Tag| -> u32 {
        // The first occurrence wins, as `hb_face_builder_sort_tables`
        // counts each tag found (a repeated tag gets the later order).
        order
            .iter()
            .filter(|t| sorted.iter().any(|(s, _)| s == *t))
            .enumerate()
            .filter(|(_, t)| **t == tag)
            .map(|(i, _)| u32::try_from(i).unwrap_or(u32::MAX))
            .last()
            .unwrap_or(u32::MAX)
    };
    let mut data_order: Vec<(u32, Tag, &[u8])> =
        sorted.iter().map(|&(tag, d)| (rank(tag), tag, d)).collect();
    data_order.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.2.len().cmp(&b.2.len()))
            .then(a.1.cmp(&b.1))
    });

    let dir_len = 12usize.saturating_add(usize::from(count).saturating_mul(16));
    let total = data_order
        .iter()
        .try_fold(dir_len, |acc, (_, _, d)| {
            acc.checked_add(d.len().checked_add(3)? & !3)
        })
        .ok_or(WriteError::TooLarge)?;
    if total >= 1 << 31 {
        return Err(WriteError::TooLarge);
    }

    let version: u32 = if sorted.iter().any(|(t, _)| *t == CFF || *t == CFF2) {
        0x4F54_544F
    } else {
        0x0001_0000
    };
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&version.to_be_bytes());
    let entry_selector = u16::BITS
        .saturating_sub(count.leading_zeros())
        .max(1)
        .saturating_sub(1);
    let search_range = 16u32 << entry_selector;
    let range_shift = (16 * u32::from(count)).saturating_sub(search_range);
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(&be16(search_range));
    out.extend_from_slice(&be16(entry_selector));
    out.extend_from_slice(&be16(range_shift));
    out.resize(dir_len, 0);

    // Records in data order first; sorted by tag below.
    let mut records: Vec<(Tag, u32, u32, u32)> = Vec::with_capacity(sorted.len());
    let mut adjustment_at = None;
    for &(_, tag, d) in &data_order {
        let start = out.len();
        out.extend_from_slice(d);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        if tag == HEAD && out.len().saturating_sub(start) >= HEAD_SIZE {
            let at = start.saturating_add(CHECKSUM_ADJUSTMENT);
            if let Some(s) = out.get_mut(at..at.saturating_add(4)) {
                s.fill(0);
            }
            adjustment_at = Some(at);
        }
        let sum = checksum(out.get(start..).unwrap_or(&[]));
        records.push((tag, sum, to_u32(start), to_u32(d.len())));
    }
    records.sort_by_key(|r| r.0);
    for (i, (tag, sum, offset, len)) in records.iter().enumerate() {
        let at = 12usize.saturating_add(i.saturating_mul(16));
        let mut rec = [0u8; 16];
        rec[..4].copy_from_slice(&tag.0);
        rec[4..8].copy_from_slice(&sum.to_be_bytes());
        rec[8..12].copy_from_slice(&offset.to_be_bytes());
        rec[12..].copy_from_slice(&len.to_be_bytes());
        if let Some(s) = out.get_mut(at..at.saturating_add(16)) {
            s.copy_from_slice(&rec);
        }
    }
    if let Some(at) = adjustment_at {
        let sum = records
            .iter()
            .fold(checksum(out.get(..dir_len).unwrap_or(&[])), |acc, r| {
                acc.wrapping_add(r.1)
            });
        let v = 0xB1B0_AFBAu32.wrapping_sub(sum);
        if let Some(s) = out.get_mut(at..at.saturating_add(4)) {
            s.copy_from_slice(&v.to_be_bytes());
        }
    }
    Ok(out)
}

/// The sfnt checksum of `data`: the sum of its big-endian 32-bit words,
/// the last one padded with zeros.
pub fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |acc, c| {
        let mut w = [0u8; 4];
        w.iter_mut().zip(c).for_each(|(d, s)| *d = *s);
        acc.wrapping_add(u32::from_be_bytes(w))
    })
}

fn be16(v: u32) -> [u8; 2] {
    u16::try_from(v).unwrap_or(u16::MAX).to_be_bytes()
}

fn to_u32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}
