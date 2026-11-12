//! Extended AAT metamorphosis chains and state-driven rearrangement.
use sofel_font::read::{aat_lookup::AatLookup, be};

/// A chain subtable and its application flags.
#[derive(Clone, Debug)]
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
                0 | 1 => Action::Machine(StateTable::new(payload, glyphs, coverage & 255 == 1)?),
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

#[derive(Clone, Debug)]
pub enum Action<'a> {
    Replace(AatLookup<'a>),
    Machine(StateTable<'a>),
}

#[derive(Clone, Debug)]
pub struct StateTable<'a> {
    classes: AatLookup<'a>,
    num_classes: usize,
    states: &'a [u8],
    entries: &'a [u8],
    lookups: Option<alloc::vec::Vec<AatLookup<'a>>>,
}

#[derive(Clone, Copy)]
struct Entry {
    next: u16,
    flags: u16,
    substitutions: [u16; 2],
}

impl<'a> StateTable<'a> {
    fn new(data: &'a [u8], glyphs: u32, contextual: bool) -> Option<Self> {
        let num_classes = be::u32_at(data, 0)? as usize;
        if num_classes < 4 {
            return None;
        }
        let offset = |at| data.get(be::u32_at(data, at)? as usize..);
        let mut table = Self {
            classes: AatLookup::new(offset(4)?, glyphs)?,
            num_classes,
            states: offset(8)?,
            entries: offset(12)?,
            lookups: contextual.then(alloc::vec::Vec::new),
        };
        // Start at the text entry state and check each reachable row.
        let mut states = 1;
        let mut lookup_count = 0;
        let mut state = 0;
        while state < states {
            for class in 0..num_classes {
                let entry = table.entry(state, class)?;
                let next = entry.next;
                for index in entry.substitutions.into_iter().filter(|i| *i != 0xffff) {
                    lookup_count = lookup_count.max(usize::from(index).saturating_add(1));
                }
                states = states.max(usize::from(next).saturating_add(1));
            }
            state = state.saturating_add(1);
        }
        if let Some(lookups) = &mut table.lookups {
            let list = offset(16)?;
            let offsets = be::chunks_at::<4>(list, 0, lookup_count)?;
            for raw in offsets {
                lookups.push(AatLookup::new(
                    list.get(u32::from_be_bytes(*raw) as usize..)?,
                    glyphs,
                )?);
            }
        }
        Some(table)
    }
    fn entry(&self, state: usize, class: usize) -> Option<Entry> {
        let cell = state
            .checked_mul(self.num_classes)?
            .checked_add(class)?
            .checked_mul(2)?;
        let stride = if self.lookups.is_some() { 8 } else { 4 };
        let offset = usize::from(be::u16_at(self.states, cell)?).checked_mul(stride)?;
        let row = self.entries.get(offset..offset.checked_add(stride)?)?;
        Some(Entry {
            next: be::u16_at(row, 0)?,
            flags: be::u16_at(row, 2)?,
            substitutions: if stride == 8 {
                [be::u16_at(row, 4)?, be::u16_at(row, 6)?]
            } else {
                [0xffff; 2]
            },
        })
    }
    fn actionable(&self, entry: Entry) -> bool {
        if self.lookups.is_some() {
            entry.substitutions != [0xffff; 2]
        } else {
            entry.flags & 15 != 0
        }
    }
    pub fn drive(
        &self,
        buffer: &mut crate::buffer::Buffer,
        replace: &mut impl FnMut(&mut crate::buffer::Info, u16),
    ) {
        let mut state = 0;
        let mut idx = 0usize;
        let mut first = 0;
        let mut marked = false;
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
            let Some(entry) = self.entry(state, class) else {
                break;
            };
            let Entry {
                next,
                flags,
                substitutions,
            } = entry;
            let dont_advance = flags & 0x4000 != 0;
            let safe = !self.actionable(entry)
                && (state == 0
                    || (dont_advance && next == 0)
                    || self.entry(0, class).is_some_and(|e| {
                        e.next == next && !self.actionable(e) && (e.flags ^ flags) & 0x4000 == 0
                    }))
                && self.entry(state, 0).is_some_and(|e| !self.actionable(e));
            if !safe && idx > 0 && idx < len {
                buffer.unsafe_to_break(idx.saturating_sub(1), idx.saturating_add(1));
            }
            if let Some(lookups) = &self.lookups {
                if idx < len || marked {
                    for (index, slot, is_mark) in [
                        (substitutions[0], first, true),
                        (substitutions[1], idx.min(len.saturating_sub(1)), false),
                    ] {
                        let glyph = buffer
                            .infos()
                            .get(slot)
                            .and_then(|g| lookups.get(usize::from(index))?.get(g.codepoint));
                        if let Some(glyph) = glyph {
                            if is_mark {
                                buffer.unsafe_to_break(first, idx.saturating_add(1).min(len));
                            }
                            if let Some(info) = buffer.infos_mut().get_mut(slot) {
                                replace(info, glyph);
                            }
                        }
                    }
                    if flags & 0x8000 != 0 {
                        marked = true;
                        first = idx;
                    }
                }
            } else {
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
