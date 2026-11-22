//! Common character and glyph access to a Type 1 program.

use alloc::vec::Vec;
use core::fmt;

use crate::afm::Afm;
use crate::charstrings::Charstrings;
use crate::encoding::Encoding;
use crate::metrics::{Metrics, parse_metrics};
use crate::multiple_masters::MultipleMasters;
use crate::pfm::Pfm;
use crate::source::Type1Source;

/// Invalid glyph dictionary or encoding in a Type 1 source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceError;

impl fmt::Display for FaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Type 1 face")
    }
}

impl core::error::Error for FaceError {}

/// One Type 1 face with its encoded character and glyph programs.
#[derive(Clone, Debug)]
pub struct Type1Face<'a> {
    encoding: Encoding<'a>,
    charstrings: Charstrings<'a>,
    multiple_masters: Option<MultipleMasters<'a>>,
    master_weights: Option<Vec<i32>>,
    attached: Option<AttachedMetrics<'a>>,
    native_stem_blues_empty: bool,
    native_blue_zones: Option<NativeBlueZones>,
}

/// The four Type 1 blue-zone arrays used by the Adobe native hinter.
#[derive(Clone, Debug)]
pub struct NativeBlueZones {
    /// BlueValues in font units.
    pub blue_values: Vec<i32>,
    /// OtherBlues in font units.
    pub other_blues: Vec<i32>,
    /// FamilyBlues in font units.
    pub family_blues: Vec<i32>,
    /// FamilyOtherBlues in font units.
    pub family_other_blues: Vec<i32>,
    /// BlueScale in 16.16 units times 1000.
    pub blue_scale: i64,
    /// BlueShift in font units.
    pub blue_shift: i64,
    /// BlueFuzz in font units.
    pub blue_fuzz: i64,
    /// LanguageGroup.
    pub language_group: i32,
}

#[derive(Clone, Debug)]
enum AttachedMetrics<'a> {
    Afm(Afm<'a>),
    Pfm(Vec<(usize, usize, i16)>),
}

impl<'a> Type1Face<'a> {
    /// Open a previously unpacked Type 1 source.
    ///
    /// Allocates the tables of the CharStrings and Subrs, the Multiple Masters axes and a copy of their default weights (ADR 0094).
    pub fn new(source: &'a Type1Source) -> Result<Self, FaceError> {
        let encoding = Encoding::parse(source.cleartext()).map_err(|_| FaceError)?;
        let charstrings = Charstrings::parse(source.private()).map_err(|_| FaceError)?;
        let multiple_masters = MultipleMasters::parse(source.cleartext()).map_err(|_| FaceError)?;
        let master_weights = multiple_masters
            .as_ref()
            .map(|mm| mm.default_weights().to_vec());
        Ok(Self {
            encoding,
            charstrings,
            multiple_masters,
            master_weights,
            attached: None,
            native_stem_blues_empty: blue_zones_empty(source.private()),
            native_blue_zones: native_blue_zones(source.private()),
        })
    }

    /// Whether the native single-stem path can ignore PostScript blue zones.
    pub fn native_stem_blues_empty(&self) -> bool {
        self.native_stem_blues_empty
    }

    /// Type 1 blue-zone arrays for the Adobe native hinter.
    pub fn native_blue_zones(&self) -> Option<&NativeBlueZones> {
        self.native_blue_zones.as_ref()
    }

    /// Multiple Masters axis metadata, when the Type 1 source has it.
    pub fn multiple_masters(&self) -> Option<&MultipleMasters<'a>> {
        self.multiple_masters.as_ref()
    }

    /// Select a Type 1 Multiple Masters design location in signed 16.16 coordinates.
    ///
    /// Allocates the new master weights (ADR 0094).
    pub fn set_design_coordinates(&mut self, coordinates: &[i32]) -> Result<(), FaceError> {
        let mm = self.multiple_masters.as_ref().ok_or(FaceError)?;
        self.master_weights = Some(mm.master_weights(coordinates).map_err(|_| FaceError)?);
        Ok(())
    }

    /// The currently selected Type 1 Multiple Masters interpolation weights.
    pub fn master_weights(&self) -> Option<&[i32]> {
        self.master_weights.as_deref()
    }

    /// Number of indexed glyphs.
    pub fn num_glyphs(&self) -> usize {
        self.charstrings.glyph_count()
    }

    /// The glyph index for an eight-bit encoded character.
    pub fn nominal_glyph(&self, code: u8) -> Option<usize> {
        self.charstrings.glyph_index(self.encoding.name(code)?)
    }

    /// Unicode mapping synthesized from PostScript glyph names.
    pub fn unicode_glyph(&self, codepoint: u32) -> Option<usize> {
        let freetype_alias = match codepoint {
            0x02C9 => Some(b"macron".as_slice()),
            0x0394 => Some(b"Delta".as_slice()),
            0x03A9 => Some(b"Omega".as_slice()),
            _ => None,
        };
        (1..self.num_glyphs()).find(|&gid| {
            self.glyph_name(gid).is_some_and(|name| {
                crate::type42::glyph_unicode(name) == Some(codepoint)
                    || freetype_alias == Some(name)
            })
        })
    }

    /// The PostScript name of a glyph index.
    pub fn glyph_name(&self, glyph: usize) -> Option<&'a [u8]> {
        self.charstrings.glyph_name(glyph)
    }

    /// The decrypted charstring of a glyph index.
    ///
    /// Allocates the decrypted charstring (ADR 0094).
    pub fn glyph_program(&self, glyph: usize) -> Option<Vec<u8>> {
        self.charstrings.glyph_program(self.glyph_name(glyph)?)
    }

    /// Decrypted local Type 1 subroutine by index.
    ///
    /// Allocates the decrypted subroutine (ADR 0094).
    pub fn subroutine_program(&self, index: usize) -> Option<Vec<u8>> {
        self.charstrings.subroutine_program(index)
    }

    /// The unscaled side bearings and advances of a glyph.
    ///
    /// Allocates the decrypted charstring (ADR 0094); the operand stack is a fixed array (ADR 0098).
    pub fn glyph_metrics(&self, glyph: usize) -> Option<Metrics> {
        parse_metrics(&self.glyph_program(glyph)?).ok()
    }

    /// Attach AFM metrics to the face, replacing any previous AFM data.
    ///
    /// Allocates the table of AFM kerning pairs (ADR 0094).
    pub fn attach_afm(&mut self, data: &'a [u8]) -> Result<(), FaceError> {
        let afm = Afm::parse(data).map_err(|_| FaceError)?;
        self.attached = Some(AttachedMetrics::Afm(afm));
        Ok(())
    }

    /// Attach PFM metrics to the face, replacing any previous metrics data.
    ///
    /// Allocates the table of PFM kerning pairs (ADR 0094).
    pub fn attach_pfm(&mut self, data: &[u8]) -> Result<(), FaceError> {
        let pfm = Pfm::parse(data).map_err(|_| FaceError)?;
        let pairs = pfm
            .pairs()
            .map(|(left, right, value)| {
                (
                    self.nominal_glyph(left).unwrap_or(0),
                    self.nominal_glyph(right).unwrap_or(0),
                    value,
                )
            })
            .collect();
        self.attached = Some(AttachedMetrics::Pfm(pairs));
        Ok(())
    }

    /// The unscaled horizontal and vertical adjustment of a glyph pair.
    pub fn kerning(&self, left: usize, right: usize) -> (i32, i32) {
        match &self.attached {
            Some(AttachedMetrics::Afm(afm)) => {
                let Some(left) = self.glyph_name(left) else {
                    return (0, 0);
                };
                let Some(right) = self.glyph_name(right) else {
                    return (0, 0);
                };
                afm.kerning(left, right)
            }
            Some(AttachedMetrics::Pfm(pairs)) => pairs
                .iter()
                .find(|(a, b, _)| *a == left && *b == right)
                .map_or((0, 0), |(_, _, x)| (i32::from(*x), 0)),
            None => (0, 0),
        }
    }
}

pub(crate) fn blue_zones_empty(data: &[u8]) -> bool {
    [
        b"/BlueValues".as_slice(),
        b"/OtherBlues",
        b"/FamilyBlues",
        b"/FamilyOtherBlues",
    ]
    .into_iter()
    .all(|name| {
        let mut rest = data;
        while let Some(at) = rest.windows(name.len()).position(|window| window == name) {
            let Some(value) = at.checked_add(name.len()).and_then(|at| rest.get(at..)) else {
                return false;
            };
            let value = value.trim_ascii_start();
            let Some(value) = value.strip_prefix(b"[") else {
                return false;
            };
            if !value.trim_ascii_start().starts_with(b"]") {
                return false;
            }
            rest = value;
        }
        true
    })
}

fn parse_blue_array(data: &[u8], name: &[u8], limit: usize) -> Option<Vec<i32>> {
    let Some(at) = data.windows(name.len()).position(|window| window == name) else {
        return Some(Vec::new());
    };
    let start = at.checked_add(name.len())?;
    let value = data.get(start..)?.trim_ascii_start();
    let value = value.strip_prefix(b"[")?;
    let end = value.iter().position(|&byte| byte == b']')?;
    let numbers = core::str::from_utf8(value.get(..end)?).ok()?;
    let values = numbers
        .split_ascii_whitespace()
        .map(str::parse::<i32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (values.len() <= limit && values.len().is_multiple_of(2)).then_some(values)
}

pub(crate) fn native_blue_zones(data: &[u8]) -> Option<NativeBlueZones> {
    Some(NativeBlueZones {
        blue_values: parse_blue_array(data, b"/BlueValues", 14)?,
        other_blues: parse_blue_array(data, b"/OtherBlues", 10)?,
        family_blues: parse_blue_array(data, b"/FamilyBlues", 14)?,
        family_other_blues: parse_blue_array(data, b"/FamilyOtherBlues", 10)?,
        blue_scale: parse_blue_scalar(data, b"/BlueScale", 2_596_864, 65_536_000)?,
        blue_shift: parse_blue_scalar(data, b"/BlueShift", 7, 1)?,
        blue_fuzz: parse_blue_scalar(data, b"/BlueFuzz", 1, 1)?,
        language_group: i32::try_from(parse_blue_scalar(data, b"/LanguageGroup", 0, 1)?).ok()?,
    })
}

fn parse_blue_scalar(data: &[u8], name: &[u8], default: i64, multiplier: i64) -> Option<i64> {
    let Some(at) = data.windows(name.len()).position(|window| window == name) else {
        return Some(default);
    };
    let start = at.checked_add(name.len())?;
    let value = data.get(start..)?.trim_ascii_start();
    let end = value
        .iter()
        .position(|byte| !byte.is_ascii_digit() && !matches!(*byte, b'.' | b'-' | b'+'))
        .unwrap_or(value.len());
    let number = core::str::from_utf8(value.get(..end)?).ok()?;
    let (negative, number) = if let Some(rest) = number.strip_prefix('-') {
        (true, rest)
    } else {
        (false, number.strip_prefix('+').unwrap_or(number))
    };
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let whole = if whole.is_empty() {
        0
    } else {
        whole.parse::<i128>().ok()?
    };
    let fractional = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i128>().ok()?
    };
    let denominator = 10_i128.checked_pow(u32::try_from(fraction.len()).ok()?)?;
    let numerator = whole.checked_mul(denominator)?.checked_add(fractional)?;
    let scaled = numerator.checked_mul(i128::from(multiplier))?;
    let rounded = scaled
        .checked_add(denominator / 2)?
        .checked_div(denominator)?;
    i64::try_from(if negative {
        rounded.checked_neg()?
    } else {
        rounded
    })
    .ok()
}

#[cfg(test)]
mod tests {
    use super::parse_blue_scalar;

    #[test]
    fn adr0064_i1_type1_blue_scale_fixed_values() {
        assert_eq!(
            parse_blue_scalar(b"/BlueScale 0.039625 def", b"/BlueScale", 0, 65_536_000),
            Some(2_596_864)
        );
        assert_eq!(
            parse_blue_scalar(b"/BlueScale 0.08 def", b"/BlueScale", 0, 65_536_000),
            Some(5_242_880)
        );
    }
}
