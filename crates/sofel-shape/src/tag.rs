//! OpenType script and language system tags (`src/hb-ot-tag.cc`).
//!
//! Both directions of HarfBuzz 15.1.0's conversion between a script and
//! a BCP 47 language on one side and the tags of the ScriptList and
//! LangSys records of GSUB and GPOS on the other. The language tables
//! and the two generated functions of `src/hb-ot-tag-table.hh` are in
//! [`crate::tag_table`], written by `tools/gen-ot-tag/`; this module is
//! a port of the hand-written rest of `src/hb-ot-tag.cc` (HarfBuzz,
//! MIT; see THIRD_PARTY_NOTICES.md).
//!
//! HarfBuzz walks NUL-terminated C strings with pointer arithmetic; the
//! port keeps those semantics by reading a language as a byte string
//! whose bytes past the end are 0, and positions as offsets in it, so
//! that every edge case (an empty subtag, a private-use part without a
//! language, a `limit` that is a null pointer) gives the same result.

use alloc::vec::Vec;

use sofel_font::read::Tag;

use crate::tag_table::{
    AMBIGUOUS, COMPLEX, COMPLEX_SUBTAGS, LANGUAGES2, LANGUAGES3, LANGUAGES3_BLOCKED,
    LANGUAGES3_MULTI, LANGUAGES3_MULTI_VALUES,
};
use crate::{Language, Script};

/// `HB_OT_TAG_DEFAULT_SCRIPT` (`DFLT`).
pub const OT_TAG_DEFAULT_SCRIPT: Tag = Tag(*b"DFLT");
/// `HB_OT_TAG_DEFAULT_LANGUAGE` (`dflt`).
pub const OT_TAG_DEFAULT_LANGUAGE: Tag = Tag(*b"dflt");
/// `HB_OT_MAX_TAGS_PER_SCRIPT`: the number of script tags HarfBuzz's
/// shaper asks for.
pub const MAX_TAGS_PER_SCRIPT: usize = 3;
/// `HB_OT_MAX_TAGS_PER_LANGUAGE`: the number of language tags
/// HarfBuzz's shaper asks for.
pub const MAX_TAGS_PER_LANGUAGE: usize = 3;

/// `HB_TAG` of four bytes.
pub(crate) const fn t(b: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*b)
}

/// The condition of a branch of the `switch (lang_str[0])` of
/// `hb_ot_tags_from_complex_language`; strings are compared from the
/// second byte of the language.
pub(crate) enum Cond {
    /// `lang_matches (&lang_str[1], limit, s, len)`.
    LangMatches(&'static [u8]),
    /// `0 == strncmp (&lang_str[1], prefix, len) && subtag_matches
    /// (lang_str, limit, subtag, len)`.
    PrefixSubtag(&'static [u8], &'static [u8]),
    /// `0 == strcmp (&lang_str[1], s)`.
    Exact(&'static [u8]),
}

/// One branch of that switch: its `case` byte, condition and tags.
pub(crate) struct Branch {
    pub(crate) first: u8,
    pub(crate) cond: Cond,
    pub(crate) tags: &'static [u32],
}

/// `hb_script_t` values are tags; 0 is `HB_SCRIPT_INVALID`.
const SCRIPT_INVALID: u32 = 0;
const SCRIPT_UNKNOWN: u32 = t(b"Zzzz");
const SCRIPT_MATH: u32 = t(b"Zmth");
const DFLT: u32 = t(b"DFLT");
const MATH: u32 = t(b"math");
const KANA: u32 = t(b"kana");
const MYM2: u32 = t(b"mym2");

/// `hb_ot_old_tag_from_script`.
fn old_tag_from_script(script: u32) -> u32 {
    match &script.to_be_bytes() {
        [0, 0, 0, 0] => DFLT,
        b"Zmth" => MATH,
        b"Hira" | b"Hrkt" => KANA,
        b"Laoo" => t(b"lao "),
        b"Yiii" => t(b"yi  "),
        b"Nkoo" => t(b"nko "),
        b"Vaii" => t(b"vai "),
        _ => script | 0x2000_0000,
    }
}

/// `hb_ot_old_tag_to_script`.
fn old_tag_to_script(tag: u32) -> u32 {
    if tag == DFLT {
        return SCRIPT_INVALID;
    }
    if tag == MATH {
        return SCRIPT_MATH;
    }
    // Spaces at the end repeat the letter before them: 'nko ' -> 'Nkoo'.
    let tag = if tag & 0x0000_FF00 == 0x0000_2000 {
        tag | ((tag >> 8) & 0x0000_FF00)
    } else {
        tag
    };
    let tag = if tag & 0x0000_00FF == 0x0000_0020 {
        tag | ((tag >> 8) & 0x0000_00FF)
    } else {
        tag
    };
    tag & !0x2000_0000
}

/// The scripts with a second-generation Indic tag, and that tag.
const NEW_TAGS: [(&[u8; 4], &[u8; 4]); 10] = [
    (b"Beng", b"bng2"),
    (b"Deva", b"dev2"),
    (b"Gujr", b"gjr2"),
    (b"Guru", b"gur2"),
    (b"Knda", b"knd2"),
    (b"Mlym", b"mlm2"),
    (b"Orya", b"ory2"),
    (b"Taml", b"tml2"),
    (b"Telu", b"tel2"),
    (b"Mymr", b"mym2"),
];

/// `hb_ot_new_tag_from_script`: `DFLT` for a script without one.
fn new_tag_from_script(script: u32) -> u32 {
    NEW_TAGS
        .iter()
        .find(|(s, _)| t(s) == script)
        .map_or(DFLT, |(_, tag)| t(tag))
}

/// `hb_ot_new_tag_to_script`.
fn new_tag_to_script(tag: u32) -> u32 {
    NEW_TAGS
        .iter()
        .find(|(_, x)| t(x) == tag)
        .map_or(SCRIPT_UNKNOWN, |(s, _)| t(s))
}

/// `hb_ot_all_tags_from_script` with room for `count` (at least 1)
/// tags: the third-generation tag (none for Myanmar), the
/// second-generation tag, then the old tag.
fn all_tags_from_script(script: u32, count: usize) -> Vec<u32> {
    let mut tags = Vec::new();
    let new_tag = new_tag_from_script(script);
    if new_tag != DFLT {
        if new_tag != MYM2 {
            tags.push(new_tag | u32::from(b'3'));
        }
        if count > tags.len() {
            tags.push(new_tag);
        }
    }
    if count > tags.len() {
        let old_tag = old_tag_from_script(script);
        if old_tag != DFLT {
            tags.push(old_tag);
        }
    }
    tags
}

/// `hb_ot_tag_to_script`, as an `hb_script_t` value.
fn tag_to_script(tag: u32) -> u32 {
    let digit = tag & 0xFF;
    if digit == u32::from(b'2') || digit == u32::from(b'3') {
        new_tag_to_script(tag & 0xFFFF_FF32)
    } else {
        old_tag_to_script(tag)
    }
}

/// An `hb_script_t` value as a [`Script`], preserving all nonzero bytes.
fn script_from_raw(raw: u32) -> Option<Script> {
    if raw == SCRIPT_INVALID {
        return None;
    }
    Some(Script::from_raw_tag(raw.to_be_bytes()))
}

/// The byte at `i`, 0 past the end (the NUL of a C string and what
/// follows it).
fn at(s: &[u8], i: usize) -> u8 {
    s.get(i).copied().unwrap_or(0)
}

/// `strchr (s + from, c)`.
fn strchr(s: &[u8], from: usize, c: u8) -> Option<usize> {
    s.get(from..)?
        .iter()
        .position(|&x| x == c)
        .map(|p| p.saturating_add(from))
}

/// `strstr (s + from, needle)`.
fn strstr(s: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    let rest = s.get(from..)?;
    (0..=rest.len().checked_sub(needle.len())?)
        .find(|&i| rest.get(i..).is_some_and(|r| r.starts_with(needle)))
        .map(|p| p.saturating_add(from))
}

/// `strlen (s + from)`.
fn strlen(s: &[u8], from: usize) -> usize {
    s.len().saturating_sub(from)
}

/// `0 == strncmp (s + from, spec, n)`.
fn strncmp_eq(s: &[u8], from: usize, spec: &[u8], n: usize) -> bool {
    (0..n)
        .map(|k| (at(s, from.saturating_add(k)), at(spec, k)))
        .find(|&(a, b)| a != b || a == 0)
        .is_none_or(|(a, b)| a == b)
}

/// `0 == strcmp (s + from, spec)`.
fn strcmp_eq(s: &[u8], from: usize, spec: &[u8]) -> bool {
    s.get(from..).unwrap_or(&[]) == spec
}

/// `limit - from` as a signed difference: `None` when `limit` is the
/// null pointer (then it is below every position).
fn diff(limit: Option<usize>, from: usize) -> Option<isize> {
    let l = isize::try_from(limit?).ok()?;
    let f = isize::try_from(from).ok()?;
    l.checked_sub(f)
}

/// `limit - from >= k` (signed).
fn span_at_least(limit: Option<usize>, from: usize, k: isize) -> bool {
    diff(limit, from).is_some_and(|d| d >= k)
}

/// `(unsigned) (limit - from) < n`: a negative difference is a large
/// unsigned value.
fn span_below(limit: Option<usize>, from: usize, n: usize) -> bool {
    diff(limit, from)
        .and_then(|d| usize::try_from(d).ok())
        .is_some_and(|d| d < n)
}

/// `pos >= limit` for pointers; every position is above a null
/// `limit`.
fn at_or_after(pos: usize, limit: Option<usize>) -> bool {
    limit.is_none_or(|l| pos >= l)
}

/// `subtag_matches`: `subtag` occurs in `s` from `from`, starts before
/// `limit` and is not followed by a letter or digit.
fn subtag_matches(s: &[u8], from: usize, limit: Option<usize>, subtag: &[u8]) -> bool {
    if span_below(limit, from, subtag.len()) {
        return false;
    }
    let mut from = from;
    loop {
        let Some(p) = strstr(s, from, subtag) else {
            return false;
        };
        if at_or_after(p, limit) {
            return false;
        }
        let end = p.saturating_add(subtag.len());
        if !at(s, end).is_ascii_alphanumeric() {
            return true;
        }
        from = end;
    }
}

/// `lang_matches`: `spec` is `s` from `from`, or a prefix of it ending
/// at a `-`.
fn lang_matches(s: &[u8], from: usize, limit: Option<usize>, spec: &[u8]) -> bool {
    if span_below(limit, from, spec.len()) {
        return false;
    }
    let next = at(s, from.saturating_add(spec.len()));
    strncmp_eq(s, from, spec, spec.len()) && (next == 0 || next == b'-')
}

/// The first `count` tags of `tags`.
fn first_tags(tags: &[u32], count: usize) -> Vec<u32> {
    tags.iter().take(count).copied().collect()
}

/// `hb_ot_tags_from_complex_language`: `None` when no branch matches.
fn tags_from_complex_language(s: &[u8], limit: Option<usize>, count: usize) -> Option<Vec<u32>> {
    if span_at_least(limit, 0, 7) {
        let p =
            strchr(s, 0, b'-').filter(|&p| !at_or_after(p, limit) && span_at_least(limit, p, 5));
        if let Some(p) = p {
            let hit = COMPLEX_SUBTAGS
                .iter()
                .find(|(sub, _)| subtag_matches(s, p, limit, sub));
            if let Some((_, tags)) = hit {
                return Some(first_tags(tags, count));
            }
        }
    }
    let first = at(s, 0);
    COMPLEX
        .iter()
        .filter(|b| b.first == first)
        .find(|b| match b.cond {
            Cond::LangMatches(spec) => lang_matches(s, 1, limit, spec),
            Cond::PrefixSubtag(prefix, subtag) => {
                strncmp_eq(s, 1, prefix, prefix.len()) && subtag_matches(s, 0, limit, subtag)
            }
            Cond::Exact(spec) => strcmp_eq(s, 1, spec),
        })
        .map(|b| first_tags(b.tags, count))
}

/// `hb_tag_from_string (s + from, len)`.
fn tag_from_string(s: &[u8], from: usize, len: usize) -> u32 {
    if len == 0 || at(s, from) == 0 {
        return 0;
    }
    let len = len.min(4);
    let mut tag = [b' '; 4];
    tag.iter_mut()
        .enumerate()
        .take(len)
        .map(|(i, slot)| (slot, at(s, from.saturating_add(i))))
        .take_while(|&(_, c)| c != 0)
        .for_each(|(slot, c)| *slot = c);
    u32::from_be_bytes(tag)
}

/// The tags of a two-letter language in `ot_languages2`.
fn languages2_tags(key: u32, count: usize) -> Option<Vec<u32>> {
    let start = LANGUAGES2.partition_point(|e| e.0 < key);
    let run = LANGUAGES2.get(start..)?;
    (run.first()?.0 == key).then(|| {
        run.iter()
            .take_while(|e| e.0 == key && e.1 != 0)
            .take(count)
            .map(|e| e.1)
            .collect()
    })
}

/// The tags of a three-letter language: `ot_languages3`, then
/// `ot_languages3_multi`, then none if blocked, else the language
/// itself upper-cased as an ISO 639-3 tag.
fn languages3_tags(key: u32, count: usize) -> Vec<u32> {
    if let Ok(i) = LANGUAGES3.binary_search_by_key(&key, |e| e.0) {
        return LANGUAGES3
            .get(i)
            .map(|e| first_tags(&[e.1], count))
            .unwrap_or_default();
    }
    if let Ok(i) = LANGUAGES3_MULTI.binary_search_by_key(&key, |e| e.0) {
        return LANGUAGES3_MULTI
            .get(i)
            .and_then(|&(_, off, n)| {
                let off = usize::from(off);
                LANGUAGES3_MULTI_VALUES.get(off..off.saturating_add(usize::from(n)))
            })
            .map(|v| first_tags(v, count))
            .unwrap_or_default();
    }
    if LANGUAGES3_BLOCKED.binary_search(&key).is_ok() {
        return Vec::new();
    }
    first_tags(&[key & !0x2020_2000], count)
}

/// `hb_ot_tags_from_language` with room for `count` (at least 1) tags;
/// `limit` is the end of the part before the private-use and other
/// singleton subtags.
fn tags_from_language(s: &[u8], limit: Option<usize>, count: usize) -> Vec<u32> {
    if let Some(tags) = tags_from_complex_language(s, limit, count) {
        return tags;
    }
    // An extended language subtag (`zh-yue`) stands for the language.
    let first_dash = strchr(s, 0, b'-');
    let lang = match first_dash {
        Some(d) if span_at_least(limit, 0, 6) => {
            let next = d.saturating_add(1);
            let extlang_len =
                strchr(s, next, b'-').map_or_else(|| strlen(s, next), |e| e.saturating_sub(next));
            if extlang_len == 3 && at(s, next).is_ascii_alphabetic() {
                next
            } else {
                0
            }
        }
        _ => 0,
    };
    let first_len = match strchr(s, lang, b'-') {
        Some(d) => Some(d.saturating_sub(lang)),
        None => diff(limit, lang).and_then(|d| usize::try_from(d).ok()),
    };
    match first_len {
        Some(2) => {
            if let Some(tags) = languages2_tags(tag_from_string(s, lang, 2), count) {
                return tags;
            }
        }
        Some(3) => return languages3_tags(tag_from_string(s, lang, 3), count),
        _ => {}
    }
    // `s - lang_str`, with `s` the first dash of the whole tag and
    // `lang_str` possibly moved past it.
    let end = first_dash.unwrap_or_else(|| lang.saturating_add(strlen(s, lang)));
    if end.checked_sub(lang) == Some(3) {
        return first_tags(&[tag_from_string(s, lang, 3) & !0x2020_2000], count);
    }
    Vec::new()
}

/// `HB_OT_TAG_DEFAULT_SCRIPT`-like tags flip case: a private `dflt` or
/// `DFLT` must not name the default script or language system.
fn flip_default(tag: u32) -> u32 {
    if tag & 0xDFDF_DFDF == DFLT {
        tag ^ !0xDFDF_DFDF
    } else {
        tag
    }
}

fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c.wrapping_sub(b'0')),
        b'a'..=b'f' => Some(c.wrapping_sub(b'a').wrapping_add(10)),
        b'A'..=b'F' => Some(c.wrapping_sub(b'A').wrapping_add(10)),
        _ => None,
    }
}

/// `parse_private_use_subtag`: the tag after `prefix` in the
/// private-use part starting at `private_use`, either eight hex digits
/// after a `-` or up to four letters and digits passed through
/// `normalize`.
fn parse_private_use_subtag(
    s: &[u8],
    private_use: Option<usize>,
    count: usize,
    prefix: &[u8],
    normalize: fn(&u8) -> u8,
) -> Option<u32> {
    let pu = private_use.filter(|_| count > 0)?;
    let start = strstr(s, pu, prefix)?.saturating_add(prefix.len());
    let mut tag = [0u8; 4];
    if at(s, start) == b'-' {
        let digits: Vec<u8> = (0..8)
            .map_while(|i| hex_value(at(s, start.saturating_add(1).saturating_add(i))))
            .collect();
        if digits.len() != 8 {
            return None;
        }
        tag.iter_mut()
            .zip(digits.chunks(2))
            .for_each(|(slot, pair)| {
                let hi = pair.first().copied().unwrap_or(0);
                let lo = pair.get(1).copied().unwrap_or(0);
                *slot = hi.wrapping_shl(4).wrapping_add(lo);
            });
    } else {
        let chars: Vec<u8> = (0..4)
            .map(|i| at(s, start.saturating_add(i)))
            .take_while(u8::is_ascii_alphanumeric)
            .collect();
        if chars.is_empty() {
            return None;
        }
        tag = [b' '; 4];
        tag.iter_mut()
            .zip(&chars)
            .for_each(|(slot, c)| *slot = normalize(c));
    }
    Some(flip_default(u32::from_be_bytes(tag)))
}

/// `hb_ot_tags_from_script_and_language` on `hb_script_t` and language
/// string values.
fn tags_from_raw(
    script: u32,
    language: Option<&[u8]>,
    max_script: usize,
    max_language: usize,
) -> (Vec<u32>, Vec<u32>) {
    let mut script_tags = None;
    let mut language_tags = Vec::new();
    if let Some(s) = language {
        let (limit, private_use) = if at(s, 0) == b'x' && at(s, 1) == b'-' {
            (None, Some(0))
        } else {
            let mut limit = None;
            let mut private_use = None;
            let mut i = 1usize;
            while at(s, i) != 0 {
                if at(s, i.wrapping_sub(1)) == b'-' && at(s, i.saturating_add(1)) == b'-' {
                    if at(s, i) == b'x' {
                        private_use = Some(i);
                        limit = limit.or(Some(i.wrapping_sub(1)));
                        break;
                    }
                    limit = limit.or(Some(i.wrapping_sub(1)));
                }
                i = i.saturating_add(1);
            }
            (Some(limit.unwrap_or(i)), private_use)
        };
        script_tags =
            parse_private_use_subtag(s, private_use, max_script, b"-hbsc", u8::to_ascii_lowercase)
                .map(|t| alloc::vec![t]);
        language_tags = match parse_private_use_subtag(
            s,
            private_use,
            max_language,
            b"-hbot",
            u8::to_ascii_uppercase,
        ) {
            Some(t) => alloc::vec![t],
            None if max_language > 0 => tags_from_language(s, limit, max_language),
            None => Vec::new(),
        };
    }
    let script_tags = script_tags.unwrap_or_else(|| {
        if max_script > 0 {
            all_tags_from_script(script, max_script)
        } else {
            Vec::new()
        }
    });
    (script_tags, language_tags)
}

fn to_tags(v: Vec<u32>) -> Vec<Tag> {
    v.into_iter().map(Tag::from_u32).collect()
}

/// The OpenType script and language system tags to look up for a
/// script and a language, best first, as HarfBuzz's shaper asks for
/// them (`hb_ot_tags_from_script_and_language` with
/// [`MAX_TAGS_PER_SCRIPT`] and [`MAX_TAGS_PER_LANGUAGE`]).
///
/// `None` script is `HB_SCRIPT_INVALID` and gives no script tags;
/// `None` language gives no language tags. A private-use subtag
/// `-x-hbsc-<tag>` or `-x-hbot-<tag>` in the language overrides the
/// script tags or the language tags with that one tag.
pub fn ot_tags_from_script_and_language(
    script: Option<Script>,
    language: Option<&Language>,
) -> (Vec<Tag>, Vec<Tag>) {
    ot_tags_from_script_and_language_max(
        script,
        language,
        MAX_TAGS_PER_SCRIPT,
        MAX_TAGS_PER_LANGUAGE,
    )
}

/// [`ot_tags_from_script_and_language`] with room for at most
/// `max_script` script tags and `max_language` language tags (the
/// `script_count` and `language_count` arguments of HarfBuzz).
pub fn ot_tags_from_script_and_language_max(
    script: Option<Script>,
    language: Option<&Language>,
    max_script: usize,
    max_language: usize,
) -> (Vec<Tag>, Vec<Tag>) {
    let raw = script.map_or(SCRIPT_INVALID, |s| u32::from_be_bytes(s.tag()));
    let (s, l) = tags_from_raw(
        raw,
        language.map(|l| l.as_str().as_bytes()),
        max_script,
        max_language,
    );
    (to_tags(s), to_tags(l))
}

/// The script of an OpenType script tag (`hb_ot_tag_to_script`): `None`
/// for `DFLT`.
///
/// HarfBuzz's four-byte result is preserved, including values outside
/// ISO 15924 and aliases such as `Aran` and `Qaai`.
pub fn ot_tag_to_script(tag: Tag) -> Option<Script> {
    script_from_raw(tag_to_script(tag.to_u32()))
}

/// The two- or three-letter language of a table entry's tag bytes.
fn table_language(lang: u32, len: usize) -> Option<Language> {
    lang.to_be_bytes().get(..len).and_then(Language::new)
}

/// A lowercase hex digit (`TOHEX`).
fn to_hex(v: u32) -> u8 {
    b"0123456789abcdef"
        .get(usize::try_from(v & 0xF).unwrap_or(0))
        .copied()
        .unwrap_or(b'0')
}

/// Eight lowercase hex digits of `v`.
fn hex8(v: u32) -> [u8; 8] {
    [28u32, 24, 20, 16, 12, 8, 4, 0].map(|shift| to_hex(v >> shift))
}

/// `hb_ot_tag_to_language` on a tag value.
fn tag_to_language(tag: u32) -> Option<Language> {
    if tag == t(b"dflt") {
        return None;
    }
    if let Some((_, l)) = AMBIGUOUS.iter().find(|(x, _)| *x == tag) {
        return Language::new(l);
    }
    if let Some(&(l, _)) = LANGUAGES2.iter().find(|(_, x)| *x == tag) {
        return table_language(l, 2);
    }
    if let Some(&(l, _)) = LANGUAGES3.iter().find(|(_, x)| *x == tag) {
        return table_language(l, 3);
    }
    let multi = LANGUAGES3_MULTI.iter().find(|&&(_, off, n)| {
        let off = usize::from(off);
        LANGUAGES3_MULTI_VALUES
            .get(off..off.saturating_add(usize::from(n)))
            .is_some_and(|v| v.contains(&tag))
    });
    if let Some(&(l, _, _)) = multi {
        return table_language(l, 3);
    }
    // A custom language "x-hbot-AABBCCDD", after the tag as a guessed
    // ISO 639-3 language when it is three letters and a space.
    let b = tag.to_be_bytes();
    let mut buf = Vec::with_capacity(19);
    if let [a, b2, c, b' '] = b
        && [a, b2, c].iter().all(u8::is_ascii_alphabetic)
    {
        buf.extend([a, b2, c].map(|x| x.to_ascii_lowercase()));
        buf.push(b'-');
    }
    buf.extend_from_slice(b"x-hbot-");
    buf.extend_from_slice(&hex8(tag));
    Language::new(&buf)
}

/// The language of an OpenType language system tag
/// (`hb_ot_tag_to_language`): `None` for `dflt`. A tag of no known
/// language gives a private-use language `x-hbot-<hex>` that converts
/// back to the same tag.
pub fn ot_tag_to_language(tag: Tag) -> Option<Language> {
    tag_to_language(tag.to_u32())
}

/// The script and language of an OpenType script tag and language
/// system tag (`hb_ot_tags_to_script_and_language`). `None` language
/// tag is `dflt`.
///
/// When the script tag is not the first tag the script converts back
/// to (`DFLT`, `deva` rather than `dev3`), the language gets a
/// private-use subtag `-x-hbsc-<hex>` naming the script tag, so that
/// [`ot_tags_from_script_and_language`] gives the same tags back.
pub fn ot_tags_to_script_and_language(
    script_tag: Tag,
    language_tag: Option<Tag>,
) -> (Option<Script>, Option<Language>) {
    let raw_tag = script_tag.to_u32();
    let raw_script = tag_to_script(raw_tag);
    let (primary, _) = tags_from_raw(raw_script, None, 1, 0);
    let language = tag_to_language(language_tag.unwrap_or(OT_TAG_DEFAULT_LANGUAGE).to_u32())
        .and_then(|l| {
            if primary.first() == Some(&raw_tag) {
                return Some(l);
            }
            let s = l.as_str().as_bytes();
            let mut buf = Vec::with_capacity(s.len().saturating_add(16));
            buf.extend_from_slice(s);
            if !(at(s, 0) == b'x' && at(s, 1) == b'-') {
                buf.extend_from_slice(b"-x");
            }
            buf.extend_from_slice(b"-hbsc-");
            buf.extend_from_slice(&hex8(raw_tag));
            Language::new(&buf)
        });
    (script_from_raw(raw_script), language)
}
