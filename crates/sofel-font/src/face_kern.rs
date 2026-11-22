//! FreeType's format-zero horizontal `kern` pair query (root ADR 0100).

use crate::{FaceError, Font};
use sofel_read::{GlyphId, Tag, be};

pub(crate) fn sfnt_pair(font: &Font<'_>, left: GlyphId, right: GlyphId) -> Result<i32, FaceError> {
    let Some(table) = font.table(Tag::new(b"kern")) else {
        return Ok(0);
    };
    if table.len() < 4 || left.0 > u32::from(u16::MAX) || right.0 > u32::from(u16::MAX) {
        return Ok(0);
    }
    let count = usize::from(be::u16_at(table, 2).unwrap_or(0)).min(32);
    let key = (left.0 << 16) | right.0;
    let mut at = 4usize;
    let mut result = 0i32;
    for _ in 0..count {
        let Some(header) = table.get(at..at.saturating_add(6)) else {
            break;
        };
        let length = usize::from(be::u16_at(header, 2).unwrap_or(0));
        let coverage = be::u16_at(header, 4).unwrap_or(0);
        if length <= 14 {
            break;
        }
        let end = at.saturating_add(length).min(table.len());
        let Some(body) = table.get(at.saturating_add(6)..end) else {
            break;
        };
        if coverage >> 8 == 0 && coverage & 3 == 1 && body.len() >= 8 {
            let pairs = body.get(8..).unwrap_or_default();
            let declared = usize::from(be::u16_at(body, 0).unwrap_or(0));
            let n = declared.min(pairs.len() / 6);
            let mut value = None;
            for row in pairs.as_chunks::<6>().0.iter().take(n) {
                if be::u32_at(row, 0) == Some(key) {
                    value = be::i16_at(row, 4).map(i32::from);
                    break;
                }
            }
            if let Some(value) = value {
                result = if coverage & 8 != 0 {
                    value
                } else {
                    result.checked_add(value).ok_or(FaceError::Overflow)?
                };
            }
        }
        at = end;
    }
    Ok(result)
}
