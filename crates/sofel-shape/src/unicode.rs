//! The Unicode functions HarfBuzz writes by hand rather than generating
//! from the Unicode Character Database (ADR 0019, ADR 0020):
//! [`modified_combining_class`], [`is_default_ignorable`],
//! [`space_fallback_type`], [`vertical_char_for`] and
//! [`is_regional_indicator`], and [`is_variation_selector`].
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-unicode.hh` and
//! `src/hb-unicode.cc` (`_hb_modified_combining_class`), MIT; see
//! THIRD_PARTY_NOTICES.md.

use sofel_unicode::{GeneralCategory, combining_class};

/// `_hb_modified_combining_class`: the Canonical_Combining_Class
/// renumbered so that Hebrew points sort as the SBL Hebrew manual
/// describes, Arabic shadda sorts before the other marks, the Telugu
/// length marks and Thai sara u / uu sort before the virama, and the
/// Tibetan vowel signs in the order Dzongkha needs.
fn modified(ccc: u8) -> u8 {
    match ccc {
        // Hebrew.
        10 => 22,
        11 => 15,
        12 => 16,
        13 => 17,
        14 => 23,
        15 => 18,
        16 => 19,
        17 => 20,
        18 => 21,
        19 => 14,
        20 => 24,
        21 => 12,
        22 => 25,
        23 => 13,
        24 => 10,
        25 => 11,
        // Arabic.
        27 => 28,
        28 => 29,
        29 => 30,
        30 => 31,
        31 => 32,
        32 => 33,
        33 => 27,
        // Telugu.
        84 => 4,
        91 => 5,
        // Thai.
        103 => 3,
        // Tibetan.
        130 => 132,
        132 => 131,
        c => c,
    }
}

/// The combining class HarfBuzz reorders marks by
/// (`hb_unicode_funcs_t::modified_combining_class`): the
/// Canonical_Combining_Class through HarfBuzz's renumbering table, with
/// U+1A60 TAI THAM SIGN SAKOT and U+0FC6 TIBETAN SYMBOL PADMA GDAN
/// moved to 254 and U+0F39 TIBETAN MARK TSA -PHRU to 127.
pub fn modified_combining_class(cp: u32) -> u8 {
    match cp {
        0x1A60 | 0x0FC6 => 254,
        0x0F39 => 127,
        _ => modified(combining_class(cp)),
    }
}

/// Whether `cp` is one of the Default_Ignorable_Code_Points HarfBuzz
/// hides (`hb_unicode_funcs_t::is_default_ignorable`): those of Unicode
/// 14.0 except U+115F, U+1160, U+3164, U+FFA0 (the Hangul fillers, which
/// fonts draw as spacing glyphs) and U+1BCA0..U+1BCA3.
pub fn is_default_ignorable(cp: u32) -> bool {
    let plane = cp >> 16;
    if plane == 0 {
        match cp >> 8 {
            0x00 => cp == 0x00AD,
            0x03 => cp == 0x034F,
            0x06 => cp == 0x061C,
            0x17 => (0x17B4..=0x17B5).contains(&cp),
            0x18 => (0x180B..=0x180F).contains(&cp),
            0x20 => matches!(cp, 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F),
            0xFE => (0xFE00..=0xFE0F).contains(&cp) || cp == 0xFEFF,
            0xFF => (0xFFF0..=0xFFF8).contains(&cp),
            _ => false,
        }
    } else {
        match plane {
            0x01 => (0x1D173..=0x1D17A).contains(&cp),
            0x0E => (0xE0000..=0xE0FFF).contains(&cp),
            _ => false,
        }
    }
}

/// Whether `cp` is a variation selector HarfBuzz matches with the
/// character before it (`hb_unicode_funcs_t::is_variation_selector`):
/// U+FE00..U+FE0F and U+E0100..U+E01EF. The Mongolian free variation
/// selectors are not.
pub fn is_variation_selector(cp: u32) -> bool {
    matches!(cp, 0xFE00..=0xFE0F | 0xE0100..=0xE01EF)
}

/// Whether `cp` is a regional indicator symbol, U+1F1E6..U+1F1FF
/// (`_hb_codepoint_is_regional_indicator`).
pub fn is_regional_indicator(cp: u32) -> bool {
    (0x1F1E6..=0x1F1FF).contains(&cp)
}

/// How wide a space character is when the font has no glyph for it
/// (`hb_unicode_funcs_t::space_t`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum SpaceFallback {
    /// Not a space with a fallback (`NOT_SPACE`).
    NotSpace = 0,
    /// One em.
    Em = 1,
    /// Half an em.
    Em2 = 2,
    /// A third of an em.
    Em3 = 3,
    /// A quarter of an em.
    Em4 = 4,
    /// A fifth of an em.
    Em5 = 5,
    /// A sixth of an em.
    Em6 = 6,
    /// A sixteenth of an em.
    Em16 = 16,
    /// Four eighteenths of an em.
    FourEm18 = 17,
    /// The width of U+0020 SPACE.
    Space = 18,
    /// The width of a digit.
    Figure = 19,
    /// The width of `.`.
    Punctuation = 20,
    /// A narrow space.
    Narrow = 21,
}

impl SpaceFallback {
    /// The value stored in the high byte of the Unicode properties.
    pub(crate) fn from_u8(v: u8) -> SpaceFallback {
        use SpaceFallback::*;
        match v {
            1 => Em,
            2 => Em2,
            3 => Em3,
            4 => Em4,
            5 => Em5,
            6 => Em6,
            16 => Em16,
            17 => FourEm18,
            18 => Space,
            19 => Figure,
            20 => Punctuation,
            21 => Narrow,
            _ => NotSpace,
        }
    }
}

/// The fallback width class of a space character
/// (`hb_unicode_funcs_t::space_fallback_type`). U+1680 OGHAM SPACE MARK
/// and characters that are not spaces have none.
pub fn space_fallback_type(cp: u32) -> SpaceFallback {
    use SpaceFallback::*;
    match cp {
        0x0020 | 0x00A0 => Space,
        0x2000 | 0x2002 => Em2,
        0x2001 | 0x2003 | 0x3000 => Em,
        0x2004 => Em3,
        0x2005 => Em4,
        0x2006 => Em6,
        0x2007 => Figure,
        0x2008 => Punctuation,
        0x2009 => Em5,
        0x200A => Em16,
        0x202F => Narrow,
        0x205F => FourEm18,
        _ => NotSpace,
    }
}

/// The vertical presentation form HarfBuzz uses in vertical text when
/// the font has no `vert` feature (`hb_unicode_funcs_t::vertical_char_for`);
/// `cp` itself when there is none.
pub fn vertical_char_for(cp: u32) -> u32 {
    match cp {
        0x2013 => 0xFE32,
        0x2014 => 0xFE31,
        0x2025 => 0xFE30,
        0x2026 => 0xFE19,
        0x3001 => 0xFE11,
        0x3002 => 0xFE12,
        0x3008 => 0xFE3F,
        0x3009 => 0xFE40,
        0x300A => 0xFE3D,
        0x300B => 0xFE3E,
        0x300C => 0xFE41,
        0x300D => 0xFE42,
        0x300E => 0xFE43,
        0x300F => 0xFE44,
        0x3010 => 0xFE3B,
        0x3011 => 0xFE3C,
        0x3014 => 0xFE39,
        0x3015 => 0xFE3A,
        0x3016 => 0xFE17,
        0x3017 => 0xFE18,
        0xFE4F => 0xFE34,
        0xFF01 => 0xFE15,
        0xFF08 => 0xFE35,
        0xFF09 => 0xFE36,
        0xFF0C => 0xFE10,
        0xFF1A => 0xFE13,
        0xFF1B => 0xFE14,
        0xFF1F => 0xFE16,
        0xFF3B => 0xFE47,
        0xFF3D => 0xFE48,
        0xFF3F => 0xFE33,
        0xFF5B => 0xFE37,
        0xFF5D => 0xFE38,
        _ => cp,
    }
}

/// `HB_UNICODE_GENERAL_CATEGORY_IS_MARK`.
pub(crate) fn is_mark_category(gc: GeneralCategory) -> bool {
    matches!(
        gc,
        GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark
            | GeneralCategory::NonspacingMark
    )
}

/// `HB_UNICODE_GENERAL_CATEGORY_IS_LETTER`.
pub(crate) fn is_letter_category(gc: GeneralCategory) -> bool {
    matches!(
        gc,
        GeneralCategory::LowercaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::UppercaseLetter
    )
}

/// The general category with HarfBuzz's number.
pub(crate) fn category_from_u8(v: u8) -> GeneralCategory {
    use GeneralCategory::*;
    const ALL: [GeneralCategory; 30] = [
        Control,
        Format,
        Unassigned,
        PrivateUse,
        Surrogate,
        LowercaseLetter,
        ModifierLetter,
        OtherLetter,
        TitlecaseLetter,
        UppercaseLetter,
        SpacingMark,
        EnclosingMark,
        NonspacingMark,
        DecimalNumber,
        LetterNumber,
        OtherNumber,
        ConnectPunctuation,
        DashPunctuation,
        ClosePunctuation,
        FinalPunctuation,
        InitialPunctuation,
        OtherPunctuation,
        OpenPunctuation,
        CurrencySymbol,
        ModifierSymbol,
        MathSymbol,
        OtherSymbol,
        LineSeparator,
        ParagraphSeparator,
        SpaceSeparator,
    ];
    ALL.get(usize::from(v)).copied().unwrap_or(Unassigned)
}

/// Modified combining classes the shapers test
/// (`HB_MODIFIED_COMBINING_CLASS_*` and `HB_UNICODE_COMBINING_CLASS_*` of
/// `src/hb-unicode.hh`).
#[allow(dead_code)] // Used by the shapers of ADR 0024 as they land.
pub(crate) mod mcc {
    /// Sheva.
    pub(crate) const CCC10: u8 = 22;
    /// Hiriq.
    pub(crate) const CCC14: u8 = 23;
    /// Patah.
    pub(crate) const CCC17: u8 = 20;
    /// Qamats and qamats qatan.
    pub(crate) const CCC18: u8 = 21;
    /// Meteg.
    pub(crate) const CCC22: u8 = 25;
    /// `HB_UNICODE_COMBINING_CLASS_BELOW`.
    pub(crate) const BELOW: u8 = 220;
    /// `HB_UNICODE_COMBINING_CLASS_ABOVE`.
    pub(crate) const ABOVE: u8 = 230;
}
