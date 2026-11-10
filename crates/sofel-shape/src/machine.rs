//! Running the syllable machines (ADR 0024, 音節の状態機械): the tables
//! `tools/gen-machine/` writes to `tables/machine_<name>.rs`, looked up
//! by hand-written functions.
//!
//! The scanner works as Ragel's `|* ... *|` that HarfBuzz's
//! `hb-ot-shaper-*-machine.rl` use: from the current position, the
//! longest sequence of categories a rule accepts, the first rule on ties.
//!
//! This file uses nothing else of the crate, so that sofel-conformance
//! can include it as it is.

/// The tables of one machine (`tables/machine_<name>.rs`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Machine {
    /// The input class of each category value; values past the end are
    /// in `other`.
    pub(crate) classes: &'static [u8],
    /// The class of the values no rule names.
    pub(crate) other: u8,
    /// The number of input classes.
    pub(crate) num_classes: usize,
    /// `transitions[state * num_classes + class]`; state 0 is dead, 1
    /// the start.
    pub(crate) transitions: &'static [u8],
    /// The action of the rule each state accepts, [`NO_ACTION`] for none.
    pub(crate) accept: &'static [u8],
}

/// The USE category of CGJ (`hb-ot-shaper-use-machine.rl`).
pub(crate) const USE_CGJ: u8 = 6;
/// The USE category of ZWNJ (`hb-ot-shaper-use-machine.rl`).
pub(crate) const USE_ZWNJ: u8 = 14;

/// The action of states that accept nothing.
pub(crate) const NO_ACTION: u8 = 0xFF;

impl Machine {
    fn next(&self, state: usize, category: u8) -> usize {
        let class = usize::from(
            self.classes
                .get(usize::from(category))
                .copied()
                .unwrap_or(self.other),
        );
        state
            .checked_mul(self.num_classes)
            .and_then(|i| i.checked_add(class))
            .and_then(|i| self.transitions.get(i))
            .map_or(0, |&s| usize::from(s))
    }

    /// The longest match at the start of `categories`: its length and
    /// action. Every grammar ends with `other = any`, so the length is at
    /// least 1 for a non-empty input; `(1, NO_ACTION)` otherwise.
    fn longest(&self, len: usize, category: &impl Fn(usize) -> u8, start: usize) -> (usize, u8) {
        let mut state = 1;
        let mut last = (1, NO_ACTION);
        for (n, i) in (start..len).enumerate() {
            state = self.next(state, category(i));
            if state == 0 {
                break;
            }
            let action = self.accept.get(state).copied().unwrap_or(NO_ACTION);
            if action != NO_ACTION {
                last = (n.saturating_add(1), action);
            }
        }
        last
    }

    /// Splits the `len` categories into syllables, calling
    /// `found(start, end, action)` for each, in order.
    pub(crate) fn scan(
        &self,
        len: usize,
        category: impl Fn(usize) -> u8,
        mut found: impl FnMut(usize, usize, u8),
    ) {
        let mut p = 0;
        while p < len {
            let (n, action) = self.longest(len, &category, p);
            let end = p.saturating_add(n).min(len);
            found(p, end, action);
            p = end;
        }
    }

    /// As HarfBuzz's `find_syllables_*`: the syllable byte of every
    /// item, the serial (1 to 15, then 1 again) in the high four bits and
    /// the action in the low four, through `set(index, syllable)`.
    pub(crate) fn syllables(
        &self,
        len: usize,
        category: impl Fn(usize) -> u8,
        mut set: impl FnMut(usize, u8),
    ) {
        let mut serial: u8 = 1;
        self.scan(len, category, |start, end, action| {
            (start..end).for_each(|i| set(i, (serial << 4) | (action & 0x0F)));
            serial = if serial >= 15 {
                1
            } else {
                serial.saturating_add(1)
            };
        });
    }

    /// As HarfBuzz's `find_syllables_use`: the machine runs over the
    /// items that are not CGJ ([`USE_CGJ`]), and not a ZWNJ ([`USE_ZWNJ`]) whose next
    /// non-CGJ item is a Unicode mark (`is_mark`); a syllable covers the
    /// items from its first kept item to the next syllable's first kept
    /// item (or the end). Items before the first kept item keep their
    /// syllable. `kept` is scratch space.
    pub(crate) fn syllables_use(
        &self,
        len: usize,
        category: impl Fn(usize) -> u8,
        is_mark: impl Fn(usize) -> bool,
        kept: &mut impl KeptIndices,
        mut set: impl FnMut(usize, u8),
    ) {
        kept.clear_indices();
        (0..len)
            .filter(|&i| category(i) != USE_CGJ)
            .filter(|&i| {
                category(i) != USE_ZWNJ
                    || ((i.saturating_add(1))..len)
                        .find(|&j| category(j) != USE_CGJ)
                        .is_none_or(|j| !is_mark(j))
            })
            .for_each(|i| kept.push_index(i));
        let n = kept.indices().len();
        let at = |k: usize| kept.indices().get(k).copied().unwrap_or(len);
        let mut serial: u8 = 1;
        self.scan(
            n,
            |k| category(at(k)),
            |ts, te, action| {
                (at(ts)..at(te)).for_each(|i| set(i, (serial << 4) | (action & 0x0F)));
                serial = if serial >= 15 {
                    1
                } else {
                    serial.saturating_add(1)
                };
            },
        );
    }
}

/// A growable list of indices (a `Vec<usize>`), so that this file needs
/// no `alloc` of its own.
pub(crate) trait KeptIndices {
    /// Empties the list.
    fn clear_indices(&mut self);
    /// Appends an index.
    fn push_index(&mut self, i: usize);
    /// The indices.
    fn indices(&self) -> &[usize];
}
