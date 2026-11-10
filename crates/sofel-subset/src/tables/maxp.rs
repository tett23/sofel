//! maxp and MAXP (`src/hb-ot-maxp-table.hh`).

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::be::u16_at;
use sofel_write::Serializer;

use crate::error::SubsetError;
use crate::input::Flags;
use crate::plan::Plan;
use crate::tables::Tables;

/// `maxp_MAXP::subset`.
pub(crate) fn subset(
    plan: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let wide = tag == Tag::new(b"MAXP");
    let head = if wide { 7 } else { 6 };
    let major = u16_at(src, 0).ok_or(SubsetError::Malformed { tag })?;
    let minor = u16_at(src, 2).ok_or(SubsetError::Malformed { tag })?;
    let v1 = major == 1;
    if src.len() < head
        || !(v1 || (major == 0 && minor == 0x5000))
        || (v1 && src.len() < head.saturating_add(26))
    {
        return Err(SubsetError::Malformed { tag });
    }
    s.extend(src.get(..head).unwrap_or(&[]));
    if wide {
        let n = plan.num_output_glyphs.min(0xFF_FFFF);
        s.set_u24(4, n);
    } else {
        s.set_u16(4, (plan.num_output_glyphs.min(0xFFFF) & 0xFFFF) as u16);
    }
    if v1 {
        let at = s.extend(src.get(head..head.saturating_add(26)).unwrap_or(&[]));
        if plan.flags.has(Flags::NO_HINTING) {
            // maxZones = 1, then the six hinting fields 0.
            s.set_u16(at.saturating_add(8), 1);
            (10..22)
                .step_by(2)
                .for_each(|o| s.set_u16(at.saturating_add(o), 0));
        }
    }
    Ok(true)
}
