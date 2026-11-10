//! The per-glyph output of shaping.

/// Flags of an output glyph (`hb_glyph_flags_t`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct GlyphFlags(pub u32);

impl GlyphFlags {
    /// Breaking the text before this glyph's cluster and shaping the
    /// two sides separately may give a different result
    /// (`HB_GLYPH_FLAG_UNSAFE_TO_BREAK`).
    pub const UNSAFE_TO_BREAK: GlyphFlags = GlyphFlags(0x1);
    /// Concatenating separately shaped text at this point may give a
    /// different result (`HB_GLYPH_FLAG_UNSAFE_TO_CONCAT`).
    pub const UNSAFE_TO_CONCAT: GlyphFlags = GlyphFlags(0x2);
    /// A tatweel may be inserted before this cluster for elongation
    /// (`HB_GLYPH_FLAG_SAFE_TO_INSERT_TATWEEL`).
    pub const SAFE_TO_INSERT_TATWEEL: GlyphFlags = GlyphFlags(0x4);
    /// All defined flags (`HB_GLYPH_FLAG_DEFINED`).
    pub const DEFINED: GlyphFlags = GlyphFlags(0x7);

    /// Whether every flag of `other` is set.
    pub const fn contains(self, other: GlyphFlags) -> bool {
        self.0 & other.0 == other.0
    }
    /// The union.
    pub const fn union(self, other: GlyphFlags) -> GlyphFlags {
        GlyphFlags(self.0 | other.0)
    }
}

/// A shaped glyph: the glyph id, the cluster value of the characters it
/// came from and its flags.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct GlyphInfo {
    /// The glyph id.
    pub glyph: u32,
    /// The cluster value.
    pub cluster: u32,
    /// The flags.
    pub flags: GlyphFlags,
}

/// The position of a shaped glyph, in scaled units with HarfBuzz's signs
/// (vertical advances are negative downward).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct GlyphPosition {
    /// How far the pen moves horizontally after the glyph.
    pub x_advance: i32,
    /// How far the pen moves vertically after the glyph.
    pub y_advance: i32,
    /// The horizontal offset of the glyph from the pen.
    pub x_offset: i32,
    /// The vertical offset of the glyph from the pen.
    pub y_offset: i32,
}
