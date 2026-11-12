use super::*;

pub(super) fn encode(font: Sfnt<'_>, records: &[TableRecord]) -> Result<Vec<u8>> {
    let mut output = header(
        b"wOFF",
        font.version(),
        font.num_tables(),
        size(font.file().len())?,
    );
    output.resize(
        44usize.saturating_add(20 * usize::from(font.num_tables())),
        0,
    );
    let mut directory = Vec::with_capacity(records.len());
    for &r in records {
        let original = table(r, font.file())?;
        let compressed = crate::compress::zlib(original);
        let stored = if compressed.len() < original.len() {
            &compressed
        } else {
            original
        };
        let offset = size(output.len())?;
        directory.push((r.tag, offset, size(stored.len())?, r.length, r.checksum));
        output.extend(stored);
        output.resize(aligned(output.len())?, 0);
    }
    directory.sort_by_key(|r| r.0);
    for (record, (tag, offset, stored, length, sum)) in output
        .get_mut(44..)
        .ok_or_else(invalid)?
        .as_chunks_mut::<20>()
        .0
        .iter_mut()
        .zip(directory)
    {
        for (dst, value) in record.as_chunks_mut::<4>().0.iter_mut().zip([
            tag.to_u32(),
            offset,
            stored,
            length,
            sum,
        ]) {
            dst.copy_from_slice(&value.to_be_bytes());
        }
    }
    finish(output)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use alloc::vec;
    use sofel_write::build_sfnt;
    #[test]
    fn i2_store_only_strictly_shorter_compressed_tables() {
        let mut seen = [false; 3];
        for len in 4..64usize {
            let original = vec![42; len];
            let compressed = crate::compress::zlib(&original);
            let index = match compressed
                .len()
                .checked_add(1)
                .and_then(|n| n.checked_sub(len))
            {
                Some(i @ 0..=2) => i,
                _ => continue,
            };
            let sfnt = build_sfnt(&[
                (Tag::new(b"Xfoo"), &original),
                (Tag::new(b"head"), &[0; 54]),
            ])
            .unwrap();
            let result = super::super::encode(&sfnt, Format::Woff).unwrap();
            let offset =
                u32::from_be_bytes(result.get(48..52).unwrap().try_into().unwrap()) as usize;
            let stored =
                u32::from_be_bytes(result.get(52..56).unwrap().try_into().unwrap()) as usize;
            let expected = if index == 0 { &compressed } else { &original };
            assert_eq!(
                result.get(offset..offset.saturating_add(stored)).unwrap(),
                expected
            );
            *seen.get_mut(index).unwrap() = true;
        }
        assert_eq!(seen, [true; 3]);
    }
}
