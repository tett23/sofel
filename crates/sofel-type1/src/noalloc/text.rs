//! PostScript text helpers shared by the Type 1 readers.

/// Recognize either cleartext Type 1 signature accepted by FreeType.
pub(crate) fn has_text_header(data: &[u8]) -> bool {
    data.starts_with(b"%!PS-AdobeFont") || data.starts_with(b"%!FontType")
}

pub(crate) fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => byte.checked_sub(b'0'),
        b'a'..=b'f' => byte.checked_sub(b'a').and_then(|n| n.checked_add(10)),
        b'A'..=b'F' => byte.checked_sub(b'A').and_then(|n| n.checked_add(10)),
        _ => None,
    }
}

/// Locate the end of the encryption operator among top-level PostScript tokens.
pub(crate) fn eexec_end(data: &[u8]) -> Option<usize> {
    let mut bytes = data.iter().copied().enumerate().peekable();
    let mut procedure_depth = 0usize;
    while let Some((start, byte)) = bytes.next() {
        match byte {
            b'%' => {
                bytes
                    .by_ref()
                    .take_while(|(_, b)| !matches!(b, b'\r' | b'\n'))
                    .count();
            }
            b'(' => {
                let mut depth = 1usize;
                while depth > 0 {
                    match bytes.next()?.1 {
                        b'\\' => {
                            bytes.next()?;
                        }
                        b'(' => depth = depth.checked_add(1)?,
                        b')' => depth = depth.checked_sub(1)?,
                        _ => {}
                    }
                }
            }
            b'<' if bytes.next_if(|(_, b)| *b == b'<').is_none() => {
                bytes.find(|(_, b)| *b == b'>')?;
            }
            b'{' => procedure_depth = procedure_depth.checked_add(1)?,
            b'}' => procedure_depth = procedure_depth.saturating_sub(1),
            b if b == b'/' || !ps_delimiter(b) => {
                while bytes.next_if(|(_, next)| !ps_delimiter(*next)).is_some() {}
                let end = bytes.peek().map_or(data.len(), |(offset, _)| *offset);
                if procedure_depth == 0 && data.get(start..end) == Some(b"eexec") {
                    return Some(end);
                }
            }
            _ => {}
        }
    }
    None
}

fn ps_delimiter(byte: u8) -> bool {
    byte.is_ascii_whitespace() || b"()<>[]{}/%".contains(&byte)
}

/// The offset of `inner` in `outer`, which contains it.
pub(crate) fn offset_in(outer: &[u8], inner: &[u8]) -> usize {
    (inner.as_ptr() as usize).saturating_sub(outer.as_ptr() as usize)
}
