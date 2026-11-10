//! Comparing buffers (`hb_buffer_diff`).
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-buffer.cc`, MIT; see
//! THIRD_PARTY_NOTICES.md.

use crate::buffer::{GlyphBuffer, UnicodeBuffer};

/// How two buffers differ (`hb_buffer_diff_flags_t`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct DiffFlags(pub u32);

impl DiffFlags {
    /// Equal (`HB_BUFFER_DIFF_FLAG_EQUAL`).
    pub const EQUAL: DiffFlags = DiffFlags(0);
    /// One holds characters and the other glyphs.
    pub const CONTENT_TYPE_MISMATCH: DiffFlags = DiffFlags(0x1);
    /// The lengths differ.
    pub const LENGTH_MISMATCH: DiffFlags = DiffFlags(0x2);
    /// The reference has a `.notdef` glyph.
    pub const NOTDEF_PRESENT: DiffFlags = DiffFlags(0x4);
    /// The reference has the dotted circle glyph.
    pub const DOTTED_CIRCLE_PRESENT: DiffFlags = DiffFlags(0x8);
    /// A glyph or character differs.
    pub const CODEPOINT_MISMATCH: DiffFlags = DiffFlags(0x10);
    /// A cluster value differs.
    pub const CLUSTER_MISMATCH: DiffFlags = DiffFlags(0x20);
    /// Glyph flags differ.
    pub const GLYPH_FLAGS_MISMATCH: DiffFlags = DiffFlags(0x40);
    /// A position differs by more than the fuzz.
    pub const POSITION_MISMATCH: DiffFlags = DiffFlags(0x80);

    /// Whether every flag of `other` is set.
    pub const fn contains(self, other: DiffFlags) -> bool {
        self.0 & other.0 == other.0
    }
    /// The union.
    pub const fn union(self, other: DiffFlags) -> DiffFlags {
        DiffFlags(self.0 | other.0)
    }
}

/// The glyph ids, cluster values and flags compared.
struct Item {
    codepoint: u32,
    cluster: u32,
    flags: u32,
}

fn diff_items(
    a: &[Item],
    reference: &[Item],
    dottedcircle: u32,
) -> DiffFlags {
    let contains = dottedcircle != u32::MAX;
    let mut r = 0u32;
    let mark = |r: &mut u32, g: u32| {
        if contains && g == dottedcircle {
            *r |= DiffFlags::DOTTED_CIRCLE_PRESENT.0;
        }
        if contains && g == 0 {
            *r |= DiffFlags::NOTDEF_PRESENT.0;
        }
    };
    if a.len() != reference.len() {
        for i in reference {
            mark(&mut r, i.codepoint);
        }
        return DiffFlags(r | DiffFlags::LENGTH_MISMATCH.0);
    }
    for (x, y) in a.iter().zip(reference) {
        if x.codepoint != y.codepoint {
            r |= DiffFlags::CODEPOINT_MISMATCH.0;
        }
        if x.cluster != y.cluster {
            r |= DiffFlags::CLUSTER_MISMATCH.0;
        }
        if (x.flags ^ y.flags) & 0x7 != 0 {
            r |= DiffFlags::GLYPH_FLAGS_MISMATCH.0;
        }
        mark(&mut r, y.codepoint);
    }
    DiffFlags(r)
}

/// Compares `buffer` with `reference` as `hb_buffer_diff` does: the
/// lengths (and then which special glyphs the reference has), and glyph
/// by glyph the glyph ids, cluster values, flags, and positions with
/// differences up to `position_fuzz` allowed. `dottedcircle_glyph` is
/// the glyph of U+25CC; `u32::MAX` turns
/// [`DiffFlags::DOTTED_CIRCLE_PRESENT`] and
/// [`DiffFlags::NOTDEF_PRESENT`] off.
pub fn diff(
    buffer: &GlyphBuffer,
    reference: &GlyphBuffer,
    dottedcircle_glyph: u32,
    position_fuzz: u32,
) -> DiffFlags {
    let items = |b: &GlyphBuffer| -> alloc::vec::Vec<Item> {
        b.glyph_infos()
            .iter()
            .map(|i| Item {
                codepoint: i.glyph,
                cluster: i.cluster,
                flags: i.flags.0,
            })
            .collect()
    };
    let (a, b) = (items(buffer), items(reference));
    let mut r = diff_items(&a, &b, dottedcircle_glyph);
    if a.len() != b.len() || a.is_empty() {
        return r;
    }
    let far = |x: i32, y: i32| (i64::from(x) - i64::from(y)).unsigned_abs() > u64::from(position_fuzz);
    if buffer
        .glyph_positions()
        .iter()
        .zip(reference.glyph_positions())
        .any(|(p, q)| {
            far(p.x_advance, q.x_advance)
                || far(p.y_advance, q.y_advance)
                || far(p.x_offset, q.x_offset)
                || far(p.y_offset, q.y_offset)
        })
    {
        r = r.union(DiffFlags::POSITION_MISMATCH);
    }
    r
}

/// [`diff`] of two Unicode buffers: the lengths, characters and
/// cluster values (`hb_buffer_diff` of Unicode buffers, whose flags
/// are 0 and which have no positions).
pub fn diff_unicode(
    buffer: &UnicodeBuffer,
    reference: &UnicodeBuffer,
    dottedcircle_glyph: u32,
) -> DiffFlags {
    let items = |b: &UnicodeBuffer| -> alloc::vec::Vec<Item> {
        b.codepoints()
            .zip(b.clusters())
            .map(|(codepoint, cluster)| Item {
                codepoint,
                cluster,
                flags: 0,
            })
            .collect()
    };
    diff_items(&items(buffer), &items(reference), dottedcircle_glyph)
}
