use core::fmt;

/// A 4-byte table or feature tag.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Tag(pub [u8; 4]);

impl Tag {
    /// A tag from its four bytes.
    pub const fn new(bytes: &[u8; 4]) -> Tag {
        Tag(*bytes)
    }
    /// A tag from its big-endian `u32` value.
    pub const fn from_u32(v: u32) -> Tag {
        Tag(v.to_be_bytes())
    }
    /// The tag as a big-endian `u32` value.
    pub const fn to_u32(self) -> u32 {
        u32::from_be_bytes(self.0)
    }
}

/// Printable ASCII bytes as is, anything else as `?`.
impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|&b| {
            let c = if (0x20..0x7F).contains(&b) {
                char::from(b)
            } else {
                '?'
            };
            fmt::Write::write_char(f, c)
        })
    }
}

/// A glyph id. 32 bits wide so that 24-bit glyph ids (ISO OFF 5th
/// edition) need no other type.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct GlyphId(pub u32);

impl From<u16> for GlyphId {
    fn from(v: u16) -> GlyphId {
        GlyphId(u32::from(v))
    }
}

/// A 16.16 fixed-point number, as its raw value.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Fixed(pub i32);

/// A 2.14 fixed-point number, as its raw value.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct F2Dot14(pub i16);

/// Seconds since 1904-01-01 00:00 UTC.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct LongDateTime(pub i64);

/// The width of glyph and metric counts: 16 bits in `maxp`, `hhea` and
/// `hmtx`, 24 bits in `MAXP`, `HHEA` and `HMTX`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CountWidth {
    /// 16-bit counts (`maxp`, `hhea`, `hmtx`).
    U16,
    /// 24-bit counts (`MAXP`, `HHEA`, `HMTX`).
    U24,
}

impl CountWidth {
    pub(crate) const fn bytes(self) -> usize {
        match self {
            CountWidth::U16 => 2,
            CountWidth::U24 => 3,
        }
    }
}
