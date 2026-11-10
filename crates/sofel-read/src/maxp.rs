//! The `maxp` and `MAXP` (maximum profile) tables.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/maxp>

use crate::be::{self, to_u32};
use crate::{CountWidth, Diagnostic, Kind, ReadError, Tag};

/// The `maxp` tag.
pub const TAG: Tag = Tag::new(b"maxp");
/// The `MAXP` tag, the variant with 24-bit counts.
pub const TAG_WIDE: Tag = Tag::new(b"MAXP");

const V05: u32 = 0x0000_5000;
const V10: u32 = 0x0001_0000;
const TRUETYPE_LEN: usize = 26;

/// The TrueType maxima of version 1.0.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct TrueTypeMaxima {
    /// `maxPoints`.
    pub max_points: u16,
    /// `maxContours`.
    pub max_contours: u16,
    /// `maxCompositePoints`.
    pub max_composite_points: u16,
    /// `maxCompositeContours`.
    pub max_composite_contours: u16,
    /// `maxZones`.
    pub max_zones: u16,
    /// `maxTwilightPoints`.
    pub max_twilight_points: u16,
    /// `maxStorage`.
    pub max_storage: u16,
    /// `maxFunctionDefs`.
    pub max_function_defs: u16,
    /// `maxInstructionDefs`.
    pub max_instruction_defs: u16,
    /// `maxStackElements`.
    pub max_stack_elements: u16,
    /// `maxSizeOfInstructions`.
    pub max_size_of_instructions: u16,
    /// `maxComponentElements`.
    pub max_component_elements: u16,
    /// `maxComponentDepth`.
    pub max_component_depth: u16,
}

/// The `maxp` or `MAXP` (maximum profile) table, read eagerly.
#[derive(Clone, Copy, Debug)]
pub struct Maxp {
    version: u32,
    num_glyphs: u32,
    truetype: Option<TrueTypeMaxima>,
}

fn needed(version: u32, width: CountWidth) -> usize {
    let base = 4usize.saturating_add(width.bytes());
    if version == V10 {
        base.saturating_add(TRUETYPE_LEN)
    } else {
        base
    }
}

impl Maxp {
    /// `width` is `U16` for `maxp` and `U24` for `MAXP`.
    ///
    /// Fails with [`ReadError::UnknownVersion`] if the version is neither
    /// 0.5 nor 1.0, and with [`ReadError::Truncated`] if the fields of that
    /// version do not fit.
    pub fn new(data: &[u8], width: CountWidth) -> Result<Maxp, ReadError> {
        let version = be::u32_at(data, 0).ok_or(ReadError::truncated(4, data.len()))?;
        if version != V05 && version != V10 {
            return Err(ReadError::UnknownVersion(version));
        }
        let need = needed(version, width);
        if data.len() < need {
            return Err(ReadError::truncated(need, data.len()));
        }
        let num_glyphs = match width {
            CountWidth::U16 => be::u16_at(data, 4).map(u32::from),
            CountWidth::U24 => be::u24_at(data, 4),
        }
        .ok_or(ReadError::truncated(need, data.len()))?;
        let truetype = if version == V10 {
            let t = be::array_at::<TRUETYPE_LEN>(data, 4usize.saturating_add(width.bytes()))
                .ok_or(ReadError::truncated(need, data.len()))?;
            let f = |i: usize| -> u16 {
                let pair = t.as_chunks::<2>().0.get(i).copied().unwrap_or_default();
                u16::from_be_bytes(pair)
            };
            Some(TrueTypeMaxima {
                max_points: f(0),
                max_contours: f(1),
                max_composite_points: f(2),
                max_composite_contours: f(3),
                max_zones: f(4),
                max_twilight_points: f(5),
                max_storage: f(6),
                max_function_defs: f(7),
                max_instruction_defs: f(8),
                max_stack_elements: f(9),
                max_size_of_instructions: f(10),
                max_component_elements: f(11),
                max_component_depth: f(12),
            })
        } else {
            None
        };
        Ok(Maxp {
            version,
            num_glyphs,
            truetype,
        })
    }

    /// `version`: 0x00005000 (0.5) or 0x00010000 (1.0).
    pub fn version(&self) -> u32 {
        self.version
    }

    /// `numGlyphs` (16 bits in `maxp`, 24 bits in `MAXP`).
    pub fn num_glyphs(&self) -> u32 {
        self.num_glyphs
    }

    /// The TrueType maxima, present in version 1.0 only.
    pub fn truetype(&self) -> Option<TrueTypeMaxima> {
        self.truetype
    }
}

/// Checks a `maxp` (`U16`) or `MAXP` (`U24`) table and reports an
/// unknown version or truncation to `report`.
pub fn validate(data: &[u8], width: CountWidth, report: &mut dyn FnMut(Diagnostic)) {
    let tag = match width {
        CountWidth::U16 => TAG,
        CountWidth::U24 => TAG_WIDE,
    };
    let mut emit = |offset: u32, kind: Kind| report(Diagnostic::new(tag, offset, kind));
    match Maxp::new(data, width) {
        Ok(_) => {}
        Err(ReadError::UnknownVersion(found)) => emit(0, Kind::UnknownVersion { found }),
        Err(_) => {
            let version = be::u32_at(data, 0).unwrap_or(V05);
            emit(
                0,
                Kind::Truncated {
                    needed: to_u32(needed(version, width).max(4)),
                    available: to_u32(data.len()),
                },
            );
        }
    }
}
