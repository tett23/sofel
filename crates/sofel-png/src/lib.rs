//! PNG decoding for embedded bitmap strikes (ADR 0066).
#![no_std]
extern crate alloc;
use alloc::vec::Vec;
mod pixels;
use pixels::Header;

/// A decoded PNG, in top-down premultiplied BGRA order.
#[derive(Debug)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Four bytes per pixel, with no row padding.
    pub bgra: Vec<u8>,
}
/// PNG validation or resource error.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Invalid PNG structure or pixel data.
    Invalid,
    /// A PNG chunk or zlib checksum failed.
    Checksum,
    /// Dimensions exceed the caller's bound or address space.
    Limit,
}
type Result<T> = core::result::Result<T, Error>;

/// Decodes a complete static PNG, bounding output by `max_pixels`.
///
/// # Errors
/// Returns an error for corrupt data, unsupported critical chunks or excess output.
pub fn decode(input: &[u8], max_pixels: usize) -> Result<Image> {
    let mut image = decode_rgba(input, max_pixels)?;
    for pixel in image.rgba.as_chunks_mut::<4>().0 {
        let [r, g, b, a] = *pixel;
        // FreeType pngshim.c rounds multiplication by alpha/255.
        let premul = |c: u8| {
            let t = u32::from(c)
                .saturating_mul(u32::from(a))
                .saturating_add(128);
            u8::try_from(t.saturating_add(t >> 8) >> 8).unwrap_or_default()
        };
        *pixel = [premul(b), premul(g), premul(r), a];
    }
    Ok(Image {
        width: image.width,
        height: image.height,
        bgra: image.rgba,
    })
}

/// A decoded PNG with unassociated color channels, suitable for a PDF soft mask.
pub struct RgbaImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Top-down RGBA samples without row padding.
    pub rgba: Vec<u8>,
}

/// Decodes a static PNG while preserving straight RGB values even at low alpha.
///
/// # Errors
/// Returns an error for invalid PNG data or dimensions exceeding the given limit.
pub fn decode_rgba(input: &[u8], max_pixels: usize) -> Result<RgbaImage> {
    let mut rest = input
        .strip_prefix(b"\x89PNG\r\n\x1a\n")
        .ok_or(Error::Invalid)?;
    let mut header = None;
    let mut palette = None;
    let mut transparency = None;
    let mut compressed = Vec::new();
    let mut started = false;
    let mut ended = false;
    loop {
        let (size, tail) = rest.split_first_chunk::<4>().ok_or(Error::Invalid)?;
        let size = u32::from_be_bytes(*size);
        if size > i32::MAX as u32 {
            return Err(Error::Invalid);
        }
        let (chunk, tail) = tail
            .split_at_checked((size as usize).checked_add(4).ok_or(Error::Limit)?)
            .ok_or(Error::Invalid)?;
        let (tag, data) = chunk.split_first_chunk::<4>().ok_or(Error::Invalid)?;
        let (crc, tail) = tail.split_first_chunk::<4>().ok_or(Error::Invalid)?;
        if sofel_inflate::crc32(chunk) != u32::from_be_bytes(*crc) {
            return Err(Error::Checksum);
        }
        if !tag.iter().all(u8::is_ascii_alphabetic) || !tag[2].is_ascii_uppercase() {
            return Err(Error::Invalid);
        }
        rest = tail;
        if header.is_none() && tag != b"IHDR" {
            return Err(Error::Invalid);
        }
        if started && tag != b"IDAT" {
            ended = true;
        }
        match tag {
            b"IHDR" if header.is_none() => header = Some(Header::parse(data, max_pixels)?),
            b"PLTE" if !started && palette.is_none() && transparency.is_none() => {
                let h = header.as_ref().ok_or(Error::Invalid)?;
                if matches!(h.color, 0 | 4)
                    || data.is_empty()
                    || data.len() > 768
                    || !data.len().is_multiple_of(3)
                    || (h.color == 3 && data.len() / 3 > 1usize << h.depth)
                {
                    return Err(Error::Invalid);
                }
                palette = Some(data);
            }
            b"tRNS" if !started && transparency.is_none() => {
                match header.as_ref().ok_or(Error::Invalid)?.color {
                    0 if data.len() == 2 => {}
                    2 if data.len() == 6 => {}
                    3 if palette.is_some_and(|p| data.len() <= p.len() / 3) => {}
                    _ => return Err(Error::Invalid),
                }
                transparency = Some(data);
            }
            b"IDAT" if !ended => {
                started = true;
                compressed.extend(data);
            }
            b"IEND" if started && data.is_empty() && rest.is_empty() => break,
            b"tRNS" | b"IHDR" | b"IDAT" | b"IEND" | b"PLTE" => return Err(Error::Invalid),
            _ if tag[0].is_ascii_lowercase() => {}
            _ => return Err(Error::Invalid),
        }
    }
    let header = header.ok_or(Error::Invalid)?;
    if header.color == 3 && palette.is_none() {
        return Err(Error::Invalid);
    }
    let passes = header.passes()?;
    let length = passes
        .iter()
        .try_fold(0usize, |n, p| n.checked_add(p.bytes()?))
        .ok_or(Error::Limit)?;
    let raw = sofel_inflate::zlib(&compressed, length).map_err(|e| match e {
        sofel_inflate::Error::Checksum => Error::Checksum,
        sofel_inflate::Error::Limit => Error::Limit,
        sofel_inflate::Error::Invalid => Error::Invalid,
    })?;
    if raw.len() != length {
        return Err(Error::Invalid);
    }
    let mut out = alloc::vec![0;header.output_bytes];
    let mut rest = raw.as_slice();
    for pass in passes {
        let (data, tail) = rest
            .split_at_checked(pass.bytes().ok_or(Error::Limit)?)
            .ok_or(Error::Invalid)?;
        pass.draw(
            &header,
            data,
            palette.unwrap_or(&[]),
            transparency.unwrap_or(&[]),
            &mut out,
        )?;
        rest = tail;
    }
    Ok(RgbaImage {
        width: header.width,
        height: header.height,
        rgba: out,
    })
}
