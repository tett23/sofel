//! The PostScript wrapper of a Type 42 font: its `/sfnts` array, the
//! table directory of the sfnt it holds, its `/CharStrings` and its
//! `/Encoding`.

use core::fmt;

use crate::noalloc::Problem;
use crate::noalloc::encoding;
use crate::noalloc::text::hex_nibble;

/// An invalid or truncated Type 42 wrapper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Type42Error(pub(crate) usize);

impl Type42Error {
    /// Byte offset in the PostScript wrapper where parsing failed.
    pub fn offset(self) -> usize {
        self.0
    }
}

impl fmt::Display for Type42Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Type 42 font")
    }
}

impl core::error::Error for Type42Error {}

/// Where the `/sfnts` array is and how long the sfnt it holds is.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sfnts {
    /// The offset of `/sfnts`.
    pub(crate) marker: usize,
    /// The offset of the closing `]`.
    pub(crate) end: usize,
    /// The length of the joined sfnt.
    pub(crate) total: usize,
}

/// Read the `/sfnts` array, passing the joined sfnt to `chunk` piece by
/// piece. The sfnt may not exceed `max_output` bytes.
pub(crate) fn sfnts(
    data: &[u8],
    max_output: usize,
    chunk: &mut dyn FnMut(&[u8]),
) -> Result<Sfnts, Type42Error> {
    if !data.starts_with(b"%!PS-TrueTypeFont") {
        return Err(Type42Error(0));
    }
    let font_type = find_name(data, b"/FontType").ok_or(Type42Error(data.len()))?;
    let mut at = font_type
        .checked_add(b"/FontType".len())
        .ok_or(Type42Error(font_type))?;
    skip_space(data, &mut at);
    if token(data, &mut at) != Some(&b"42"[..]) {
        return Err(Type42Error(at));
    }
    let marker = find_name(data, b"/sfnts").ok_or(Type42Error(data.len()))?;
    at = marker.checked_add(b"/sfnts".len()).ok_or(Type42Error(at))?;
    skip_space(data, &mut at);
    if data.get(at) != Some(&b'[') {
        return Err(Type42Error(at));
    }
    at = at.checked_add(1).ok_or(Type42Error(at))?;
    let mut total = 0usize;
    let mut kind = None;
    loop {
        skip_space(data, &mut at);
        match data.get(at).copied().ok_or(Type42Error(at))? {
            b']' => break,
            b'<' => {
                if matches!(kind, Some(b'B')) {
                    return Err(Type42Error(at));
                }
                kind = Some(b'H');
                at = at.checked_add(1).ok_or(Type42Error(at))?;
                let start = at;
                let mut len = 0usize;
                let mut last = 0u8;
                let mut high = None;
                loop {
                    let byte = *data.get(at).ok_or(Type42Error(at))?;
                    at = at.checked_add(1).ok_or(Type42Error(at))?;
                    if byte == b'>' {
                        break;
                    }
                    if byte.is_ascii_whitespace() {
                        continue;
                    }
                    let nibble = hex_nibble(byte).ok_or(Type42Error(at.saturating_sub(1)))?;
                    if let Some(upper) = high.take() {
                        if len > max_output.saturating_sub(total) {
                            return Err(Type42Error(at));
                        }
                        last = (upper << 4) | nibble;
                        len = len.saturating_add(1);
                    } else {
                        high = Some(nibble);
                    }
                }
                if let Some(upper) = high {
                    if len > max_output.saturating_sub(total) {
                        return Err(Type42Error(at));
                    }
                    last = upper << 4;
                    len = len.saturating_add(1);
                }
                let keep = part_length(len, last, total, max_output).ok_or(Type42Error(at))?;
                let hex = data.get(start..at).ok_or(Type42Error(at))?;
                hex_bytes(hex).take(keep).for_each(|byte| chunk(&[byte]));
                total = total.saturating_add(keep);
            }
            b'0'..=b'9' => {
                if matches!(kind, Some(b'H')) {
                    return Err(Type42Error(at));
                }
                kind = Some(b'B');
                let length = decimal(token(data, &mut at).ok_or(Type42Error(at))?)
                    .map_err(|_| Type42Error(at))?;
                skip_space(data, &mut at);
                if token(data, &mut at) != Some(&b"RD"[..]) {
                    return Err(Type42Error(at));
                }
                let sep = *data.get(at).ok_or(Type42Error(at))?;
                if !sep.is_ascii_whitespace() {
                    return Err(Type42Error(at));
                }
                at = at.checked_add(1).ok_or(Type42Error(at))?;
                let end = at.checked_add(length).ok_or(Type42Error(at))?;
                let part = data.get(at..end).ok_or(Type42Error(at))?;
                let keep = part_length(
                    part.len(),
                    part.last().copied().unwrap_or(1),
                    total,
                    max_output,
                )
                .ok_or(Type42Error(at))?;
                chunk(part.get(..keep).ok_or(Type42Error(at))?);
                total = total.saturating_add(keep);
                at = end;
            }
            _ => return Err(Type42Error(at)),
        }
    }
    Ok(Sfnts {
        marker,
        end: at,
        total,
    })
}

/// The bytes of one part that join the sfnt: an odd part loses its last
/// byte when it is a zero pad. Empty parts and parts past the limit fail.
fn part_length(len: usize, last: u8, total: usize, limit: usize) -> Option<usize> {
    let keep = if len & 1 == 1 && last == 0 {
        len.saturating_sub(1)
    } else {
        len
    };
    let end = total.checked_add(keep)?;
    (end <= limit && keep != 0).then_some(keep)
}

/// The bytes of a hexadecimal part already checked by [`sfnts`].
fn hex_bytes(hex: &[u8]) -> impl Iterator<Item = u8> + '_ {
    let mut nibbles = hex
        .iter()
        .take_while(|byte| **byte != b'>')
        .filter_map(|byte| hex_nibble(*byte));
    core::iter::from_fn(move || {
        let high = nibbles.next()?;
        Some((high << 4) | nibbles.next().unwrap_or(0))
    })
}

/// The table directory of an sfnt, checked byte by byte: the version, and
/// that every table record lies inside the `total` bytes.
pub(crate) struct SfntCheck {
    total: usize,
    position: usize,
    head: [u8; 12],
    entry: [u8; 16],
    bad: bool,
}

impl SfntCheck {
    pub(crate) fn new(total: usize) -> Self {
        Self {
            total,
            position: 0,
            head: [0; 12],
            entry: [0; 16],
            bad: false,
        }
    }

    fn count(&self) -> usize {
        usize::from(u16::from_be_bytes([
            self.head.get(4).copied().unwrap_or(0),
            self.head.get(5).copied().unwrap_or(0),
        ]))
    }

    pub(crate) fn feed(&mut self, byte: u8) {
        let position = self.position;
        self.position = self.position.saturating_add(1);
        if let Some(slot) = self.head.get_mut(position) {
            *slot = byte;
            return;
        }
        let end = self.count().saturating_mul(16).saturating_add(12);
        let Some(index) = position.checked_sub(12).filter(|_| position < end) else {
            return;
        };
        if let Some(slot) = self.entry.get_mut(index % 16) {
            *slot = byte;
        }
        if index % 16 == 15 {
            let word = |at: usize| {
                self.entry
                    .get(at..at.saturating_add(4))
                    .and_then(|bytes| bytes.try_into().ok())
                    .map(u32::from_be_bytes)
                    .and_then(|value| usize::try_from(value).ok())
            };
            let inside = word(8)
                .zip(word(12))
                .and_then(|(offset, length)| offset.checked_add(length))
                .is_some_and(|end| end <= self.total);
            self.bad |= !inside;
        }
    }

    pub(crate) fn finish(&self) -> bool {
        if self.total < 6 {
            return false;
        }
        let version = self.head.get(..4).unwrap_or(&[]);
        if version != b"\0\x01\0\0" && version != b"true" {
            return false;
        }
        let end = self.count().checked_mul(16).and_then(|n| n.checked_add(12));
        end.is_some_and(|end| end <= self.total) && !self.bad
    }
}

/// Read `/CharStrings`, passing each glyph name and sfnt glyph index to
/// `glyph`. There is at least one glyph, and `.notdef` is among them.
pub(crate) fn charstrings<'a>(
    data: &'a [u8],
    glyph: &mut dyn FnMut(&'a [u8], u32),
) -> Result<(), Type42Error> {
    let marker = find_name(data, b"/CharStrings").ok_or(Type42Error(data.len()))?;
    let mut at = marker
        .checked_add(b"/CharStrings".len())
        .ok_or(Type42Error(marker))?;
    skip_space(data, &mut at);
    let dictionary = data.get(at..at.saturating_add(2)) == Some(b"<<");
    let count = if dictionary {
        at = at.checked_add(2).ok_or(Type42Error(at))?;
        None
    } else {
        let count = decimal(token(data, &mut at).ok_or(Type42Error(at))?)?;
        if count == 0 || count > data.len() / 4 {
            return Err(Type42Error(at));
        }
        while let Some(word) = next_word(data, &mut at) {
            if word == b"begin" {
                break;
            }
        }
        Some(count)
    };
    let mut found = 0usize;
    let mut notdef = false;
    loop {
        skip_space(data, &mut at);
        if dictionary && data.get(at..at.saturating_add(2)) == Some(b">>") {
            break;
        }
        if count.is_some_and(|count| found >= count) {
            break;
        }
        let word = next_word(data, &mut at).ok_or(Type42Error(at))?;
        let name = if let Some(name) = word.strip_prefix(b"/") {
            name
        } else if let Some(name) = word
            .strip_prefix(b"(")
            .and_then(|word| word.strip_suffix(b")"))
        {
            if next_word(data, &mut at) != Some(&b"cvn"[..]) {
                return Err(Type42Error(at));
            }
            name
        } else {
            continue;
        };
        if name.is_empty() {
            return Err(Type42Error(at));
        }
        let index = decimal(next_word(data, &mut at).ok_or(Type42Error(at))?)?;
        glyph(name, u32::try_from(index).map_err(|_| Type42Error(at))?);
        notdef |= name == b".notdef";
        found = found.saturating_add(1);
        if found > data.len() / 4 {
            return Err(Type42Error(at));
        }
    }
    if found == 0 || !notdef {
        return Err(Type42Error(at));
    }
    Ok(())
}

/// The `/Encoding` declaration of a Type 42 wrapper.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Declared<'a> {
    /// `ISOLatin1Encoding`.
    IsoLatin1,
    /// `ExpertEncoding`.
    Expert,
    /// `StandardEncoding` or an explicit array, read by
    /// `crate::encoding::Encoding` from this text.
    Type1(&'a [u8]),
}

/// Find and check `/Encoding`. `at` is the offset that a problem reports.
pub(crate) fn encoding(data: &[u8], at: usize) -> Result<Declared<'_>, Type42Error> {
    let marker = find_name(data, b"/Encoding").ok_or(Type42Error(data.len()))?;
    let after = marker
        .checked_add(b"/Encoding".len())
        .ok_or(Type42Error(at))?;
    let declaration = data.get(after..).ok_or(Type42Error(at))?.trim_ascii_start();
    if declaration.starts_with(b"ISOLatin1Encoding") {
        return Ok(Declared::IsoLatin1);
    }
    if declaration.starts_with(b"ExpertEncoding") {
        return Ok(Declared::Expert);
    }
    let text = data.get(marker..).ok_or(Type42Error(at))?;
    encoding::scan(text, &mut |_, _| {}).map_err(|_| Type42Error(at))?;
    Ok(Declared::Type1(text))
}

/// Validate a Type 42 font: its `/sfnts` array and the table directory of
/// the sfnt in it, its `/CharStrings` and its `/Encoding`. The sfnt is
/// read twice and never joined.
pub fn validate(data: &[u8], max_output: usize, report: &mut dyn FnMut(Problem)) {
    if let Err(error) = check(data, max_output) {
        report(Problem {
            offset: error.offset(),
            reason: "invalid Type 42 wrapper",
        });
    }
}

fn check(data: &[u8], max_output: usize) -> Result<(), Type42Error> {
    let found = sfnts(data, max_output, &mut |_| {})?;
    let mut directory = SfntCheck::new(found.total);
    sfnts(data, max_output, &mut |bytes| {
        bytes.iter().for_each(|byte| directory.feed(*byte));
    })?;
    if !directory.finish() {
        return Err(Type42Error(found.marker));
    }
    charstrings(data, &mut |_, _| {})?;
    encoding(data, found.end)?;
    Ok(())
}

fn find_name(data: &[u8], name: &[u8]) -> Option<usize> {
    let mut at = 0usize;
    while let Some(&byte) = data.get(at) {
        if byte == b'%' {
            while let Some(&next) = data.get(at) {
                at = at.saturating_add(1);
                if matches!(next, b'\r' | b'\n') {
                    break;
                }
            }
            continue;
        }
        let end = at.checked_add(name.len())?;
        if data.get(at..end) == Some(name)
            && data
                .get(end)
                .is_none_or(|next| next.is_ascii_whitespace() || b"[]<>".contains(next))
        {
            return Some(at);
        }
        at = at.saturating_add(1);
    }
    None
}

fn skip_space(data: &[u8], at: &mut usize) {
    loop {
        match data.get(*at) {
            Some(byte) if byte.is_ascii_whitespace() => *at = at.saturating_add(1),
            Some(b'%') => {
                while let Some(&next) = data.get(*at) {
                    *at = at.saturating_add(1);
                    if matches!(next, b'\r' | b'\n') {
                        break;
                    }
                }
            }
            _ => break,
        }
    }
}

fn token<'a>(data: &'a [u8], at: &mut usize) -> Option<&'a [u8]> {
    let start = *at;
    while data
        .get(*at)
        .is_some_and(|b| !b.is_ascii_whitespace() && !b"[]<>".contains(b))
    {
        *at = at.saturating_add(1);
    }
    (start < *at).then(|| data.get(start..*at)).flatten()
}

fn next_word<'a>(data: &'a [u8], at: &mut usize) -> Option<&'a [u8]> {
    skip_space(data, at);
    token(data, at)
}

fn decimal(data: &[u8]) -> Result<usize, Type42Error> {
    data.iter().try_fold(0usize, |n, byte| {
        let d = byte
            .checked_sub(b'0')
            .filter(|d| *d < 10)
            .ok_or(Type42Error(0))?;
        n.checked_mul(10)
            .and_then(|n| n.checked_add(usize::from(d)))
            .ok_or(Type42Error(0))
    })
}
