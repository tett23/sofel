use super::*;

/// IEEE CRC-32 as stored in PNG chunks and gzip members.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}
fn adler32(data: &[u8]) -> u32 {
    let (a, b) = data.iter().fold((1u32, 0u32), |(a, b), byte| {
        let a = a.wrapping_add(u32::from(*byte)) % 65521;
        (a, b.wrapping_add(a) % 65521)
    });
    b << 16 | a
}

/// Decodes a complete zlib stream with an explicit byte limit.
///
/// # Errors
/// Rejects preset dictionaries, malformed data, oversized output and invalid checksums.
pub fn zlib(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    let (&[cmf, flg], body) = data.split_first_chunk::<2>().ok_or(Error::Invalid)?;
    if cmf & 15 != 8
        || cmf >> 4 > 7
        || flg & 32 != 0
        || !u16::from_be_bytes([cmf, flg]).is_multiple_of(31)
    {
        return Err(Error::Invalid);
    }
    let (payload, footer) = body.split_last_chunk::<4>().ok_or(Error::Invalid)?;
    let mut out = Vec::new();
    if blocks::decode(
        payload,
        &mut out,
        limit,
        1usize << 8u8.saturating_add(cmf >> 4),
    )? != payload.len()
    {
        return Err(Error::Invalid);
    }
    if adler32(&out) != u32::from_be_bytes(*footer) {
        return Err(Error::Checksum);
    }
    Ok(out)
}

fn member_data(header: &[u8]) -> Result<&[u8]> {
    let (fixed, mut rest) = header.split_first_chunk::<10>().ok_or(Error::Invalid)?;
    let &[0x1f, 0x8b, 8, flags, ..] = fixed else {
        return Err(Error::Invalid);
    };
    if flags & 0xe0 != 0 {
        return Err(Error::Invalid);
    }
    if flags & 4 != 0 {
        let (len, fields) = rest.split_first_chunk::<2>().ok_or(Error::Invalid)?;
        rest = fields
            .get(usize::from(u16::from_le_bytes(*len))..)
            .ok_or(Error::Invalid)?;
    }
    for flag in [8, 16] {
        if flags & flag != 0 {
            let end = rest
                .iter()
                .position(|b| *b == 0)
                .ok_or(Error::Invalid)?
                .saturating_add(1);
            rest = rest.get(end..).ok_or(Error::Invalid)?;
        }
    }
    if flags & 2 != 0 {
        let consumed = header.len().saturating_sub(rest.len());
        let (sum, body) = rest.split_first_chunk::<2>().ok_or(Error::Invalid)?;
        let crc = crc32(header.get(..consumed).ok_or(Error::Invalid)?);
        if u32::from(u16::from_le_bytes(*sum)) != crc & 0xffff {
            return Err(Error::Checksum);
        }
        rest = body;
    }
    Ok(rest)
}

/// Decodes all gzip members, verifying each trailer and bounding total output.
///
/// # Errors
/// Returns an error for malformed members, invalid checksums or oversized output.
pub fn gzip(mut input: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let payload = member_data(input)?;
        let start = out.len();
        let used = blocks::decode(payload, &mut out, limit, 32768)?;
        let (trailer, rest) = payload
            .get(used..)
            .and_then(|d| d.split_first_chunk::<8>())
            .ok_or(Error::Invalid)?;
        let member = out.get(start..).ok_or(Error::Invalid)?;
        let &[a, b, c, d, e, f, g, h] = trailer;
        if u32::from_le_bytes([a, b, c, d]) != crc32(member)
            || u64::from(u32::from_le_bytes([e, f, g, h])) != member.len() as u64 & 0xffff_ffff
        {
            return Err(Error::Checksum);
        }
        if rest.is_empty() {
            return Ok(out);
        }
        input = rest;
    }
}
