//! Decode file wrappers before sfnt face selection (ADR 0065).
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
    if lzw {
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
            if gzip || lzw {
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
