//! PFA and PFB files: their layout, their `eexec` data decrypted while it
//! is read, and their validators (sofel ADR 0096).

use crate::noalloc::Problem;
use crate::noalloc::charstrings::{self, Bytes};
use crate::noalloc::cipher::{EEXEC_SEED, step};
use crate::noalloc::encoding;
use crate::noalloc::mm;
use crate::noalloc::pfb::{self, SectionKind, Sections};
use crate::noalloc::text::{eexec_end, has_text_header, hex_nibble, offset_in};

/// Where the parts of a PFA are: the cleartext ends at `start`, where the
/// hexadecimal `eexec` data of `cipher_len` bytes begins.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PfaLayout {
    pub(crate) start: usize,
    pub(crate) cipher_len: usize,
}

/// Check the cleartext header and the hexadecimal `eexec` data of a PFA,
/// or return the file offset of the problem.
pub(crate) fn pfa_layout(data: &[u8], max_output: usize) -> Result<PfaLayout, usize> {
    if !has_text_header(data) {
        return Err(0);
    }
    let after = eexec_end(data).ok_or(data.len())?;
    let whitespace = data
        .get(after..)
        .ok_or(data.len())?
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count();
    let start = after.checked_add(whitespace).ok_or(data.len())?;
    let mut cipher_len = 0usize;
    let mut high = false;
    let mut ended = false;
    for line in data
        .get(start..)
        .ok_or(data.len())?
        .split(|byte| matches!(byte, b'\r' | b'\n'))
    {
        let line = line.trim_ascii();
        if line.starts_with(b"cleartomark") {
            ended = true;
            break;
        }
        let base = offset_in(data, line);
        for (index, &byte) in line.iter().enumerate() {
            if byte.is_ascii_whitespace() {
                continue;
            }
            hex_nibble(byte).ok_or(base.saturating_add(index))?;
            if high {
                if cipher_len >= max_output {
                    return Err(data.len());
                }
                cipher_len = cipher_len.saturating_add(1);
            }
            high = !high;
        }
    }
    if !ended || high || cipher_len < 4 {
        return Err(data.len());
    }
    Ok(PfaLayout { start, cipher_len })
}

/// The `eexec` bytes of a PFA whose [`pfa_layout`] was accepted.
#[derive(Clone)]
pub(crate) struct PfaCipher<'a> {
    data: &'a [u8],
    at: usize,
    line_checked: bool,
}

impl<'a> PfaCipher<'a> {
    pub(crate) fn new(data: &'a [u8], layout: PfaLayout) -> Self {
        Self {
            data,
            at: layout.start,
            line_checked: false,
        }
    }

    fn nibble(&mut self) -> Option<u8> {
        loop {
            let byte = *self.data.get(self.at)?;
            if matches!(byte, b'\r' | b'\n') {
                self.at = self.at.checked_add(1)?;
                self.line_checked = false;
                continue;
            }
            if !self.line_checked {
                let rest = self.data.get(self.at..)?;
                let end = rest
                    .iter()
                    .position(|byte| matches!(byte, b'\r' | b'\n'))
                    .unwrap_or(rest.len());
                if rest.get(..end)?.trim_ascii().starts_with(b"cleartomark") {
                    return None;
                }
                self.line_checked = true;
            }
            self.at = self.at.checked_add(1)?;
            if !byte.is_ascii_whitespace() {
                return hex_nibble(byte);
            }
        }
    }
}

impl Iterator for PfaCipher<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        let high = self.nibble()?;
        let low = self.nibble()?;
        Some((high << 4) | low)
    }
}

/// Where the parts of a PFB are: the cleartext of its first section, and
/// `cipher_len` bytes of binary sections after it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PfbLayout<'a> {
    pub(crate) cleartext: &'a [u8],
    pub(crate) cipher_len: usize,
}

/// Check the sections of a PFB, or return the file offset of the problem.
pub(crate) fn pfb_layout(data: &[u8], max_output: usize) -> Result<PfbLayout<'_>, usize> {
    let mut first = None;
    let mut cipher_len = 0usize;
    let mut trailing = false;
    let mut late_binary = None;
    pfb::sections(data, max_output, &mut |section| {
        if first.is_none() {
            first = Some(section);
            return;
        }
        match section.kind {
            SectionKind::Binary if !trailing => {
                cipher_len = cipher_len.saturating_add(section.data.len());
            }
            SectionKind::Ascii => trailing = true,
            SectionKind::Binary => {
                late_binary.get_or_insert(offset_in(data, section.data));
            }
        }
    })?;
    let first = first.ok_or(data.len())?;
    if first.kind != SectionKind::Ascii
        || !has_text_header(first.data)
        || eexec_end(first.data).is_none()
    {
        return Err(6);
    }
    if let Some(offset) = late_binary {
        return Err(offset);
    }
    if cipher_len < 4 {
        return Err(data.len());
    }
    Ok(PfbLayout {
        cleartext: first.data,
        cipher_len,
    })
}

/// The `eexec` bytes of a PFB whose [`pfb_layout`] was accepted.
#[derive(Clone)]
pub(crate) struct PfbCipher<'a> {
    sections: Sections<'a>,
    current: &'a [u8],
    started: bool,
}

impl<'a> PfbCipher<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self {
            sections: Sections::new(data),
            current: &[],
            started: false,
        }
    }
}

impl Iterator for PfbCipher<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        if !self.started {
            self.sections.next()?;
            self.started = true;
        }
        loop {
            if let Some((&byte, rest)) = self.current.split_first() {
                self.current = rest;
                return Some(byte);
            }
            let section = self.sections.next()?;
            if section.kind != SectionKind::Binary {
                self.sections = Sections::new(&[]);
                return None;
            }
            self.current = section.data;
        }
    }
}

/// How many decrypted bytes [`Decrypted`] keeps to answer reads just
/// behind its cursor without starting again.
const RECENT: usize = 64;

/// The private dictionary decrypted from `eexec` bytes as it is read,
/// without the four random bytes. Reading far behind the cursor decrypts
/// again from the start.
pub(crate) struct Decrypted<I> {
    origin: I,
    cipher: I,
    state: u16,
    next: usize,
    size: usize,
    recent: [u8; RECENT],
}

impl<I: Iterator<Item = u8> + Clone> Decrypted<I> {
    pub(crate) fn new(cipher: I, cipher_len: usize) -> Self {
        let mut decrypted = Self {
            origin: cipher.clone(),
            cipher,
            state: EEXEC_SEED,
            next: 0,
            size: cipher_len.saturating_sub(4),
            recent: [0; RECENT],
        };
        decrypted.restart();
        decrypted
    }

    fn restart(&mut self) {
        self.cipher = self.origin.clone();
        self.state = EEXEC_SEED;
        self.next = 0;
        for _ in 0..4 {
            if let Some(byte) = self.cipher.next() {
                step(&mut self.state, byte);
            }
        }
    }
}

impl<I: Iterator<Item = u8> + Clone> Bytes for Decrypted<I> {
    fn size(&self) -> usize {
        self.size
    }

    fn at(&mut self, index: usize) -> Option<u8> {
        if index >= self.size {
            return None;
        }
        if index.saturating_add(RECENT) < self.next {
            self.restart();
        }
        while self.next <= index {
            let byte = step(&mut self.state, self.cipher.next()?);
            *self.recent.get_mut(self.next % RECENT)? = byte;
            self.next = self.next.checked_add(1)?;
        }
        self.recent.get(index % RECENT).copied()
    }
}

/// Check what `Type1Face::new` reads: the encoding and Multiple Masters
/// dictionaries of the cleartext, and the private dictionary. Fails with
/// the offset in the private dictionary of a charstring problem, if any.
fn check_face<I: Iterator<Item = u8> + Clone>(
    cleartext: &[u8],
    private: &mut Decrypted<I>,
) -> Result<(), Option<usize>> {
    let encoding = encoding::scan(cleartext, &mut |_, _| {});
    let charstrings = charstrings::scan(private, &mut |_| {});
    let mm = mm::scan(cleartext, &mut |_| {}, &mut |_| {});
    match charstrings {
        Err(index) => Err(Some(index)),
        Ok(_) if encoding.is_err() || mm.is_err() => Err(None),
        Ok(_) => Ok(()),
    }
}

const PFA_REASON: &str = "invalid Type 1 font (PFA)";
const PFB_REASON: &str = "invalid Type 1 font (PFB)";

/// Validate a PFA font: its header, its hexadecimal `eexec` data, and the
/// dictionaries `Type1Face::new` reads, decrypting while reading.
pub fn validate_pfa(data: &[u8], max_output: usize, report: &mut dyn FnMut(Problem)) {
    let offset = match pfa_layout(data, max_output) {
        Err(offset) => offset,
        Ok(layout) => {
            let cleartext = data.get(..layout.start).unwrap_or(&[]);
            let mut private = Decrypted::new(PfaCipher::new(data, layout), layout.cipher_len);
            match check_face(cleartext, &mut private) {
                Ok(()) => return,
                Err(index) => index
                    .and_then(|index| pfa_private_offset(data, layout.start, index))
                    .unwrap_or(0),
            }
        }
    };
    report(Problem {
        offset,
        reason: PFA_REASON,
    });
}

/// Validate a PFB font: its sections, and the dictionaries
/// `Type1Face::new` reads, decrypting while reading.
pub fn validate_pfb(data: &[u8], max_output: usize, report: &mut dyn FnMut(Problem)) {
    let offset = match pfb_layout(data, max_output) {
        Err(offset) => offset,
        Ok(layout) => {
            let mut private = Decrypted::new(PfbCipher::new(data), layout.cipher_len);
            match check_face(layout.cleartext, &mut private) {
                Ok(()) => return,
                Err(index) => index
                    .and_then(|index| pfb_private_offset(data, index))
                    .unwrap_or(0),
            }
        }
    };
    report(Problem {
        offset,
        reason: PFB_REASON,
    });
}

/// The file offset of a private dictionary byte of a PFA.
pub(crate) fn pfa_private_offset(file: &[u8], start: usize, index: usize) -> Option<usize> {
    let cipher_index = index.checked_add(4)?;
    let (relative, _) = file
        .get(start..)?
        .iter()
        .enumerate()
        .filter(|(_, byte)| byte.is_ascii_hexdigit())
        .nth(cipher_index.checked_mul(2)?)?;
    start.checked_add(relative)
}

/// The file offset of a private dictionary byte of a PFB.
pub(crate) fn pfb_private_offset(file: &[u8], index: usize) -> Option<usize> {
    let mut cipher_index = index.checked_add(4)?;
    for section in Sections::new(file).filter(|s| s.kind == SectionKind::Binary) {
        if cipher_index < section.data.len() {
            return offset_in(file, section.data).checked_add(cipher_index);
        }
        cipher_index = cipher_index.checked_sub(section.data.len())?;
    }
    Some(file.len())
}
