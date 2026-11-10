//! The `CPAL` (color palette) table, versions 0 and 1 (ADR 0023).
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/cpal>

use crate::be::{self, to_u32, to_usize};
use crate::{Diagnostic, Kind, ReadError, Tag};

/// The `CPAL` tag.
pub const TAG: Tag = Tag::new(b"CPAL");

/// Palette flag: usable on a light background.
pub const USABLE_WITH_LIGHT_BACKGROUND: u32 = 0x1;
/// Palette flag: usable on a dark background.
pub const USABLE_WITH_DARK_BACKGROUND: u32 = 0x2;
/// The name id of a palette or color without a name.
pub const NO_NAME: u16 = 0xFFFF;

/// A view over the `CPAL` table. Building it checks the header, the
/// palette indices, the color records and, for version 1, the arrays
/// the non-zero offsets point to (HarfBuzz's `CPAL::sanitize`).
#[derive(Clone, Copy, Debug)]
pub struct Cpal<'a> {
    version: u16,
    num_colors: u16,
    num_palettes: u16,
    indices: &'a [[u8; 2]],
    records: &'a [[u8; 4]],
    flags: Option<&'a [[u8; 4]]>,
    labels: Option<&'a [[u8; 2]]>,
    color_labels: Option<&'a [[u8; 2]]>,
}

fn header_len(data: &[u8]) -> usize {
    let palettes = be::u16_at(data, 4).map_or(0, usize::from);
    12usize.saturating_add(palettes.saturating_mul(2))
}

impl<'a> Cpal<'a> {
    /// A view over `data`.
    ///
    /// Fails with [`ReadError::Truncated`] when the header, the palette
    /// indices or the color records do not fit, and with
    /// [`ReadError::OutOfBounds`] when a version 1 array does not.
    pub fn new(data: &'a [u8]) -> Result<Cpal<'a>, ReadError> {
        let truncated = ReadError::truncated(header_len(data), data.len());
        let h = data.first_chunk::<12>().ok_or(truncated)?;
        let version = u16::from_be_bytes([h[0], h[1]]);
        let num_colors = u16::from_be_bytes([h[2], h[3]]);
        let num_palettes = u16::from_be_bytes([h[4], h[5]]);
        let num_records = u16::from_be_bytes([h[6], h[7]]);
        let records_at = to_usize(u32::from_be_bytes([h[8], h[9], h[10], h[11]]));
        let indices = be::chunks_at::<2>(data, 12, usize::from(num_palettes)).ok_or(truncated)?;
        let records = be::chunks_at::<4>(data, records_at, usize::from(num_records))
            .ok_or(ReadError::OutOfBounds)?;
        let (mut flags, mut labels, mut color_labels) = (None, None, None);
        if version != 0 {
            let tail = 12usize.saturating_add(usize::from(num_palettes).saturating_mul(2));
            let off = |i: usize| {
                be::u32_at(data, tail.saturating_add(i.saturating_mul(4)))
                    .map(to_usize)
                    .ok_or(truncated)
            };
            let (f, l, c) = (off(0)?, off(1)?, off(2)?);
            let palettes = usize::from(num_palettes);
            if f != 0 {
                flags = Some(be::chunks_at::<4>(data, f, palettes).ok_or(ReadError::OutOfBounds)?);
            }
            if l != 0 {
                labels = Some(be::chunks_at::<2>(data, l, palettes).ok_or(ReadError::OutOfBounds)?);
            }
            if c != 0 {
                color_labels = Some(
                    be::chunks_at::<2>(data, c, usize::from(num_colors))
                        .ok_or(ReadError::OutOfBounds)?,
                );
            }
        }
        Ok(Cpal {
            version,
            num_colors,
            num_palettes,
            indices,
            records,
            flags,
            labels,
            color_labels,
        })
    }

    /// The table version.
    pub fn version(&self) -> u16 {
        self.version
    }
    /// `numPaletteEntries`: the number of colors in each palette.
    pub fn num_palette_entries(&self) -> u16 {
        self.num_colors
    }
    /// `numPalettes`.
    pub fn num_palettes(&self) -> u16 {
        self.num_palettes
    }
    /// `numColorRecords`.
    pub fn num_color_records(&self) -> usize {
        self.records.len()
    }
    /// `colorRecordIndices[palette]`: the palette's first color record.
    pub fn first_color_record(&self, palette: u16) -> Option<u16> {
        self.indices
            .get(usize::from(palette))
            .map(|b| u16::from_be_bytes(*b))
    }
    /// Color record `i` as stored: blue, green, red, alpha.
    pub fn color_record(&self, i: usize) -> Option<[u8; 4]> {
        self.records.get(i).copied()
    }
    /// Color `entry` of `palette` (blue, green, red, alpha), as HarfBuzz
    /// reads it: `None` past the palettes, past `numPaletteEntries` or
    /// past the color records.
    pub fn color(&self, palette: u16, entry: u16) -> Option<[u8; 4]> {
        if entry >= self.num_colors {
            return None;
        }
        let first = self.first_color_record(palette)?;
        self.color_record(usize::from(first).saturating_add(usize::from(entry)))
    }
    /// The number of colors `palette` has in the color records: at most
    /// `numPaletteEntries`, fewer when the records end early (HarfBuzz's
    /// `sub_array`); 0 past the palettes.
    pub fn palette_len(&self, palette: u16) -> u16 {
        self.first_color_record(palette).map_or(0, |first| {
            let left = self.records.len().saturating_sub(usize::from(first));
            u16::try_from(left).unwrap_or(u16::MAX).min(self.num_colors)
        })
    }
    /// The flags of `palette` (version 1): 0 without the array or past
    /// the palettes.
    pub fn palette_flags(&self, palette: u16) -> u32 {
        self.flags
            .and_then(|f| f.get(usize::from(palette)))
            .map_or(0, |b| u32::from_be_bytes(*b))
    }
    /// The name id of `palette` (version 1): [`NO_NAME`] without the
    /// array or past the palettes.
    pub fn palette_name_id(&self, palette: u16) -> u16 {
        self.labels
            .and_then(|l| l.get(usize::from(palette)))
            .map_or(NO_NAME, |b| u16::from_be_bytes(*b))
    }
    /// The name id of palette entry `entry` (version 1): [`NO_NAME`]
    /// without the array or past `numPaletteEntries`.
    pub fn color_name_id(&self, entry: u16) -> u16 {
        self.color_labels
            .and_then(|l| l.get(usize::from(entry)))
            .map_or(NO_NAME, |b| u16::from_be_bytes(*b))
    }
}

/// Checks a `CPAL` table: truncation and arrays out of bounds (and
/// nothing else then), and palettes whose colors run past the color
/// records ([`Kind::ColorIndexOutOfRange`], at the palette index).
pub fn validate(data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    let mut emit = |offset: usize, kind: Kind| report(Diagnostic::new(TAG, to_u32(offset), kind));
    let cpal = match Cpal::new(data) {
        Ok(c) => c,
        Err(ReadError::Truncated { needed, available }) => {
            return emit(
                0,
                Kind::Truncated {
                    needed: to_u32(needed),
                    available: to_u32(available),
                },
            );
        }
        Err(_) => {
            return emit(
                0,
                Kind::OffsetOutOfBounds {
                    offset: 0,
                    length: to_u32(data.len()),
                    available: to_u32(data.len()),
                },
            );
        }
    };
    let count = to_u32(cpal.num_color_records());
    for p in 0..cpal.num_palettes() {
        let Some(first) = cpal.first_color_record(p) else {
            continue;
        };
        let end = u32::from(first).saturating_add(u32::from(cpal.num_palette_entries()));
        if end > count {
            emit(
                12usize.saturating_add(usize::from(p).saturating_mul(2)),
                Kind::ColorIndexOutOfRange {
                    index: end.saturating_sub(1),
                    count,
                },
            );
        }
    }
}
