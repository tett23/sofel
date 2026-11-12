//! UNIX compress streams pack eight little-endian codes into each group.
//! A reset or a width change discards the unused tail of the current group.
use super::{Diagnostic, diagnostic};

struct Codes<'a> {
    input: &'a [u8],
    group: &'a [u8],
    offset: usize,
    bit: usize,
}

impl Codes<'_> {
    fn next(&mut self, width: usize) -> Result<Option<usize>, Diagnostic> {
        if self.bit.saturating_add(width) > self.group.len().saturating_mul(8) {
            let count = width.min(self.input.len());
            if count == 0 {
                return Ok(None);
            }
            self.offset = self.offset.saturating_add(self.group.len());
            (self.group, self.input) = self.input.split_at(count);
            self.bit = 0;
            if count.saturating_mul(8) < width {
                return Err(diagnostic(self.offset, "truncated LZW code"));
            }
        }
        let mut code = 0;
        for shift in 0..width {
            let pos = self.bit.saturating_add(shift);
            let byte = self.group.get(pos / 8).copied().unwrap_or(0);
            code |= usize::from((byte >> (pos % 8)) & 1) << shift;
        }
        self.bit = self.bit.saturating_add(width);
        Ok(Some(code))
    }
    fn align(&mut self) {
        self.bit = self.group.len().saturating_mul(8);
    }
}

pub(super) fn decode(data: &[u8], limit: usize) -> Result<Vec<u8>, Diagnostic> {
    let (&flags, input) = data
        .get(2..)
        .and_then(|s| s.split_first())
        .ok_or_else(|| diagnostic(data.len(), "missing LZW flags"))?;
    let max_width = usize::from(flags & 31);
    if !(9..=16).contains(&max_width) || flags & 0x60 != 0 {
        return Err(diagnostic(2, "invalid LZW flags"));
    }
    let block_mode = flags & 0x80 != 0;
    let first_entry: usize = if block_mode { 257 } else { 256 };
    let mut codes = Codes {
        input,
        group: &[],
        offset: 3,
        bit: 0,
    };
    let mut dictionary: Vec<(usize, u8)> = Vec::new();
    let mut reverse = Vec::new();
    let mut out = Vec::new();
    let mut previous = None;
    let mut width = 9;
    loop {
        let next = first_entry.saturating_add(dictionary.len());
        if next >= 1 << width && width < max_width {
            width = width.saturating_add(1);
            codes.align();
        }
        let Some(code) = codes.next(width)? else {
            break;
        };
        if block_mode && code == 256 {
            previous = None;
            dictionary.clear();
            codes.align();
            width = 9;
            continue;
        }
        let invalid = || {
            diagnostic(
                codes
                    .offset
                    .saturating_add(codes.bit.saturating_sub(width) / 8),
                "invalid LZW dictionary code",
            )
        };
        let repeated = code == next;
        let mut current = if repeated {
            previous.ok_or_else(invalid)?
        } else {
            code
        };
        reverse.clear();
        while current >= 256 {
            let &(prefix, suffix) = current
                .checked_sub(first_entry)
                .and_then(|i| dictionary.get(i))
                .ok_or_else(invalid)?;
            reverse.push(suffix);
            current = prefix;
        }
        let first = u8::try_from(current).map_err(|_| invalid())?;
        reverse.push(first);
        let count = reverse.len().saturating_add(usize::from(repeated));
        if count > limit.saturating_sub(out.len()) {
            return Err(diagnostic(
                codes.offset,
                "LZW output exceeds the font size limit",
            ));
        }
        out.extend(reverse.iter().rev().copied());
        if repeated {
            out.push(first);
        }
        if let Some(prefix) = previous
            && next < 1 << max_width
        {
            dictionary.push((prefix, first));
        }
        previous = Some(code);
    }
    Ok(out)
}
