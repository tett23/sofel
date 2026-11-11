//! The forbidden vowel sequences (ADR 0024, indic、khmer、myanmar): a
//! dotted circle is put between the characters of a sequence that would
//! look like another vowel, with the data of
//! [`crate::tables::vowel_constraints`].
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shaper-vowel-constraints.cc`
//! (`_hb_preprocess_text_vowel_constraints`), MIT; see
//! THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use super::syllabic::DOTTED_CIRCLE;
use crate::buffer::{Buffer, BufferFlags};
use crate::font::ShapeFont;
use crate::plan::ShapePlan;
use crate::tables::vowel_constraints::{VOWEL_CONSTRAINTS, vowel_constraint_match};

/// `_output_with_dotted_circle`: a dotted circle (a copy of the current
/// item with U+25CC, not a continuation), then the current item.
fn output_with_dotted_circle(buffer: &mut Buffer) {
    let _ = buffer.output_glyph(DOTTED_CIRCLE);
    buffer.prev_mut().clear_continuation();
    let _ = buffer.next_glyph();
}

/// `_hb_preprocess_text_vowel_constraints`: for the buffer's script,
/// puts a dotted circle before the last character of each forbidden
/// sequence. Nothing is done with
/// [`BufferFlags::DO_NOT_INSERT_DOTTED_CIRCLE`].
pub(crate) fn preprocess_vowel_constraints(buffer: &mut Buffer) {
    if buffer
        .flags
        .contains(BufferFlags::DO_NOT_INSERT_DOTTED_CIRCLE)
    {
        return;
    }
    let Some(script) = buffer.script.map(|s| s.tag()) else {
        return;
    };
    if !VOWEL_CONSTRAINTS.iter().any(|(s, _)| *s == script) {
        return;
    }
    // The items at and after `idx` are not written while the loop runs,
    // so their characters can be read once.
    let cps: Vec<u32> = buffer.infos().iter().map(|i| i.codepoint).collect();
    buffer.clear_output();
    let count = buffer.len;
    buffer.idx = 0;
    while buffer.idx.saturating_add(1) < count && buffer.successful {
        match vowel_constraint_match(script, &cps, buffer.idx) {
            Some(n) => {
                for _ in 1..n {
                    let _ = buffer.next_glyph();
                }
                output_with_dotted_circle(buffer);
            }
            None => {
                let _ = buffer.next_glyph();
            }
        }
    }
    buffer.sync();
}

/// [`preprocess_vowel_constraints`] as a shaper's `preprocess_text`.
pub(crate) fn preprocess_text(
    _: &ShapePlan<'_>,
    buffer: &mut Buffer,
    _: &ShapeFont<'_, '_, '_, '_>,
) {
    preprocess_vowel_constraints(buffer);
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::cast_possible_truncation)]
mod tests {
    use alloc::vec::Vec;

    use super::preprocess_vowel_constraints;
    use crate::buffer::{Buffer, BufferFlags, uprops};
    use sofel_unicode::Script;

    fn run(script: &[u8; 4], text: &[u32], flags: BufferFlags) -> Buffer {
        let mut b = Buffer::new();
        b.script = Script::from_iso15924(script);
        b.flags = flags;
        for (i, &c) in text.iter().enumerate() {
            b.add(c, i as u32);
            b.info[i].unicode_props = uprops::CONTINUATION;
        }
        preprocess_vowel_constraints(&mut b);
        b
    }

    fn cps(b: &Buffer) -> Vec<u32> {
        b.infos().iter().map(|i| i.codepoint).collect()
    }

    #[test]
    fn dotted_circle_between_vowel_parts() {
        // A, AA and RA, VIRAMA, I; KA stays.
        let b = run(
            b"Deva",
            &[0x905, 0x93E, 0x915, 0x930, 0x94D, 0x907],
            BufferFlags::DEFAULT,
        );
        assert_eq!(
            cps(&b),
            [0x905, 0x25CC, 0x93E, 0x915, 0x930, 0x94D, 0x25CC, 0x907]
        );
        // The dotted circle is a copy of the character after it, not a
        // continuation.
        let circle = b.infos()[1];
        assert_eq!(circle.cluster, 1);
        assert!(!circle.is_continuation());
        assert!(b.infos()[2].is_continuation());
    }

    #[test]
    fn nothing_for_other_scripts_or_with_the_flag() {
        let text = [0x905, 0x93E];
        assert_eq!(cps(&run(b"Latn", &text, BufferFlags::DEFAULT)), text);
        assert_eq!(
            cps(&run(
                b"Deva",
                &text,
                BufferFlags::DO_NOT_INSERT_DOTTED_CIRCLE
            )),
            text
        );
        // A sequence of three needs its third character.
        assert_eq!(
            cps(&run(b"Deva", &[0x930, 0x94D], BufferFlags::DEFAULT)),
            [0x930, 0x94D]
        );
    }
}
