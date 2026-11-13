//! WOFF2 directory and untransformed sfnt table reconstruction.
use super::{DecodeError, Vec, read_entry};
use crate::decode::word;

struct CollectionFace {
    flavor: u32,
    indices: Vec<usize>,
}

struct CollectionDirectory {
    version: u32,
    faces: Vec<CollectionFace>,
    size: usize,
}

fn collection_directory(
    remaining: &mut &[u8],
    file_len: usize,
    entries: &[super::Entry],
    limit: usize,
) -> Result<CollectionDirectory, DecodeError> {
    let error = |data: &[u8], reason| DecodeError {
        offset: file_len.saturating_sub(data.len()),
        reason,
    };
    let read_word = |data: &mut &[u8]| {
        let (bytes, tail) = data
            .split_first_chunk::<4>()
            .ok_or(error(data, "truncated collection directory"))?;
        *data = tail;
        Ok::<_, DecodeError>(u32::from_be_bytes(*bytes))
    };
    let version = read_word(remaining)?;
    if !matches!(version, 0x10000 | 0x20000) {
        return Err(error(remaining, "invalid collection version"));
    }
    let count = usize::from(
        super::glyf::uint255(remaining).ok_or(error(remaining, "missing collection face count"))?,
    );
    if count == 0 {
        return Err(error(remaining, "empty collection"));
    }
    let header = if version == 0x20000 { 24usize } else { 12 };
    let mut size = header.saturating_add(count.saturating_mul(4));
    let mut faces = Vec::new();
    for _ in 0..count {
        let tables = usize::from(
            super::glyf::uint255(remaining).ok_or(error(remaining, "missing face table count"))?,
        );
        if tables == 0 || tables > entries.len() {
            return Err(error(remaining, "invalid face table count"));
        }
        size = size
            .checked_add(12usize.saturating_add(tables.saturating_mul(16)))
            .filter(|n| *n <= limit)
            .ok_or(error(remaining, "collection directories exceed limit"))?;
        let flavor = read_word(remaining)?;
        if !matches!(flavor, 0x10000 | 0x4f54544f) {
            return Err(error(remaining, "invalid collection face flavor"));
        }
        let mut indices = Vec::with_capacity(tables);
        let mut glyf = None;
        let mut loca = None;
        for _ in 0..tables {
            let before = *remaining;
            let index = usize::from(
                super::glyf::uint255(remaining)
                    .ok_or(error(before, "truncated collection table index"))?,
            );
            let entry = entries
                .get(index)
                .ok_or(error(before, "invalid collection table index"))?;
            if entry.tag == super::Tag::new(b"glyf") {
                glyf = Some(index);
            } else if entry.tag == super::Tag::new(b"loca") {
                loca = Some(index);
            }
            indices.push(index);
        }
        if glyf.and_then(|index| index.checked_add(1)) != loca {
            return Err(error(
                remaining,
                "collection glyf and loca must be consecutive",
            ));
        }
        faces.push(CollectionFace { flavor, indices });
    }
    Ok(CollectionDirectory {
        version,
        faces,
        size,
    })
}

fn build_collection(
    collection: CollectionDirectory,
    entries: &[super::Entry],
    mut stream: &[u8],
    total: usize,
) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"ttcf");
    out.extend(collection.version.to_be_bytes());
    out.extend(u32::try_from(collection.faces.len()).ok()?.to_be_bytes());
    let extra = if collection.version == 0x20000 { 12 } else { 0 };
    let header = 12usize.checked_add(collection.faces.len().checked_mul(4)?)?;
    out.resize(collection.size, 0);
    let mut tables = Vec::with_capacity(entries.len());
    for entry in entries {
        let (table, tail) = stream.split_at_checked(entry.stored as usize)?;
        tables.push(alloc::borrow::Cow::Borrowed(table));
        stream = tail;
    }
    let mut partners = alloc::vec![None; entries.len()];
    for face in &collection.faces {
        let index = |name| {
            face.indices.iter().copied().find(|i| {
                entries
                    .get(*i)
                    .is_some_and(|entry| entry.tag == super::Tag::new(name))
            })
        };
        let transformed =
            |name| index(name).filter(|i| entries.get(*i).is_some_and(|e| e.transformed));
        match (transformed(b"glyf"), transformed(b"loca")) {
            (Some(g), Some(l)) if g.checked_add(1) == Some(l) => {
                if partners.get(g)?.is_none() {
                    let original = entries.get(g)?.original as usize;
                    let (glyf, loca) = super::glyf::restore(
                        tables.get(g)?,
                        entries.get(l)?.original as usize,
                        original.checked_next_multiple_of(4)?,
                    )?;
                    *tables.get_mut(g)? = alloc::borrow::Cow::Owned(glyf);
                    *tables.get_mut(l)? = alloc::borrow::Cow::Owned(loca);
                    *partners.get_mut(g)? = Some(l);
                }
            }
            (None, None) => {}
            _ => return None,
        }
        if let Some(h) = transformed(b"hmtx")
            && partners.get(h)?.is_none()
        {
            let context = face
                .indices
                .iter()
                .map(|i| Some((entries.get(*i)?.tag, tables.get(*i)?.as_ref())))
                .collect::<Option<Vec<_>>>()?;
            let restored =
                restore_hmtx(&context, tables.get(h)?, entries.get(h)?.original as usize)?;
            *tables.get_mut(h)? = alloc::borrow::Cow::Owned(restored);
            *partners.get_mut(h)? = Some(h);
        }
    }
    let mut records = Vec::with_capacity(entries.len());
    for (entry, data) in entries.iter().zip(&tables) {
        if entry.transformed && matches!(data, alloc::borrow::Cow::Borrowed(_)) {
            return None;
        }
        let table = data.as_ref();
        let offset = out.len();
        out.extend(table);
        if entry.tag == super::Tag::new(b"head") && table.len() >= 12 {
            out.get_mut(offset.checked_add(8)?..offset.checked_add(12)?)?
                .fill(0);
        }
        out.resize(out.len().checked_next_multiple_of(4)?, 0);
        let checksum = out
            .get(offset..)?
            .as_chunks::<4>()
            .0
            .iter()
            .fold(0u32, |sum, chunk| {
                sum.wrapping_add(u32::from_be_bytes(*chunk))
            });
        let mut record = Vec::with_capacity(16);
        record.extend(entry.tag.to_u32().to_be_bytes());
        record.extend(checksum.to_be_bytes());
        record.extend(u32::try_from(offset).ok()?.to_be_bytes());
        record.extend(u32::try_from(table.len()).ok()?.to_be_bytes());
        records.push(record);
    }
    let mut position = header.checked_add(extra)?;
    for (slot, face) in collection.faces.into_iter().enumerate() {
        let pointer = 12usize.checked_add(slot.checked_mul(4)?)?;
        out.get_mut(pointer..)?
            .first_chunk_mut::<4>()?
            .copy_from_slice(&u32::try_from(position).ok()?.to_be_bytes());
        let mut directory = Vec::new();
        directory.extend(face.flavor.to_be_bytes());
        let count = u16::try_from(face.indices.len()).ok()?;
        let selector = u16::try_from(count.checked_ilog2()?).ok()?;
        let search = 16u16.checked_shl(u32::from(selector))?;
        directory.extend(count.to_be_bytes());
        directory.extend(search.to_be_bytes());
        directory.extend(selector.to_be_bytes());
        directory.extend(count.checked_mul(16)?.checked_sub(search)?.to_be_bytes());
        let mut selected: Vec<_> = face
            .indices
            .iter()
            .map(|index| records.get(*index))
            .collect::<Option<_>>()?;
        selected.sort_unstable();
        let mut previous = None;
        for record in selected {
            let tag = record.first_chunk::<4>()?;
            if previous == Some(tag) {
                return None;
            }
            previous = Some(tag);
            directory.extend(record);
        }
        let end = position.checked_add(directory.len())?;
        out.get_mut(position..end)?.copy_from_slice(&directory);
        position = end;
    }
    (out.len() == total && stream.is_empty()).then_some(out)
}

fn restore_hmtx(tables: &[(super::Tag, &[u8])], stream: &[u8], length: usize) -> Option<Vec<u8>> {
    let table = |name| {
        tables
            .iter()
            .find(|(tag, _)| *tag == super::Tag::new(name))
            .map(|(_, data)| *data)
    };
    let field = |data: &[u8], offset| Some(u16::from_be_bytes(*data.get(offset..)?.first_chunk()?));
    let glyphs = usize::from(field(table(b"maxp")?, 4)?);
    let metrics = usize::from(field(table(b"hhea")?, 34)?);
    if !(1..=glyphs).contains(&metrics) || glyphs.checked_add(metrics)?.checked_mul(2)? != length {
        return None;
    }
    let (&flags, bytes) = stream.split_first()?;
    if !(1..=3).contains(&flags) {
        return None;
    }
    let (widths, mut bearings) = bytes.split_at_checked(metrics * 2)?;
    let format = i16::from_be_bytes(field(table(b"head")?, 50)?.to_be_bytes());
    let loca = sofel_read::loca::Loca::new(table(b"loca")?, format).ok()?;
    let glyf = table(b"glyf")?;
    let mut output = Vec::with_capacity(length);
    for index in 0..glyphs {
        let flag = if index < metrics {
            output.extend_from_slice(widths.get(index.checked_mul(2)?..)?.first_chunk::<2>()?);
            1
        } else {
            2
        };
        if flags & flag == 0 {
            let (bearing, tail) = bearings.split_first_chunk::<2>()?;
            output.extend_from_slice(bearing);
            bearings = tail;
        } else {
            let glyph = glyf
                .get(loca.offset(index)? as usize..loca.offset(index.checked_add(1)?)? as usize)?;
            let xmin = if glyph.is_empty() {
                0
            } else {
                field(glyph, 2)?
            };
            output.extend_from_slice(&xmin.to_be_bytes());
        }
    }
    bearings.is_empty().then_some(output)
}

/// Reconstructs an sfnt or TTC from a WOFF2 font or collection.
///
/// # Errors
/// Reports invalid input, unsupported transformations or compression, and output limit violations.
pub fn decode_woff2(input: &[u8], limit: usize) -> Result<Vec<u8>, DecodeError> {
    let error = |offset, reason| DecodeError { offset, reason };
    if input.get(..4) != Some(b"wOF2") {
        return Err(error(0, "invalid WOFF2 signature"));
    }
    let flavor = word(input, 4)?;
    if !matches!(flavor, 0x10000 | 0x4f54544f | 0x74746366) {
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
    let mut remaining = input
        .get(48..)
        .ok_or(error(input.len(), "truncated WOFF2 header"))?;
    let mut entries = Vec::with_capacity(count);
    let mut sfnt_size = if flavor == 0x74746366 {
        0
    } else {
        12usize.saturating_add(count << 4)
    };
    let mut stored_size = 0usize;
    for _ in 0..count {
        let at = input.len().saturating_sub(remaining.len());
        let entry = read_entry(&mut remaining).ok_or(error(at, "invalid WOFF2 table entry"))?;
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
        if stored_size > limit {
            return Err(error(at, "transformed stream exceeds limit"));
        }
        entries.push(entry);
    }
    let collection = if flavor == 0x74746366 {
        let directory = collection_directory(&mut remaining, input.len(), &entries, limit)?;
        sfnt_size = sfnt_size
            .checked_add(directory.size)
            .ok_or(error(16, "collection size overflow"))?;
        Some(directory)
    } else {
        None
    };
    if sfnt_size != total {
        return Err(error(16, "sfnt size mismatch"));
    }
    let at = input.len().saturating_sub(remaining.len());
    let stream = remaining
        .get(..compressed)
        .ok_or(error(input.len(), "truncated Brotli data"))?;
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
    let decoded = sofel_brotli::decompress(stream, stored_size)
        .ok_or(error(at, "unsupported or malformed Brotli stream"))?;
    if let Some(collection) = collection {
        return build_collection(collection, &entries, &decoded, total)
            .ok_or(error(at, "invalid collection tables"));
    }
    let mut data = decoded.as_slice();
    let mut tables = Vec::with_capacity(count);
    let mut transformed = None;
    let mut outline = None;
    let mut offsets = None;
    for entry in entries {
        let (table, tail) = data
            .split_at_checked(entry.stored as usize)
            .ok_or(error(at, "truncated table data"))?;
        if entry.transformed {
            match &entry.tag.to_u32().to_be_bytes() {
                b"glyf" => outline = Some((tables.len(), table, entry.original as usize)),
                b"loca" => offsets = Some((tables.len(), entry.original as usize)),
                _ => transformed = Some((tables.len(), table, entry.original as usize)),
            }
        }
        tables.push((entry.tag, table));
        data = tail;
    }
    let rebuilt;
    match (outline, offsets) {
        (Some((g, stream, original)), Some((l, len))) => {
            rebuilt = super::glyf::restore(
                stream,
                len,
                limit
                    .checked_sub(total.saturating_sub(original.next_multiple_of(4)))
                    .ok_or(error(at, "output limit exceeded"))?,
            )
            .ok_or(error(at, "invalid glyf transform"))?;
            tables.get_mut(g).ok_or(error(at, "missing glyf"))?.1 = &rebuilt.0;
            tables.get_mut(l).ok_or(error(at, "missing loca"))?.1 = &rebuilt.1;
        }
        (None, None) => {}
        _ => return Err(error(at, "inconsistent glyf/loca transforms")),
    }
    let restored;
    if let Some((index, stream, length)) = transformed {
        restored =
            restore_hmtx(&tables, stream, length).ok_or(error(at, "invalid hmtx transform"))?;
        tables.get_mut(index).ok_or(error(at, "missing hmtx"))?.1 = &restored;
    }
    let sfnt = sofel_write::build_sfnt(&tables).map_err(|_| error(at, "invalid sfnt directory"))?;
    if word(&sfnt, 0)? != flavor {
        return Err(error(4, "flavor does not match outline tables"));
    }
    Ok(sfnt)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn adr0065_i2_transformed_stream_limit_before_brotli() {
        let mut input = [0; 51];
        input[..4].copy_from_slice(b"wOF2");
        input[4..8].copy_from_slice(&0x10000_u32.to_be_bytes());
        input[8..12].copy_from_slice(&51u32.to_be_bytes());
        input[12..14].copy_from_slice(&1u16.to_be_bytes());
        input[16..20].copy_from_slice(&32u32.to_be_bytes());
        input[48..].copy_from_slice(&[0x43, 4, 33]);
        let rejected = decode_woff2(&input, 32).unwrap_err();
        assert_eq!(
            (rejected.offset, rejected.reason),
            (48, "transformed stream exceeds limit")
        );
        // At the limit, the directory is accepted and decoding reaches the empty stream.
        assert_eq!(decode_woff2(&input, 33).unwrap_err().offset, 51);
    }

    #[test]
    fn i1_hmtx_restores_omitted_bearings_and_empty_glyph() {
        let mut hhea = [0; 36];
        hhea[35] = 1;
        let tables: Vec<(super::super::Tag, &[u8])> = [
            (b"head", &[0; 54][..]),
            (b"maxp", &[0, 1, 0, 0, 0, 2]),
            (b"hhea", &hhea),
            (b"loca", &[0, 0, 0, 5, 0, 5]),
            (b"glyf", &[0, 1, 255, 249, 0, 0, 0, 0, 0, 0]),
        ]
        .into_iter()
        .map(|(tag, data)| (super::super::Tag::new(tag), data))
        .collect();
        // Put hmtx first so reconstruction cannot depend on directory order.
        let mut payload = Vec::from([3, 1, 244]);
        let mut directory = Vec::from([0x43, 6, 3]);
        for ((_, data), flags) in tables.iter().zip([1, 4, 2, 0xcb, 0xca]) {
            directory.extend_from_slice(&[flags, u8::try_from(data.len()).unwrap()]);
            payload.extend_from_slice(data);
        }
        let compressed = crate::compress::brotli(&payload);
        let mut input = [0; 48].to_vec();
        input[..4].copy_from_slice(b"wOF2");
        input[4..8].copy_from_slice(&0x10000_u32.to_be_bytes());
        input[12..14].copy_from_slice(&6_u16.to_be_bytes());
        let total = 12 + 6 * 16 + 8 + 56 + 8 + 36 + 8 + 12;
        input[16..20].copy_from_slice(&u32::try_from(total).unwrap().to_be_bytes());
        input[20..24].copy_from_slice(&u32::try_from(compressed.len()).unwrap().to_be_bytes());
        input.extend_from_slice(&directory);
        input.extend_from_slice(&compressed);
        let size = u32::try_from(input.len()).unwrap();
        input[8..12].copy_from_slice(&size.to_be_bytes());
        let decoded = decode_woff2(&input, total).unwrap();
        let face = sofel_read::sfnt::Sfnt::new(&decoded).unwrap();
        assert_eq!(
            face.table(super::super::Tag::new(b"hmtx")),
            Some(&[1, 244, 255, 249, 0, 0][..])
        );
        for (stream, expected) in [
            (&[1, 1, 244, 0, 20][..], [1, 244, 255, 249, 0, 20]),
            (&[2, 1, 244, 0, 8], [1, 244, 0, 8, 0, 0]),
            (&[3, 1, 244], [1, 244, 255, 249, 0, 0]),
        ] {
            assert_eq!(
                restore_hmtx(&tables, stream, 6).as_deref(),
                Some(&expected[..])
            );
        }
        for stream in [&[0, 1, 244][..], &[7, 1, 244], &[3, 1], &[3, 1, 244, 0]] {
            assert!(restore_hmtx(&tables, stream, 6).is_none());
        }
    }
}

#[cfg(test)]
mod transformed_outline_tests {
    extern crate std;
    use super::*;
    use alloc::{boxed::Box, vec};

    #[test]
    fn adr0065_i1_transformed_simple_outline_and_offsets() -> Result<(), Box<dyn std::error::Error>>
    {
        // One empty glyph and one single-point contour with point (4, 5).
        let streams: [&[u8]; 7] = [&[0, 0, 0, 1], &[1], &[23], &[0x34, 0], &[], &[0; 4], &[]];
        let mut transformed = Vec::from([0, 0, 0, 0, 0, 2, 0, 0]);
        for stream in streams {
            transformed.extend_from_slice(&u32::try_from(stream.len())?.to_be_bytes());
        }
        for stream in streams {
            transformed.extend_from_slice(stream);
        }
        let mut directory = vec![10, 20, u8::try_from(transformed.len())?, 11, 6, 0];
        let compressed = crate::compress::brotli(&transformed);
        let mut input = [0; 48].to_vec();
        input.get_mut(..4).ok_or("header")?.copy_from_slice(b"wOF2");
        input
            .get_mut(4..8)
            .ok_or("header")?
            .copy_from_slice(&0x10000u32.to_be_bytes());
        input
            .get_mut(12..14)
            .ok_or("header")?
            .copy_from_slice(&2u16.to_be_bytes());
        input
            .get_mut(16..20)
            .ok_or("header")?
            .copy_from_slice(&72u32.to_be_bytes());
        input
            .get_mut(20..24)
            .ok_or("header")?
            .copy_from_slice(&u32::try_from(compressed.len())?.to_be_bytes());
        input.append(&mut directory);
        input.extend(compressed);
        let size = u32::try_from(input.len())?;
        input
            .get_mut(8..12)
            .ok_or("header")?
            .copy_from_slice(&size.to_be_bytes());
        let decoded = decode_woff2(&input, 128).map_err(|_| "outline decode")?;
        let sfnt = sofel_read::sfnt::Sfnt::new(&decoded).map_err(|_| "sfnt")?;
        let loca = sfnt.table(super::super::Tag::new(b"loca")).ok_or("loca")?;
        let glyf = sfnt.table(super::super::Tag::new(b"glyf")).ok_or("glyf")?;
        assert_eq!(loca, &[0, 0, 0, 0, 0, 10]);
        assert_eq!(
            glyf.get(..10),
            Some([0, 1, 0, 4, 0, 5, 0, 4, 0, 5].as_slice())
        );
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod collection_tests {
    use super::*;
    use sofel_read::{Tag, sfnt::Collection};

    #[test]
    fn adr0065_i2_collection_rejects_nonconsecutive_untransformed_glyf_loca() {
        let mut input = [0; 48].to_vec();
        input[..8].copy_from_slice(b"wOF2ttcf");
        input[12..14].copy_from_slice(&3u16.to_be_bytes());
        input[16..20].copy_from_slice(&88u32.to_be_bytes());
        let packed = crate::compress::brotli(&[0; 6]);
        input[20..24].copy_from_slice(&u32::try_from(packed.len()).unwrap().to_be_bytes());
        input.extend([0xca, 2, 1, 2, 0xcb, 2]);
        input.extend([0, 1, 0, 0, 1, 3, 0, 1, 0, 0, 0, 1, 2]);
        input.extend(packed);
        let length = u32::try_from(input.len()).unwrap();
        input[8..12].copy_from_slice(&length.to_be_bytes());
        assert!(decode_woff2(&input, 88).is_err());
    }

    #[test]
    fn adr0065_i1_collection_restores_each_faces_transformed_metrics() {
        let contours: [&[u8]; 7] = [&[0, 0, 0, 1], &[1], &[23], &[0x34, 0], &[], &[0; 4], &[]];
        let mut glyphs = Vec::from([0, 0, 0, 0, 0, 2, 0, 0]);
        for bytes in contours {
            glyphs.extend(u32::try_from(bytes.len()).unwrap().to_be_bytes());
        }
        for bytes in contours {
            glyphs.extend(bytes);
        }
        let mut payload = glyphs.clone();
        payload.extend([0; 54]);
        payload.extend([0, 1, 0, 0, 0, 2]);
        let mut hhea = [0; 36];
        hhea[35] = 1;
        payload.extend(hhea);
        payload.extend([3, 1, 244, 3, 2, 88]);
        let packed = crate::compress::brotli(&payload);
        let mut input = [0; 48].to_vec();
        input[..8].copy_from_slice(b"wOF2ttcf");
        input[12..14].copy_from_slice(&7u16.to_be_bytes());
        let total = 20 + 2 * 108 + 20 + 8 + 56 + 8 + 36 + 2 * 8;
        input[16..20].copy_from_slice(&u32::try_from(total).unwrap().to_be_bytes());
        input[20..24].copy_from_slice(&u32::try_from(packed.len()).unwrap().to_be_bytes());
        input.extend([
            10,
            20,
            u8::try_from(glyphs.len()).unwrap(),
            11,
            6,
            0,
            1,
            54,
            4,
            6,
            2,
            36,
            0x43,
            6,
            3,
            0x43,
            6,
            3,
        ]);
        input.extend([0, 1, 0, 0, 2]);
        for index in [5, 6] {
            input.extend([6, 0, 1, 0, 0, 0, 1, 2, 3, 4, index]);
        }
        input.extend(packed);
        let len = u32::try_from(input.len()).unwrap();
        input[8..12].copy_from_slice(&len.to_be_bytes());
        let decoded = decode_woff2(&input, total).unwrap();
        let collection = Collection::new(&decoded).unwrap();
        for (index, width) in [(0, [1, 244]), (1, [2, 88])] {
            let font = collection.font(index).unwrap();
            assert_eq!(
                font.table(Tag::new(b"hmtx")).unwrap(),
                &[width[0], width[1], 0, 0, 0, 4]
            );
            assert_eq!(font.table(Tag::new(b"loca")).unwrap(), &[0, 0, 0, 0, 0, 10]);
        }
    }

    #[test]
    fn adr0065_i1_collection_preserves_shared_and_face_specific_tables() {
        let mut input = [0; 48].to_vec();
        input[..8].copy_from_slice(b"wOF2ttcf");
        input[12..14].copy_from_slice(&3u16.to_be_bytes());
        // 20-byte TTC header, two 44-byte directories, three 4-byte padded tables.
        input[16..20].copy_from_slice(&120u32.to_be_bytes());
        let stream = crate::compress::brotli(&[42, 1, 2]);
        input[20..24].copy_from_slice(&u32::try_from(stream.len()).unwrap().to_be_bytes());
        input.extend_from_slice(b"\x3fTest\x01\x3fFace\x01\x3fFace\x01");
        input.extend_from_slice(&[0, 1, 0, 0, 2]);
        for index in [1, 2] {
            input.extend_from_slice(&[2, 0, 1, 0, 0, index, 0]);
        }
        input.extend(stream);
        let length = u32::try_from(input.len()).unwrap();
        input[8..12].copy_from_slice(&length.to_be_bytes());
        let decoded = decode_woff2(&input, 120).unwrap();
        let collection = Collection::new(&decoded).unwrap();
        assert_eq!(collection.num_fonts(), 2);
        for (index, value) in [(0, 1), (1, 2)] {
            let font = collection.font(index).unwrap();
            assert_eq!(font.table(Tag::new(b"Test")).unwrap(), &[42]);
            assert_eq!(font.table(Tag::new(b"Face")).unwrap(), &[value]);
        }
        assert!(decode_woff2(&input, 119).is_err());
    }
}
