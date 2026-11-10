//! Names (ADR 0016): the `name` table as HarfBuzz lists and converts
//! it, FreeType's records and PostScript name, and glyph names from
//! `post` (and from the CFF charset, ADR 0021) as HarfBuzz and FreeType
//! give them.
//!
//! The HarfBuzz parts are ported from HarfBuzz 15.1.0 (commit
//! 3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372): `src/OT/name/name.hh`,
//! `src/hb-ot-name.cc`, `src/hb-common.cc` (`hb_language_from_string`,
//! `hb_language_matches`), `src/hb-aat-ltag-table.hh`, `src/hb-utf.hh`,
//! `src/hb-ot-post-table.hh` and `src/hb-algs.hh` (`hb_bsearch`,
//! `hb_qsort_inline`). Copyright © the HarfBuzz authors. Used under
//! HarfBuzz's "Old MIT" license.
//!
//! The FreeType parts are ported from FreeType 2.15.0 (commit
//! 37515cf56ba3349656049edbed89ffc6f6466f2d): `src/sfnt/ttload.c`
//! (`tt_face_load_name`), `src/sfnt/sfdriver.c` (`sfnt_get_ps_name`,
//! `sfnt_get_name_id`, `get_win_string`, `get_apple_string`,
//! `sfnt_ps_map`, `sfnt_get_name_index`) and `src/sfnt/ttpost.c`.
//! Copyright (C) 1996-2026 by David Turner, Robert Wilhelm, and Werner
//! Lemberg. Used under the FreeType License (FTL). See
//! THIRD_PARTY_NOTICES.md.

use core::cmp::Ordering;

use sofel_read::be;
use sofel_read::name::{Name, NameRecord};
use sofel_read::post::{self, Post};
use sofel_read::{GlyphId, Tag};

use crate::name_language;
use crate::{Font, HbCff1};

/// `HB_BUFFER_REPLACEMENT_CODEPOINT_DEFAULT`.
const REPLACEMENT: u32 = 0xFFFD;
/// HarfBuzz's score of unsupported encodings.
const UNSUPPORTED: u16 = 42;
/// HarfBuzz keeps at most this many bytes of a language tag.
const MAX_LANGUAGE: usize = 63;

/// The language of a name record, as HarfBuzz's `hb_language_t` holds
/// it: a lowercase BCP 47 tag. Compare languages with `==`, which
/// compares the canonical tags.
#[derive(Clone, Copy, Debug)]
pub struct Language<'a> {
    bytes: &'a [u8],
}

/// HarfBuzz's `canon_map`: lowercase letters, digits and `-` (also for
/// `_`); 0 for anything else, which ends the tag.
fn canon(b: u8) -> u8 {
    match b {
        b'a'..=b'z' | b'0'..=b'9' | b'-' => b,
        b'A'..=b'Z' => b.to_ascii_lowercase(),
        b'_' => b'-',
        _ => 0,
    }
}

impl<'a> Language<'a> {
    /// A language from a tag as `hb_language_from_string` reads it: at
    /// most 63 bytes, up to the first byte that is not a letter, digit,
    /// `-` or `_`. `None` for an empty tag (HarfBuzz's
    /// `HB_LANGUAGE_INVALID`).
    pub fn new(tag: &'a [u8]) -> Option<Language<'a>> {
        if tag.first().is_none_or(|&b| b == 0) {
            return None;
        }
        Some(Language { bytes: tag })
    }

    /// The canonical tag, one byte at a time.
    pub fn bytes(&self) -> impl Iterator<Item = u8> + 'a {
        self.bytes
            .iter()
            .take(MAX_LANGUAGE)
            .map(|&b| canon(b))
            .take_while(|&b| b != 0)
    }

    /// The number of bytes of the canonical tag.
    pub fn len(&self) -> usize {
        self.bytes().count()
    }

    /// Whether the canonical tag is empty (a tag that starts with a
    /// byte HarfBuzz drops).
    pub fn is_empty(&self) -> bool {
        self.bytes().next().is_none()
    }

    /// Writes the canonical tag to `out` and returns its length; writes
    /// as much as fits.
    pub fn write(&self, out: &mut [u8]) -> usize {
        for (d, s) in out.iter_mut().zip(self.bytes()) {
            *d = s;
        }
        self.len()
    }

    fn cmp_tag(&self, other: &Language<'_>) -> Ordering {
        self.bytes().cmp(other.bytes())
    }

    /// `hb_language_matches (self, specific)`: `specific` is this
    /// language or a more specific one (`en` matches `en-us`).
    fn matches(&self, specific: &Language<'_>) -> bool {
        let mut s = specific.bytes();
        for b in self.bytes() {
            if s.next() != Some(b) {
                return false;
            }
        }
        matches!(s.next(), None | Some(b'-'))
    }
}

impl PartialEq for Language<'_> {
    fn eq(&self, other: &Language<'_>) -> bool {
        self.cmp_tag(other) == Ordering::Equal
    }
}

impl Eq for Language<'_> {}

/// One name HarfBuzz lists (`hb_ot_name_entry_t`): the name id, the
/// language, and the record HarfBuzz picked for the pair.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NameEntry<'a> {
    /// `nameID`.
    pub name_id: u16,
    /// The record's language.
    pub language: Language<'a>,
    /// Index of the record in the table.
    pub index: u16,
    score: u16,
}

/// What a `name_utf*` call wrote (`hb_ot_name_get_utf*`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct NameLength {
    /// Code units written before the terminating 0.
    pub written: usize,
    /// Code units of the whole string, without the terminating 0.
    pub total: usize,
}

/// A raw name record as FreeType's `FT_Get_Sfnt_Name` gives it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SfntName<'a> {
    /// `platformID`.
    pub platform_id: u16,
    /// `encodingID`.
    pub encoding_id: u16,
    /// `languageID`.
    pub language_id: u16,
    /// `nameID`.
    pub name_id: u16,
    /// The string's bytes, as stored.
    pub string: &'a [u8],
}

/// The name table as HarfBuzz uses it: only when every record's string
/// is in the table (`name::sanitize`).
pub(crate) fn hb_name<'a>(font: &Font<'a>) -> Option<Name<'a>> {
    let data = font.table(sofel_read::name::TAG)?;
    let name = Name::new(data).ok()?;
    if usize::from(name.storage_offset()) > data.len() {
        return None;
    }
    name.records()
        .all(|r| name.string(&r).is_some())
        .then_some(name)
}

/// `NameRecord::score`.
fn score(r: &NameRecord) -> u16 {
    match (r.platform_id, r.encoding_id) {
        (3, 10) => 0,
        (0, 6) => 1,
        (0, 4) => 2,
        (3, 1) => 3,
        (0, 3) => 4,
        (0, 2) => 5,
        (0, 1) => 6,
        (0, 0) => 7,
        (3, 0) => 8,
        (1, 0) => 10,
        _ => UNSUPPORTED,
    }
}

/// `hb_bsearch` over a table of `(code, tag)`.
fn bsearch_code(code: u16, table: &[(u16, &'static str)]) -> Option<&'static str> {
    let (mut min, mut max) = (0i64, i64::try_from(table.len()).ok()?.checked_sub(1)?);
    while min <= max {
        let mid = min.saturating_add(max) / 2;
        let (c, tag) = *table.get(usize::try_from(mid).ok()?)?;
        match code.cmp(&c) {
            Ordering::Less => max = mid.saturating_sub(1),
            Ordering::Greater => min = mid.saturating_add(1),
            Ordering::Equal => return Some(tag),
        }
    }
    None
}

/// The `ltag` table's tag `i`, if the table passes HarfBuzz's checks.
fn ltag<'a>(font: &Font<'a>, i: u16) -> Option<&'a [u8]> {
    let data = font.table(Tag::new(b"ltag"))?;
    if be::u32_at(data, 0)? < 1 {
        return None;
    }
    let count = be::to_usize(be::u32_at(data, 8)?);
    let ranges = be::chunks_at::<4>(data, 12, count)?;
    let range = |r: &[u8; 4]| {
        let off = usize::from(u16::from_be_bytes([r[0], r[1]]));
        let len = usize::from(u16::from_be_bytes([r[2], r[3]]));
        be::slice_at(data, off, len)
    };
    // The whole table fails when any range is out of it.
    if !ranges.iter().all(|r| range(r).is_some()) {
        return None;
    }
    range(ranges.get(usize::from(i))?)
}

/// `NameRecord::language`.
fn language<'a>(font: &Font<'a>, r: &NameRecord) -> Option<Language<'a>> {
    match r.platform_id {
        3 => {
            bsearch_code(r.language_id, name_language::MS).and_then(|t| Language::new(t.as_bytes()))
        }
        1 => bsearch_code(r.language_id, name_language::MAC)
            .and_then(|t| Language::new(t.as_bytes())),
        0 => ltag(font, r.language_id).and_then(Language::new),
        _ => None,
    }
}

/// `_hb_ot_name_entry_cmp`: name id, language (the invalid one first),
/// score, index.
fn entry_cmp(a: &NameEntry<'_>, b: &NameEntry<'_>) -> Ordering {
    a.name_id
        .cmp(&b.name_id)
        .then_with(|| a.language.cmp_tag(&b.language))
        .then(a.score.cmp(&b.score))
        .then(a.index.cmp(&b.index))
}

/// The names HarfBuzz lists, in its order; see [`Font::names`].
#[derive(Clone, Debug)]
pub struct Names<'a> {
    font: Font<'a>,
    name: Option<Name<'a>>,
    last: Option<NameEntry<'a>>,
}

impl<'a> Names<'a> {
    /// The best record for each (name id, language) pair, as a list
    /// sorted by HarfBuzz's order, without unsupported encodings and
    /// unknown languages: the smallest entry after `after`.
    fn next_after(&self, after: Option<&NameEntry<'a>>) -> Option<NameEntry<'a>> {
        let name = self.name?;
        let all = || {
            name.records().zip(0u16..).filter_map(|(r, i)| {
                Some(NameEntry {
                    name_id: r.name_id,
                    language: language(&self.font, &r)?,
                    index: i,
                    score: score(&r),
                })
            })
        };
        let pair = |e: &NameEntry<'a>| (e.name_id, e.language);
        let past = |e: &NameEntry<'a>| {
            after.is_none_or(|a| {
                e.name_id
                    .cmp(&a.name_id)
                    .then_with(|| e.language.cmp_tag(&a.language))
                    == Ordering::Greater
            })
        };
        // The smallest pair past `after`, then its best record.
        let best = all().filter(past).min_by(entry_cmp)?;
        (best.score != UNSUPPORTED).then_some(best).or_else(|| {
            // Every record of this pair is unsupported: skip the pair.
            let skipped = all().filter(|e| pair(e) == pair(&best)).last()?;
            self.next_after(Some(&skipped))
        })
    }
}

impl<'a> Iterator for Names<'a> {
    type Item = NameEntry<'a>;

    fn next(&mut self) -> Option<NameEntry<'a>> {
        let e = self.next_after(self.last.as_ref())?;
        self.last = Some(e);
        Some(e)
    }
}

/// `hb_utf16_be_t::next` and `hb_ascii_t::next`.
fn decode(bytes: &[u8], wide: bool) -> impl Iterator<Item = u32> + '_ {
    let units = bytes.as_chunks::<2>().0;
    let mut i = 0usize;
    let mut j = 0usize;
    core::iter::from_fn(move || {
        if !wide {
            let b = *bytes.get(j)?;
            j = j.saturating_add(1);
            return Some(if b >= 0x80 { REPLACEMENT } else { u32::from(b) });
        }
        let c = u32::from(u16::from_be_bytes(*units.get(i)?));
        i = i.saturating_add(1);
        if !(0xD800..=0xDFFF).contains(&c) {
            return Some(c);
        }
        if c <= 0xDBFF
            && let Some(l) = units.get(i).map(|u| u32::from(u16::from_be_bytes(*u)))
            && (0xDC00..=0xDFFF).contains(&l)
        {
            i = i.saturating_add(1);
            return Some(
                c.wrapping_shl(10)
                    .wrapping_add(l)
                    .wrapping_sub((0xD800u32 << 10).wrapping_sub(0x10000).wrapping_add(0xDC00)),
            );
        }
        Some(REPLACEMENT)
    })
}

/// A code unit type HarfBuzz's name functions write.
trait Unit: Copy {
    /// `encode` into `out` (empty when it does not fit), its length.
    fn encode(c: u32, out: &mut [Self]) -> Option<usize>;
    /// `encode_len`.
    fn encode_len(c: u32) -> usize;
    const ZERO: Self;
}

fn sanitize(c: u32) -> u32 {
    if c >= 0xD800 && (c <= 0xDFFF || c > 0x10FFFF) {
        REPLACEMENT
    } else {
        c
    }
}

/// The byte `b` of a code point (already masked to 8 bits).
fn byte(v: u32) -> u8 {
    u8::try_from(v & 0xFF).unwrap_or(0)
}

impl Unit for u8 {
    fn encode(c: u32, out: &mut [u8]) -> Option<usize> {
        let c = sanitize(c);
        let bytes: (usize, [u8; 4]) = if c < 0x80 {
            (1, [byte(c), 0, 0, 0])
        } else if c < 0x800 {
            (2, [byte(0xC0 | (c >> 6)), byte(0x80 | (c & 0x3F)), 0, 0])
        } else if c < 0x10000 {
            (
                3,
                [
                    byte(0xE0 | (c >> 12)),
                    byte(0x80 | ((c >> 6) & 0x3F)),
                    byte(0x80 | (c & 0x3F)),
                    0,
                ],
            )
        } else {
            (
                4,
                [
                    byte(0xF0 | (c >> 18)),
                    byte(0x80 | ((c >> 12) & 0x3F)),
                    byte(0x80 | ((c >> 6) & 0x3F)),
                    byte(0x80 | (c & 0x3F)),
                ],
            )
        };
        let (n, b) = bytes;
        let dst = out.get_mut(..n)?;
        dst.copy_from_slice(b.get(..n)?);
        Some(n)
    }
    fn encode_len(c: u32) -> usize {
        match c {
            0..0x80 => 1,
            0x80..0x800 => 2,
            0x800..0x10000 => 3,
            0x10000..0x110000 => 4,
            _ => 3,
        }
    }
    const ZERO: u8 = 0;
}

impl Unit for u16 {
    fn encode(c: u32, out: &mut [u16]) -> Option<usize> {
        let c = sanitize(c);
        let unit = |v: u32| u16::try_from(v & 0xFFFF).unwrap_or(0);
        if c < 0x10000 {
            *out.first_mut()? = unit(c);
            return Some(1);
        }
        let dst = out.get_mut(..2)?;
        let v = c.wrapping_sub(0x10000);
        dst.copy_from_slice(&[unit(0xD800 | (v >> 10)), unit(0xDC00 | (v & 0x3FF))]);
        Some(2)
    }
    fn encode_len(c: u32) -> usize {
        if c < 0x10000 { 1 } else { 2 }
    }
    const ZERO: u16 = 0;
}

impl Unit for u32 {
    fn encode(c: u32, out: &mut [u32]) -> Option<usize> {
        *out.first_mut()? = sanitize(c);
        Some(1)
    }
    fn encode_len(_: u32) -> usize {
        1
    }
    const ZERO: u32 = 0;
}

/// `hb_ot_name_convert_utf`.
fn convert<T: Unit>(bytes: &[u8], wide: bool, out: &mut [T]) -> NameLength {
    let mut src = decode(bytes, wide).peekable();
    let mut written = 0usize;
    if let Some(cap) = out.len().checked_sub(1) {
        while let Some(&c) = src.peek() {
            let room = out.get_mut(written..cap).unwrap_or_default();
            if room.is_empty() {
                break;
            }
            let Some(n) = T::encode(c, room) else { break };
            written = written.saturating_add(n);
            src.next();
        }
        if let Some(z) = out.get_mut(written) {
            *z = T::ZERO;
        }
    }
    let total = src.fold(written, |n, c| n.saturating_add(T::encode_len(c)));
    NameLength { written, total }
}

/// FreeType's `sfnt_ps_map`: printable ASCII but space and `[](){}<>/%`.
fn ps_char(b: u8) -> bool {
    matches!(b, 0x21..=0x7E) && !b"[](){}<>/%".contains(&b)
}

impl<'a> Font<'a> {
    /// The names HarfBuzz lists (`hb_ot_name_list_names`): for each name
    /// id and language, the record of the best encoding, sorted by name
    /// id and language. Records of unsupported encodings and unknown
    /// languages are left out, and so is everything when a record's
    /// string lies outside the table. Each step takes time in proportion
    /// to the number of records.
    pub fn names(&self) -> Names<'a> {
        Names {
            font: *self,
            name: hb_name(self),
            last: None,
        }
    }

    /// The entry `hb_ot_name_get_utf*` uses for `name_id` and
    /// `language` (`None`: English, as HarfBuzz uses for
    /// `HB_LANGUAGE_INVALID`): the entry with that language, else one
    /// whose language is a prefix of it (`en` for `en-us`), found as
    /// HarfBuzz's binary search finds it.
    pub fn name_entry(&self, name_id: u16, language: Option<&[u8]>) -> Option<NameEntry<'a>> {
        let lang = language
            .and_then(Language::new)
            .unwrap_or(Language { bytes: b"en" });
        let count = self.names().count();
        let nth = |i: usize| self.names().nth(i);
        let key = |e: &NameEntry<'_>, exact: bool| -> Ordering {
            // _hb_ot_name_entry_cmp_key (key, entry).
            let c = name_id.cmp(&e.name_id);
            if c != Ordering::Equal {
                return c;
            }
            let s = lang.cmp_tag(&e.language);
            if !exact && s != Ordering::Equal && e.language.matches(&lang) {
                return Ordering::Equal;
            }
            s
        };
        let search = |exact: bool| {
            let (mut min, mut max) = (0i64, i64::try_from(count).ok()?.checked_sub(1)?);
            while min <= max {
                let mid = min.saturating_add(max) / 2;
                let e = nth(usize::try_from(mid).ok()?)?;
                match key(&e, exact) {
                    Ordering::Less => max = mid.saturating_sub(1),
                    Ordering::Greater => min = mid.saturating_add(1),
                    Ordering::Equal => return Some(e),
                }
            }
            None
        };
        search(true).or_else(|| search(false))
    }

    fn name_utf<T: Unit>(
        &self,
        name_id: u16,
        language: Option<&[u8]>,
        out: &mut [T],
    ) -> NameLength {
        let found = self.name_entry(name_id, language).and_then(|e| {
            let name = hb_name(self)?;
            let bytes = name.string(&name.record(usize::from(e.index))?)?;
            Some((bytes, e.score < 10))
        });
        match found {
            Some((bytes, wide)) => convert(bytes, wide, out),
            None => {
                if let Some(z) = out.first_mut() {
                    *z = T::ZERO;
                }
                NameLength::default()
            }
        }
    }

    /// A name in UTF-8, as `hb_ot_name_get_utf8` writes it: the record of
    /// [`Font::name_entry`] decoded from UTF-16BE (or ASCII for Mac
    /// records, other bytes becoming U+FFFD), as much as fits in `out`
    /// before a terminating 0. Nothing is allocated.
    pub fn name_utf8(&self, name_id: u16, language: Option<&[u8]>, out: &mut [u8]) -> NameLength {
        self.name_utf(name_id, language, out)
    }

    /// [`Font::name_utf8`] in UTF-16 (`hb_ot_name_get_utf16`).
    pub fn name_utf16(&self, name_id: u16, language: Option<&[u8]>, out: &mut [u16]) -> NameLength {
        self.name_utf(name_id, language, out)
    }

    /// [`Font::name_utf8`] in UTF-32 (`hb_ot_name_get_utf32`).
    pub fn name_utf32(&self, name_id: u16, language: Option<&[u8]>, out: &mut [u32]) -> NameLength {
        self.name_utf(name_id, language, out)
    }

    /// The name records FreeType keeps (`tt_face_load_name`): those
    /// with a non-empty string inside the storage area (after the
    /// records and, in format 1, the language tag records) and, in
    /// format 1, a valid language tag when the language id is 0x8000 or
    /// more.
    fn ft_names(&self) -> impl Iterator<Item = SfntName<'a>> + 'a {
        let data = self.table(sofel_read::name::TAG).unwrap_or_default();
        let name = Name::new(data).ok();
        let start = name.map_or(0, |n| {
            let records = 6usize.saturating_add(usize::from(n.count()).saturating_mul(12));
            if n.format() == 1 {
                records
                    .saturating_add(2)
                    .saturating_add(usize::from(n.lang_tag_count()).saturating_mul(4))
            } else {
                records
            }
        });
        let in_storage = move |n: &Name<'a>, offset: u16, length: u16| {
            let at = usize::from(n.storage_offset()).saturating_add(usize::from(offset));
            at >= start && at.saturating_add(usize::from(length)) <= data.len()
        };
        name.into_iter().flat_map(move |n| {
            (0..usize::from(n.count())).filter_map(move |i| {
                let r = n.record(i)?;
                if r.length == 0 || !in_storage(&n, r.offset, r.length) {
                    return None;
                }
                if n.format() == 1 && r.language_id >= 0x8000 {
                    let t = n.lang_tag(usize::from(r.language_id & 0x7FFF))?;
                    if t.length == 0 || !in_storage(&n, t.offset, t.length) {
                        return None;
                    }
                }
                Some(SfntName {
                    platform_id: r.platform_id,
                    encoding_id: r.encoding_id,
                    language_id: r.language_id,
                    name_id: r.name_id,
                    string: n.string(&r)?,
                })
            })
        })
    }

    /// The number of records FreeType keeps (`FT_Get_Sfnt_Name_Count`).
    pub fn sfnt_name_count(&self) -> usize {
        self.ft_names().count()
    }

    /// Record `index` of those FreeType keeps, raw (`FT_Get_Sfnt_Name`).
    pub fn sfnt_name(&self, index: usize) -> Option<SfntName<'a>> {
        self.ft_names().nth(index)
    }

    /// The PostScript name, as FreeType's `FT_Get_Postscript_Name`
    /// finds it for sfnt fonts (`sfnt_get_ps_name`): of the records
    /// FreeType keeps with name id 6, the Windows one (platform 3,
    /// encoding 0 or 1; the last with language 0x409, else the first),
    /// keeping the UTF-16BE characters that are printable ASCII other
    /// than space and `[](){}<>/%`; if none is left, the Macintosh one
    /// (platform 1, encoding 0; the last with language 0, else the first)
    /// filtered byte by byte. Writes as much as fits in `out` (FreeType
    /// limits names to 127 bytes) and returns the name's full length;
    /// `None` when there is no name.
    pub fn postscript_name(&self, out: &mut [u8]) -> Option<usize> {
        let pick = |platform: u16, encodings: &[u16], lang: u16| {
            let mut found: Option<SfntName<'a>> = None;
            for r in self.ft_names().filter(|r| {
                r.name_id == 6 && r.platform_id == platform && encodings.contains(&r.encoding_id)
            }) {
                if r.language_id == lang || found.is_none() {
                    found = Some(r);
                }
            }
            found
        };
        let write = |chars: &mut dyn Iterator<Item = u8>, out: &mut [u8]| {
            let mut n = 0usize;
            for c in chars {
                if let Some(d) = out.get_mut(n) {
                    *d = c;
                }
                n = n.saturating_add(1);
            }
            n
        };
        if let Some(win) = pick(3, &[0, 1], 0x409) {
            let mut chars = win
                .string
                .as_chunks::<2>()
                .0
                .iter()
                .filter(|u| u[0] == 0 && ps_char(u[1]))
                .map(|u| u[1]);
            let n = write(&mut chars, out);
            if n > 0 {
                return Some(n);
            }
        }
        let apple = pick(1, &[0], 0)?;
        let mut chars = apple.string.iter().copied().filter(|&b| ps_char(b));
        let n = write(&mut chars, out);
        (n > 0).then_some(n)
    }

    /// The `post` table as HarfBuzz reads glyph names from it.
    fn hb_post(&self) -> Option<Post<'a>> {
        self.post
    }

    /// The number of glyphs with a name in HarfBuzz's view of `post`:
    /// 258 for version 1.0, `numGlyphs` for version 2.0, else 0.
    fn glyph_name_count(&self) -> usize {
        match self.hb_post() {
            Some(p) if p.version() == post::VERSION_1 => 258,
            Some(p) if p.version() == post::VERSION_2 => usize::from(p.num_glyphs().unwrap_or(0)),
            _ => 0,
        }
    }

    /// The number of `post` strings HarfBuzz uses (at most 65535).
    fn post_string_count(&self) -> usize {
        self.hb_post()
            .filter(|p| p.version() == post::VERSION_2)
            .map_or(0, |p| p.names().take(65535).count())
    }

    /// The name of a glyph as HarfBuzz's `hb_font_get_glyph_name` gives
    /// it (`hb_ot_get_glyph_name`): from `post` (versions 1.0 and 2.0),
    /// else from the charset of a CFF font that is not CID-keyed, when
    /// HarfBuzz's CFF accelerator is valid ([`HbCff1::glyph_name`],
    /// ADR 0021). `None` for glyphs without a name or with an empty one.
    /// HarfBuzz copies CFF names with `strncpy`, so it ends them at a 0
    /// byte; the name returned here is not cut.
    ///
    /// A stored `post` name takes time in proportion to the strings
    /// before it, and a CFF name builds [`HbCff1`] each time;
    /// [`GlyphNames`] looks names up in constant time.
    pub fn glyph_name(&self, glyph: GlyphId) -> Option<&'a [u8]> {
        self.glyph_name_with(glyph, HbCff1::new(self).as_ref())
    }

    /// [`Font::glyph_name`] with HarfBuzz's CFF accelerator built once by
    /// the caller ([`HbCff1::new`] of this font; `None` when it is not
    /// valid), so that CFF names take constant time for format 0
    /// charsets.
    pub fn glyph_name_with(&self, glyph: GlyphId, cff: Option<&HbCff1<'a>>) -> Option<&'a [u8]> {
        self.hb_glyph_name(glyph, &|k| self.hb_post()?.names().take(65535).nth(k))
            .or_else(|| cff?.glyph_name(glyph.0))
    }

    fn hb_glyph_name(
        &self,
        glyph: GlyphId,
        string: &dyn Fn(usize) -> Option<&'a [u8]>,
    ) -> Option<&'a [u8]> {
        let p = self.hb_post()?;
        let name: &'a [u8] = match p.version() {
            post::VERSION_1 => post::mac_glyph_name(u16::try_from(glyph.0).ok()?)?.as_bytes(),
            post::VERSION_2 => {
                let index = p.glyph_name_index(glyph)?;
                match index.checked_sub(post::NUM_MAC_GLYPH_NAMES) {
                    None => post::mac_glyph_name(index)?.as_bytes(),
                    Some(k) => string(usize::from(k))?,
                }
            }
            _ => return None,
        };
        (!name.is_empty()).then_some(name)
    }

    /// FreeType's glyph names (`tt_face_get_ps_name`): `None` when
    /// FreeType has no glyph names (no `post` of version 1.0, 2.0 or 2.5,
    /// or version 3.0) or for glyph ids not below the glyph count.
    fn ft_glyph_name_with(
        &self,
        glyph: GlyphId,
        string: &dyn Fn(usize) -> Option<&'a [u8]>,
    ) -> Option<&'a [u8]> {
        match self.ft_face() {
            FtFace::Sfnt => self.ft_post_glyph_name(glyph, self.num_glyphs, string),
            FtFace::Cff(c) => c.ft_glyph_name(glyph),
            FtFace::Cff2(n) => self.ft_post_glyph_name(glyph, n, string),
            FtFace::Closed => None,
        }
    }

    /// How FreeType opens the face for glyph names: the TrueType driver
    /// (sfnt names from `post`), the CFF driver with CFF (the charset) or
    /// CFF2 (`post`, with the CharStrings count), or not at all (ADR 0021).
    pub(crate) fn ft_face(&self) -> FtFace<'a> {
        if self.sfnt.version() != u32::from_be_bytes(*b"OTTO") {
            return FtFace::Sfnt;
        }
        match crate::Cff::new(self) {
            Ok(c) if c.version() == sofel_read::cff::Version::Cff => FtFace::Cff(c),
            Ok(c) if c.has_glyph_names() => FtFace::Cff2(c.num_glyphs()),
            Ok(_) => FtFace::Closed,
            Err(_) => FtFace::Closed,
        }
    }

    fn ft_post_glyph_name(
        &self,
        glyph: GlyphId,
        num_glyphs: u32,
        string: &dyn Fn(usize) -> Option<&'a [u8]>,
    ) -> Option<&'a [u8]> {
        let data = self.table(post::TAG)?;
        let version = be::u32_at(data, 0)?;
        if data.len() < 32
            || !matches!(
                version,
                post::VERSION_1 | post::VERSION_2 | post::VERSION_2_5
            )
            || glyph.0 >= num_glyphs
        {
            return None;
        }
        let notdef = post::MAC_GLYPH_NAMES[0].as_bytes();
        let mac = |i: u16| post::mac_glyph_name(i).map_or(notdef, str::as_bytes);
        let n = be::u16_at(data, 32).map_or(0, u32::from);
        let loaded = data.len() >= 34 && n != 0 && n <= num_glyphs;
        let g = u16::try_from(glyph.0).ok();
        Some(match version {
            post::VERSION_1 if num_glyphs == 258 => mac(g?),
            post::VERSION_1 => notdef,
            _ if !loaded || glyph.0 >= n => notdef,
            post::VERSION_2 => {
                let len = data.len().saturating_sub(34);
                if usize::try_from(n).ok()?.saturating_mul(2) > len {
                    return Some(notdef);
                }
                let index = be::u16_at(
                    data,
                    34usize.saturating_add(usize::from(g?).saturating_mul(2)),
                )?;
                match index.checked_sub(post::NUM_MAC_GLYPH_NAMES) {
                    None => mac(index),
                    Some(k) => string(usize::from(k)).unwrap_or_default(),
                }
            }
            _ => {
                let len = data.len().saturating_sub(34);
                if usize::try_from(n).ok()? > len || n > 258 + 128 {
                    return Some(notdef);
                }
                let off = be::i8_at(data, 34usize.saturating_add(usize::from(g?)))?;
                let idx = i32::from(g?).saturating_add(i32::from(off));
                mac(u16::try_from(idx).ok().filter(|&i| i <= 257).unwrap_or(0))
            }
        })
    }

    /// FreeType's stored glyph name `k` of version 2.0 (`load_format_20`):
    /// strings are read one after the other up to the end of the table
    /// (the last one cut there), each up to its first 0 byte; missing
    /// ones are empty.
    fn ft_post_string(&self, k: usize) -> Option<&'a [u8]> {
        let data = self.table(post::TAG)?;
        let n = usize::from(be::u16_at(data, 32)?);
        let mut at = 34usize.saturating_add(n.saturating_mul(2));
        for _ in 0..k {
            let len = usize::from(be::u8_at(data, at)?);
            at = at.saturating_add(1).saturating_add(len);
        }
        let len = usize::from(be::u8_at(data, at)?);
        let s = data.get(at.saturating_add(1)..)?;
        let s = s.get(..len).unwrap_or(s);
        Some(s.split(|&b| b == 0).next().unwrap_or_default())
    }

    /// The name of a glyph as FreeType's `FT_Get_Glyph_Name` gives it. For
    /// CFF fonts (ADR 0021): the charset name ([`crate::Cff::ft_glyph_name`];
    /// this builds the [`crate::Cff`] at each call), for CFF2 the `post`
    /// names below. For other sfnt fonts (`tt_face_get_ps_name`), which
    /// differs from
    /// [`Font::glyph_name`]: `.notdef` for glyphs without a stored name,
    /// version 1.0 only with exactly 258 glyphs, version 2.5 read, and
    /// stored names cut at a 0 byte or at the end of the table. `None`
    /// when FreeType has no glyph names (no `post` of version 1.0, 2.0 or
    /// 2.5) or for glyph ids not below the glyph count.
    pub fn ft_glyph_name(&self, glyph: GlyphId) -> Option<&'a [u8]> {
        self.ft_glyph_name_with(glyph, &|k| self.ft_post_string(k))
    }

    /// HarfBuzz's CFF accelerator when it gives glyph names: valid and
    /// not CID-keyed.
    fn hb_cff_names(&self) -> Option<HbCff1<'a>> {
        HbCff1::new(self).filter(|c| !c.is_cid())
    }

    /// The number of `u32` words [`Font::glyph_names`] needs: the `post`
    /// part, and two per glyph for the CFF names.
    pub fn glyph_names_len(&self) -> usize {
        self.post_names_len()
            .saturating_add(cff_names_len(self.hb_cff_names().as_ref()))
    }

    fn post_names_len(&self) -> usize {
        self.glyph_name_count()
            .saturating_add(self.post_string_count())
            .saturating_add(1)
    }

    /// An index of the glyph names in `buf`, which must have at least
    /// [`Font::glyph_names_len`] words: the offsets of the `post`
    /// strings, the glyph ids sorted by name as HarfBuzz sorts them
    /// (`gids_sorted_by_name`), and for CFF fonts the SID of each glyph
    /// and the glyphs sorted by name (equal names by glyph id, ADR 0021).
    /// The index is the caller's value (ADR 0003); building it takes
    /// O(n log n) time. `None` when `buf` is too short.
    pub fn glyph_names<'b>(&self, buf: &'b mut [u32]) -> Option<GlyphNames<'a, 'b>> {
        let cff = self.hb_cff_names();
        let post_len = self.post_names_len();
        let buf = buf.get_mut(..post_len.saturating_add(cff_names_len(cff.as_ref())))?;
        let (buf, cff_buf) = buf.split_at_mut(post_len);
        let (cff_gids, cff_sids) = cff_buf.split_at_mut(cff_buf.len() / 2);
        if let Some(c) = &cff {
            for ((g, slot), gid) in (0u32..).zip(cff_sids.iter_mut()).zip(cff_gids.iter_mut()) {
                *slot = c.sid(g);
                *gid = g;
            }
            let sids: &[u32] = cff_sids;
            let name = |g: u32| c.string(sids.get(be::to_usize(g)).copied().unwrap_or(0));
            sort(cff_gids, &|a, b| {
                gname_cmp(name(*a), name(*b)).then_with(|| a.cmp(b))
            });
        }
        let count = self.glyph_name_count();
        let strings = self.post_string_count();
        let (gids, offsets) = buf.split_at_mut(count);
        // offsets[k]: where string k starts; offsets[strings]: the end.
        if let Some(p) = self.hb_post() {
            let mut at = 0u32;
            for (slot, s) in offsets.iter_mut().zip(p.names().take(strings)) {
                *slot = at;
                let len = u32::try_from(s.len()).unwrap_or(0);
                at = at.saturating_add(1).saturating_add(len);
            }
            if let Some(end) = offsets.get_mut(strings) {
                *end = at;
            }
        }
        for (g, slot) in (0u32..).zip(gids.iter_mut()) {
            *slot = g;
        }
        let offsets: &'b [u32] = offsets;
        let names = GlyphNames {
            font: *self,
            gids: &[],
            offsets,
            ft: self.ft_face(),
            cff,
            cff_gids: &[],
            cff_sids,
        };
        sort(gids, &|a, b| {
            bytes_cmp(names.post_name(GlyphId(*b)), names.post_name(GlyphId(*a)))
        });
        Some(GlyphNames {
            gids,
            cff_gids,
            ..names
        })
    }

    /// The index in a buffer that [`Font::glyph_names`] filled for this
    /// font, without sorting again. `None` when `buf` is too short.
    pub fn glyph_names_from<'b>(&self, buf: &'b [u32]) -> Option<GlyphNames<'a, 'b>> {
        let cff = self.hb_cff_names();
        let post_len = self.post_names_len();
        let buf = buf.get(..post_len.saturating_add(cff_names_len(cff.as_ref())))?;
        let (buf, cff_buf) = buf.split_at(post_len);
        let (cff_gids, cff_sids) = cff_buf.split_at(cff_buf.len() / 2);
        let count = self.glyph_name_count();
        let (gids, offsets) = buf.split_at(count);
        Some(GlyphNames {
            font: *self,
            gids,
            offsets,
            ft: self.ft_face(),
            cff,
            cff_gids,
            cff_sids,
        })
    }
}

/// The words of a glyph name index for CFF names: two per glyph.
fn cff_names_len(cff: Option<&HbCff1<'_>>) -> usize {
    cff.map_or(0, |c| be::to_usize(c.num_glyphs()).saturating_mul(2))
}

/// `gname_t::cmp` of HarfBuzz's CFF name index: `strncmp` over the
/// shorter length (so a 0 byte ends the comparison), then the shorter
/// first.
fn gname_cmp(a: &[u8], b: &[u8]) -> Ordering {
    let n = a.len().min(b.len());
    for (&x, &y) in a.iter().zip(b).take(n) {
        if x != y {
            return x.cmp(&y);
        }
        if x == 0 {
            return a.len().cmp(&b.len());
        }
    }
    a.len().cmp(&b.len())
}

/// `hb_bytes_t::cmp`: the shorter first, then by bytes, with the
/// arguments reversed as HarfBuzz's are (`a.cmp (b)` is the sign of
/// `b - a`).
fn bytes_cmp(a: Option<&[u8]>, b: Option<&[u8]>) -> Ordering {
    let (a, b) = (a.unwrap_or_default(), b.unwrap_or_default());
    b.len().cmp(&a.len()).then_with(|| b.cmp(a))
}

/// HarfBuzz's `hb_qsort_inline`, so that glyphs with equal names end up
/// in the same order.
fn sort(base: &mut [u32], cmp: &dyn Fn(&u32, &u32) -> Ordering) {
    let depth = 2u32.saturating_mul(usize::BITS.saturating_sub(base.len().leading_zeros()));
    qsort_loop(base, cmp, depth);
    for i in 1..base.len() {
        let mut j = i;
        while j > 0 {
            let (Some(a), Some(b)) = (base.get(j.saturating_sub(1)), base.get(j)) else {
                break;
            };
            if cmp(a, b) != Ordering::Greater {
                break;
            }
            base.swap(j.saturating_sub(1), j);
            j = j.saturating_sub(1);
        }
    }
}

fn at(base: &[u32], i: usize) -> u32 {
    base.get(i).copied().unwrap_or(0)
}

fn qsort_loop(mut base: &mut [u32], cmp: &dyn Fn(&u32, &u32) -> Ordering, mut depth: u32) {
    while base.len() > 24 {
        if depth == 0 {
            heapsort(base, cmp);
            return;
        }
        depth = depth.saturating_sub(1);
        let nel = base.len();
        let last = nel.saturating_sub(1);
        let mid = nel / 2;
        let c = |base: &[u32], i: usize, j: usize| cmp(&at(base, i), &at(base, j));
        if c(base, 0, mid) == Ordering::Greater {
            base.swap(0, mid);
        }
        if c(base, mid, last) == Ordering::Greater {
            base.swap(mid, last);
            if c(base, 0, mid) == Ordering::Greater {
                base.swap(0, mid);
            }
        }
        let p = last.saturating_sub(1);
        base.swap(mid, p);
        let pivot = at(base, p);
        let (mut i, mut j) = (0usize, p);
        loop {
            i = i.saturating_add(1);
            while cmp(&at(base, i), &pivot) == Ordering::Less {
                i = i.saturating_add(1);
            }
            j = j.saturating_sub(1);
            while cmp(&at(base, j), &pivot) == Ordering::Greater {
                j = j.saturating_sub(1);
            }
            if i >= j {
                break;
            }
            base.swap(i, j);
        }
        base.swap(i, p);
        let left = i;
        let right = nel.saturating_sub(left).saturating_sub(1);
        let (l, r) = base.split_at_mut(i);
        let r = r.get_mut(1..).unwrap_or_default();
        if left < right {
            qsort_loop(l, cmp, depth);
            base = r;
        } else {
            qsort_loop(r, cmp, depth);
            base = l;
        }
    }
}

fn heapsort(base: &mut [u32], cmp: &dyn Fn(&u32, &u32) -> Ordering) {
    let sift = |base: &mut [u32], mut root: usize, end: usize| loop {
        let child = root.saturating_mul(2).saturating_add(1);
        if child >= end {
            break;
        }
        let mut child = child;
        if child.saturating_add(1) < end
            && cmp(&at(base, child), &at(base, child.saturating_add(1))) == Ordering::Less
        {
            child = child.saturating_add(1);
        }
        if cmp(&at(base, root), &at(base, child)) != Ordering::Less {
            break;
        }
        base.swap(root, child);
        root = child;
    };
    let nel = base.len();
    if nel < 2 {
        return;
    }
    for i in (0..nel / 2).rev() {
        sift(base, i, nel);
    }
    for end in (1..nel).rev() {
        base.swap(0, end);
        sift(base, 0, end);
    }
}

/// Glyph names looked up in constant time and glyphs looked up by name,
/// over a buffer the caller holds; see [`Font::glyph_names`].
#[derive(Debug)]
pub struct GlyphNames<'a, 'b> {
    font: Font<'a>,
    gids: &'b [u32],
    offsets: &'b [u32],
    cff: Option<HbCff1<'a>>,
    /// The glyphs sorted by CFF name, equal names by glyph id.
    cff_gids: &'b [u32],
    /// The SID of each glyph.
    cff_sids: &'b [u32],
    ft: FtFace<'a>,
}

/// The driver FreeType opens a face with, as far as glyph names go.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum FtFace<'a> {
    Sfnt,
    Cff(crate::Cff<'a>),
    Cff2(u32),
    Closed,
}

impl<'a> GlyphNames<'a, '_> {
    fn string(&self, k: usize) -> Option<&'a [u8]> {
        let p = self.font.hb_post()?;
        let start = be::to_usize(*self.offsets.get(k)?);
        let end = be::to_usize(*self.offsets.get(k.saturating_add(1))?);
        let pool = p.names_offset();
        let data = self.font.table(post::TAG)?;
        data.get(pool.saturating_add(start).saturating_add(1)..pool.saturating_add(end))
    }

    fn post_name(&self, glyph: GlyphId) -> Option<&'a [u8]> {
        self.font.hb_glyph_name(glyph, &|k| {
            (k < self.offsets.len().saturating_sub(1))
                .then(|| self.string(k))
                .flatten()
        })
    }

    /// The CFF name of `glyph` from the SIDs in the index.
    fn cff_name(&self, glyph: u32) -> Option<&'a [u8]> {
        let c = self.cff?;
        let sid = *self.cff_sids.get(be::to_usize(glyph))?;
        let s = c.string(sid);
        (!s.is_empty()).then_some(s)
    }

    fn name(&self, glyph: GlyphId) -> Option<&'a [u8]> {
        self.post_name(glyph).or_else(|| self.cff_name(glyph.0))
    }

    /// [`Font::glyph_name`].
    pub fn glyph_name(&self, glyph: GlyphId) -> Option<&'a [u8]> {
        self.name(glyph)
    }

    /// The glyph with `name`, as HarfBuzz's `hb_font_get_glyph_from_name`
    /// finds it (`hb_ot_get_glyph_from_name`): a binary search over the
    /// glyphs sorted by `post` name, else over those sorted by CFF name
    /// (ADR 0021). For a CFF name, the glyph is the first one with the
    /// SID of the match (HarfBuzz's `sid_to_glyph`); of glyphs with equal
    /// names, the one with the smallest glyph id is matched, where
    /// HarfBuzz's depends on its sort. `None` for an empty name or no
    /// match.
    pub fn glyph_from_name(&self, name: &[u8]) -> Option<GlyphId> {
        self.post_glyph_from_name(name)
            .or_else(|| self.cff_glyph_from_name(name))
    }

    fn cff_glyph_from_name(&self, name: &[u8]) -> Option<GlyphId> {
        let c = self.cff?;
        if name.is_empty() {
            return None;
        }
        let string = |g: &u32| c.string(self.cff_sids.get(be::to_usize(*g)).copied().unwrap_or(0));
        let i = self
            .cff_gids
            .partition_point(|g| gname_cmp(string(g), name) == Ordering::Less);
        let g = self.cff_gids.get(i)?;
        if gname_cmp(string(g), name) != Ordering::Equal {
            return None;
        }
        // gname_t keeps the SID in 16 bits.
        let sid = self.cff_sids.get(be::to_usize(*g)).copied()? & 0xFFFF;
        let gid = c.glyph_of_sid(sid);
        (gid != 0 || sid == 0).then_some(GlyphId(gid))
    }

    fn post_glyph_from_name(&self, name: &[u8]) -> Option<GlyphId> {
        if name.is_empty() || self.gids.is_empty() {
            return None;
        }
        let (mut min, mut max) = (0i64, i64::try_from(self.gids.len()).ok()?.checked_sub(1)?);
        while min <= max {
            let mid = min.saturating_add(max) / 2;
            let g = *self.gids.get(usize::try_from(mid).ok()?)?;
            // cmp_key: find_glyph_name (o).cmp (key).
            match bytes_cmp(self.post_name(GlyphId(g)), Some(name)) {
                Ordering::Less => max = mid.saturating_sub(1),
                Ordering::Greater => min = mid.saturating_add(1),
                Ordering::Equal => return Some(GlyphId(g)),
            }
        }
        None
    }

    /// FreeType's `FT_Get_Name_Index` for sfnt fonts
    /// (`sfnt_get_name_index`): the first glyph whose
    /// [`Font::ft_glyph_name`] is `name`. FreeType returns 0 both for
    /// glyph 0 and for no match; this gives `None` for no match.
    pub fn ft_glyph_from_name(&self, name: &[u8]) -> Option<GlyphId> {
        match self.ft {
            FtFace::Cff(c) => {
                let g = c.ft_glyph_from_name(name);
                return (c.ft_glyph_name(g) == Some(name)).then_some(g);
            }
            FtFace::Closed => return None,
            _ => {}
        }
        let n = match self.ft {
            FtFace::Cff2(n) => n,
            _ => self.font.num_glyphs(),
        };
        (0..n)
            .map(GlyphId)
            .find(|&g| self.ft_glyph_name(g) == Some(name))
    }

    /// [`Font::ft_glyph_name`], with the strings looked up in the index.
    pub fn ft_glyph_name(&self, glyph: GlyphId) -> Option<&'a [u8]> {
        let string = |k: usize| self.font.ft_post_string_indexed(k, self);
        match self.ft {
            FtFace::Sfnt => self
                .font
                .ft_post_glyph_name(glyph, self.font.num_glyphs, &string),
            FtFace::Cff(c) => c.ft_glyph_name(glyph),
            FtFace::Cff2(n) => self.font.ft_post_glyph_name(glyph, n, &string),
            FtFace::Closed => None,
        }
    }
}

impl<'a> Font<'a> {
    /// [`Font::ft_post_string`], using the index for the strings
    /// HarfBuzz also has.
    fn ft_post_string_indexed(&self, k: usize, names: &GlyphNames<'a, '_>) -> Option<&'a [u8]> {
        let complete = names.offsets.len().saturating_sub(1);
        if k < complete && self.hb_post().is_some() {
            let s = names.string(k)?;
            return Some(s.split(|&b| b == 0).next().unwrap_or_default());
        }
        self.ft_post_string(k)
    }
}
