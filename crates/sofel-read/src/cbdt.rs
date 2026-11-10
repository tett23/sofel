//! The bitmap data tables `CBDT`, `EBDT` and Apple's `bdat`, which share
//! one layout (ADR 0023): a version and the glyph images that the
//! location tables ([`crate::cblc`]) point to.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/ebdt>,
//! <https://learn.microsoft.com/typography/opentype/spec/cbdt>

use crate::be::{self, to_u32, to_usize};
use crate::{Diagnostic, Kind, ReadError, Tag};

/// The `CBDT` tag.
pub const TAG_CBDT: Tag = Tag::new(b"CBDT");
/// The `EBDT` tag.
pub const TAG_EBDT: Tag = Tag::new(b"EBDT");
/// Apple's `bdat` tag.
pub const TAG_BDAT: Tag = Tag::new(b"bdat");

/// `SmallGlyphMetrics` or `BigGlyphMetrics` (small metrics have no
/// vertical values; they read as 0).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct GlyphMetrics {
    /// `height`.
    pub height: u8,
    /// `width`.
    pub width: u8,
    /// `horiBearingX` (`bearingX` of small metrics).
    pub hori_bearing_x: i8,
    /// `horiBearingY` (`bearingY` of small metrics).
    pub hori_bearing_y: i8,
    /// `horiAdvance` (`advance` of small metrics).
    pub hori_advance: u8,
    /// `vertBearingX`.
    pub vert_bearing_x: i8,
    /// `vertBearingY`.
    pub vert_bearing_y: i8,
    /// `vertAdvance`.
    pub vert_advance: u8,
}

impl GlyphMetrics {
    /// Small metrics from the first 5 bytes of `b`.
    pub fn small(b: &[u8]) -> Option<GlyphMetrics> {
        let m = b.first_chunk::<5>()?;
        Some(GlyphMetrics {
            height: m[0],
            width: m[1],
            hori_bearing_x: i8::from_be_bytes([m[2]]),
            hori_bearing_y: i8::from_be_bytes([m[3]]),
            hori_advance: m[4],
            ..GlyphMetrics::default()
        })
    }
    /// Big metrics from the first 8 bytes of `b`.
    pub fn big(b: &[u8]) -> Option<GlyphMetrics> {
        let m = b.first_chunk::<8>()?;
        Some(GlyphMetrics {
            vert_bearing_x: i8::from_be_bytes([m[5]]),
            vert_bearing_y: i8::from_be_bytes([m[6]]),
            vert_advance: m[7],
            ..GlyphMetrics::small(b)?
        })
    }
}

/// A view over a `CBDT`, `EBDT` or `bdat` table.
#[derive(Clone, Copy, Debug)]
pub struct Cbdt<'a> {
    data: &'a [u8],
    major: u16,
    minor: u16,
}

impl<'a> Cbdt<'a> {
    /// A view over `data`; fails with [`ReadError::Truncated`] without
    /// the 4-byte version.
    pub fn new(data: &'a [u8]) -> Result<Cbdt<'a>, ReadError> {
        let h = data
            .first_chunk::<4>()
            .ok_or(ReadError::truncated(4, data.len()))?;
        Ok(Cbdt {
            data,
            major: u16::from_be_bytes([h[0], h[1]]),
            minor: u16::from_be_bytes([h[2], h[3]]),
        })
    }
    /// The table's bytes.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `majorVersion`.
    pub fn major_version(&self) -> u16 {
        self.major
    }
    /// `minorVersion`.
    pub fn minor_version(&self) -> u16 {
        self.minor
    }
    /// The `length` bytes at `offset`; `None` when they do not fit.
    pub fn image(&self, offset: u32, length: u32) -> Option<&'a [u8]> {
        be::slice_at(self.data, to_usize(offset), to_usize(length))
    }
}

/// Checks a `CBDT`, `EBDT` or `bdat` table (`tag`): truncation and a
/// major version other than 2 or 3. The images are checked with the
/// location table, which says where they are.
pub fn validate(tag: Tag, data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    let mut emit = |kind: Kind| report(Diagnostic::new(tag, 0, kind));
    match Cbdt::new(data) {
        Err(_) => emit(Kind::Truncated {
            needed: 4,
            available: to_u32(data.len()),
        }),
        Ok(c) if !matches!(c.major_version(), 2 | 3) => emit(Kind::UnknownVersion {
            found: be::u32_at(data, 0).unwrap_or(0),
        }),
        Ok(_) => {}
    }
}

/// Checks the images that the location table `cblc` points to in the
/// data table `data` (`tag`): images out of bounds, unknown image
/// formats, and images too short for their metrics or, for the PNG
/// formats 17 to 19, for their data length. Offsets are those of the
/// images in `data`.
pub fn validate_images(
    tag: Tag,
    data: &[u8],
    cblc: &crate::cblc::Cblc<'_>,
    report: &mut dyn FnMut(Diagnostic),
) {
    let mut emit = |offset: u32, kind: Kind| report(Diagnostic::new(tag, offset, kind));
    let Ok(cbdt) = Cbdt::new(data) else { return };
    for i in 0..cblc.num_sizes() {
        let Some(size) = cblc.size(i) else { continue };
        for j in 0..size.number_of_index_subtables {
            let Some(r) = cblc.index_record(&size, j) else {
                break;
            };
            for g in u32::from(r.first_glyph)..=u32::from(r.last_glyph) {
                let Some(loc) = cblc.locate(&r, g) else {
                    continue;
                };
                let Some(image) = cbdt.image(loc.offset, loc.length) else {
                    emit(
                        loc.offset,
                        Kind::OffsetOutOfBounds {
                            offset: loc.offset,
                            length: loc.length,
                            available: to_u32(data.len()),
                        },
                    );
                    continue;
                };
                let header = match loc.image_format {
                    1 | 2 | 8 | 17 => 5,
                    6 | 7 | 9 | 18 => 8,
                    5 | 19 => 0,
                    f => {
                        emit(
                            loc.offset,
                            Kind::UnknownFormat {
                                found: u32::from(f),
                            },
                        );
                        continue;
                    }
                };
                let png = matches!(loc.image_format, 17..=19);
                let needed = if png {
                    let len = be::u32_at(image, header).unwrap_or(u32::MAX);
                    to_u32(header.saturating_add(4)).saturating_add(len)
                } else {
                    to_u32(header)
                };
                if to_u32(image.len()) < needed {
                    emit(
                        loc.offset,
                        Kind::Truncated {
                            needed,
                            available: to_u32(image.len()),
                        },
                    );
                }
            }
        }
    }
}
