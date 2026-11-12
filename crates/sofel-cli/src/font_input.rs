//! Decode file wrappers before sfnt face selection (ADR 0065).
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
    if gzip {
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
            if gzip {
                diagnostic(0, "invalid WOFF in decompressed gzip data")
            } else {
                diagnostic(e.offset, e.reason)
            }
        });
    }
    Ok(data)
}
