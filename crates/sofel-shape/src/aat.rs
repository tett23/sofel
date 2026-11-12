//! Extended AAT metamorphosis chains and noncontextual substitutions.
use sofel_font::read::{aat_lookup::AatLookup, be};

/// A chain subtable with a glyph-to-glyph lookup.
#[derive(Clone, Copy, Debug)]
pub struct Noncontextual<'a> {
    /// Coverage direction flags.
    pub coverage: u32,
    /// Chain flags required for this subtable.
    pub flags: u32,
    /// Substitution lookup.
    pub lookup: AatLookup<'a>,
}
/// The feature selectors and noncontextual operations in a morx chain.
#[derive(Debug, Clone)]
pub struct Chain<'a> {
    /// Initially enabled feature flags.
    pub default_flags: u32,
    /// Subtables in application order.
    pub substitutions: alloc::vec::Vec<Noncontextual<'a>>,
}
/// Reads morx version 2 chains consisting of noncontextual substitutions.
/// Other subtable types require their own state machine.
pub fn noncontextual_chains(data: &[u8], glyphs: u32) -> Option<alloc::vec::Vec<Chain<'_>>> {
    if be::u16_at(data, 0)? != 2 {
        return None;
    }
    let count = be::u32_at(data, 4)?;
    let mut rest = data.get(8..)?;
    let mut chains = alloc::vec::Vec::new();
    for _ in 0..count {
        let (chain, tail) = rest.split_at_checked(be::u32_at(rest, 4)? as usize)?;
        let flags = be::u32_at(chain, 0)?;
        let n = be::u32_at(chain, 8)? as usize;
        let tables = be::u32_at(chain, 12)?;
        be::chunks_at::<12>(chain, 16, n)?;
        let mut body = chain.get(16usize.checked_add(n.checked_mul(12)?)?..)?;
        let mut substitutions = alloc::vec::Vec::new();
        for _ in 0..tables {
            let (st, tail) = body.split_at_checked(be::u32_at(body, 0)? as usize)?;
            let coverage = be::u32_at(st, 4)?;
            if coverage & 255 != 4 {
                return None;
            }
            substitutions.push(Noncontextual {
                coverage,
                flags: be::u32_at(st, 8)?,
                lookup: AatLookup::new(st.get(12..)?, glyphs)?,
            });
            body = tail;
        }
        chains.push(Chain {
            default_flags: flags,
            substitutions,
        });
        rest = tail;
    }
    Some(chains)
}
