//! Decoding text as HarfBuzz's `hb_buffer_add_utf*` does
//! (`src/hb-utf.hh`): one replacement character for each ill-formed
//! code unit (UTF-8 and UTF-16) or value (UTF-32), forward and backward.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-utf.hh`, MIT; see
//! THIRD_PARTY_NOTICES.md.

/// A text encoding: how to read one character forward or backward.
pub(crate) trait Utf {
    /// A code unit.
    type Unit: Copy;
    /// The character at the start of `text` and the number of units it
    /// takes (at least 1); `None` for empty text.
    fn next(text: &[Self::Unit], replacement: u32) -> Option<(u32, usize)>;
    /// The character at the end of `text` and the number of units it
    /// takes (at least 1); `None` for empty text.
    fn prev(text: &[Self::Unit], replacement: u32) -> Option<(u32, usize)>;
}

/// UTF-8 (`hb_utf8_t`).
pub(crate) struct Utf8;
/// UTF-16 in native order (`hb_utf16_t`).
pub(crate) struct Utf16;
/// UTF-32, with surrogates and values past U+10FFFF replaced
/// (`hb_utf32_t`).
pub(crate) struct Utf32;
/// UTF-32 taken as it is (`hb_utf32_novalidate_t`).
pub(crate) struct Codepoints;
/// Latin-1 (`hb_latin1_t`).
pub(crate) struct Latin1;

/// `t - 0x80` when it is a continuation byte.
fn cont(t: Option<&u8>) -> Option<u32> {
    let t = u32::from(*t?).wrapping_sub(0x80);
    (t <= 0x3F).then_some(t)
}

impl Utf for Utf8 {
    type Unit = u8;
    fn next(text: &[u8], replacement: u32) -> Option<(u32, usize)> {
        let c = u32::from(*text.first()?);
        if c <= 0x7F {
            return Some((c, 1));
        }
        let decoded = match c {
            0xC2..=0xDF => cont(text.get(1)).map(|t1| (((c & 0x1F) << 6) | t1, 2)),
            0xE0..=0xEF => match (cont(text.get(1)), cont(text.get(2))) {
                (Some(t1), Some(t2)) => {
                    let u = ((c & 0xF) << 12) | (t1 << 6) | t2;
                    (u >= 0x800 && !(0xD800..=0xDFFF).contains(&u)).then_some((u, 3))
                }
                _ => None,
            },
            0xF0..=0xF4 => match (cont(text.get(1)), cont(text.get(2)), cont(text.get(3))) {
                (Some(t1), Some(t2), Some(t3)) => {
                    let u = ((c & 0x7) << 18) | (t1 << 12) | (t2 << 6) | t3;
                    (0x10000..=0x10FFFF).contains(&u).then_some((u, 4))
                }
                _ => None,
            },
            _ => None,
        };
        Some(decoded.unwrap_or((replacement, 1)))
    }
    fn prev(text: &[u8], replacement: u32) -> Option<(u32, usize)> {
        let end = text.len();
        let mut start = end.checked_sub(1)?;
        while start > 0
            && text.get(start).is_some_and(|&b| b & 0xC0 == 0x80)
            && end.wrapping_sub(start) < 4
        {
            start = start.wrapping_sub(1);
        }
        let tail = text.get(start..).unwrap_or_default();
        match Utf8::next(tail, replacement) {
            Some((u, n)) if n == tail.len() => Some((u, n)),
            _ => Some((replacement, 1)),
        }
    }
}

impl Utf for Utf16 {
    type Unit = u16;
    fn next(text: &[u16], replacement: u32) -> Option<(u32, usize)> {
        let c = u32::from(*text.first()?);
        if !(0xD800..=0xDFFF).contains(&c) {
            return Some((c, 1));
        }
        if c <= 0xDBFF
            && let Some(&l) = text.get(1)
            && (0xDC00..=0xDFFF).contains(&l)
        {
            let u = ((c & 0x3FF) << 10 | (u32::from(l) & 0x3FF)).wrapping_add(0x10000);
            return Some((u, 2));
        }
        Some((replacement, 1))
    }
    fn prev(text: &[u16], replacement: u32) -> Option<(u32, usize)> {
        let (&last, rest) = text.split_last()?;
        let c = u32::from(last);
        if !(0xD800..=0xDFFF).contains(&c) {
            return Some((c, 1));
        }
        if c >= 0xDC00
            && let Some(&h) = rest.last()
            && (0xD800..=0xDBFF).contains(&h)
        {
            let u = ((u32::from(h) & 0x3FF) << 10 | (c & 0x3FF)).wrapping_add(0x10000);
            return Some((u, 2));
        }
        Some((replacement, 1))
    }
}

/// `hb_utf32_t`'s check.
fn valid_utf32(c: u32, replacement: u32) -> u32 {
    if c >= 0xD800 && (c <= 0xDFFF || c > 0x10FFFF) {
        replacement
    } else {
        c
    }
}

impl Utf for Utf32 {
    type Unit = u32;
    fn next(text: &[u32], replacement: u32) -> Option<(u32, usize)> {
        text.first().map(|&c| (valid_utf32(c, replacement), 1))
    }
    fn prev(text: &[u32], replacement: u32) -> Option<(u32, usize)> {
        text.last().map(|&c| (valid_utf32(c, replacement), 1))
    }
}

impl Utf for Codepoints {
    type Unit = u32;
    fn next(text: &[u32], _: u32) -> Option<(u32, usize)> {
        text.first().map(|&c| (c, 1))
    }
    fn prev(text: &[u32], _: u32) -> Option<(u32, usize)> {
        text.last().map(|&c| (c, 1))
    }
}

impl Utf for Latin1 {
    type Unit = u8;
    fn next(text: &[u8], _: u32) -> Option<(u32, usize)> {
        text.first().map(|&c| (u32::from(c), 1))
    }
    fn prev(text: &[u8], _: u32) -> Option<(u32, usize)> {
        text.last().map(|&c| (u32::from(c), 1))
    }
}
