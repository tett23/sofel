//! The `OS/2` (OS/2 and Windows metrics) table, versions 0 to 5 (ADR 0016).
//!
//! Version 0 has 78 bytes, version 1 86, versions 2 to 4 96 and version 5
//! 100. Versions above 5 are read up to the version 5 fields. Fields of
//! later versions read as `None`.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/os2>

use crate::be::{self, to_u32};
use crate::{Diagnostic, Kind, ReadError, Tag};

/// The `OS/2` tag.
pub const TAG: Tag = Tag::new(b"OS/2");

/// `fsSelection` bit 0: the font is italic.
pub const ITALIC: u16 = 1 << 0;
/// `fsSelection` bit 5: the font is bold.
pub const BOLD: u16 = 1 << 5;
/// `fsSelection` bit 6: the font is regular.
pub const REGULAR: u16 = 1 << 6;
/// `fsSelection` bit 7: use `sTypoAscender`, `sTypoDescender` and
/// `sTypoLineGap` for line metrics.
pub const USE_TYPO_METRICS: u16 = 1 << 7;
/// `fsSelection` bit 9: the font is oblique.
pub const OBLIQUE: u16 = 1 << 9;

const V0: usize = 78;
const V1: usize = 86;
const V2: usize = 96;
const V5: usize = 100;

/// The length of the fields of `version`.
pub fn version_length(version: u16) -> usize {
    match version {
        0 => V0,
        1 => V1,
        2..=4 => V2,
        _ => V5,
    }
}

/// A view over the `OS/2` table.
#[derive(Clone, Copy, Debug)]
pub struct Os2<'a> {
    h: &'a [u8; V0],
    v1: Option<&'a [u8; 8]>,
    v2: Option<&'a [u8; 10]>,
    v5: Option<&'a [u8; 4]>,
}

fn u16_of(a: &[u8], i: usize) -> u16 {
    be::u16_at(a, i).unwrap_or(0)
}

fn i16_of(a: &[u8], i: usize) -> i16 {
    be::i16_at(a, i).unwrap_or(0)
}

fn u32_of(a: &[u8], i: usize) -> u32 {
    be::u32_at(a, i).unwrap_or(0)
}

impl<'a> Os2<'a> {
    /// A view over the fields of the table's version.
    ///
    /// Fails with [`ReadError::Truncated`] if `data` is shorter than the
    /// fields of its version (78, 86, 96 or 100 bytes).
    pub fn new(data: &'a [u8]) -> Result<Os2<'a>, ReadError> {
        let h = data
            .first_chunk::<V0>()
            .ok_or(ReadError::truncated(V0, data.len()))?;
        let version = u16::from_be_bytes([h[0], h[1]]);
        let need = version_length(version);
        if data.len() < need {
            return Err(ReadError::truncated(need, data.len()));
        }
        let v1 = (version >= 1)
            .then(|| be::array_at::<8>(data, V0))
            .flatten();
        let v2 = (version >= 2)
            .then(|| be::array_at::<10>(data, V1))
            .flatten();
        let v5 = (version >= 5)
            .then(|| be::array_at::<4>(data, V2))
            .flatten();
        Ok(Os2 { h, v1, v2, v5 })
    }

    /// `version`.
    pub fn version(&self) -> u16 {
        u16_of(self.h, 0)
    }
    /// `xAvgCharWidth`, in font units.
    pub fn x_avg_char_width(&self) -> i16 {
        i16_of(self.h, 2)
    }
    /// `usWeightClass`.
    pub fn us_weight_class(&self) -> u16 {
        u16_of(self.h, 4)
    }
    /// `usWidthClass`.
    pub fn us_width_class(&self) -> u16 {
        u16_of(self.h, 6)
    }
    /// `fsType`: the embedding permissions, as raw bits.
    pub fn fs_type(&self) -> u16 {
        u16_of(self.h, 8)
    }
    /// `ySubscriptXSize`, in font units.
    pub fn y_subscript_x_size(&self) -> i16 {
        i16_of(self.h, 10)
    }
    /// `ySubscriptYSize`, in font units.
    pub fn y_subscript_y_size(&self) -> i16 {
        i16_of(self.h, 12)
    }
    /// `ySubscriptXOffset`, in font units.
    pub fn y_subscript_x_offset(&self) -> i16 {
        i16_of(self.h, 14)
    }
    /// `ySubscriptYOffset`, in font units.
    pub fn y_subscript_y_offset(&self) -> i16 {
        i16_of(self.h, 16)
    }
    /// `ySuperscriptXSize`, in font units.
    pub fn y_superscript_x_size(&self) -> i16 {
        i16_of(self.h, 18)
    }
    /// `ySuperscriptYSize`, in font units.
    pub fn y_superscript_y_size(&self) -> i16 {
        i16_of(self.h, 20)
    }
    /// `ySuperscriptXOffset`, in font units.
    pub fn y_superscript_x_offset(&self) -> i16 {
        i16_of(self.h, 22)
    }
    /// `ySuperscriptYOffset`, in font units.
    pub fn y_superscript_y_offset(&self) -> i16 {
        i16_of(self.h, 24)
    }
    /// `yStrikeoutSize`, in font units.
    pub fn y_strikeout_size(&self) -> i16 {
        i16_of(self.h, 26)
    }
    /// `yStrikeoutPosition`, in font units.
    pub fn y_strikeout_position(&self) -> i16 {
        i16_of(self.h, 28)
    }
    /// `sFamilyClass`.
    pub fn s_family_class(&self) -> i16 {
        i16_of(self.h, 30)
    }
    /// `panose`, the 10 PANOSE classification bytes.
    pub fn panose(&self) -> [u8; 10] {
        be::array_at::<10>(self.h, 32).copied().unwrap_or_default()
    }
    /// `ulUnicodeRange1` to `ulUnicodeRange4`.
    pub fn ul_unicode_range(&self) -> [u32; 4] {
        [
            u32_of(self.h, 42),
            u32_of(self.h, 46),
            u32_of(self.h, 50),
            u32_of(self.h, 54),
        ]
    }
    /// `achVendID`.
    pub fn ach_vend_id(&self) -> Tag {
        Tag(be::array_at::<4>(self.h, 58).copied().unwrap_or_default())
    }
    /// `fsSelection`, as raw bits.
    pub fn fs_selection(&self) -> u16 {
        u16_of(self.h, 62)
    }
    /// `usFirstCharIndex`.
    pub fn us_first_char_index(&self) -> u16 {
        u16_of(self.h, 64)
    }
    /// `usLastCharIndex`.
    pub fn us_last_char_index(&self) -> u16 {
        u16_of(self.h, 66)
    }
    /// `sTypoAscender`, in font units.
    pub fn s_typo_ascender(&self) -> i16 {
        i16_of(self.h, 68)
    }
    /// `sTypoDescender`, in font units.
    pub fn s_typo_descender(&self) -> i16 {
        i16_of(self.h, 70)
    }
    /// `sTypoLineGap`, in font units.
    pub fn s_typo_line_gap(&self) -> i16 {
        i16_of(self.h, 72)
    }
    /// `usWinAscent`, in font units.
    pub fn us_win_ascent(&self) -> u16 {
        u16_of(self.h, 74)
    }
    /// `usWinDescent`, in font units.
    pub fn us_win_descent(&self) -> u16 {
        u16_of(self.h, 76)
    }
    /// `ulCodePageRange1` and `ulCodePageRange2` (version 1 and later).
    pub fn ul_code_page_range(&self) -> Option<[u32; 2]> {
        self.v1.map(|t| [u32_of(t, 0), u32_of(t, 4)])
    }
    /// `sxHeight`, in font units (version 2 and later).
    pub fn sx_height(&self) -> Option<i16> {
        self.v2.map(|t| i16_of(t, 0))
    }
    /// `sCapHeight`, in font units (version 2 and later).
    pub fn s_cap_height(&self) -> Option<i16> {
        self.v2.map(|t| i16_of(t, 2))
    }
    /// `usDefaultChar` (version 2 and later).
    pub fn us_default_char(&self) -> Option<u16> {
        self.v2.map(|t| u16_of(t, 4))
    }
    /// `usBreakChar` (version 2 and later).
    pub fn us_break_char(&self) -> Option<u16> {
        self.v2.map(|t| u16_of(t, 6))
    }
    /// `usMaxContext` (version 2 and later).
    pub fn us_max_context(&self) -> Option<u16> {
        self.v2.map(|t| u16_of(t, 8))
    }
    /// `usLowerOpticalPointSize`, in TWIPs (version 5 and later).
    pub fn us_lower_optical_point_size(&self) -> Option<u16> {
        self.v5.map(|t| u16_of(t, 0))
    }
    /// `usUpperOpticalPointSize`, in TWIPs (version 5 and later).
    pub fn us_upper_optical_point_size(&self) -> Option<u16> {
        self.v5.map(|t| u16_of(t, 2))
    }
}

/// Checks an `OS/2` table: data shorter than the fields of its version
/// (and nothing else then), data longer than them (versions 0 to 5),
/// `usWeightClass` outside 1..=1000, REGULAR set in `fsSelection`
/// together with ITALIC or BOLD, and `usFirstCharIndex` above
/// `usLastCharIndex`.
pub fn validate(data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    let mut emit = |offset: u32, kind: Kind| report(Diagnostic::new(TAG, offset, kind));
    let os2 = match Os2::new(data) {
        Ok(o) => o,
        Err(_) => {
            let need = be::u16_at(data, 0).map_or(V0, version_length);
            return emit(
                0,
                Kind::Truncated {
                    needed: to_u32(need),
                    available: to_u32(data.len()),
                },
            );
        }
    };
    let need = version_length(os2.version());
    if os2.version() <= 5 && data.len() > need {
        emit(
            0,
            Kind::LengthMismatch {
                declared: to_u32(need),
                available: to_u32(data.len()),
            },
        );
    }
    if !(1..=1000).contains(&os2.us_weight_class()) {
        emit(
            4,
            Kind::WeightClassOutOfRange {
                found: u32::from(os2.us_weight_class()),
            },
        );
    }
    let sel = os2.fs_selection();
    if sel & REGULAR != 0 && sel & (ITALIC | BOLD) != 0 {
        emit(
            62,
            Kind::FsSelectionConflict {
                found: u32::from(sel),
            },
        );
    }
    if os2.us_first_char_index() > os2.us_last_char_index() {
        emit(
            64,
            Kind::FirstCharAfterLast {
                first: u32::from(os2.us_first_char_index()),
                last: u32::from(os2.us_last_char_index()),
            },
        );
    }
}
