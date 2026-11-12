//! Extended AAT metamorphosis chains and state-driven rearrangement.
use sofel_font::read::{aat_lookup::AatLookup, be};

/// A chain subtable and its application flags.
#[derive(Clone, Copy, Debug)]
pub struct Subtable<'a> {
    /// Coverage direction flags.
    pub coverage: u32,
    /// Chain flags required for this subtable.
    pub flags: u32,
    /// Operation on the glyph buffer.
    pub action: Action<'a>,
}
/// The default flags and ordered operations in a morx chain.
#[derive(Debug, Clone)]
pub struct Chain<'a> {
    /// Initially enabled feature flags.
    pub default_flags: u32,
    /// Subtables in application order.
    pub substitutions: alloc::vec::Vec<Subtable<'a>>,
}
/// Reads morx version 2 chains of supported operations.
/// Other subtable types require their own state machine.
pub fn chains(data: &[u8], glyphs: u32) -> Option<alloc::vec::Vec<Chain<'_>>> {
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
            let payload = st.get(12..)?;
            let action = match coverage & 255 {
                0 => Action::Rearrange(StateTable::new(payload, glyphs)?),
                4 => Action::Replace(AatLookup::new(payload, glyphs)?),
                _ => return None,
            };
            substitutions.push(Subtable {
                coverage,
                flags: be::u32_at(st, 8)?,
                action,
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

#[derive(Clone, Copy, Debug)]
pub enum Action<'a> {
    Replace(AatLookup<'a>),
    Rearrange(StateTable<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct StateTable<'a> {
    classes: AatLookup<'a>,
    num_classes: usize,
    states: &'a [u8],
    entries: &'a [u8],
}

impl<'a> StateTable<'a> {
    fn new(data: &'a [u8], glyphs: u32) -> Option<Self> {
        let num_classes = be::u32_at(data, 0)? as usize;
        if num_classes < 4 {
            return None;
        }
        let offset = |at| data.get(be::u32_at(data, at)? as usize..);
        let table = Self {
            classes: AatLookup::new(offset(4)?, glyphs)?,
            num_classes,
            states: offset(8)?,
            entries: offset(12)?,
        };
        // Start at the text entry state and check each reachable row.
        let mut states = 1;
        let mut state = 0;
        while state < states {
            for class in 0..num_classes {
                let (next, _) = table.entry(state, class)?;
                states = states.max(usize::from(next).saturating_add(1));
            }
            state = state.saturating_add(1);
        }
        Some(table)
    }
    fn entry(&self, state: usize, class: usize) -> Option<(u16, u16)> {
        let cell = state
            .checked_mul(self.num_classes)?
            .checked_add(class)?
            .checked_mul(2)?;
        let offset = usize::from(be::u16_at(self.states, cell)?).checked_mul(4)?;
        Some((
            be::u16_at(self.entries, offset)?,
            be::u16_at(self.entries, offset.checked_add(2)?)?,
        ))
    }
    pub fn rearrange(&self, buffer: &mut crate::buffer::Buffer) {
        let mut state = 0;
        let mut idx = 0usize;
        let mut first = 0;
        let mut last = 0;
        let len = buffer.infos().len();
        while idx <= len {
            let class = buffer.infos().get(idx).map_or(0, |g| {
                if g.codepoint == 0xffff {
                    return 2;
                }
                let c = usize::from(self.classes.get(g.codepoint).unwrap_or(1));
                if c < self.num_classes { c } else { 1 }
            });
            let Some((next, flags)) = self.entry(state, class) else {
                break;
            };
            let dont_advance = flags & 0x4000 != 0;
            let safe = flags & 15 == 0
                && (state == 0
                    || (dont_advance && next == 0)
                    || self.entry(0, class).is_some_and(|(s, f)| {
                        s == next && f & 15 == 0 && (f ^ flags) & 0x4000 == 0
                    }))
                && self.entry(state, 0).is_some_and(|(_, f)| f & 15 == 0);
            if !safe && idx > 0 && idx < len {
                buffer.unsafe_to_break(idx.saturating_sub(1), idx.saturating_add(1));
            }
            if flags & 0x8000 != 0 {
                first = idx;
            }
            if flags & 0x2000 != 0 {
                last = idx.saturating_add(1).min(len);
            }
            let verb = usize::from(flags & 15);
            if verb != 0 && first < last {
                apply_verb(buffer, first, last, idx.saturating_add(1).min(len), verb);
            }
            if idx == len {
                break;
            }
            state = usize::from(next);
            if dont_advance {
                buffer.max_ops = buffer.max_ops.saturating_sub(1);
            }
            if !dont_advance || buffer.max_ops < 0 {
                idx = idx.saturating_add(1);
            }
        }
    }
}

fn apply_verb(
    buffer: &mut crate::buffer::Buffer,
    first: usize,
    last: usize,
    current_end: usize,
    verb: usize,
) {
    // Each nibble is the count moved to the other end; 3 moves two in reverse.
    const VERBS: [u8; 16] = [
        0, 0x10, 1, 0x11, 0x20, 0x30, 2, 3, 0x12, 0x13, 0x21, 0x31, 0x22, 0x32, 0x23, 0x33,
    ];
    let Some(&sides) = VERBS.get(verb) else {
        return;
    };
    let left = usize::from((sides >> 4).min(2));
    let right = usize::from((sides & 15).min(2));
    let length = last.saturating_sub(first);
    if length > 64 || length < left.saturating_add(right) {
        return;
    }
    buffer.merge_clusters(first, current_end);
    buffer.merge_clusters(first, last);
    let Some(span) = buffer.infos_mut().get_mut(first..last) else {
        return;
    };
    span.rotate_left(left);
    if let Some(prefix) = span.get_mut(..length.saturating_sub(left)) {
        prefix.rotate_right(right);
    }
    if sides >> 4 == 3
        && let Some(tail) = span.get_mut(length.saturating_sub(left)..)
    {
        tail.reverse();
    }
    if sides & 15 == 3
        && let Some(head) = span.get_mut(..right)
    {
        head.reverse();
    }
}
