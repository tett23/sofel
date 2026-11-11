//! What the syllabic shapers share (ADR 0024, indic、khmer、myanmar): the
//! dotted circle inserted at the start of each broken syllable, and the
//! pause after which the syllables are no longer needed.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shaper-syllabic.cc`, MIT; see
//! THIRD_PARTY_NOTICES.md.

use crate::buffer::{Buffer, BufferFlags, Info, scratch};
use crate::font::ShapeFont;
use crate::plan::ShapePlan;

/// U+25CC DOTTED CIRCLE.
pub(crate) const DOTTED_CIRCLE: u32 = 0x25CC;

/// `hb_syllabic_insert_dotted_circles`: inserts the font's dotted circle
/// glyph at the start of every syllable whose type (the low four bits of
/// `syllable`) is `broken_syllable_type`, after the items of category
/// `repha_category` that start it. The dotted circle takes the cluster,
/// mask and syllable of the item it is put before, the category
/// `dotted_circle_category`, and the position `dotted_circle_position`
/// (in `shaper_position`) when given.
///
/// Nothing is done with [`BufferFlags::DO_NOT_INSERT_DOTTED_CIRCLE`],
/// without a broken syllable (`HAS_BROKEN_SYLLABLE`, which the syllable
/// machine's caller sets) or without a dotted circle in the font.
/// Returns whether the buffer was rewritten.
pub(crate) fn insert_dotted_circles(
    font: &ShapeFont<'_, '_, '_, '_>,
    buffer: &mut Buffer,
    broken_syllable_type: u8,
    dotted_circle_category: u8,
    repha_category: Option<u8>,
    dotted_circle_position: Option<u8>,
) -> bool {
    if buffer
        .flags
        .contains(BufferFlags::DO_NOT_INSERT_DOTTED_CIRCLE)
    {
        return false;
    }
    if buffer.scratch_flags & scratch::HAS_BROKEN_SYLLABLE == 0 {
        return false;
    }
    let Some(glyph) = font.nominal_glyph(DOTTED_CIRCLE) else {
        return false;
    };
    insert_dotted_circle_glyphs(
        buffer,
        glyph,
        broken_syllable_type,
        dotted_circle_category,
        repha_category,
        dotted_circle_position,
    );
    true
}

/// The insertion of [`insert_dotted_circles`] with the dotted circle's
/// glyph.
fn insert_dotted_circle_glyphs(
    buffer: &mut Buffer,
    glyph: u32,
    broken_syllable_type: u8,
    dotted_circle_category: u8,
    repha_category: Option<u8>,
    dotted_circle_position: Option<u8>,
) {
    let dotted_circle = Info {
        codepoint: glyph,
        shaper_category: dotted_circle_category,
        shaper_position: dotted_circle_position.unwrap_or(0),
        ..Info::default()
    };

    buffer.clear_output();
    buffer.idx = 0;
    let mut last_syllable = 0u8;
    while buffer.idx < buffer.len && buffer.successful {
        let cur = *buffer.cur(0);
        if last_syllable != cur.syllable && cur.syllable & 0x0F == broken_syllable_type {
            last_syllable = cur.syllable;
            let info = Info {
                cluster: cur.cluster,
                mask: cur.mask,
                syllable: cur.syllable,
                ..dotted_circle
            };
            if let Some(repha) = repha_category {
                while buffer.idx < buffer.len
                    && buffer.successful
                    && last_syllable == buffer.cur(0).syllable
                    && buffer.cur(0).shaper_category == repha
                {
                    let _ = buffer.next_glyph();
                }
            }
            let _ = buffer.output_info(info);
        } else {
            let _ = buffer.next_glyph();
        }
    }
    buffer.sync();
}

/// `hb_syllabic_clear_var`: the pause after which the syllables are no
/// longer needed. HarfBuzz frees the buffer variable; the named field
/// keeps its values, which the later per-syllable lookups still see, as
/// in HarfBuzz.
pub(crate) fn clear_syllables(
    _: &ShapePlan<'_>,
    _: &ShapeFont<'_, '_, '_, '_>,
    _: &mut Buffer,
) -> bool {
    false
}

/// The `(start, end)` of each syllable (`foreach_syllable`), found as
/// the loop goes: `f` may reorder items within the syllable it is given
/// but must leave their `syllable` values as they were.
pub(crate) fn for_each_syllable(buffer: &mut Buffer, mut f: impl FnMut(&mut Buffer, usize, usize)) {
    let count = buffer.len;
    let mut start = 0;
    let mut end = if count > 0 {
        buffer.next_syllable(0)
    } else {
        0
    };
    while start < count {
        f(buffer, start, end);
        start = end;
        end = buffer.next_syllable(start);
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::cast_possible_truncation)]
mod tests {
    use alloc::vec::Vec;

    use super::insert_dotted_circle_glyphs;
    use crate::buffer::Buffer;

    /// A buffer of `(glyph, category, syllable)` items, cluster `i`.
    fn buffer(items: &[(u32, u8, u8)]) -> Buffer {
        let mut b = Buffer::new();
        for (i, &(g, c, s)) in items.iter().enumerate() {
            b.add(g, i as u32);
            let info = &mut b.info[i];
            info.shaper_category = c;
            info.syllable = s;
        }
        b
    }

    fn items(b: &Buffer) -> Vec<(u32, u8, u8, u32, u8)> {
        b.infos()
            .iter()
            .map(|i| {
                (
                    i.codepoint,
                    i.shaper_category,
                    i.syllable,
                    i.cluster,
                    i.shaper_position,
                )
            })
            .collect()
    }

    #[test]
    fn dotted_circle_starts_each_broken_syllable_after_repha() {
        // Syllable 1 (type 0), syllable 2 (broken, type 4: a repha, 14,
        // then a mark), syllable 3 (broken: two marks, one circle).
        let mut b = buffer(&[
            (10, 1, 0x10),
            (11, 14, 0x24),
            (12, 7, 0x24),
            (13, 7, 0x34),
            (14, 7, 0x34),
        ]);
        insert_dotted_circle_glyphs(&mut b, 99, 4, 11, Some(14), Some(14));
        assert_eq!(
            items(&b),
            [
                (10, 1, 0x10, 0, 0),
                (11, 14, 0x24, 1, 0),
                (99, 11, 0x24, 1, 14),
                (12, 7, 0x24, 2, 0),
                (99, 11, 0x34, 3, 14),
                (13, 7, 0x34, 3, 0),
                (14, 7, 0x34, 4, 0),
            ]
        );
        // Without a repha category the circle comes first; without a
        // position it is 0.
        let mut b = buffer(&[(11, 14, 0x14), (12, 7, 0x14)]);
        insert_dotted_circle_glyphs(&mut b, 99, 4, 11, None, None);
        assert_eq!(
            items(&b),
            [
                (99, 11, 0x14, 0, 0),
                (11, 14, 0x14, 0, 0),
                (12, 7, 0x14, 1, 0)
            ]
        );
    }
}
