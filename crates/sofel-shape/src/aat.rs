//! Extended AAT metamorphosis chains and state-driven rearrangement.
pub(crate) mod features;
pub(crate) mod kerning;
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
    pub(super) ranges: features::Ranges,
    /// Subtables in application order.
    pub substitutions: alloc::vec::Vec<Subtable<'a>>,
}
/// Reads extended morx or legacy mort substitution chains.
pub fn chains<'a>(
    data: &'a [u8],
    glyphs: u32,
    legacy: bool,
    tags: Option<sofel_font::read::ltag::Ltag<'_>>,
    language: Option<&crate::Language>,
    selections: &features::Selections,
) -> Option<alloc::vec::Vec<Chain<'a>>> {
    let version = be::u16_at(data, 0)?;
    if version == 0 {
        return None;
    }
    let count = be::u32_at(data, 4)?;
    let mut rest = data.get(8..)?;
    let mut chains = alloc::vec::Vec::new();
    let mut map_budget = 1usize << 24;
    let mut remaining_subtables = 1024u32;
    for _ in 0..count {
        let (chain, tail) = rest.split_at_checked(be::u32_at(rest, 4)? as usize)?;
        let flags = be::u32_at(chain, 0)?;
        let (header, n, tables) = if legacy {
            (
                12usize,
                usize::from(be::u16_at(chain, 8)?),
                u32::from(be::u16_at(chain, 10)?),
            )
        } else {
            (16, be::u32_at(chain, 8)? as usize, be::u32_at(chain, 12)?)
        };
        remaining_subtables = remaining_subtables.checked_sub(tables)?;
        let features = be::chunks_at::<12>(chain, header, n)?;
        let ranges =
            features::compile(flags, features, tags, language, selections, &mut map_budget);
        let mut body = chain.get(header.checked_add(n.checked_mul(12)?)?..)?;
        let mut substitutions = alloc::vec::Vec::new();
        for _ in 0..tables {
            let (length, coverage, flags_at) = if legacy {
                let c = u32::from(be::u16_at(body, 2)?);
                (
                    usize::from(be::u16_at(body, 0)?),
                    ((c & 0xff00) << 16) | (c & 255),
                    4usize,
                )
            } else {
                (be::u32_at(body, 0)? as usize, be::u32_at(body, 4)?, 8usize)
            };
            let (st, tail) = body.split_at_checked(length)?;
            body = tail;
            let payload = st.get(flags_at.checked_add(4)?..)?;
            let kind = coverage & 255;
            let action = match kind {
                0 | 1 | 2 | 5 if legacy => Action::Machine(StateTable::mort(payload, kind)?),
                0 | 1 | 2 | 5 => Action::Machine(StateTable::new(payload, glyphs, kind)?),
                4 => Action::Replace(AatLookup::new(payload, glyphs)?),
                _ => continue,
            };
            substitutions.push(Subtable {
                coverage,
                flags: be::u32_at(st, flags_at)?,
                action,
            });
        }
        if version >= 3 {
            let bytes = usize::try_from(glyphs.div_ceil(8)).ok()?;
            for offset in be::chunks_at::<4>(body, 0, tables as usize)? {
                let offset = u32::from_be_bytes(*offset);
                if offset != 0 && offset != u32::MAX {
                    body.get(offset as usize..)?.get(..bytes)?;
                }
            }
        }
        chains.push(Chain {
            ranges,
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
    classes: StateClasses<'a>,
    num_classes: usize,
    states: &'a [u8],
    entries: &'a [u8],
    kind: MachineKind<'a>,
}

#[derive(Clone, Debug)]
enum StateClasses<'a> {
    Extended(AatLookup<'a>),
    Legacy {
        first_glyph: u16,
        classes: &'a [u8],
        state_origin: u16,
    },
}

#[derive(Clone, Debug)]
enum MachineKind<'a> {
    Rearrange,
    Kerning,
    Contextual(alloc::vec::Vec<AatLookup<'a>>),
    LegacyContext(&'a [u8], usize),
    Insert(&'a [u8]),
    Ligature(Ligature<'a>),
}

#[derive(Clone, Copy)]
struct Entry {
    next: u16,
    flags: u16,
    substitutions: [u16; 2],
}

impl<'a> StateTable<'a> {
    fn new(data: &'a [u8], glyphs: u32, kind: u32) -> Option<Self> {
        let offset = |at| data.get(be::u32_at(data, at)? as usize..);
        let kind = match kind {
            0 => MachineKind::Rearrange,
            1 => MachineKind::Contextual(alloc::vec::Vec::new()),
            2 => MachineKind::Ligature(Ligature {
                legacy_bases: None,
                actions: offset(16)?,
                components: offset(20)?,
                glyphs: offset(24)?,
            }),
            5 => MachineKind::Insert(offset(16)?),
            _ => return None,
        };
        Self::from_kind(data, glyphs, kind)
    }
    fn from_kind(data: &'a [u8], glyphs: u32, kind: MachineKind<'a>) -> Option<Self> {
        let num_classes = be::u32_at(data, 0)? as usize;
        if num_classes < 4 {
            return None;
        }
        let offset = |at| data.get(be::u32_at(data, at)? as usize..);
        let mut table = Self {
            classes: StateClasses::Extended(AatLookup::new(offset(4)?, glyphs)?),
            num_classes,
            states: offset(8)?,
            entries: offset(12)?,
            kind,
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
        if let MachineKind::Contextual(lookups) = &mut table.kind {
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
    fn mort(data: &'a [u8], kind: u32) -> Option<Self> {
        let offset = |at| be::u16_at(data, at).map(usize::from);
        let kind = match kind {
            0 => MachineKind::Rearrange,
            1 => {
                let base = offset(8)?;
                MachineKind::LegacyContext(data.get(base..)?, base)
            }
            2 => {
                let bases = [offset(8)?, offset(10)?, offset(12)?];
                MachineKind::Ligature(Ligature {
                    actions: data.get(bases[0]..)?,
                    components: data.get(bases[1]..)?,
                    glyphs: data.get(bases[2]..)?,
                    legacy_bases: Some(bases),
                })
            }
            5 => MachineKind::Insert(data.get(offset(8)?..)?),
            _ => return None,
        };
        Self::legacy(data, kind)
    }
    fn legacy(data: &'a [u8], kind: MachineKind<'a>) -> Option<Self> {
        let num_classes = usize::from(be::u16_at(data, 0)?);
        if num_classes < 4 {
            return None;
        }
        let class = data.get(usize::from(be::u16_at(data, 2)?)..)?;
        let state_origin = be::u16_at(data, 4)?;
        let table = Self {
            classes: StateClasses::Legacy {
                first_glyph: be::u16_at(class, 0)?,
                classes: be::slice_at(class, 4, usize::from(be::u16_at(class, 2)?))?,
                state_origin,
            },
            num_classes,
            states: data,
            entries: data.get(usize::from(be::u16_at(data, 6)?)..)?,
            kind,
        };
        let mut seen = alloc::collections::BTreeSet::new();
        let mut pending = alloc::vec![state_origin];
        seen.insert(state_origin);
        while let Some(state) = pending.pop() {
            for c in 0..num_classes {
                let next = table.entry(usize::from(state), c)?.next;
                if seen.insert(next) {
                    pending.push(next);
                }
            }
        }
        Some(table)
    }
    fn entry(&self, state: usize, class: usize) -> Option<Entry> {
        if let StateClasses::Legacy { state_origin, .. } = self.classes {
            // Unlike morx row indices, kern entries point to byte offsets.
            // Rows can precede the start row; the quotient rounds toward zero.
            let cell = state.checked_add(class)?;
            let stride = match self.kind {
                MachineKind::LegacyContext(..) | MachineKind::Insert(_) => 8,
                _ => 4,
            };
            let row = usize::from(be::u8_at(self.states, cell)?).checked_mul(stride)?;
            let origin = i32::from(state_origin);
            let raw = i32::from(be::u16_at(self.entries, row)?);
            let width = i32::try_from(self.num_classes).ok()?;
            let delta = raw
                .checked_sub(origin)?
                .checked_div(width)?
                .checked_mul(width)?;
            let next = u16::try_from(origin.checked_add(delta)?).ok()?;
            let flags = be::u16_at(self.entries, row.checked_add(2)?)?;
            let offset = flags & 0x3fff;
            return Some(Entry {
                next,
                flags,
                substitutions: if stride == 8 {
                    [
                        be::u16_at(self.entries, row.checked_add(4)?)?,
                        be::u16_at(self.entries, row.checked_add(6)?)?,
                    ]
                } else {
                    [if offset == 0 { 0xffff } else { offset }, 0xffff]
                },
            });
        }
        let cell = state
            .checked_mul(self.num_classes)?
            .checked_add(class)?
            .checked_mul(2)?;
        let stride = match self.kind {
            MachineKind::Rearrange => 4,
            MachineKind::Kerning | MachineKind::Ligature(_) => 6,
            _ => 8,
        };
        let offset = usize::from(be::u16_at(self.states, cell)?).checked_mul(stride)?;
        let row = self.entries.get(offset..offset.checked_add(stride)?)?;
        Some(Entry {
            next: be::u16_at(row, 0)?,
            flags: be::u16_at(row, 2)?,
            substitutions: if stride == 8 {
                [be::u16_at(row, 4)?, be::u16_at(row, 6)?]
            } else if stride == 6 {
                [be::u16_at(row, 4)?, 0xffff]
            } else {
                [0xffff; 2]
            },
        })
    }
    fn actionable(&self, entry: Entry) -> bool {
        match self.kind {
            MachineKind::Kerning => entry.substitutions[0] != 0xffff,
            MachineKind::Ligature(ref t) => t.actionable(entry),
            MachineKind::LegacyContext(..) => entry.substitutions != [0; 2],
            MachineKind::Rearrange => entry.flags & 15 != 0,
            MachineKind::Contextual(_) => entry.substitutions != [0xffff; 2],
            MachineKind::Insert(_) => {
                entry.flags & 0x3ff != 0 && entry.substitutions != [0xffff; 2]
            }
        }
    }
    fn class(&self, glyph: Option<&crate::buffer::Info>) -> usize {
        glyph.map_or(0, |g| {
            if g.codepoint == 0xffff {
                return 2;
            }
            let c = match &self.classes {
                StateClasses::Extended(lookup) => usize::from(lookup.get(g.codepoint).unwrap_or(1)),
                StateClasses::Legacy {
                    first_glyph,
                    classes,
                    ..
                } => g
                    .codepoint
                    .checked_sub(u32::from(*first_glyph))
                    .and_then(|i| classes.get(i as usize))
                    .copied()
                    .map_or(1, usize::from),
            };
            if c < self.num_classes { c } else { 1 }
        })
    }
    fn start_state(&self) -> u16 {
        match self.classes {
            StateClasses::Extended(_) => 0,
            StateClasses::Legacy { state_origin, .. } => state_origin,
        }
    }
    fn safe_boundary(&self, state: usize, class: usize, entry: Entry) -> bool {
        !self.actionable(entry)
            && (state == usize::from(self.start_state())
                || (entry.flags & 0x4000 != 0 && entry.next == self.start_state())
                || self
                    .entry(usize::from(self.start_state()), class)
                    .is_some_and(|e| {
                        e.next == entry.next
                            && !self.actionable(e)
                            && (e.flags ^ entry.flags) & 0x4000 == 0
                    }))
            && self.entry(state, 0).is_some_and(|e| !self.actionable(e))
    }
    pub fn drive(
        &self,
        buffer: &mut crate::buffer::Buffer,
        replace: &mut impl FnMut(&mut crate::buffer::Info, u16),
        enabled: impl Fn(u32) -> bool,
    ) {
        let mut state = usize::from(self.start_state());
        let separate = matches!(self.kind, MachineKind::Insert(_) | MachineKind::Ligature(_));
        let mut stack = Components::default();
        if separate {
            buffer.clear_output();
        } else {
            buffer.idx = 0;
        }
        let mut first = 0;
        let mut marked = false;
        let mut last = 0;
        let mut cluster = 0;
        while buffer.successful {
            let (idx, len) = (buffer.idx, buffer.len);
            if let Some(info) = buffer.infos().get(idx) {
                cluster = info.cluster;
            }
            if !enabled(cluster) {
                if idx == len {
                    break;
                }
                state = usize::from(self.start_state());
                if !buffer.next_glyph() {
                    break;
                }
                continue;
            }
            let class = self.class(buffer.infos().get(idx));
            let Some(entry) = self.entry(state, class) else {
                break;
            };
            let Entry {
                next,
                flags,
                substitutions,
            } = entry;
            let dont_advance = flags & 0x4000 != 0;
            let safe = self.safe_boundary(state, class, entry);
            let back = buffer.backtrack_len();
            if !safe && back > 0 && idx < len {
                buffer
                    .unsafe_to_break_from_outbuffer(back.saturating_sub(1), idx.saturating_add(1));
            }
            if let MachineKind::Ligature(table) = &self.kind {
                if !table.transition(buffer, entry, &mut stack, replace) {
                    break;
                }
            } else if let MachineKind::Insert(data) = self.kind {
                let mark_loc = buffer.out_len;
                if !insert(buffer, data, first, entry, true, replace) {
                    break;
                }
                if flags & 0x8000 != 0 {
                    first = mark_loc;
                }
                if !insert(buffer, data, first, entry, false, replace) {
                    break;
                }
            } else if matches!(
                self.kind,
                MachineKind::Contextual(_) | MachineKind::LegacyContext(..)
            ) {
                if idx < len || marked {
                    for (index, slot, is_mark) in [
                        (substitutions[0], first, true),
                        (substitutions[1], idx.min(len.saturating_sub(1)), false),
                    ] {
                        let glyph = buffer.infos().get(slot).and_then(|g| match &self.kind {
                            MachineKind::Contextual(lookups) => {
                                lookups.get(usize::from(index))?.get(g.codepoint)
                            }
                            MachineKind::LegacyContext(data, base) if index != 0 => {
                                let signed = i32::from(i16::from_be_bytes(index.to_be_bytes()));
                                let glyph = g.codepoint.checked_add_signed(signed)? as usize;
                                let at = glyph.checked_mul(2)?.checked_sub(*base)?;
                                be::u16_at(data, at).filter(|value| *value != 0)
                            }
                            _ => None,
                        });
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
            if buffer.idx == buffer.len {
                break;
            }
            state = usize::from(next);
            if dont_advance {
                buffer.max_ops = buffer.max_ops.saturating_sub(1);
            }
            if (!dont_advance || buffer.max_ops < 0) && !buffer.next_glyph() {
                break;
            }
        }
        if separate {
            buffer.sync();
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

fn insert(
    buffer: &mut crate::buffer::Buffer,
    data: &[u8],
    mark: usize,
    entry: Entry,
    marked: bool,
    replace: &mut impl FnMut(&mut crate::buffer::Info, u16),
) -> bool {
    let [current_index, marked_index] = entry.substitutions;
    let (index, count, before) = if marked {
        (marked_index, entry.flags & 31, entry.flags & 0x0400 != 0)
    } else {
        (
            current_index,
            (entry.flags >> 5) & 31,
            entry.flags & 0x0800 != 0,
        )
    };
    if index == 0xffff {
        return true;
    }
    buffer.charge(usize::from(count));
    if buffer.max_ops <= 0 {
        return true;
    }
    let end = buffer.out_len;
    if marked && !buffer.move_to(mark) {
        return false;
    }
    let copy = buffer.idx < buffer.len && !before;
    if copy && !buffer.copy_glyph() {
        return false;
    }
    let glyphs = be::chunks_at::<2>(
        data,
        usize::from(index).saturating_mul(2),
        usize::from(count),
    )
    .unwrap_or_default();
    for raw in glyphs {
        let glyph = u16::from_be_bytes(*raw);
        // Replacement properties go on the output item, which inherits the current cluster.
        let mut info = if buffer.idx < buffer.len {
            *buffer.cur(0)
        } else {
            *buffer.prev()
        };
        replace(&mut info, glyph);
        if !buffer.output_info(info) {
            return false;
        }
    }
    if copy {
        buffer.skip_glyph();
    }
    let resume = if !marked && entry.flags & 0x4000 != 0 {
        end
    } else {
        end.saturating_add(glyphs.len())
    };
    if !buffer.move_to(resume) {
        return false;
    }
    if marked {
        buffer.unsafe_to_break_from_outbuffer(mark, buffer.idx.saturating_add(1).min(buffer.len));
    }
    true
}

#[derive(Clone, Debug)]
struct Ligature<'a> {
    legacy_bases: Option<[usize; 3]>,
    actions: &'a [u8],
    components: &'a [u8],
    glyphs: &'a [u8],
}

struct Components {
    positions: [usize; 64],
    len: usize,
}
impl Default for Components {
    fn default() -> Self {
        Self {
            positions: [0; 64],
            len: 0,
        }
    }
}
impl Components {
    fn slot(&self, index: usize) -> usize {
        self.positions.get(index % 64).copied().unwrap_or(0)
    }
    fn push(&mut self, position: usize) {
        if self.len > 0 && self.slot(self.len.saturating_sub(1)) == position {
            return;
        }
        if let Some(slot) = self.positions.get_mut(self.len % 64) {
            *slot = position;
        }
        self.len = self.len.saturating_add(1);
    }
}
impl Ligature<'_> {
    fn actionable(&self, entry: Entry) -> bool {
        if self.legacy_bases.is_some() {
            entry.flags & 0x3fff != 0
        } else {
            entry.flags & 0x2000 != 0
        }
    }
    fn transition(
        &self,
        buffer: &mut crate::buffer::Buffer,
        entry: Entry,
        stack: &mut Components,
        replace: &mut impl FnMut(&mut crate::buffer::Info, u16),
    ) -> bool {
        let end = buffer.out_len;
        if entry.flags & 0x8000 != 0 {
            stack.push(end);
        }
        if !self.actionable(entry) || stack.len == 0 || buffer.idx >= buffer.len {
            return true;
        }
        let mut cursor = stack.len;
        let mut ligature = 0usize;
        let mut action_at = if let Some([base, _, _]) = self.legacy_bases {
            let Some(offset) = usize::from(entry.flags & 0x3fff).checked_sub(base) else {
                return true;
            };
            offset & !3
        } else {
            usize::from(entry.substitutions[0]).saturating_mul(4)
        };
        loop {
            if cursor == 0 {
                stack.len = 0;
                break;
            }
            cursor = cursor.saturating_sub(1);
            if !buffer.move_to(stack.slot(cursor)) {
                return false;
            }
            let Some(action) = be::u32_at(self.actions, action_at) else {
                break;
            };
            let mut delta = action & 0x3fff_ffff;
            if delta & 0x2000_0000 != 0 {
                delta |= 0xc000_0000;
            }
            let component = buffer.cur(0).codepoint.wrapping_add(delta) as usize;
            let component_base = self.legacy_bases.map_or(0, |b| b[1]);
            let Some(index) = component
                .checked_mul(2)
                .and_then(|v| v.checked_sub(component_base))
                .and_then(|at| be::u16_at(self.components, at & !1))
            else {
                break;
            };
            ligature = ligature.saturating_add(usize::from(index));
            if action & 0xc000_0000 != 0 {
                if let Some([_, _, base]) = self.legacy_bases {
                    let Some(relative) = ligature.checked_sub(base) else {
                        break;
                    };
                    ligature = relative / 2;
                }
                let Some(glyph) = be::u16_at(self.glyphs, ligature.saturating_mul(2)) else {
                    break;
                };
                replace(buffer.cur_mut(0), glyph);
                if !buffer.replace_glyph(u32::from(glyph)) {
                    return false;
                }
                let lig_end = stack.slot(stack.len.saturating_sub(1)).saturating_add(1);
                while stack.len.saturating_sub(1) > cursor {
                    stack.len = stack.len.saturating_sub(1);
                    if !buffer.move_to(stack.slot(stack.len)) || !buffer.replace_glyph(0xffff) {
                        return false;
                    }
                }
                if !buffer.move_to(lig_end) {
                    return false;
                }
                buffer.merge_out_clusters(stack.slot(cursor), buffer.out_len);
            }
            if action & 0x8000_0000 != 0 {
                break;
            }
            action_at = action_at.saturating_add(4);
        }
        buffer.move_to(end)
    }
}

#[cfg(test)]
mod chain_tests {
    use super::chains;

    #[test]
    fn i2_unknown_type_skips_only_its_bounded_subtable() {
        // morx header, chain header, reserved type 3, then a format 8 lookup.
        let words: [u32; 14] = [
            0x00020000, 1, 1, 48, 0, 2, 12, 3, 1, 20, 4, 1, 0x00080001, 0x00010002,
        ];
        let mut data: alloc::vec::Vec<u8> = words.into_iter().flat_map(u32::to_be_bytes).collect();
        let count = chains(
            &data,
            3,
            false,
            None,
            None,
            &super::features::Selections::default(),
        )
        .and_then(|c| c.first().map(|c| c.substitutions.len()));
        assert_eq!(count, Some(1));
        data.pop();
        assert!(
            chains(
                &data,
                3,
                false,
                None,
                None,
                &super::features::Selections::default()
            )
            .is_none()
        );
    }
}
