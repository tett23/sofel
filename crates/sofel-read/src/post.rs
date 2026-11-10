//! The `post` (PostScript) table, versions 1.0, 2.0, 2.5, 3.0 and Apple's
//! 4.0 (ADR 0016), with the 258 standard Macintosh glyph names.
//!
//! How glyph names are looked up (HarfBuzz and FreeType differ) is
//! decided in `sofel-font`.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/post>

use crate::be::{self, to_u32, to_usize};
use crate::{Diagnostic, Fixed, GlyphId, Kind, ReadError, Tag};

/// The `post` tag.
pub const TAG: Tag = Tag::new(b"post");

/// Version 1.0: the glyphs are the 258 standard Macintosh glyphs.
pub const VERSION_1: u32 = 0x0001_0000;
/// Version 2.0: name indices and Pascal strings.
pub const VERSION_2: u32 = 0x0002_0000;
/// Version 2.5 (deprecated): offsets into the standard Macintosh order.
pub const VERSION_2_5: u32 = 0x0002_5000;
/// Version 3.0: no glyph names.
pub const VERSION_3: u32 = 0x0003_0000;
/// Version 4.0 (Apple): a character code for each glyph.
pub const VERSION_4: u32 = 0x0004_0000;

const HEADER: usize = 32;

/// The number of standard Macintosh glyph names.
pub const NUM_MAC_GLYPH_NAMES: u16 = 258;

/// The standard Macintosh glyph names, in the order of their indices.
pub const MAC_GLYPH_NAMES: [&str; 258] = [
    ".notdef",
    ".null",
    "nonmarkingreturn",
    "space",
    "exclam",
    "quotedbl",
    "numbersign",
    "dollar",
    "percent",
    "ampersand",
    "quotesingle",
    "parenleft",
    "parenright",
    "asterisk",
    "plus",
    "comma",
    "hyphen",
    "period",
    "slash",
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "colon",
    "semicolon",
    "less",
    "equal",
    "greater",
    "question",
    "at",
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    "bracketleft",
    "backslash",
    "bracketright",
    "asciicircum",
    "underscore",
    "grave",
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
    "braceleft",
    "bar",
    "braceright",
    "asciitilde",
    "Adieresis",
    "Aring",
    "Ccedilla",
    "Eacute",
    "Ntilde",
    "Odieresis",
    "Udieresis",
    "aacute",
    "agrave",
    "acircumflex",
    "adieresis",
    "atilde",
    "aring",
    "ccedilla",
    "eacute",
    "egrave",
    "ecircumflex",
    "edieresis",
    "iacute",
    "igrave",
    "icircumflex",
    "idieresis",
    "ntilde",
    "oacute",
    "ograve",
    "ocircumflex",
    "odieresis",
    "otilde",
    "uacute",
    "ugrave",
    "ucircumflex",
    "udieresis",
    "dagger",
    "degree",
    "cent",
    "sterling",
    "section",
    "bullet",
    "paragraph",
    "germandbls",
    "registered",
    "copyright",
    "trademark",
    "acute",
    "dieresis",
    "notequal",
    "AE",
    "Oslash",
    "infinity",
    "plusminus",
    "lessequal",
    "greaterequal",
    "yen",
    "mu",
    "partialdiff",
    "summation",
    "product",
    "pi",
    "integral",
    "ordfeminine",
    "ordmasculine",
    "Omega",
    "ae",
    "oslash",
    "questiondown",
    "exclamdown",
    "logicalnot",
    "radical",
    "florin",
    "approxequal",
    "Delta",
    "guillemotleft",
    "guillemotright",
    "ellipsis",
    "nonbreakingspace",
    "Agrave",
    "Atilde",
    "Otilde",
    "OE",
    "oe",
    "endash",
    "emdash",
    "quotedblleft",
    "quotedblright",
    "quoteleft",
    "quoteright",
    "divide",
    "lozenge",
    "ydieresis",
    "Ydieresis",
    "fraction",
    "currency",
    "guilsinglleft",
    "guilsinglright",
    "fi",
    "fl",
    "daggerdbl",
    "periodcentered",
    "quotesinglbase",
    "quotedblbase",
    "perthousand",
    "Acircumflex",
    "Ecircumflex",
    "Aacute",
    "Edieresis",
    "Egrave",
    "Iacute",
    "Icircumflex",
    "Idieresis",
    "Igrave",
    "Oacute",
    "Ocircumflex",
    "apple",
    "Ograve",
    "Uacute",
    "Ucircumflex",
    "Ugrave",
    "dotlessi",
    "circumflex",
    "tilde",
    "macron",
    "breve",
    "dotaccent",
    "ring",
    "cedilla",
    "hungarumlaut",
    "ogonek",
    "caron",
    "Lslash",
    "lslash",
    "Scaron",
    "scaron",
    "Zcaron",
    "zcaron",
    "brokenbar",
    "Eth",
    "eth",
    "Yacute",
    "yacute",
    "Thorn",
    "thorn",
    "minus",
    "multiply",
    "onesuperior",
    "twosuperior",
    "threesuperior",
    "onehalf",
    "onequarter",
    "threequarters",
    "franc",
    "Gbreve",
    "gbreve",
    "Idotaccent",
    "Scedilla",
    "scedilla",
    "Cacute",
    "cacute",
    "Ccaron",
    "ccaron",
    "dcroat",
];

/// The standard Macintosh glyph name with `index`, below 258.
pub fn mac_glyph_name(index: u16) -> Option<&'static str> {
    MAC_GLYPH_NAMES.get(usize::from(index)).copied()
}

/// A view over the `post` table.
#[derive(Clone, Copy, Debug)]
pub struct Post<'a> {
    h: &'a [u8; HEADER],
    /// Version 2.0: the name indices; version 4.0: the codes.
    words: &'a [[u8; 2]],
    /// Version 2.5: the offsets.
    offsets: &'a [u8],
    /// Version 2.0: the bytes after the name indices.
    pool: &'a [u8],
}

fn needed(data: &[u8]) -> usize {
    let n = be::u16_at(data, HEADER).map_or(0, usize::from);
    let per_glyph = match be::u32_at(data, 0) {
        Some(VERSION_2) => 2,
        Some(VERSION_2_5) => 1,
        _ => return HEADER,
    };
    HEADER
        .saturating_add(2)
        .saturating_add(n.saturating_mul(per_glyph))
}

impl<'a> Post<'a> {
    /// A view over the header and the arrays of the version.
    ///
    /// Fails with [`ReadError::Truncated`] if the header or, for
    /// versions 2.0 and 2.5, `numGlyphs` and its array do not fit, and
    /// with [`ReadError::UnknownVersion`] for other versions.
    pub fn new(data: &'a [u8]) -> Result<Post<'a>, ReadError> {
        let h = data
            .first_chunk::<HEADER>()
            .ok_or(ReadError::truncated(HEADER, data.len()))?;
        let version = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
        let truncated = ReadError::truncated(needed(data), data.len());
        let rest = be::tail_at(data, HEADER).unwrap_or_default();
        let count = || be::u16_at(data, HEADER).map(usize::from).ok_or(truncated);
        let (words, offsets, pool): (&[[u8; 2]], &[u8], &[u8]) = match version {
            VERSION_1 | VERSION_3 => (&[], &[], &[]),
            VERSION_2 => {
                let n = count()?;
                let words =
                    be::chunks_at::<2>(data, HEADER.saturating_add(2), n).ok_or(truncated)?;
                let pool_at = HEADER.saturating_add(2).saturating_add(n.saturating_mul(2));
                (words, &[], be::tail_at(data, pool_at).unwrap_or_default())
            }
            VERSION_2_5 => {
                let n = count()?;
                let offsets = be::slice_at(data, HEADER.saturating_add(2), n).ok_or(truncated)?;
                (&[], offsets, &[])
            }
            VERSION_4 => (rest.as_chunks::<2>().0, &[], &[]),
            v => return Err(ReadError::UnknownVersion(v)),
        };
        Ok(Post {
            h,
            words,
            offsets,
            pool,
        })
    }

    /// `version`, as its 16.16 value: [`VERSION_1`], [`VERSION_2`],
    /// [`VERSION_2_5`], [`VERSION_3`] or [`VERSION_4`].
    pub fn version(&self) -> u32 {
        u32::from_be_bytes([self.h[0], self.h[1], self.h[2], self.h[3]])
    }
    /// `italicAngle`, in degrees counter-clockwise from the vertical.
    pub fn italic_angle(&self) -> Fixed {
        Fixed(i32::from_be_bytes([
            self.h[4], self.h[5], self.h[6], self.h[7],
        ]))
    }
    /// `underlinePosition`: the top of the underline, in font units.
    pub fn underline_position(&self) -> i16 {
        i16::from_be_bytes([self.h[8], self.h[9]])
    }
    /// `underlineThickness`, in font units.
    pub fn underline_thickness(&self) -> i16 {
        i16::from_be_bytes([self.h[10], self.h[11]])
    }
    /// `isFixedPitch`: non-zero for monospaced fonts.
    pub fn is_fixed_pitch(&self) -> u32 {
        u32::from_be_bytes([self.h[12], self.h[13], self.h[14], self.h[15]])
    }
    /// `minMemType42`.
    pub fn min_mem_type42(&self) -> u32 {
        u32::from_be_bytes([self.h[16], self.h[17], self.h[18], self.h[19]])
    }
    /// `maxMemType42`.
    pub fn max_mem_type42(&self) -> u32 {
        u32::from_be_bytes([self.h[20], self.h[21], self.h[22], self.h[23]])
    }
    /// `minMemType1`.
    pub fn min_mem_type1(&self) -> u32 {
        u32::from_be_bytes([self.h[24], self.h[25], self.h[26], self.h[27]])
    }
    /// `maxMemType1`.
    pub fn max_mem_type1(&self) -> u32 {
        u32::from_be_bytes([self.h[28], self.h[29], self.h[30], self.h[31]])
    }

    /// `numGlyphs` of versions 2.0 and 2.5.
    pub fn num_glyphs(&self) -> Option<u16> {
        match self.version() {
            VERSION_2 => u16::try_from(self.words.len()).ok(),
            VERSION_2_5 => u16::try_from(self.offsets.len()).ok(),
            _ => None,
        }
    }

    /// The name index of `glyph` (version 2.0): below 258 a standard
    /// Macintosh name, otherwise 258 plus the index of a string in
    /// [`Post::names`].
    pub fn glyph_name_index(&self, glyph: GlyphId) -> Option<u16> {
        if self.version() != VERSION_2 {
            return None;
        }
        self.words
            .get(to_usize(glyph.0))
            .map(|w| u16::from_be_bytes(*w))
    }

    /// The offset of `glyph` from its index in the standard Macintosh
    /// order (version 2.5).
    pub fn v25_offset(&self, glyph: GlyphId) -> Option<i8> {
        self.offsets
            .get(to_usize(glyph.0))
            .map(|&b| i8::from_be_bytes([b]))
    }

    /// The character code of `glyph` (version 4.0).
    pub fn v4_code(&self, glyph: GlyphId) -> Option<u16> {
        if self.version() != VERSION_4 {
            return None;
        }
        self.words
            .get(to_usize(glyph.0))
            .map(|w| u16::from_be_bytes(*w))
    }

    /// The Pascal strings of version 2.0, in order, without their length
    /// bytes. Stops at the first string that does not fit in the table.
    pub fn names(&self) -> Names<'a> {
        Names { rest: self.pool }
    }

    /// Where the strings of version 2.0 start, from the start of the table.
    pub fn names_offset(&self) -> usize {
        HEADER
            .saturating_add(2)
            .saturating_add(self.words.len().saturating_mul(2))
    }
}

/// The Pascal strings of a `post` table version 2.0; see [`Post::names`].
#[derive(Clone, Debug)]
pub struct Names<'a> {
    rest: &'a [u8],
}

impl<'a> Iterator for Names<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        let (&len, tail) = self.rest.split_first()?;
        let Some((name, rest)) = tail.split_at_checked(usize::from(len)) else {
            self.rest = &[];
            return None;
        };
        self.rest = rest;
        Some(name)
    }
}

/// Checks a `post` table: an unknown version and truncated arrays (and
/// nothing else then), the deprecated version 2.5, and for version 2.0 a
/// string that runs past the table and name indices past the stored
/// strings.
pub fn validate(data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    let mut emit = |offset: usize, kind: Kind| report(Diagnostic::new(TAG, to_u32(offset), kind));
    let post = match Post::new(data) {
        Ok(p) => p,
        Err(ReadError::UnknownVersion(found)) => return emit(0, Kind::UnknownVersion { found }),
        Err(_) => {
            return emit(
                0,
                Kind::Truncated {
                    needed: to_u32(needed(data)),
                    available: to_u32(data.len()),
                },
            );
        }
    };
    if post.version() == VERSION_2_5 {
        emit(
            0,
            Kind::DeprecatedVersion {
                found: post.version(),
            },
        );
    }
    if post.version() != VERSION_2 {
        return;
    }
    let mut count = 0usize;
    let mut at = post.names_offset();
    let mut names = post.names();
    for name in names.by_ref() {
        count = count.saturating_add(1);
        at = at.saturating_add(1).saturating_add(name.len());
    }
    if at < data.len() {
        let len = be::u8_at(data, at).map_or(0, usize::from);
        emit(
            at,
            Kind::Truncated {
                needed: to_u32(at.saturating_add(1).saturating_add(len)),
                available: to_u32(data.len()),
            },
        );
    }
    for g in 0..post.num_glyphs().unwrap_or(0) {
        let Some(index) = post.glyph_name_index(GlyphId(u32::from(g))) else {
            continue;
        };
        let stored = index
            .checked_sub(NUM_MAC_GLYPH_NAMES)
            .is_none_or(|i| usize::from(i) < count);
        if !stored {
            emit(
                HEADER
                    .saturating_add(2)
                    .saturating_add(usize::from(g).saturating_mul(2)),
                Kind::NameIndexOutOfRange {
                    glyph: u32::from(g),
                    index: u32::from(index),
                },
            );
        }
    }
}
