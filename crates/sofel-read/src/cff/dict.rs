//! DICT data: the lexer, FreeType's number conversions and the Top,
//! Font and Private DICTs as FreeType parses them.
//!
//! Ported from FreeType 2.15.0 (`src/cff/cffparse.c`, `src/cff/cfftoken.h`
//! and `cff_subfont_load`, `cff_load_private_dict`, `cff_blend_doBlend`
//! in `src/cff/cffload.c`). Copyright (C) 1996-2026 by David Turner,
//! Robert Wilhelm, and Werner Lemberg. Used under the FreeType License
//! (FTL); see THIRD_PARTY_NOTICES.md.
//!
//! FreeType keeps `FT_Long` and `FT_Fixed` values in 64 bits on the
//! targets it is compared with; so do the fields here. A field that
//! FreeType stores in a narrower C type is truncated the same way.

use super::Version;
use super::ft;
use crate::be::{self, to_usize};

/// One token of DICT data.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DictToken<'a> {
    /// An integer operand (bytes 28, 29 and 32 to 254; FreeType also
    /// reads byte 27 as the one-byte integer `27 - 139`). As FreeType's
    /// `cff_parse_integer` gives it on 64-bit targets: the five-byte form
    /// (29) is not sign-extended, and an integer cut by the end of the
    /// data is 0.
    Integer(i64),
    /// A real operand (byte 30): the nibble bytes after the 30, up to
    /// and including the byte with the terminating nibble 0xF, or to the
    /// end of the data when there is none.
    Real(&'a [u8]),
    /// An operator: the byte for one-byte operators, `0x0C00 | b` for
    /// the two-byte operators `12 b`.
    Operator(u16),
}

/// The two-byte operator `12 b`.
pub const fn escape(b: u8) -> u16 {
    0x0C00 | b as u16
}

/// Why DICT data could not be parsed, as FreeType fails.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DictErrorKind {
    /// More operands than the stack holds (FreeType's `Stack_Overflow`).
    StackOverflow,
    /// An operator without the operands it needs.
    StackUnderflow {
        /// The operator.
        operator: u16,
    },
    /// A two-byte operator cut by the end of the data.
    TruncatedOperator,
    /// An operand value FreeType rejects: a negative Private DICT size or
    /// offset, a `MultipleMaster` design count outside 2 to 16, a `blend`
    /// count larger than the stack, or `vsindex` after `blend`.
    InvalidValue {
        /// The operator.
        operator: u16,
    },
    /// `blend` without a usable blend vector: `vsindex` outside the
    /// Variation Store, a region index outside it, coordinates that do
    /// not match its axes, or no Variation Store.
    Blend,
}

/// A DICT parse error and where it is, from the start of the DICT data.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DictError {
    /// The offset of the token that failed.
    pub offset: usize,
    /// What failed.
    pub kind: DictErrorKind,
}

/// The tokens of DICT data, in order, with their offsets, as FreeType's
/// `cff_parser_run` splits them. A two-byte operator cut by the end of
/// the data is an error and ends the tokens; so does an unterminated
/// real number, after it is returned.
#[derive(Clone, Debug)]
pub struct DictTokens<'a> {
    data: &'a [u8],
    pos: usize,
    done: bool,
}

impl<'a> DictTokens<'a> {
    /// The tokens of `data`.
    pub fn new(data: &'a [u8]) -> DictTokens<'a> {
        DictTokens {
            data,
            pos: 0,
            done: false,
        }
    }
}

/// `cff_parse_integer` at `p`: the integer whose first byte is at `p`.
fn integer_at(data: &[u8], p: usize) -> i64 {
    let v = i64::from(be::u8_at(data, p).unwrap_or(0));
    let next = p.saturating_add(1);
    let byte = |k: usize| be::u8_at(data, next.saturating_add(k)).map(i64::from);
    match v {
        28 => be::i16_at(data, next).map_or(0, i64::from),
        29 => be::u32_at(data, next).map_or(0, i64::from),
        _ if v < 247 => v.wrapping_sub(139),
        _ if v < 251 => byte(0).map_or(0, |b| {
            v.wrapping_sub(247)
                .wrapping_mul(256)
                .wrapping_add(b)
                .wrapping_add(108)
        }),
        _ => byte(0).map_or(0, |b| {
            v.wrapping_sub(251)
                .wrapping_mul(256)
                .wrapping_neg()
                .wrapping_sub(b)
                .wrapping_sub(108)
        }),
    }
}

impl<'a> Iterator for DictTokens<'a> {
    type Item = Result<(usize, DictToken<'a>), DictError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.pos >= self.data.len() {
            return None;
        }
        let at = self.pos;
        let v = be::u8_at(self.data, at)?;
        if v >= 27 && v != 31 && v != 255 {
            let token = match v {
                30 => {
                    let start = at.saturating_add(1);
                    let rest = self.data.get(start..).unwrap_or_default();
                    match rest.iter().position(|&b| b >> 4 == 15 || b & 0xF == 15) {
                        Some(k) => {
                            self.pos = start.saturating_add(k).saturating_add(1);
                            DictToken::Real(rest.get(..=k).unwrap_or_default())
                        }
                        None => {
                            self.done = true;
                            DictToken::Real(rest)
                        }
                    }
                }
                _ => {
                    let len = match v {
                        28 => 3,
                        29 => 5,
                        _ if v > 246 => 2,
                        _ => 1,
                    };
                    self.pos = at.saturating_add(len);
                    DictToken::Integer(integer_at(self.data, at))
                }
            };
            return Some(Ok((at, token)));
        }
        let op = if v == 12 {
            match be::u8_at(self.data, at.saturating_add(1)) {
                Some(b) => {
                    self.pos = at.saturating_add(2);
                    escape(b)
                }
                None => {
                    self.done = true;
                    return Some(Err(DictError {
                        offset: at,
                        kind: DictErrorKind::TruncatedOperator,
                    }));
                }
            }
        } else {
            self.pos = at.saturating_add(1);
            u16::from(v)
        };
        Some(Ok((at, DictToken::Operator(op))))
    }
}

/// An operand on FreeType's DICT stack.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Operand<'a> {
    /// An integer token.
    Integer(i64),
    /// A real token: its nibble bytes.
    Real(&'a [u8]),
    /// A value computed by a CFF2 `blend`, which FreeType keeps as a
    /// 16.16 number behind the reserved byte 255.
    Blended(i32),
}

const POWER_TENS: [i64; 10] = [
    1,
    10,
    100,
    1000,
    10000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    1_000_000_000,
];

fn power_ten(k: i64) -> i64 {
    usize::try_from(k)
        .ok()
        .and_then(|k| POWER_TENS.get(k))
        .copied()
        .unwrap_or(1)
}

/// C's truncating division; 0 for a zero divisor.
fn div(a: i64, b: i64) -> i64 {
    a.checked_div(b).unwrap_or(0)
}

fn power_ten_limit(k: i64) -> i64 {
    div(i64::MAX, power_ten(k))
}

/// The nibbles of a real number, one at a time, as FreeType reads them.
struct Nibbles<'a> {
    bytes: &'a [u8],
    at: usize,
    high: bool,
}

impl Nibbles<'_> {
    fn next(&mut self) -> Option<u8> {
        let b = *self.bytes.get(self.at)?;
        let n = if self.high {
            b >> 4
        } else {
            self.at = self.at.saturating_add(1);
            b & 0xF
        };
        self.high = !self.high;
        Some(n)
    }
}

/// FreeType's `cff_parse_real`: the 16.16 value of the real number whose
/// nibbles are `nibbles`, times `10^power_ten`. With `scaling`, returns
/// the value with as many significant digits as fit and the power of
/// ten to divide it by (`cff_parse_fixed_dynamic`). Overflows give
/// `0x7FFFFFFF`, underflows and cut data 0.
pub fn parse_real(nibbles: &[u8], power_ten_in: i64, scaling: bool) -> (i64, i64) {
    real(nibbles, power_ten_in, scaling).unwrap_or((0, 0))
}

fn real(nibbles: &[u8], power: i64, want_scaling: bool) -> Option<(i64, i64)> {
    let mut n = Nibbles {
        bytes: nibbles,
        at: 0,
        high: true,
    };
    let mut sign = false;
    let mut number: i64 = 0;
    let mut exponent: i64 = 0;
    let mut exponent_add: i64 = 0;
    let mut integer_length: i64 = 0;
    let mut fraction_length: i64 = 0;
    let mut nib;
    loop {
        nib = n.next()?;
        if nib == 0xE {
            sign = true;
        } else if nib > 9 {
            break;
        } else if number >= 0xCCC_CCCC {
            exponent_add = exponent_add.saturating_add(1);
        } else if nib != 0 || number != 0 {
            integer_length = integer_length.saturating_add(1);
            number = number.saturating_mul(10).saturating_add(i64::from(nib));
        }
    }
    if nib == 0xA {
        loop {
            nib = n.next()?;
            if nib >= 10 {
                break;
            }
            if nib == 0 && number == 0 {
                exponent_add = exponent_add.saturating_sub(1);
            } else if number < 0xCCC_CCCC && fraction_length < 9 {
                fraction_length = fraction_length.saturating_add(1);
                number = number.saturating_mul(10).saturating_add(i64::from(nib));
            }
        }
    }
    let mut exponent_sign = false;
    if nib == 12 {
        exponent_sign = true;
        nib = 11;
    }
    let mut have_overflow = false;
    if nib == 11 {
        loop {
            let d = n.next()?;
            if d >= 10 {
                break;
            }
            if exponent > 1000 {
                have_overflow = true;
            } else {
                exponent = exponent.saturating_mul(10).saturating_add(i64::from(d));
            }
        }
        if exponent_sign {
            exponent = exponent.saturating_neg();
        }
    }
    let signed = |r: i64| if sign { r.wrapping_neg() } else { r };
    if number == 0 {
        return Some((0, 0));
    }
    if have_overflow {
        return Some(if exponent_sign {
            (0, 0)
        } else {
            (signed(0x7FFF_FFFF), 0)
        });
    }
    exponent = exponent.saturating_add(power).saturating_add(exponent_add);
    let (result, scaling) = if want_scaling {
        let fraction_length = fraction_length.saturating_add(integer_length);
        let mut exponent = exponent.saturating_add(integer_length);
        if fraction_length <= 5 {
            if number > 0x7FFF {
                (
                    ft::div_fix(number, 10),
                    exponent.saturating_sub(fraction_length).saturating_add(1),
                )
            } else {
                if exponent > 0 {
                    let new_fraction_length = exponent.min(5);
                    let shift = new_fraction_length.saturating_sub(fraction_length);
                    if shift > 0 {
                        exponent = exponent.saturating_sub(new_fraction_length);
                        number = number.saturating_mul(power_ten(shift));
                        if number > 0x7FFF {
                            number /= 10;
                            exponent = exponent.saturating_add(1);
                        }
                    } else {
                        exponent = exponent.saturating_sub(fraction_length);
                    }
                } else {
                    exponent = exponent.saturating_sub(fraction_length);
                }
                (
                    number.cast_unsigned().wrapping_shl(16).cast_signed(),
                    exponent,
                )
            }
        } else if div(number, power_ten(fraction_length.saturating_sub(5))) > 0x7FFF {
            (
                ft::div_fix(number, power_ten(fraction_length.saturating_sub(4))),
                exponent.saturating_sub(4),
            )
        } else {
            (
                ft::div_fix(number, power_ten(fraction_length.saturating_sub(5))),
                exponent.saturating_sub(5),
            )
        }
    } else {
        let integer_length = integer_length.saturating_add(exponent);
        let mut fraction_length = fraction_length.saturating_sub(exponent);
        if integer_length > 5 {
            return Some((signed(0x7FFF_FFFF), 0));
        }
        if integer_length < -5 {
            return Some((0, 0));
        }
        if integer_length < 0 {
            number = div(number, power_ten(integer_length.saturating_neg()));
            fraction_length = fraction_length.saturating_add(integer_length);
        }
        if fraction_length == 10 {
            number /= 10;
            fraction_length = fraction_length.saturating_sub(1);
        }
        if fraction_length > 0 {
            if div(number, power_ten(fraction_length)) > 0x7FFF {
                return Some((signed(0), 0));
            }
            (ft::div_fix(number, power_ten(fraction_length)), 0)
        } else {
            number = number.saturating_mul(power_ten(fraction_length.saturating_neg()));
            if number > 0x7FFF {
                return Some((signed(0x7FFF_FFFF), 0));
            }
            (number.cast_unsigned().wrapping_shl(16).cast_signed(), 0)
        }
    };
    Some((signed(result), scaling))
}

/// The bytes FreeType writes for a blended value: 255, then the 16.16
/// value. `cff_parse_integer` reads the 255 as a two-byte negative
/// integer.
fn blended_bytes(v: i32) -> [u8; 5] {
    let [a, b, c, d] = v.to_be_bytes();
    [255, a, b, c, d]
}

impl Operand<'_> {
    /// `cff_parse_integer` on the operand's bytes.
    fn integer(&self) -> i64 {
        match *self {
            Operand::Integer(v) => v,
            // A real starts with 30, a one-byte integer.
            Operand::Real(_) => 30 - 139,
            Operand::Blended(v) => integer_at(&blended_bytes(v), 0),
        }
    }

    /// FreeType's `cff_parse_num`: a real is truncated to an integer, a
    /// blended value rounded.
    pub fn num(&self) -> i64 {
        match *self {
            Operand::Real(n) => parse_real(n, 0, false).0 >> 16,
            Operand::Blended(v) => {
                let r = ((v.cast_unsigned() >> 8).wrapping_add(0x80)) >> 8;
                i64::from(u16::try_from(r & 0xFFFF).unwrap_or(0).cast_signed())
            }
            Operand::Integer(v) => v,
        }
    }

    /// FreeType's `cff_parse_fixed`: the value as 16.16.
    pub fn fixed(&self) -> i64 {
        self.fixed_scaled(0)
    }

    /// FreeType's `cff_parse_fixed_scaled`: `10^scaling` times the value,
    /// as 16.16.
    pub fn fixed_scaled(&self, scaling: i64) -> i64 {
        match *self {
            Operand::Real(n) => parse_real(n, scaling, false).0,
            Operand::Blended(v) => {
                let v = i64::from(v);
                if scaling != 0 {
                    if v.saturating_abs() > power_ten_limit(scaling) {
                        return if v > 0 { 0x7FFF_FFFF } else { -0x7FFF_FFFF };
                    }
                    return v.wrapping_mul(power_ten(scaling));
                }
                v
            }
            Operand::Integer(_) => {
                let mut v = self.integer();
                if scaling != 0 {
                    if v.saturating_abs().wrapping_shl(16) > power_ten_limit(scaling) {
                        return if v > 0 { 0x7FFF_FFFF } else { -0x7FFF_FFFF };
                    }
                    v = v.wrapping_mul(power_ten(scaling));
                }
                if v > 0x7FFF {
                    0x7FFF_FFFF
                } else if v < -0x7FFF {
                    -0x7FFF_FFFF
                } else {
                    v.cast_unsigned().wrapping_shl(16).cast_signed()
                }
            }
        }
    }

    /// FreeType's `cff_parse_fixed_dynamic`: the value as 16.16 with as
    /// many significant digits as fit, and the power of ten it is to be
    /// divided by.
    pub fn fixed_dynamic(&self) -> (i64, i64) {
        if let Operand::Real(n) = *self {
            return parse_real(n, 0, true);
        }
        let number = self.integer();
        if number > 0x7FFF {
            let integer_length = (5..10).find(|&k| number < power_ten(k)).unwrap_or(10);
            if div(number, power_ten(integer_length.saturating_sub(5))) > 0x7FFF {
                (
                    ft::div_fix(number, power_ten(integer_length.saturating_sub(4))),
                    integer_length.saturating_sub(4),
                )
            } else {
                (
                    ft::div_fix(number, power_ten(integer_length.saturating_sub(5))),
                    integer_length.saturating_sub(5),
                )
            }
        } else {
            (number.cast_unsigned().wrapping_shl(16).cast_signed(), 0)
        }
    }
}

/// Which DICT is parsed: FreeType's `CFFCODE` values select the known
/// operators.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DictKind {
    /// A CFF Top DICT, or a CFF Font DICT (FreeType parses both alike).
    Top,
    /// A CFF Private DICT.
    Private,
    /// A CFF2 Top DICT.
    Top2,
    /// A CFF2 Font DICT.
    Font2,
    /// A CFF2 Private DICT.
    Private2,
}

impl DictKind {
    /// FreeType's stack size for this DICT: 96 for CFF Top and Font
    /// DICTs, 97 for CFF Private DICTs, 513 for CFF2 Top and Font DICTs
    /// and `maxstack + 1` (514) for CFF2 Private DICTs.
    pub fn stack_size(self) -> usize {
        match self {
            DictKind::Top => 96,
            DictKind::Private => 97,
            DictKind::Top2 | DictKind::Font2 => 513,
            DictKind::Private2 => usize::try_from(MAX_STACK).unwrap_or(513).saturating_add(1),
        }
    }

    /// The version of the table this DICT is in.
    pub fn version(self) -> Version {
        match self {
            DictKind::Top | DictKind::Private => Version::Cff,
            _ => Version::Cff2,
        }
    }
}

/// How FreeType reads an operator's operands (`cff_kind_*`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    /// An integer (`cff_kind_num`).
    Num,
    /// A 16.16 number (`cff_kind_fixed`).
    Fixed,
    /// A 16.16 number times 1000 (`cff_kind_fixed_thousand`).
    Fixed1000,
    /// A SID (`cff_kind_string`).
    Sid,
    /// A boolean (`cff_kind_bool`).
    Bool,
    /// A delta array of integers with at most this many values.
    Delta(u8),
    /// A delta array of 16.16 numbers with at most this many values.
    DeltaFixed(u8),
    /// An operator with its own reader (`cff_kind_callback`).
    Callback,
    /// `blend` (`cff_kind_blend`).
    Blend,
}

/// Operators.
pub mod op {
    use super::escape;
    /// `version`.
    pub const VERSION: u16 = 0;
    /// `Notice`.
    pub const NOTICE: u16 = 1;
    /// `FullName`.
    pub const FULL_NAME: u16 = 2;
    /// `FamilyName`.
    pub const FAMILY_NAME: u16 = 3;
    /// `Weight`.
    pub const WEIGHT: u16 = 4;
    /// `FontBBox`.
    pub const FONT_BBOX: u16 = 5;
    /// `BlueValues`.
    pub const BLUE_VALUES: u16 = 6;
    /// `OtherBlues`.
    pub const OTHER_BLUES: u16 = 7;
    /// `FamilyBlues`.
    pub const FAMILY_BLUES: u16 = 8;
    /// `FamilyOtherBlues`.
    pub const FAMILY_OTHER_BLUES: u16 = 9;
    /// `StdHW`.
    pub const STD_HW: u16 = 10;
    /// `StdVW`.
    pub const STD_VW: u16 = 11;
    /// `UniqueID`.
    pub const UNIQUE_ID: u16 = 13;
    /// `XUID`.
    pub const XUID: u16 = 14;
    /// `charset`.
    pub const CHARSET: u16 = 15;
    /// `Encoding`.
    pub const ENCODING: u16 = 16;
    /// `CharStrings`.
    pub const CHAR_STRINGS: u16 = 17;
    /// `Private`.
    pub const PRIVATE: u16 = 18;
    /// `Subrs`.
    pub const SUBRS: u16 = 19;
    /// `defaultWidthX`.
    pub const DEFAULT_WIDTH_X: u16 = 20;
    /// `nominalWidthX`.
    pub const NOMINAL_WIDTH_X: u16 = 21;
    /// `vsindex` (CFF2).
    pub const VSINDEX: u16 = 22;
    /// `blend` (CFF2).
    pub const BLEND: u16 = 23;
    /// `vstore` (CFF2).
    pub const VSTORE: u16 = 24;
    /// `maxstack` (CFF2).
    pub const MAXSTACK: u16 = 25;
    /// `Copyright`.
    pub const COPYRIGHT: u16 = escape(0);
    /// `isFixedPitch`.
    pub const IS_FIXED_PITCH: u16 = escape(1);
    /// `ItalicAngle`.
    pub const ITALIC_ANGLE: u16 = escape(2);
    /// `UnderlinePosition`.
    pub const UNDERLINE_POSITION: u16 = escape(3);
    /// `UnderlineThickness`.
    pub const UNDERLINE_THICKNESS: u16 = escape(4);
    /// `PaintType`.
    pub const PAINT_TYPE: u16 = escape(5);
    /// `CharstringType`.
    pub const CHARSTRING_TYPE: u16 = escape(6);
    /// `FontMatrix`.
    pub const FONT_MATRIX: u16 = escape(7);
    /// `StrokeWidth`.
    pub const STROKE_WIDTH: u16 = escape(8);
    /// `BlueScale`.
    pub const BLUE_SCALE: u16 = escape(9);
    /// `BlueShift`.
    pub const BLUE_SHIFT: u16 = escape(10);
    /// `BlueFuzz`.
    pub const BLUE_FUZZ: u16 = escape(11);
    /// `StemSnapH`.
    pub const STEM_SNAP_H: u16 = escape(12);
    /// `StemSnapV`.
    pub const STEM_SNAP_V: u16 = escape(13);
    /// `ForceBold`.
    pub const FORCE_BOLD: u16 = escape(14);
    /// `ForceBoldThreshold`.
    pub const FORCE_BOLD_THRESHOLD: u16 = escape(15);
    /// `lenIV`.
    pub const LEN_IV: u16 = escape(16);
    /// `LanguageGroup`.
    pub const LANGUAGE_GROUP: u16 = escape(17);
    /// `ExpansionFactor`.
    pub const EXPANSION_FACTOR: u16 = escape(18);
    /// `initialRandomSeed`.
    pub const INITIAL_RANDOM_SEED: u16 = escape(19);
    /// `SyntheticBase`.
    pub const SYNTHETIC_BASE: u16 = escape(20);
    /// `PostScript`.
    pub const POSTSCRIPT: u16 = escape(21);
    /// `BaseFontName`.
    pub const BASE_FONT_NAME: u16 = escape(22);
    /// `BaseFontBlend`.
    pub const BASE_FONT_BLEND: u16 = escape(23);
    /// `MultipleMaster` (removed from the specification).
    pub const MULTIPLE_MASTER: u16 = escape(24);
    /// `ROS`.
    pub const ROS: u16 = escape(30);
    /// `CIDFontVersion`.
    pub const CID_FONT_VERSION: u16 = escape(31);
    /// `CIDFontRevision`.
    pub const CID_FONT_REVISION: u16 = escape(32);
    /// `CIDFontType`.
    pub const CID_FONT_TYPE: u16 = escape(33);
    /// `CIDCount`.
    pub const CID_COUNT: u16 = escape(34);
    /// `UIDBase`.
    pub const UID_BASE: u16 = escape(35);
    /// `FDArray`.
    pub const FD_ARRAY: u16 = escape(36);
    /// `FDSelect`.
    pub const FD_SELECT: u16 = escape(37);
    /// `FontName`.
    pub const FONT_NAME: u16 = escape(38);
}

/// The operators FreeType knows in a DICT of `kind`, and how it reads
/// them (`cfftoken.h`). `None` for operators FreeType ignores.
pub fn field(kind: DictKind, operator: u16) -> Option<Field> {
    use Field::*;
    use op::*;
    match kind {
        DictKind::Top => Some(match operator {
            VERSION | NOTICE | COPYRIGHT | FULL_NAME | FAMILY_NAME | WEIGHT | POSTSCRIPT
            | FONT_NAME => Sid,
            IS_FIXED_PITCH => Bool,
            ITALIC_ANGLE => Fixed,
            UNDERLINE_POSITION | UNDERLINE_THICKNESS | PAINT_TYPE | CHARSTRING_TYPE | UNIQUE_ID
            | STROKE_WIDTH | CHARSET | ENCODING | CHAR_STRINGS | SYNTHETIC_BASE
            | CID_FONT_VERSION | CID_FONT_REVISION | CID_FONT_TYPE | CID_COUNT | UID_BASE
            | FD_ARRAY | FD_SELECT => Num,
            FONT_MATRIX | FONT_BBOX | PRIVATE | MULTIPLE_MASTER | ROS => Callback,
            _ => return None,
        }),
        DictKind::Private | DictKind::Private2 => Some(match operator {
            BLUE_VALUES | FAMILY_BLUES => DeltaFixed(14),
            OTHER_BLUES | FAMILY_OTHER_BLUES => DeltaFixed(10),
            BLUE_SCALE => Fixed1000,
            BLUE_SHIFT | BLUE_FUZZ | STD_HW | STD_VW | LANGUAGE_GROUP | SUBRS => Num,
            STEM_SNAP_H | STEM_SNAP_V => Delta(13),
            EXPANSION_FACTOR => Fixed,
            FORCE_BOLD if kind == DictKind::Private => Bool,
            FORCE_BOLD_THRESHOLD if kind == DictKind::Private => Fixed,
            LEN_IV | INITIAL_RANDOM_SEED | DEFAULT_WIDTH_X | NOMINAL_WIDTH_X
                if kind == DictKind::Private =>
            {
                Num
            }
            VSINDEX if kind == DictKind::Private2 => Callback,
            BLEND if kind == DictKind::Private2 => Blend,
            _ => return None,
        }),
        DictKind::Top2 => Some(match operator {
            FONT_MATRIX | MAXSTACK => Callback,
            CHAR_STRINGS | FD_ARRAY | FD_SELECT | VSTORE => Num,
            _ => return None,
        }),
        DictKind::Font2 => Some(match operator {
            PRIVATE | FONT_MATRIX => Callback,
            _ => return None,
        }),
    }
}

/// FreeType's CFF2 `maxstack`: always 513, the default and the limit.
pub const MAX_STACK: u32 = 513;

/// The largest operand stack of any DICT (a CFF2 Private DICT).
const STACK_CAPACITY: usize = 514;

/// FreeType's DICT operand stack: the operands before an operator.
#[derive(Clone, Debug)]
pub struct Operands<'a> {
    items: [Operand<'a>; STACK_CAPACITY],
    len: usize,
}

impl<'a> Operands<'a> {
    fn new() -> Operands<'a> {
        Operands {
            items: [Operand::Integer(0); STACK_CAPACITY],
            len: 0,
        }
    }

    /// The operands.
    pub fn as_slice(&self) -> &[Operand<'a>] {
        self.items.get(..self.len).unwrap_or_default()
    }

    /// Operand `i`, or integer 0 when there is none.
    pub fn get(&self, i: usize) -> Operand<'a> {
        self.as_slice()
            .get(i)
            .copied()
            .unwrap_or(Operand::Integer(0))
    }

    /// The number of operands.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn set(&mut self, i: usize, v: Operand<'a>) {
        if let Some(slot) = self.items.get_mut(i) {
            *slot = v;
        }
    }

    fn truncate(&mut self, len: usize) {
        self.len = self.len.min(len);
    }
}

/// What the operator handler of [`run`] asks for after an operator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum After {
    /// Clear the operands (every operator but `blend`).
    Clear,
    /// Keep them (`blend` leaves its results).
    Keep,
}

/// Runs FreeType's `cff_parser_run` over `data` with a stack of
/// `stack_size` operands, calling `handler` for each operator with its
/// offset and the operands before it.
pub fn run<'a>(
    data: &'a [u8],
    stack_size: usize,
    handler: &mut dyn FnMut(u16, usize, &mut Operands<'a>) -> Result<After, DictError>,
) -> Result<(), DictError> {
    let mut stack = Operands::new();
    let stack_size = stack_size.min(STACK_CAPACITY);
    for token in DictTokens::new(data) {
        let (at, token) = token?;
        let overflow = DictError {
            offset: at,
            kind: DictErrorKind::StackOverflow,
        };
        match token {
            DictToken::Integer(v) => {
                if stack.len >= stack_size {
                    return Err(overflow);
                }
                stack.set(stack.len, Operand::Integer(v));
                stack.len = stack.len.saturating_add(1);
            }
            DictToken::Real(n) => {
                if stack.len >= stack_size {
                    return Err(overflow);
                }
                stack.set(stack.len, Operand::Real(n));
                stack.len = stack.len.saturating_add(1);
            }
            DictToken::Operator(o) => {
                if stack.len >= stack_size {
                    return Err(overflow);
                }
                if handler(o, at, &mut stack)? == After::Clear {
                    stack.len = 0;
                }
            }
        }
    }
    Ok(())
}

/// FreeType's `FontMatrix` as `cff_parse_font_matrix` reads it: the
/// values scaled by a common power of ten, which is `units_per_em`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FontMatrix {
    /// Whether the DICT has a `FontMatrix` (`has_font_matrix`), even one
    /// FreeType replaced with the default because it is degenerate.
    pub present: bool,
    /// `xx`, 16.16 times `units_per_em`.
    pub xx: i64,
    /// `yx`.
    pub yx: i64,
    /// `xy`.
    pub xy: i64,
    /// `yy`.
    pub yy: i64,
    /// The x translation (`font_offset.x`).
    pub x: i64,
    /// The y translation (`font_offset.y`).
    pub y: i64,
    /// The common power of ten (`units_per_em` before FreeType's
    /// normalization in `cff_face_init`); 0 when there is no matrix.
    pub units_per_em: u64,
}

impl Default for FontMatrix {
    fn default() -> FontMatrix {
        FontMatrix {
            present: false,
            xx: 0x1_0000,
            yx: 0,
            xy: 0,
            yy: 0x1_0000,
            x: 0,
            y: 0,
            units_per_em: 0,
        }
    }
}

fn underflow(at: usize, operator: u16) -> DictError {
    DictError {
        offset: at,
        kind: DictErrorKind::StackUnderflow { operator },
    }
}

fn invalid(at: usize, operator: u16) -> DictError {
    DictError {
        offset: at,
        kind: DictErrorKind::InvalidValue { operator },
    }
}

/// `FT_Matrix_Check` on the 64-bit values.
fn matrix_check(xx: i64, xy: i64, yx: i64, yy: i64) -> bool {
    let val = xx.unsigned_abs() | xy.unsigned_abs() | yx.unsigned_abs() | yy.unsigned_abs();
    if val == 0 || val > 0x7FFF_FFFF {
        return false;
    }
    let det = xx.wrapping_mul(yy).wrapping_sub(xy.wrapping_mul(yx));
    let frob = [xx, xy, yx, yy].iter().fold(0u64, |s, v| {
        s.wrapping_add(v.wrapping_mul(*v).cast_unsigned())
    });
    det.unsigned_abs() > frob / 32
}

fn font_matrix(at: usize, s: &Operands<'_>) -> Result<FontMatrix, DictError> {
    if s.len() < 6 {
        return Err(underflow(at, op::FONT_MATRIX));
    }
    let mut values = [0i64; 6];
    let mut scalings = [0i64; 6];
    let mut max_scaling = i64::MIN;
    let mut min_scaling = i64::MAX;
    for (i, (v, sc)) in values.iter_mut().zip(scalings.iter_mut()).enumerate() {
        let (value, scaling) = s.get(i).fixed_dynamic();
        *v = value;
        *sc = scaling;
        if value != 0 {
            max_scaling = max_scaling.max(scaling);
            min_scaling = min_scaling.min(scaling);
        }
    }
    let unlikely = FontMatrix {
        present: true,
        units_per_em: 1,
        ..FontMatrix::default()
    };
    let spread = max_scaling.wrapping_sub(min_scaling);
    if !(-9..=0).contains(&max_scaling) || !(0..=9).contains(&spread) {
        return Ok(unlikely);
    }
    for (v, sc) in values.iter_mut().zip(scalings.iter()) {
        let value = *v;
        if value == 0 {
            continue;
        }
        let divisor = power_ten(max_scaling.wrapping_sub(*sc));
        let half = divisor >> 1;
        *v = if value < 0 {
            if i64::MIN.wrapping_add(half) < value {
                div(value.wrapping_sub(half), divisor)
            } else {
                div(i64::MIN, divisor)
            }
        } else if i64::MAX.wrapping_sub(half) > value {
            div(value.wrapping_add(half), divisor)
        } else {
            div(i64::MAX, divisor)
        };
    }
    let [xx, yx, xy, yy, x, y] = values;
    if !matrix_check(xx, xy, yx, yy) {
        return Ok(unlikely);
    }
    Ok(FontMatrix {
        present: true,
        xx,
        yx,
        xy,
        yy,
        x,
        y,
        units_per_em: power_ten(max_scaling.wrapping_neg()).cast_unsigned(),
    })
}

/// FreeType's "missing" SID.
pub const NO_SID: u32 = 0xFFFF;

/// A Top DICT as FreeType parses it (`CFF_FontRecDictRec`), with
/// FreeType's defaults for missing operators. SIDs are `NO_SID` when
/// missing. For CFF2, only `font_matrix`, `char_strings`, `fd_array`,
/// `fd_select`, `vstore` and `maxstack` are read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TopDict {
    /// `version` (SID).
    pub version: u32,
    /// `Notice` (SID).
    pub notice: u32,
    /// `Copyright` (SID).
    pub copyright: u32,
    /// `FullName` (SID).
    pub full_name: u32,
    /// `FamilyName` (SID).
    pub family_name: u32,
    /// `Weight` (SID).
    pub weight: u32,
    /// `isFixedPitch`, as the low byte FreeType keeps.
    pub is_fixed_pitch: u8,
    /// `ItalicAngle`, 16.16.
    pub italic_angle: i64,
    /// `UnderlinePosition` (default -100).
    pub underline_position: i16,
    /// `UnderlineThickness` (default 50).
    pub underline_thickness: u16,
    /// `PaintType`.
    pub paint_type: i32,
    /// `CharstringType` (default 2).
    pub charstring_type: i32,
    /// `FontMatrix`.
    pub font_matrix: FontMatrix,
    /// `UniqueID`.
    pub unique_id: u64,
    /// `FontBBox`: xMin, yMin, xMax, yMax, each 16.16 rounded to an
    /// integer (`FT_RoundFix`).
    pub font_bbox: [i64; 4],
    /// `StrokeWidth`.
    pub stroke_width: i64,
    /// The number of `XUID` values kept (at most 16). FreeType ignores
    /// `XUID`.
    pub xuid_len: u8,
    /// The `XUID` values.
    pub xuid: [i64; 16],
    /// `charset` (offset, or 0 to 2 for the predefined charsets).
    pub charset: u64,
    /// `Encoding` (offset, or 0 and 1 for the predefined encodings).
    pub encoding: u64,
    /// `CharStrings` (offset).
    pub char_strings: u64,
    /// The size given by `Private`.
    pub private_size: u64,
    /// The offset given by `Private`.
    pub private_offset: u64,
    /// `SyntheticBase`.
    pub synthetic_base: i64,
    /// `PostScript` (SID).
    pub postscript: u32,
    /// `BaseFontName` (SID). FreeType ignores it.
    pub base_font_name: u32,
    /// The registry SID of `ROS`; `NO_SID` without `ROS`.
    pub cid_registry: u32,
    /// The ordering SID of `ROS`.
    pub cid_ordering: u32,
    /// The supplement of `ROS` (a real is truncated).
    pub cid_supplement: i64,
    /// `CIDFontVersion`.
    pub cid_font_version: i64,
    /// `CIDFontRevision`.
    pub cid_font_revision: i64,
    /// `CIDFontType`.
    pub cid_font_type: i64,
    /// `CIDCount` (default 8720).
    pub cid_count: u64,
    /// `UIDBase`.
    pub uid_base: u64,
    /// `FDArray` (offset).
    pub fd_array: u64,
    /// `FDSelect` (offset).
    pub fd_select: u64,
    /// `FontName` (SID).
    pub font_name: u32,
    /// The number of designs of `MultipleMaster`.
    pub num_designs: u16,
    /// The number of axes of `MultipleMaster`.
    pub num_axes: u16,
    /// `vstore` (CFF2, offset).
    pub vstore: u64,
    /// `maxstack` (CFF2): always [`MAX_STACK`] after FreeType's clamping;
    /// 48 for CFF.
    pub maxstack: u32,
}

impl TopDict {
    /// FreeType's defaults (`cff_subfont_load`).
    pub fn defaults(version: Version) -> TopDict {
        TopDict {
            version: NO_SID,
            notice: NO_SID,
            copyright: NO_SID,
            full_name: NO_SID,
            family_name: NO_SID,
            weight: NO_SID,
            is_fixed_pitch: 0,
            italic_angle: 0,
            underline_position: -100,
            underline_thickness: 50,
            paint_type: 0,
            charstring_type: 2,
            font_matrix: FontMatrix::default(),
            unique_id: 0,
            font_bbox: [0; 4],
            stroke_width: 0,
            xuid_len: 0,
            xuid: [0; 16],
            charset: 0,
            encoding: 0,
            char_strings: 0,
            private_size: 0,
            private_offset: 0,
            synthetic_base: 0,
            postscript: NO_SID,
            base_font_name: NO_SID,
            cid_registry: NO_SID,
            cid_ordering: NO_SID,
            cid_supplement: 0,
            cid_font_version: 0,
            cid_font_revision: 0,
            cid_font_type: 0,
            cid_count: 8720,
            uid_base: 0,
            fd_array: 0,
            fd_select: 0,
            font_name: NO_SID,
            num_designs: 0,
            num_axes: 0,
            vstore: 0,
            maxstack: match version {
                Version::Cff => 48,
                Version::Cff2 => MAX_STACK,
            },
        }
    }

    /// Whether the DICT has `ROS` (a CID-keyed font).
    pub fn is_cid(&self) -> bool {
        self.cid_registry != NO_SID
    }

    /// Parses a Top DICT (or, in CFF, a Font DICT) as FreeType does.
    /// When the same operator appears twice, the last one wins.
    pub fn parse(data: &[u8], version: Version) -> Result<TopDict, DictError> {
        let kind = match version {
            Version::Cff => DictKind::Top,
            Version::Cff2 => DictKind::Top2,
        };
        let mut d = TopDict::defaults(version);
        run(data, kind.stack_size(), &mut |o, at, s| {
            d.read(kind, o, at, s)?;
            Ok(After::Clear)
        })?;
        Ok(d)
    }

    fn read(
        &mut self,
        kind: DictKind,
        o: u16,
        at: usize,
        s: &mut Operands<'_>,
    ) -> Result<(), DictError> {
        use op::*;
        let Some(f) = field(kind, o) else {
            // Operators FreeType ignores; XUID and BaseFontName are kept.
            if kind == DictKind::Top {
                if o == XUID {
                    let n = s.len().min(16);
                    self.xuid_len = u8::try_from(n).unwrap_or(16);
                    for (i, slot) in self.xuid.iter_mut().enumerate() {
                        *slot = if i < n { s.get(i).num() } else { 0 };
                    }
                } else if o == BASE_FONT_NAME && !s.is_empty() {
                    self.base_font_name = ft::to_i32(s.get(0).num()).cast_unsigned();
                }
            }
            return Ok(());
        };
        if !matches!(f, Field::Delta(_) | Field::DeltaFixed(_)) && s.is_empty() {
            return Err(underflow(at, o));
        }
        let num = s.get(0).num();
        let sid = ft::to_i32(num).cast_unsigned();
        match o {
            VERSION => self.version = sid,
            NOTICE => self.notice = sid,
            COPYRIGHT => self.copyright = sid,
            FULL_NAME => self.full_name = sid,
            FAMILY_NAME => self.family_name = sid,
            WEIGHT => self.weight = sid,
            POSTSCRIPT => self.postscript = sid,
            FONT_NAME => self.font_name = sid,
            IS_FIXED_PITCH => self.is_fixed_pitch = low8(num),
            ITALIC_ANGLE => self.italic_angle = s.get(0).fixed(),
            UNDERLINE_POSITION => self.underline_position = low16(num).cast_signed(),
            UNDERLINE_THICKNESS => self.underline_thickness = low16(num),
            PAINT_TYPE => self.paint_type = ft::to_i32(num),
            CHARSTRING_TYPE => self.charstring_type = ft::to_i32(num),
            UNIQUE_ID => self.unique_id = num.cast_unsigned(),
            STROKE_WIDTH => self.stroke_width = num,
            CHARSET => self.charset = num.cast_unsigned(),
            ENCODING => self.encoding = num.cast_unsigned(),
            CHAR_STRINGS => self.char_strings = num.cast_unsigned(),
            SYNTHETIC_BASE => self.synthetic_base = num,
            CID_FONT_VERSION => self.cid_font_version = num,
            CID_FONT_REVISION => self.cid_font_revision = num,
            CID_FONT_TYPE => self.cid_font_type = num,
            CID_COUNT => self.cid_count = num.cast_unsigned(),
            UID_BASE => self.uid_base = num.cast_unsigned(),
            FD_ARRAY => self.fd_array = num.cast_unsigned(),
            FD_SELECT => self.fd_select = num.cast_unsigned(),
            VSTORE => self.vstore = num.cast_unsigned(),
            MAXSTACK => self.maxstack = MAX_STACK,
            FONT_MATRIX => self.font_matrix = font_matrix(at, s)?,
            FONT_BBOX => {
                if s.len() < 4 {
                    return Err(underflow(at, o));
                }
                for (i, b) in self.font_bbox.iter_mut().enumerate() {
                    *b = ft::round_fix(s.get(i).fixed());
                }
            }
            PRIVATE => {
                let (size, offset) = private(at, s)?;
                self.private_size = size;
                self.private_offset = offset;
            }
            MULTIPLE_MASTER => {
                if s.len() < 5 {
                    return Err(underflow(at, o));
                }
                if !(2..=16).contains(&num) {
                    return Err(invalid(at, o));
                }
                self.num_designs = low16(num);
                self.num_axes =
                    u16::try_from(s.len().saturating_sub(4) & 0xFFFF).unwrap_or(u16::MAX);
            }
            ROS => {
                if s.len() < 3 {
                    return Err(underflow(at, o));
                }
                self.cid_registry = sid;
                self.cid_ordering = ft::to_i32(s.get(1).num()).cast_unsigned();
                self.cid_supplement = s.get(2).num();
            }
            _ => {}
        }
        Ok(())
    }
}

fn low8(v: i64) -> u8 {
    u8::try_from(v & 0xFF).unwrap_or(0)
}

fn low16(v: i64) -> u16 {
    u16::try_from(v & 0xFFFF).unwrap_or(0)
}

/// `cff_parse_private_dict`: the size and the offset.
fn private(at: usize, s: &Operands<'_>) -> Result<(u64, u64), DictError> {
    if s.len() < 2 {
        return Err(underflow(at, op::PRIVATE));
    }
    let size = s.get(0).num();
    let offset = s.get(1).num();
    if size < 0 || offset < 0 {
        return Err(invalid(at, op::PRIVATE));
    }
    Ok((size.cast_unsigned(), offset.cast_unsigned()))
}

/// A Font DICT of the FDArray: the values FreeType uses from it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FontDict {
    /// `FontMatrix`.
    pub font_matrix: FontMatrix,
    /// The size given by `Private`.
    pub private_size: u64,
    /// The offset given by `Private`.
    pub private_offset: u64,
    /// `FontName` (SID; CFF only).
    pub font_name: u32,
    /// The registry SID of `ROS` (CFF only): FreeType does not read the
    /// Private DICT of a Font DICT with `ROS`.
    pub cid_registry: u32,
}

impl FontDict {
    /// Parses a Font DICT as FreeType does: in CFF with the operators of
    /// the Top DICT, in CFF2 with `Private` and `FontMatrix` only.
    pub fn parse(data: &[u8], version: Version) -> Result<FontDict, DictError> {
        match version {
            Version::Cff => {
                let t = TopDict::parse(data, version)?;
                Ok(FontDict {
                    font_matrix: t.font_matrix,
                    private_size: t.private_size,
                    private_offset: t.private_offset,
                    font_name: t.font_name,
                    cid_registry: t.cid_registry,
                })
            }
            Version::Cff2 => {
                let mut d = FontDict {
                    font_matrix: FontMatrix::default(),
                    private_size: 0,
                    private_offset: 0,
                    font_name: NO_SID,
                    cid_registry: NO_SID,
                };
                run(data, DictKind::Font2.stack_size(), &mut |o, at, s| {
                    match o {
                        op::PRIVATE => {
                            if s.is_empty() {
                                return Err(underflow(at, o));
                            }
                            let (size, offset) = private(at, s)?;
                            d.private_size = size;
                            d.private_offset = offset;
                        }
                        op::FONT_MATRIX => {
                            if s.is_empty() {
                                return Err(underflow(at, o));
                            }
                            d.font_matrix = font_matrix(at, s)?;
                        }
                        _ => {}
                    }
                    Ok(After::Clear)
                })?;
                Ok(d)
            }
        }
    }
}

/// The weights of a CFF2 `blend` (FreeType's blend vector `BV` without
/// its first, default, element).
pub trait BlendWeights {
    /// The number of regions of ItemVariationData `vsindex`, or `None`
    /// when no blend vector can be built for it.
    fn regions(&self, vsindex: u32) -> Option<u32>;
    /// The weight of region `k` (from 0) of ItemVariationData `vsindex`,
    /// 16.16.
    fn weight(&self, vsindex: u32, k: u32) -> i32;
}

/// No Variation Store: every `blend` fails.
#[derive(Clone, Copy, Debug)]
pub struct NoBlend;

impl BlendWeights for NoBlend {
    fn regions(&self, _: u32) -> Option<u32> {
        None
    }
    fn weight(&self, _: u32, _: u32) -> i32 {
        0
    }
}

/// A delta array of at most `N` values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Deltas<const N: usize> {
    /// The number of values.
    pub len: u8,
    /// The values, accumulated.
    pub values: [i64; N],
}

impl<const N: usize> Default for Deltas<N> {
    fn default() -> Self {
        Deltas {
            len: 0,
            values: [0; N],
        }
    }
}

impl<const N: usize> Deltas<N> {
    /// The values present.
    pub fn as_slice(&self) -> &[i64] {
        self.values.get(..usize::from(self.len)).unwrap_or_default()
    }

    fn read(s: &Operands<'_>, f: &dyn Fn(&Operand<'_>) -> i64) -> Deltas<N> {
        let n = s.len().min(N);
        let mut d = Deltas {
            len: u8::try_from(n).unwrap_or(0),
            values: [0; N],
        };
        let mut val = 0i64;
        for (i, slot) in d.values.iter_mut().enumerate().take(n) {
            val = val.wrapping_add(f(&s.get(i)));
            *slot = val;
        }
        d
    }
}

/// A Private DICT as FreeType parses it (`CFF_PrivateRec`), after
/// FreeType's corrections: the count of `BlueValues` made even,
/// `initialRandomSeed` made positive (0 becomes 987654321), and
/// `BlueShift` and `BlueFuzz` outside 0 to 1000 reset. Hinting values
/// are read but not used by ADR 0021.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PrivateDict {
    /// `BlueValues`, 16.16.
    pub blue_values: Deltas<14>,
    /// `OtherBlues`, 16.16.
    pub other_blues: Deltas<10>,
    /// `FamilyBlues`, 16.16.
    pub family_blues: Deltas<14>,
    /// `FamilyOtherBlues`, 16.16.
    pub family_other_blues: Deltas<10>,
    /// `BlueScale`, 16.16 times 1000.
    pub blue_scale: i64,
    /// `BlueShift`.
    pub blue_shift: i64,
    /// `BlueFuzz`.
    pub blue_fuzz: i64,
    /// `StdHW`.
    pub std_hw: i64,
    /// `StdVW`.
    pub std_vw: i64,
    /// `StemSnapH`.
    pub stem_snap_h: Deltas<13>,
    /// `StemSnapV`.
    pub stem_snap_v: Deltas<13>,
    /// `ForceBold`, as the low byte FreeType keeps.
    pub force_bold: u8,
    /// `ForceBoldThreshold`, 16.16.
    pub force_bold_threshold: i64,
    /// `lenIV`.
    pub len_iv: i32,
    /// `LanguageGroup`.
    pub language_group: i32,
    /// `ExpansionFactor`, 16.16.
    pub expansion_factor: i64,
    /// `initialRandomSeed`.
    pub initial_random_seed: i64,
    /// `Subrs` (offset from the Private DICT).
    pub subrs: u64,
    /// `defaultWidthX`.
    pub default_width_x: i64,
    /// `nominalWidthX`.
    pub nominal_width_x: i64,
    /// `vsindex` (CFF2).
    pub vsindex: u32,
}

impl Default for PrivateDict {
    /// The values FreeType uses for a font without a Private DICT: all
    /// zero.
    fn default() -> PrivateDict {
        PrivateDict {
            blue_values: Deltas::default(),
            other_blues: Deltas::default(),
            family_blues: Deltas::default(),
            family_other_blues: Deltas::default(),
            blue_scale: 0,
            blue_shift: 0,
            blue_fuzz: 0,
            std_hw: 0,
            std_vw: 0,
            stem_snap_h: Deltas::default(),
            stem_snap_v: Deltas::default(),
            force_bold: 0,
            force_bold_threshold: 0,
            len_iv: 0,
            language_group: 0,
            expansion_factor: 0,
            initial_random_seed: 0,
            subrs: 0,
            default_width_x: 0,
            nominal_width_x: 0,
            vsindex: 0,
        }
    }
}

impl PrivateDict {
    /// FreeType's defaults before parsing (`cff_load_private_dict`).
    pub fn defaults() -> PrivateDict {
        PrivateDict {
            blue_shift: 7,
            blue_fuzz: 1,
            len_iv: -1,
            expansion_factor: 3932,
            blue_scale: 2_596_864,
            ..PrivateDict::default()
        }
    }

    /// Parses a Private DICT as FreeType does. A CFF2 `blend` takes its
    /// weights from `blend`.
    pub fn parse(
        data: &[u8],
        version: Version,
        blend: &dyn BlendWeights,
    ) -> Result<PrivateDict, DictError> {
        let kind = match version {
            Version::Cff => DictKind::Private,
            Version::Cff2 => DictKind::Private2,
        };
        let mut d = PrivateDict::defaults();
        let mut used_blend = false;
        run(data, kind.stack_size(), &mut |o, at, s| {
            d.read(kind, o, at, s, blend, &mut used_blend)
        })?;
        d.blue_values.len &= !1;
        d.initial_random_seed = match d.initial_random_seed {
            0 => 987_654_321,
            v if v < 0 => v.wrapping_neg(),
            v => v,
        };
        if !(0..=1000).contains(&d.blue_shift) {
            d.blue_shift = 7;
        }
        if !(0..=1000).contains(&d.blue_fuzz) {
            d.blue_fuzz = 1;
        }
        Ok(d)
    }

    fn read(
        &mut self,
        kind: DictKind,
        o: u16,
        at: usize,
        s: &mut Operands<'_>,
        blend: &dyn BlendWeights,
        used_blend: &mut bool,
    ) -> Result<After, DictError> {
        use op::*;
        let Some(f) = field(kind, o) else {
            return Ok(After::Clear);
        };
        if !matches!(f, Field::Delta(_) | Field::DeltaFixed(_)) && s.is_empty() {
            return Err(underflow(at, o));
        }
        let num = s.get(0).num();
        let fixed = |s: &Operands<'_>| s.get(0).fixed();
        match o {
            BLUE_VALUES => self.blue_values = Deltas::read(s, &|v| v.fixed()),
            OTHER_BLUES => self.other_blues = Deltas::read(s, &|v| v.fixed()),
            FAMILY_BLUES => self.family_blues = Deltas::read(s, &|v| v.fixed()),
            FAMILY_OTHER_BLUES => self.family_other_blues = Deltas::read(s, &|v| v.fixed()),
            STEM_SNAP_H => self.stem_snap_h = Deltas::read(s, &|v| v.num()),
            STEM_SNAP_V => self.stem_snap_v = Deltas::read(s, &|v| v.num()),
            BLUE_SCALE => self.blue_scale = s.get(0).fixed_scaled(3),
            BLUE_SHIFT => self.blue_shift = num,
            BLUE_FUZZ => self.blue_fuzz = num,
            STD_HW => self.std_hw = num,
            STD_VW => self.std_vw = num,
            FORCE_BOLD => self.force_bold = low8(num),
            FORCE_BOLD_THRESHOLD => self.force_bold_threshold = fixed(s),
            LEN_IV => self.len_iv = ft::to_i32(num),
            LANGUAGE_GROUP => self.language_group = ft::to_i32(num),
            EXPANSION_FACTOR => self.expansion_factor = fixed(s),
            INITIAL_RANDOM_SEED => self.initial_random_seed = num,
            SUBRS => self.subrs = num.cast_unsigned(),
            DEFAULT_WIDTH_X => self.default_width_x = num,
            NOMINAL_WIDTH_X => self.nominal_width_x = num,
            VSINDEX => {
                if *used_blend {
                    return Err(invalid(at, o));
                }
                self.vsindex = ft::to_i32(num).cast_unsigned();
            }
            BLEND => {
                let r = do_blend(self.vsindex, at, s, blend);
                *used_blend = true;
                r?;
                return Ok(After::Keep);
            }
            _ => {}
        }
        Ok(After::Clear)
    }
}

/// `cff_parse_blend` and `cff_blend_doBlend`.
fn do_blend(
    vsindex: u32,
    at: usize,
    s: &mut Operands<'_>,
    blend: &dyn BlendWeights,
) -> Result<(), DictError> {
    let regions = blend.regions(vsindex).ok_or(DictError {
        offset: at,
        kind: DictErrorKind::Blend,
    })?;
    let len_bv = regions.saturating_add(1);
    let count = s.len().saturating_sub(1);
    let n = ft::to_i32(s.get(count).num()).cast_unsigned();
    if to_usize(n) > DictKind::Private2.stack_size() {
        return Err(invalid(at, op::BLEND));
    }
    let operands = n.wrapping_mul(len_bv);
    if to_usize(operands) > count {
        return Err(underflow(at, op::BLEND));
    }
    let base = count.saturating_sub(to_usize(operands));
    let mut delta = base.saturating_add(to_usize(n));
    for i in 0..to_usize(n) {
        let at = base.saturating_add(i);
        let mut sum = s.get(at).fixed();
        for k in 0..regions {
            let d = s.get(delta).fixed();
            delta = delta.saturating_add(1);
            sum = sum.wrapping_add(ft::mul_fix(d, i64::from(blend.weight(vsindex, k))));
        }
        s.set(at, Operand::Blended(ft::to_i32(sum)));
    }
    s.truncate(base.saturating_add(to_usize(n)));
    Ok(())
}
