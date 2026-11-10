//! sbix (`src/OT/Color/sbix/sbix.hh`): the strikes that keep a glyph,
//! each with the glyph data of the kept glyphs in the new order.

use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::sbix::Sbix;
use sofel_write::Serializer;

use super::Tables;
use crate::error::SubsetError;
use crate::layout::data::{D, n32};
use crate::plan::Plan;

/// `SBIXGlyph::min_size`.
const GLYPH_HEADER: u32 = 8;

/// `SBIXStrike::subset` of the strike at `at` into the pushed object.
fn strike(plan: &Plan, t: D<'_>, at: usize, available: u32, s: &mut Serializer) -> bool {
    let strike = t.at(at);
    let n = plan.num_output_glyphs;
    let snap = s.snapshot();
    let header = 4usize.saturating_add((n as usize).saturating_add(1).saturating_mul(4));
    s.zeros(header);
    s.set_u16(0, strike.u16(0));
    s.set_u16(2, strike.u16(2));
    let mut head = n32(header);
    let mut has_glyphs = false;
    let slot = |g: u32| 4usize.saturating_add((g as usize).saturating_mul(4));
    for new in 0..n {
        let found = plan.glyphs.old_gid(new).and_then(|old| {
            let a = strike.u32(slot(old));
            let b = strike.u32(slot(old.saturating_add(1)));
            let ok =
                a != 0 && b != 0 && b > a && b.wrapping_sub(a) > GLYPH_HEADER && b <= available;
            ok.then_some((a, b))
        });
        s.set_u32(slot(new), head);
        let Some((a, b)) = found else {
            continue;
        };
        has_glyphs = true;
        let delta = b.wrapping_sub(a);
        let glyph = strike.at(a as usize);
        let data: Vec<u8> = (0..delta as usize).map(|i| glyph.u8(i)).collect();
        s.extend(&data);
        head = head.wrapping_add(delta);
    }
    if has_glyphs {
        s.set_u32(slot(n), head);
    } else {
        s.revert(snap);
    }
    has_glyphs
}

/// `sbix::subset`.
pub(crate) fn subset(
    plan: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    if Sbix::new(src).map_or(true, |x| x.version() < 1) {
        return Err(SubsetError::Failed { tag });
    }
    let t = D(src);
    s.u16(t.u16(0));
    s.u16(t.u16(2));
    // serialize_strike_offsets: an Array32OfOffset32To<SBIXStrike> at 4.
    s.u32(0);
    let mut len = 0u32;
    let mut slots: Vec<usize> = Vec::new();
    let mut objidxs: Vec<u32> = Vec::new();
    let table_len = n32(src.len());
    for i in (0..t.u32(4) as usize).rev() {
        // serialize_append: a slot left by a failed strike is reused.
        len = len.saturating_add(1);
        s.set_u32(4, len);
        let o = 8usize.saturating_add((len as usize).saturating_sub(1).saturating_mul(4));
        if s.len() < o.saturating_add(4) {
            s.zeros(4);
        }
        s.set_u32(o, 0);
        let snap = s.snapshot();
        s.push();
        let off = t.u32(8usize.saturating_add(i.saturating_mul(4)));
        let ok = off != 0
            && table_len >= off
            && strike(plan, t, off as usize, table_len.wrapping_sub(off), s);
        if ok {
            objidxs.push(s.pop_pack(true));
            slots.push(o);
        } else {
            s.pop_discard();
            len = len.saturating_sub(1);
            s.set_u32(4, len);
            s.revert(snap);
        }
    }
    for (i, &o) in slots.iter().enumerate() {
        let idx = objidxs
            .get(objidxs.len().saturating_sub(1).saturating_sub(i))
            .copied()
            .unwrap_or(0);
        s.link32(o, idx);
    }
    Ok(true)
}
