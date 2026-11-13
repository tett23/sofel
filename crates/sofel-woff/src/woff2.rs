use super::*;
mod glyf;
mod read;
pub use read::decode_woff2;

// WOFF2 specification, section 4.1, Known Table Tags.
const TAGS: &[u8] = b"cmapheadhheahmtxmaxpnameOS/2postcvt fpgmglyflocaprepCFF VORGEBDTEBLCgasphdmxkernLTSHPCLTVDMXvheavmtxBASEGDEFGPOSGSUBEBSCJSTFMATHCBDTCBLCCOLRCPALSVG sbixacntavarbdatblocbslncvarfdscfeatfmtxfvargvarhstyjustlcarmortmorxopbdproptrakZapfSilfGlatGlocFeatSill";

fn read_base128(input: &mut &[u8]) -> Option<u32> {
    if input.first() == Some(&0x80) {
        return None;
    }
    let mut value = 0u32;
    for _ in 0..5 {
        let (byte, rest) = input.split_first()?;
        *input = rest;
        value = value.checked_mul(128)? | u32::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

struct Entry {
    tag: Tag,
    original: u32,
    stored: u32,
    transformed: bool,
}

fn read_entry(input: &mut &[u8]) -> Option<Entry> {
    let (&flags, rest) = input.split_first()?;
    *input = rest;
    let index = usize::from(flags & 63);
    let tag = if index == 63 {
        let (tag, rest) = input.split_first_chunk::<4>()?;
        *input = rest;
        tag
    } else {
        TAGS.as_chunks::<4>().0.get(index)?
    };
    let version = flags >> 6;
    let transformed = match (tag, version) {
        (b"glyf" | b"loca", 0) | (b"hmtx", 1) => true,
        (b"glyf" | b"loca", 3) => false,
        (b"glyf" | b"loca", _) => return None,
        (_, 0) => false,
        _ => return None,
    };
    let original = read_base128(input)?;
    let stored = if transformed {
        read_base128(input)?
    } else {
        original
    };
    if transformed && tag == b"loca" && stored != 0 {
        return None;
    }
    Some(Entry {
        tag: Tag::new(tag),
        original,
        stored,
        transformed,
    })
}

fn base128(out: &mut Vec<u8>, n: u32) {
    for shift in [28, 21, 14, 7, 0] {
        if shift == 0 || n >> shift != 0 {
            let byte = u8::try_from((n >> shift) & 127).unwrap_or_default();
            out.push(if shift == 0 { byte } else { byte | 128 });
        }
    }
}

pub(super) fn encode(font: Sfnt<'_>, mut records: Vec<TableRecord>) -> Result<Vec<u8>> {
    records.retain(|r| r.tag != Tag::new(b"DSIG"));
    records.sort_by_key(|r| r.tag);
    let count = u16::try_from(records.len()).map_err(|_| invalid())?;
    let mut total = 12usize.saturating_add(16 * usize::from(count));
    let mut directory = Vec::new();
    let mut stream = Vec::new();
    for r in records {
        let tag = r.tag.to_u32().to_be_bytes();
        let index = TAGS
            .as_chunks::<4>()
            .0
            .iter()
            .position(|t| *t == tag)
            .and_then(|i| u8::try_from(i).ok())
            .unwrap_or(63);
        directory.push(if matches!(&tag, b"glyf" | b"loca") {
            192 | index
        } else {
            index
        });
        if index == 63 {
            directory.extend(tag);
        }
        base128(&mut directory, r.length);
        let data = table(r, font.file())?;
        total = total
            .checked_add(aligned(data.len())?)
            .ok_or_else(invalid)?;
        let start = stream.len();
        stream.extend(data);
        if &tag == b"head" {
            let head = stream.get_mut(start..).ok_or_else(invalid)?;
            *head.get_mut(16).ok_or_else(invalid)? |= 8;
            head.get_mut(8..12).ok_or_else(invalid)?.fill(0);
        }
    }
    let compressed = crate::compress::brotli(&stream);
    let mut out = header(b"wOF2", font.version(), count, size(total)?);
    out.extend(size(compressed.len())?.to_be_bytes());
    out.resize(48, 0);
    out.extend(directory);
    out.extend(compressed);
    out.resize(aligned(out.len())?, 0);
    finish(out)
}

#[cfg(test)]
mod tests {
    use super::{Tag, read_base128};

    #[test]
    fn adr0065_i2_directory_distinguishes_null_and_transformed_lengths() {
        use super::read_entry;
        for (record, tag, original, stream, transformed) in [
            (&[10, 64, 48][..], *b"glyf", 64, 48, true),
            (&[11, 8, 0], *b"loca", 8, 0, true),
            (&[67, 16, 6], *b"hmtx", 16, 6, true),
            (&[203, 8], *b"loca", 8, 8, false),
            (&[63, 84, 101, 115, 116, 9], *b"Test", 9, 9, false),
        ] {
            let mut input = record;
            let values =
                read_entry(&mut input).map(|e| (e.tag, e.original, e.stored, e.transformed));
            assert_eq!(
                values,
                Some((Tag::new(&tag), original, stream, transformed))
            );
            assert!(input.is_empty());
        }
        for bytes in [
            &[11, 8, 1][..],
            &[63, 0],
            &[67, 16],
            &[10, 0x80, 0, 0],
            &[195, 1, 1],
        ] {
            assert!(read_entry(&mut &bytes[..]).is_none());
        }
    }

    #[test]
    fn adr0065_i2_woff2_lengths_require_canonical_uint32_values() {
        for (encoded, expected) in [
            (&[0][..], Some(0)),
            (&[127][..], Some(127)),
            (&[0x81, 0][..], Some(128)),
            (&[0x8f, 0xff, 0xff, 0xff, 0x7f][..], Some(u32::MAX)),
            (&[0x90, 0x80, 0x80, 0x80, 0][..], None),
            (&[0x80, 0][..], None),
            (&[0x81][..], None),
            (&[0x81, 0x81, 0x81, 0x81, 0x81, 0][..], None),
        ] {
            let mut remaining = encoded;
            assert_eq!(read_base128(&mut remaining), expected);
            if expected.is_some() {
                assert!(remaining.is_empty());
            }
        }
    }
}
