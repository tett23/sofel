//! The `/Subrs` and `/CharStrings` of a decrypted private dictionary,
//! read from any byte source: a slice, or `eexec` data decrypted while it
//! is read.

/// Bytes read by position. Reading may move a cursor, hence `&mut`.
pub(crate) trait Bytes {
    /// The number of bytes.
    fn size(&self) -> usize;
    /// The byte at `index`, or `None` past the end.
    fn at(&mut self, index: usize) -> Option<u8>;
}

/// A slice as [`Bytes`].
pub(crate) struct Slice<'a>(pub(crate) &'a [u8]);

impl Bytes for Slice<'_> {
    fn size(&self) -> usize {
        self.0.len()
    }
    fn at(&mut self, index: usize) -> Option<u8> {
        self.0.get(index).copied()
    }
}

/// A byte range `start..end` of the source.
pub(crate) type Span = (usize, usize);

/// One entry found while reading the dictionary.
pub(crate) enum Item {
    /// The declared number of local subroutines.
    SubrCount(usize),
    /// A local subroutine and its encrypted program.
    Subr { index: usize, program: Span },
    /// A glyph name (without `/`) and its encrypted program.
    Glyph { name: Span, program: Span },
}

struct Fail;

struct Reader {
    at: usize,
    start: usize,
    limit: usize,
    /// The first byte of the last word, kept so that a long word need not
    /// be read again.
    first: u8,
}

impl Reader {
    fn new(at: usize, limit: usize) -> Self {
        Self {
            at,
            start: at,
            limit,
            first: 0,
        }
    }

    fn byte<B: Bytes>(&self, data: &mut B, index: usize) -> Option<u8> {
        if index < self.limit {
            data.at(index)
        } else {
            None
        }
    }

    fn word<B: Bytes>(&mut self, data: &mut B) -> Result<Span, usize> {
        loop {
            let Some(byte) = self.byte(data, self.at) else {
                return Err(self.at);
            };
            if byte == b'%' {
                let mut end = self.at;
                loop {
                    match self.byte(data, end) {
                        Some(b'\n') => break,
                        Some(_) => end = end.checked_add(1).ok_or(self.at)?,
                        None => return Err(self.at),
                    }
                }
                self.at = end;
            } else if byte.is_ascii_whitespace() {
                self.at = self.at.checked_add(1).ok_or(self.at)?;
            } else {
                self.first = byte;
                break;
            }
        }
        self.start = self.at;
        let mut end = self.at;
        while self
            .byte(data, end)
            .is_some_and(|byte| !byte.is_ascii_whitespace())
        {
            end = end.checked_add(1).ok_or(self.at)?;
        }
        self.at = end;
        Ok((self.start, end))
    }

    fn binary<B: Bytes>(&mut self, data: &mut B, length: usize) -> Result<Span, Fail> {
        let whitespace = self.byte(data, self.at).ok_or(Fail)?;
        if !whitespace.is_ascii_whitespace() {
            return Err(Fail);
        }
        self.at = self.at.checked_add(1).ok_or(Fail)?;
        if whitespace == b'\r' && self.byte(data, self.at) == Some(b'\n') {
            self.at = self.at.checked_add(1).ok_or(Fail)?;
        }
        let end = self.at.checked_add(length).ok_or(Fail)?;
        if end > self.limit.min(data.size()) {
            return Err(Fail);
        }
        let start = self.at;
        self.at = end;
        Ok((start, end))
    }
}

fn is<B: Bytes>(data: &mut B, span: Span, word: &[u8]) -> bool {
    span.1.checked_sub(span.0) == Some(word.len())
        && word
            .iter()
            .zip(span.0..)
            .all(|(expected, index)| data.at(index) == Some(*expected))
}

fn is_any<B: Bytes>(data: &mut B, span: Span, words: &[&[u8]]) -> bool {
    words.iter().any(|word| is(data, span, word))
}

fn number<B: Bytes>(data: &mut B, span: Span) -> Result<usize, Fail> {
    if span.0 >= span.1 {
        return Err(Fail);
    }
    (span.0..span.1).try_fold(0usize, |value, index| {
        let digit = data
            .at(index)
            .and_then(|byte| byte.checked_sub(b'0'))
            .filter(|digit| *digit < 10)
            .ok_or(Fail)?;
        value
            .checked_mul(10)
            .and_then(|n| n.checked_add(usize::from(digit)))
            .ok_or(Fail)
    })
}

fn signed_number<B: Bytes>(data: &mut B, span: Span) -> Result<i32, Fail> {
    let negative = span.0 < span.1 && data.at(span.0) == Some(b'-');
    let digits = if negative {
        (span.0.checked_add(1).ok_or(Fail)?, span.1)
    } else {
        span
    };
    let value = i64::try_from(number(data, digits)?).map_err(|_| Fail)?;
    let value = if negative {
        value.checked_neg().ok_or(Fail)?
    } else {
        value
    };
    i32::try_from(value).map_err(|_| Fail)
}

/// The first position of `word` in `data[..limit]`.
fn keyword<B: Bytes>(data: &mut B, word: &[u8], limit: usize) -> Option<usize> {
    let last = limit.min(data.size()).checked_sub(word.len())?;
    (0..=last).find(|&start| {
        word.iter()
            .zip(start..)
            .all(|(expected, index)| data.at(index) == Some(*expected))
    })
}

/// Read `/Subrs` and `/CharStrings` binary objects with checked lengths,
/// passing each entry to `visit`. Returns `lenIV`, or the offset in the
/// dictionary where reading failed.
pub(crate) fn scan<B: Bytes>(data: &mut B, visit: &mut dyn FnMut(Item)) -> Result<i32, usize> {
    let size = data.size();
    let charstrings_start = keyword(data, b"/CharStrings", size).ok_or(size)?;
    let subrs_start = keyword(data, b"/Subrs", size);
    let prefix_end = subrs_start
        .unwrap_or(charstrings_start)
        .min(charstrings_start);
    let len_iv = if let Some(start) = keyword(data, b"/lenIV", prefix_end) {
        let mut reader = Reader::new(start, prefix_end);
        let word = reader.word(data)?;
        if !is(data, word, b"/lenIV") {
            return Err(reader.start);
        }
        let value = reader.word(data)?;
        signed_number(data, value).map_err(|_| reader.start)?
    } else {
        4
    };
    if let Some(start) = subrs_start {
        let mut reader = Reader::new(start, size);
        let word = reader.word(data)?;
        if !is(data, word, b"/Subrs") {
            return Err(reader.start);
        }
        let word = reader.word(data)?;
        let count = number(data, word).map_err(|_| reader.start)?;
        if count > size / 4 {
            return Err(reader.start);
        }
        let word = reader.word(data)?;
        if !is(data, word, b"array") {
            return Err(reader.start);
        }
        visit(Item::SubrCount(count));
        for _ in 0..count {
            let word = reader.word(data)?;
            if !is(data, word, b"dup") {
                return Err(reader.start);
            }
            let word = reader.word(data)?;
            let index = number(data, word).map_err(|_| reader.start)?;
            let word = reader.word(data)?;
            let length = number(data, word).map_err(|_| reader.start)?;
            let word = reader.word(data)?;
            if !is_any(data, word, &[b"RD", b"-|", b"readstring"]) {
                return Err(reader.start);
            }
            let program = reader.binary(data, length).map_err(|_| size)?;
            let word = reader.word(data)?;
            if !is_any(data, word, &[b"NP", b"|-", b"put"]) || index >= count {
                return Err(reader.start);
            }
            visit(Item::Subr { index, program });
        }
    }
    let mut reader = Reader::new(charstrings_start, size);
    let word = reader.word(data)?;
    if !is(data, word, b"/CharStrings") {
        return Err(reader.start);
    }
    let word = reader.word(data)?;
    let count = number(data, word).map_err(|_| reader.start)?;
    if count == 0 || count > size / 5 {
        return Err(reader.start);
    }
    let mut first = reader.word(data)?;
    while reader.first != b'/' {
        first = reader.word(data)?;
    }
    for index in 0..count {
        let name = if index == 0 {
            first
        } else {
            reader.word(data)?
        };
        if reader.first != b'/' || name.1.checked_sub(name.0).is_none_or(|len| len < 2) {
            return Err(reader.start);
        }
        let name = (name.0.checked_add(1).ok_or(size)?, name.1);
        let word = reader.word(data)?;
        let length = number(data, word).map_err(|_| reader.start)?;
        let word = reader.word(data)?;
        if !is_any(data, word, &[b"RD", b"-|"]) {
            return Err(reader.start);
        }
        let program = reader.binary(data, length).map_err(|_| size)?;
        let word = reader.word(data)?;
        if !is_any(data, word, &[b"ND", b"|-"]) {
            return Err(reader.start);
        }
        visit(Item::Glyph { name, program });
    }
    Ok(len_iv)
}
