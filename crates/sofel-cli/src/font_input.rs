//! Decode file wrappers before sfnt face selection (ADR 0065).
mod bzip2;
mod lzw;
use sofel_read::{Diagnostic, Kind, validate};

// FreeType exposes at most a signed 31-bit stream for gzip input
// (ftgzip.c). Callers of the individual decoders can set a smaller limit.
const MAX_FONT_BYTES: usize = 0x7fff_ffff;

fn diagnostic(offset: usize, reason: &'static str) -> Diagnostic {
    let kind = Kind::InvalidFontContainer { reason };
    Diagnostic {
        table: validate::DIRECTORY,
        offset: u32::try_from(offset).unwrap_or(u32::MAX),
        severity: kind.severity(),
        kind,
    }
}

pub(crate) fn unpack(mut data: Vec<u8>) -> Result<Vec<u8>, Diagnostic> {
    let gzip = data.starts_with(&[31, 139]);
    let lzw = data.starts_with(&[31, 157]);
    let bzip = data.starts_with(b"BZh");
    if bzip {
        data = bzip2::decode(&data, MAX_FONT_BYTES)?;
    } else if lzw {
        data = lzw::decode(&data, MAX_FONT_BYTES)?;
    } else if gzip {
        data = sofel_inflate::gzip(&data, MAX_FONT_BYTES).map_err(|e| {
            diagnostic(
                0,
                match e {
                    sofel_inflate::Error::Invalid => "malformed gzip member",
                    sofel_inflate::Error::Checksum => "gzip checksum or size mismatch",
                    sofel_inflate::Error::Limit => "gzip output exceeds the font size limit",
                },
            )
        })?;
    }
    if data.starts_with(b"wOFF") {
        return sofel_woff::decode_woff(&data, MAX_FONT_BYTES).map_err(|e| {
            if gzip || lzw || bzip {
                diagnostic(0, "invalid WOFF in decompressed font data")
            } else {
                diagnostic(e.offset, e.reason)
            }
        });
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    // bzip2 -1 encodes this 100,016-byte sequence in two blocks.
    const TWO_BLOCKS: &[u8] = &[
        66, 90, 104, 49, 49, 65, 89, 38, 83, 89, 244, 63, 36, 43, 0, 12, 52, 64, 0, 127, 255, 176,
        0, 184, 10, 96, 0, 154, 20, 192, 1, 52, 10, 85, 0, 0, 124, 37, 5, 230, 49, 40, 44, 162, 80,
        89, 196, 160, 180, 137, 65, 107, 18, 130, 218, 37, 5, 188, 74, 11, 136, 148, 23, 49, 40,
        46, 162, 80, 93, 196, 160, 188, 137, 65, 118, 37, 5, 232, 148, 23, 194, 80, 95, 152, 160,
        172, 147, 41, 172, 147, 45, 173, 118, 0, 0, 6, 160, 0, 63, 255, 208, 0, 16, 85, 34, 105,
        147, 38, 2, 152, 0, 38, 156, 73, 55, 138, 61, 85, 103, 203, 176, 253, 150, 116, 220, 114,
        58, 46, 228, 138, 112, 161, 33, 156, 74, 37, 118,
    ];

    #[test]
    fn adr0065_i1_bzip2_decodes_multiple_blocks() {
        let expected: Vec<_> = (0u8..16).cycle().take(100_016).collect();
        assert_eq!(unpack(TWO_BLOCKS.to_vec()), Ok(expected));
    }

    #[test]
    fn adr0065_i1_bzip2_randomised_and_concatenated_members() {
        // An early-style randomised block, including its first bit flip.
        let member = [
            66, 90, 104, 49, 49, 65, 89, 38, 83, 89, 188, 203, 165, 171, 128, 0, 59, 64, 0, 127, 0,
            32, 0, 80, 134, 0, 77, 84, 52, 61, 17, 155, 17, 202, 26, 212, 140, 17, 200, 142, 196,
            122, 35, 226, 238, 72, 167, 10, 18, 23, 153, 116, 181, 96,
        ];
        let expected: Vec<_> = (0u8..7).cycle().take(830).collect();
        assert_eq!(bzip2::decode(&member, 830), Ok(expected.clone()));
        assert!(bzip2::decode(&member, 829).is_err());
        assert_eq!(
            bzip2::decode(&member.repeat(2), 1660),
            Ok(expected.repeat(2))
        );
    }

    #[test]
    fn adr0065_i2_bzip2_rejects_truncation_and_checksum_damage() {
        for end in [3, 4, 12, TWO_BLOCKS.len() - 1] {
            assert!(unpack(TWO_BLOCKS.iter().take(end).copied().collect()).is_err());
        }
        let mut broken = TWO_BLOCKS.to_vec();
        if let Some(crc) = broken.get_mut(10) {
            *crc ^= 1;
        }
        assert!(unpack(broken).is_err());
    }

    #[test]
    fn i2_lzw_checks_codes_and_output_limit() {
        assert!(lzw::decode(&[31, 157, 0x90, 0x41], 7).is_err());
        assert!(lzw::decode(&[31, 157, 0x90, 0xff, 1], 7).is_err());
        assert!(lzw::decode(&[31, 157, 0x90, 0x41, 0x84, 4, 0x1c, 8], 6).is_err());
    }
    #[test]
    fn i1_unix_compress_expands_dictionary_references() {
        let encoded = vec![0x1f, 0x9d, 0x90, 0x41, 0x84, 0x04, 0x1c, 0x08];
        assert_eq!(unpack(encoded).as_deref(), Ok(b"ABABABA".as_slice()));
    }
}
