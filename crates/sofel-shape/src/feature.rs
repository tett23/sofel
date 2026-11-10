//! Features and their string syntax (`hb_feature_t`,
//! `hb_feature_from_string`, `hb_feature_to_string`).
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-common.cc` (the `parse_*`
//! functions of `hb_feature_t`) and `src/hb-number.cc` (`hb_parse_int`,
//! `hb_parse_uint`), MIT; see THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use sofel_font::read::Tag;

/// A feature setting for a range of clusters (`hb_feature_t`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Feature {
    /// The feature tag.
    pub tag: Tag,
    /// The value: 0 disables, 1 enables, higher values pick alternates.
    pub value: u32,
    /// The first cluster the setting applies to.
    pub start: u32,
    /// The cluster after the last one it applies to; `u32::MAX` for the
    /// end of the buffer.
    pub end: u32,
}

impl Feature {
    /// The start of a feature that applies everywhere.
    pub const GLOBAL_START: u32 = 0;
    /// The end of a feature that applies everywhere.
    pub const GLOBAL_END: u32 = u32::MAX;

    /// A feature for the whole buffer.
    pub const fn new(tag: Tag, value: u32) -> Feature {
        Feature {
            tag,
            value,
            start: Feature::GLOBAL_START,
            end: Feature::GLOBAL_END,
        }
    }
    /// Whether the feature applies to the whole buffer.
    pub const fn is_global(&self) -> bool {
        self.start == Feature::GLOBAL_START && self.end == Feature::GLOBAL_END
    }

    /// Parses a feature string as `hb_feature_from_string` does: an
    /// optional `+` or `-`, the tag (one to four bytes, or exactly four
    /// in `'` or `"` quotes as CSS writes them), an optional range of
    /// clusters (`[]`, `[:]`, `[3]`, `[3:]`, `[:5]`, `[3:5]`, `;` for
    /// `:`), and an optional value (`=2`, `=on`, or without `=`: ` 2`,
    /// ` on`, ` off`), with whitespace around the parts. Shorter tags
    /// are padded with spaces. `[3]` is the range 3..4. The numbers are
    /// parsed as C `int`s and wrap: `[-1]` starts at `u32::MAX`.
    ///
    /// HarfBuzz's quirks are kept: a tag ends only at a space, `=`, `[`
    /// or the quote (a 0 byte without quotes), so `kern\t` is a five-byte
    /// tag and fails; a value after `=` must parse, but a word that is
    /// neither a number nor `on` or `off` where a value without `=` may
    /// stand is skipped (`kern xyz` is `kern`).
    ///
    /// `None` when the string does not parse (HarfBuzz returns `false`).
    pub fn parse(s: &[u8]) -> Option<Feature> {
        let mut p = Parser { s, pos: 0 };
        let mut value = if p.char(b'-') {
            0
        } else {
            p.char(b'+');
            1
        };
        let tag = p.tag()?;
        let (start, end) = p.indices()?;
        // parse_feature_value_postfix.
        let had_equal = p.char(b'=');
        let had_value = match p.int() {
            Some(v) => Some(v),
            None => p.bool(),
        };
        if let Some(v) = had_value {
            value = v;
        }
        if had_equal && had_value.is_none() {
            return None;
        }
        p.space();
        (p.pos == s.len()).then_some(Feature {
            tag,
            value,
            start,
            end,
        })
    }

    /// The string `hb_feature_to_string` writes: `-` for the value 0,
    /// the tag without its trailing spaces, the range when the feature
    /// is not global (`[3:5]`, `[3]` for 3..4, `[3:]`, `[:5]`, `[]` for
    /// 0..1), and `=value` for values above 1. The tag's bytes are
    /// written as they are.
    pub fn format(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.write(&mut out);
        out
    }

    /// [`Feature::format`], appended to `out`.
    pub fn write(&self, out: &mut Vec<u8>) {
        if self.value == 0 {
            out.push(b'-');
        }
        let tag = &self.tag.0;
        let len = tag
            .iter()
            .rposition(|&b| b != b' ')
            .map_or(0, |i| i.saturating_add(1));
        out.extend(tag.iter().take(len));
        if !self.is_global() {
            out.push(b'[');
            if self.start != 0 {
                write_u32(out, self.start);
            }
            if self.end != self.start.wrapping_add(1) {
                out.push(b':');
                if self.end != Feature::GLOBAL_END {
                    write_u32(out, self.end);
                }
            }
            out.push(b']');
        }
        if self.value > 1 {
            out.push(b'=');
            write_u32(out, self.value);
        }
    }
}

/// Appends `v` in decimal (`%u`).
pub(crate) fn write_u32(out: &mut Vec<u8>, v: u32) {
    let start = out.len();
    let mut n = v;
    loop {
        out.push(b'0'.wrapping_add(u8::try_from(n % 10).unwrap_or(0)));
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if let Some(digits) = out.get_mut(start..) {
        digits.reverse();
    }
}

/// Appends `v` in decimal (`%d`).
pub(crate) fn write_i32(out: &mut Vec<u8>, v: i32) {
    if v < 0 {
        out.push(b'-');
    }
    write_u32(out, v.unsigned_abs());
}

/// `ISSPACE` of `src/hb-algs.hh`.
pub(crate) const fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\x0C' | b'\n' | b'\r' | b'\t' | b'\x0B')
}

/// The digit value of `c` in `base`, as `_hb_digit_value`.
fn digit_value(c: u8, base: u32) -> Option<u32> {
    let d = match c {
        b'0'..=b'9' => c.wrapping_sub(b'0'),
        b'a'..=b'z' => c.wrapping_sub(b'a').wrapping_add(10),
        b'A'..=b'Z' => c.wrapping_sub(b'A').wrapping_add(10),
        _ => return None,
    };
    let d = u32::from(d);
    (d < base).then_some(d)
}

/// `_parse_number` of `src/hb-number.cc` before its postprocessing:
/// whitespace, a sign, for base 16 an optional `0x` before a hex digit,
/// then digits. The sign, the magnitude and the bytes consumed; `None`
/// without digits or when the magnitude passes `u64::MAX`.
fn parse_number(s: &[u8], base: u32) -> Option<(bool, u64, usize)> {
    let at = |i: usize| s.get(i).copied();
    let mut p = s.iter().take_while(|&&b| is_space(b)).count();
    let mut neg = false;
    if let Some(c @ (b'+' | b'-')) = at(p) {
        neg = c == b'-';
        p = p.saturating_add(1);
    }
    if base == 16
        && p.saturating_add(2) < s.len()
        && at(p) == Some(b'0')
        && matches!(at(p.saturating_add(1)), Some(b'x' | b'X'))
        && at(p.saturating_add(2))
            .and_then(|c| digit_value(c, 16))
            .is_some()
    {
        p = p.saturating_add(2);
    }
    let digits = s.get(p..).unwrap_or_default();
    let n = digits
        .iter()
        .take_while(|&&c| digit_value(c, base).is_some())
        .count();
    if n == 0 {
        return None;
    }
    let v = digits.iter().take(n).try_fold(0u64, |v, &c| {
        v.checked_mul(u64::from(base))?
            .checked_add(u64::from(digit_value(c, base)?))
    })?;
    Some((neg, v, p.saturating_add(n)))
}

/// `hb_parse_int`: a C `int`, with the bytes consumed.
pub(crate) fn parse_int(s: &[u8]) -> Option<(i32, usize)> {
    let (neg, v, n) = parse_number(s, 10)?;
    let v = i64::try_from(v).ok()?;
    let v = if neg { v.checked_neg()? } else { v };
    Some((i32::try_from(v).ok()?, n))
}

/// `hb_parse_uint`: a C `unsigned` in `base`, negated in unsigned
/// arithmetic after a `-`, with the bytes consumed.
pub(crate) fn parse_uint(s: &[u8], base: u32) -> Option<(u32, usize)> {
    let (neg, v, n) = parse_number(s, base)?;
    let u = u32::try_from(v).ok()?;
    Some((if neg { 0u32.wrapping_sub(u) } else { u }, n))
}

/// `hb_tag_from_string`: up to four bytes, ended early by a 0 byte,
/// padded with spaces; the tag 0 when the string is empty or starts
/// with a 0 byte.
pub(crate) fn tag_from_bytes(s: &[u8]) -> Tag {
    if s.first().is_none_or(|&b| b == 0) {
        return Tag::from_u32(0);
    }
    let mut tag = [b' '; 4];
    s.iter()
        .take(4)
        .take_while(|&&b| b != 0)
        .zip(tag.iter_mut())
        .for_each(|(&b, t)| *t = b);
    Tag(tag)
}

/// The cursor of the `parse_*` functions of `hb_feature_t`.
struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }
    fn rest(&self) -> &[u8] {
        self.s.get(self.pos..).unwrap_or_default()
    }
    fn bump(&mut self, n: usize) {
        self.pos = self.pos.saturating_add(n);
    }

    /// `parse_space`.
    fn space(&mut self) {
        let n = self.rest().iter().take_while(|&&b| is_space(b)).count();
        self.bump(n);
    }

    /// `parse_char`: whitespace, then `c`.
    fn char(&mut self, c: u8) -> bool {
        self.space();
        if self.peek() == Some(c) {
            self.bump(1);
            true
        } else {
            false
        }
    }

    /// `parse_uint` and `parse_uint32`: `hb_parse_int`, as unsigned.
    fn int(&mut self) -> Option<u32> {
        let (v, n) = parse_int(self.rest())?;
        self.bump(n);
        Some(v.cast_unsigned())
    }

    /// `parse_bool`: `on` or `off` in any case. The letters are consumed
    /// even when they are neither.
    fn bool(&mut self) -> Option<u32> {
        self.space();
        let word: Vec<u8> = self
            .rest()
            .iter()
            .take_while(|b| b.is_ascii_alphabetic())
            .map(u8::to_ascii_lowercase)
            .collect();
        self.bump(word.len());
        match word.as_slice() {
            b"on" => Some(1),
            b"off" => Some(0),
            _ => None,
        }
    }

    /// `parse_tag`.
    fn tag(&mut self) -> Option<Tag> {
        self.space();
        let quote = match self.peek() {
            Some(q @ (b'\'' | b'"')) => {
                self.bump(1);
                q
            }
            _ => 0,
        };
        let start = self.pos;
        let n = self
            .rest()
            .iter()
            .take_while(|&&b| b != b' ' && b != b'=' && b != b'[' && b != quote)
            .count();
        self.bump(n);
        if n == 0 || n > 4 {
            return None;
        }
        let tag = tag_from_bytes(self.s.get(start..self.pos).unwrap_or_default());
        if quote != 0 {
            // CSS wants exactly four bytes, and the closing quote.
            if n != 4 || self.peek() != Some(quote) {
                return None;
            }
            self.bump(1);
        }
        Some(tag)
    }

    /// `parse_feature_indices`: the range, global without brackets.
    fn indices(&mut self) -> Option<(u32, u32)> {
        self.space();
        let mut start = Feature::GLOBAL_START;
        let mut end = Feature::GLOBAL_END;
        if !self.char(b'[') {
            return Some((start, end));
        }
        let has_start = match self.int() {
            Some(v) => {
                start = v;
                true
            }
            None => false,
        };
        if self.char(b':') || self.char(b';') {
            if let Some(v) = self.int() {
                end = v;
            }
        } else if has_start {
            end = start.wrapping_add(1);
        }
        self.char(b']').then_some((start, end))
    }
}
