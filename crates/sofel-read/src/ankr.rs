//! AAT anchor points referenced by kerx format 4.
use crate::{aat_lookup::AatLookup, be};

/// A glyph lookup and its checked arrays of design-unit anchor coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Ankr<'a> {
    lookup: AatLookup<'a>,
    points: &'a [u8],
}

fn list(data: &[u8], offset: u16) -> Option<&[[u8; 4]]> {
    let body = data.get(usize::from(offset)..)?;
    be::chunks_at::<4>(body, 4, be::u32_at(body, 0)? as usize)
}

impl<'a> Ankr<'a> {
    /// Checks each referenced list. The header offsets are independent.
    pub fn new(data: &'a [u8], glyphs: u32) -> Option<Self> {
        if be::u16_at(data, 0)? != 0 {
            return None;
        }
        let points = data.get(be::u32_at(data, 8)? as usize..)?;
        let lookup = AatLookup::new(data.get(be::u32_at(data, 4)? as usize..)?, glyphs)?;
        lookup.check_offsets(|offset| list(points, offset).map(|_| ()))?;
        Some(Self { lookup, points })
    }

    /// Returns the signed coordinates of one glyph anchor in font units.
    pub fn get(&self, glyph: u32, index: u16) -> Option<(i32, i32)> {
        let [a, b, c, d] = *list(self.points, self.lookup.get(glyph)?)?.get(usize::from(index))?;
        Some((
            i32::from(i16::from_be_bytes([a, b])),
            i32::from(i16::from_be_bytes([c, d])),
        ))
    }
}
