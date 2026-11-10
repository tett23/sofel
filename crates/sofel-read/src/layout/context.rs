//! Sequence context and chained sequence context subtables, shared by
//! `GSUB` lookup types 5 and 6 and `GPOS` lookup types 7 and 8.
//!
//! The widths follow HarfBuzz 15.1.0 (`hb-ot-layout-gsubgpos.hh`):
//!
//! | format | offsets to rule sets | offsets in a rule set | rule values |
//! |---|---|---|---|
//! | context 1 | 16 | 16 | 16-bit glyphs |
//! | context 2 | 16 | 16 | 16-bit classes |
//! | context 4 | 24 (24-bit count) | 16 | 24-bit glyphs |
//! | context 5 | 24 (24-bit count) | 24 | 16-bit classes |
//! | chained 1 | 16 | 16 | 16-bit glyphs |
//! | chained 2 | 16 | 16 | 16-bit classes |
//! | chained 4 | 24 (24-bit count) | 24 | 24-bit glyphs |
//! | chained 5 | 24 (16-bit count) | 24 | 16-bit classes |
//!
//! Context formats 3 and 6 have arrays of 16- and 24-bit offsets to
//! coverage tables; chained format 3 has 16-bit ones. Coverage and
//! ClassDef offsets are 32 bits wide in formats 4 and 5.

use super::{ClassDef, Coverage, counted, follow, or_empty, record, uint};
use crate::ReadError;
use crate::be;

/// A record of a lookup to apply at a position of the input sequence.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SequenceLookupRecord {
    /// The index into the input sequence (the first glyph is 0).
    pub sequence_index: u16,
    /// The index of the lookup in the LookupList.
    pub lookup_index: u16,
}

/// A packed array of glyph ids (16 or 24 bits) or 16-bit class values.
#[derive(Clone, Copy, Debug)]
pub struct GlyphArray<'a> {
    data: &'a [u8],
    width: usize,
}

impl<'a> GlyphArray<'a> {
    pub(crate) fn new(data: &'a [u8], width: usize) -> GlyphArray<'a> {
        GlyphArray { data, width }
    }
    /// A count of `count_width` bytes at `at` and that many values of
    /// `width` bytes after it.
    pub(crate) fn counted(
        data: &'a [u8],
        at: usize,
        count_width: usize,
        width: usize,
    ) -> Result<GlyphArray<'a>, ReadError> {
        counted(data, at, count_width, width).map(|(_, d)| GlyphArray::new(d, width))
    }
    /// The bytes of the values.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// The width of a value in bytes: 2 or 3.
    pub fn width(&self) -> usize {
        self.width
    }
    /// The number of values.
    pub fn len(&self) -> usize {
        self.data.len().checked_div(self.width).unwrap_or(0)
    }
    /// Whether there are no values.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    /// Value `i`.
    pub fn get(&self, i: usize) -> Option<u32> {
        uint(self.data, i.checked_mul(self.width)?, self.width)
    }
    /// The values in order.
    pub fn iter(&self) -> impl Iterator<Item = u32> + 'a {
        let a = *self;
        (0..a.len()).filter_map(move |i| a.get(i))
    }
}

/// A packed array of [`SequenceLookupRecord`]s.
#[derive(Clone, Copy, Debug)]
pub struct LookupRecords<'a> {
    data: &'a [u8],
}

impl<'a> LookupRecords<'a> {
    pub(crate) fn new(data: &'a [u8]) -> LookupRecords<'a> {
        LookupRecords { data }
    }
    /// The bytes of the records.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// The number of records.
    pub fn len(&self) -> usize {
        self.data.len() / 4
    }
    /// Whether there are no records.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    /// Record `i`.
    pub fn get(&self, i: usize) -> Option<SequenceLookupRecord> {
        let r = record(self.data, i, 4)?;
        Some(SequenceLookupRecord {
            sequence_index: be::u16_at(r, 0)?,
            lookup_index: be::u16_at(r, 2)?,
        })
    }
    /// The records in order.
    pub fn iter(&self) -> impl Iterator<Item = SequenceLookupRecord> + 'a {
        let a = *self;
        (0..a.len()).filter_map(move |i| a.get(i))
    }
}

/// An array of offsets to Coverage tables, from the start of the
/// subtable.
#[derive(Clone, Copy, Debug)]
pub struct CoverageArray<'a> {
    base: &'a [u8],
    offsets: &'a [u8],
    width: usize,
}

impl<'a> CoverageArray<'a> {
    pub(crate) fn new(base: &'a [u8], offsets: &'a [u8], width: usize) -> CoverageArray<'a> {
        CoverageArray {
            base,
            offsets,
            width,
        }
    }
    /// The bytes of the offsets.
    pub fn offsets(&self) -> &'a [u8] {
        self.offsets
    }
    /// The width of an offset in bytes: 2 or 3.
    pub fn width(&self) -> usize {
        self.width
    }
    /// The number of offsets.
    pub fn len(&self) -> usize {
        self.offsets.len().checked_div(self.width).unwrap_or(0)
    }
    /// Whether there are no offsets.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    /// Offset `i`.
    pub fn offset(&self, i: usize) -> Option<u32> {
        uint(self.offsets, i.checked_mul(self.width)?, self.width)
    }
    /// Coverage table `i`; [`ReadError::OutOfBounds`] for an index past
    /// the offsets.
    pub fn get(&self, i: usize) -> Result<Option<Coverage<'a>>, ReadError> {
        let o = self.offset(i).ok_or(ReadError::OutOfBounds)?;
        follow(self.base, o, Coverage::new)
    }
}

/// The widths of a rule set array: offsets to the sets, offsets inside
/// a set, and the values of a rule.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Widths {
    pub(crate) sets: usize,
    pub(crate) rules: usize,
    pub(crate) values: usize,
}

/// An array of offsets to rule sets, from the start of the subtable.
#[derive(Clone, Copy, Debug)]
struct Sets<'a> {
    base: &'a [u8],
    offsets: &'a [u8],
    w: Widths,
}

impl<'a> Sets<'a> {
    fn len(&self) -> usize {
        self.offsets.len().checked_div(self.w.sets).unwrap_or(0)
    }
    fn offset(&self, i: usize) -> Option<u32> {
        uint(self.offsets, i.checked_mul(self.w.sets)?, self.w.sets)
    }
    fn get(&self, i: usize) -> Result<Option<Set<'a>>, ReadError> {
        let o = self.offset(i).ok_or(ReadError::OutOfBounds)?;
        let w = self.w;
        follow(self.base, o, |d| Set::new(d, w))
    }
}

/// A rule set: a 16-bit count and offsets to rules from its start.
#[derive(Clone, Copy, Debug)]
struct Set<'a> {
    data: &'a [u8],
    offsets: &'a [u8],
    w: Widths,
}

impl<'a> Set<'a> {
    fn new(data: &'a [u8], w: Widths) -> Result<Set<'a>, ReadError> {
        let (_, offsets) = counted(data, 0, 2, w.rules)?;
        Ok(Set { data, offsets, w })
    }
    fn len(&self) -> usize {
        self.offsets.len().checked_div(self.w.rules).unwrap_or(0)
    }
    fn offset(&self, i: usize) -> Option<u32> {
        uint(self.offsets, i.checked_mul(self.w.rules)?, self.w.rules)
    }
    fn get<T>(
        &self,
        i: usize,
        make: impl FnOnce(&'a [u8], usize) -> Result<T, ReadError>,
    ) -> Result<Option<T>, ReadError> {
        let o = self.offset(i).ok_or(ReadError::OutOfBounds)?;
        let values = self.w.values;
        follow(self.data, o, |d| make(d, values))
    }
}

/// The rule sets of a sequence context subtable of format 1, 2, 4 or 5,
/// one per coverage index (formats 1 and 4) or per class (2 and 5).
#[derive(Clone, Copy, Debug)]
pub struct RuleSets<'a>(Sets<'a>);

impl<'a> RuleSets<'a> {
    /// The number of rule sets.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Whether there are no rule sets.
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }
    /// The offset of rule set `i` from the start of the subtable.
    pub fn offset(&self, i: usize) -> Option<u32> {
        self.0.offset(i)
    }
    /// Rule set `i`; [`ReadError::OutOfBounds`] for an index past the
    /// offsets.
    pub fn get(&self, i: usize) -> Result<Option<RuleSet<'a>>, ReadError> {
        self.0.get(i).map(|s| s.map(RuleSet))
    }
}

/// A view over a SequenceRuleSet or ClassSequenceRuleSet table.
#[derive(Clone, Copy, Debug)]
pub struct RuleSet<'a>(Set<'a>);

impl<'a> RuleSet<'a> {
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.0.data
    }
    /// The number of rules.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Whether there are no rules.
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }
    /// The offset of rule `i` from the start of the set.
    pub fn offset(&self, i: usize) -> Option<u32> {
        self.0.offset(i)
    }
    /// Rule `i`; [`ReadError::OutOfBounds`] for an index past the offsets.
    pub fn get(&self, i: usize) -> Result<Option<Rule<'a>>, ReadError> {
        self.0.get(i, Rule::new)
    }
}

/// A SequenceRule or ClassSequenceRule.
#[derive(Clone, Copy, Debug)]
pub struct Rule<'a> {
    /// `glyphCount - 1` glyph ids or classes: the input sequence after
    /// its first glyph (none when `glyphCount` is 0).
    pub input: GlyphArray<'a>,
    /// The lookups to apply.
    pub lookups: LookupRecords<'a>,
}

impl<'a> Rule<'a> {
    /// A view over a rule whose values are `width` bytes wide. Fails with
    /// [`ReadError::Truncated`] if it does not fit.
    pub fn new(data: &'a [u8], width: usize) -> Result<Rule<'a>, ReadError> {
        let header = |n| ReadError::truncated(n, data.len());
        let input_count = be::u16_at(data, 0).ok_or(header(4))?;
        let lookup_count = be::u16_at(data, 2).ok_or(header(4))?;
        let n = usize::from(input_count.saturating_sub(1)).saturating_mul(width);
        let m = usize::from(lookup_count).saturating_mul(4);
        let needed = n.saturating_add(m).saturating_add(4);
        let input = be::slice_at(data, 4, n).ok_or(header(needed))?;
        let lookups = be::slice_at(data, n.saturating_add(4), m).ok_or(header(needed))?;
        Ok(Rule {
            input: GlyphArray::new(input, width),
            lookups: LookupRecords::new(lookups),
        })
    }
}

/// The rule sets of a chained sequence context subtable of format 1, 2,
/// 4 or 5.
#[derive(Clone, Copy, Debug)]
pub struct ChainRuleSets<'a>(Sets<'a>);

impl<'a> ChainRuleSets<'a> {
    /// The number of rule sets.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Whether there are no rule sets.
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }
    /// The offset of rule set `i` from the start of the subtable.
    pub fn offset(&self, i: usize) -> Option<u32> {
        self.0.offset(i)
    }
    /// Rule set `i`; [`ReadError::OutOfBounds`] for an index past the
    /// offsets.
    pub fn get(&self, i: usize) -> Result<Option<ChainRuleSet<'a>>, ReadError> {
        self.0.get(i).map(|s| s.map(ChainRuleSet))
    }
}

/// A view over a ChainedSequenceRuleSet or ChainedClassSequenceRuleSet
/// table.
#[derive(Clone, Copy, Debug)]
pub struct ChainRuleSet<'a>(Set<'a>);

impl<'a> ChainRuleSet<'a> {
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.0.data
    }
    /// The number of rules.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Whether there are no rules.
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }
    /// The offset of rule `i` from the start of the set.
    pub fn offset(&self, i: usize) -> Option<u32> {
        self.0.offset(i)
    }
    /// Rule `i`; [`ReadError::OutOfBounds`] for an index past the offsets.
    pub fn get(&self, i: usize) -> Result<Option<ChainRule<'a>>, ReadError> {
        self.0.get(i, ChainRule::new)
    }
}

/// A ChainedSequenceRule or ChainedClassSequenceRule.
#[derive(Clone, Copy, Debug)]
pub struct ChainRule<'a> {
    /// The backtrack sequence, in glyph sequence order.
    pub backtrack: GlyphArray<'a>,
    /// The input sequence after its first glyph (`inputGlyphCount - 1`
    /// values; none when the count is 0).
    pub input: GlyphArray<'a>,
    /// The lookahead sequence.
    pub lookahead: GlyphArray<'a>,
    /// The lookups to apply.
    pub lookups: LookupRecords<'a>,
}

impl<'a> ChainRule<'a> {
    /// A view over a rule whose values are `width` bytes wide. Fails with
    /// [`ReadError::Truncated`] if it does not fit.
    pub fn new(data: &'a [u8], width: usize) -> Result<ChainRule<'a>, ReadError> {
        let (_, backtrack) = counted(data, 0, 2, width)?;
        let at = backtrack.len().saturating_add(2);
        let input_count =
            be::u16_at(data, at).ok_or(ReadError::truncated(at.saturating_add(2), data.len()))?;
        let n = usize::from(input_count.saturating_sub(1)).saturating_mul(width);
        let input = be::slice_at(data, at.saturating_add(2), n).ok_or(ReadError::truncated(
            at.saturating_add(2).saturating_add(n),
            data.len(),
        ))?;
        let at = at.saturating_add(2).saturating_add(n);
        let (_, lookahead) = counted(data, at, 2, width)?;
        let at = at.saturating_add(2).saturating_add(lookahead.len());
        let (_, lookups) = counted(data, at, 2, 4)?;
        Ok(ChainRule {
            backtrack: GlyphArray::new(backtrack, width),
            input: GlyphArray::new(input, width),
            lookahead: GlyphArray::new(lookahead, width),
            lookups: LookupRecords::new(lookups),
        })
    }
}

/// A sequence context subtable (`GSUB` type 5, `GPOS` type 7).
#[derive(Clone, Copy, Debug)]
pub enum SequenceContext<'a> {
    /// Format 1 (16-bit glyphs) and 4 (24-bit glyphs and offsets).
    Glyphs {
        /// The coverage of the first glyph.
        coverage: Coverage<'a>,
        /// The rule sets, by coverage index.
        rule_sets: RuleSets<'a>,
    },
    /// Format 2 and 5 (24-bit offsets; classes stay 16-bit).
    Classes {
        /// The coverage of the first glyph.
        coverage: Coverage<'a>,
        /// The classes of the input glyphs (empty for a null offset).
        class_def: ClassDef<'a>,
        /// The rule sets, by class of the first glyph.
        rule_sets: RuleSets<'a>,
    },
    /// Format 3 (16-bit coverage offsets) and 6 (24-bit ones).
    Coverages {
        /// One coverage per input glyph (at least one).
        coverages: CoverageArray<'a>,
        /// The lookups to apply.
        lookups: LookupRecords<'a>,
    },
}

pub(crate) fn coverage_at<'a>(data: &'a [u8], at: usize, width: usize) -> Coverage<'a> {
    or_empty(data, at, width, Coverage::new, Coverage::empty())
}

pub(crate) fn class_def_at<'a>(data: &'a [u8], at: usize, width: usize) -> ClassDef<'a> {
    or_empty(data, at, width, ClassDef::new, ClassDef::empty())
}

/// The widths of the rule set arrays of context format `format`.
pub(crate) fn context_widths(format: u16) -> Option<Widths> {
    let (sets, rules, values) = match format {
        1 | 2 => (2, 2, 2),
        4 => (3, 2, 3),
        5 => (3, 3, 2),
        _ => return None,
    };
    Some(Widths {
        sets,
        rules,
        values,
    })
}

/// The widths of the rule set arrays of chained context format `format`.
pub(crate) fn chain_widths(format: u16) -> Option<Widths> {
    let (sets, rules, values) = match format {
        1 | 2 => (2, 2, 2),
        4 => (3, 3, 3),
        5 => (3, 3, 2),
        _ => return None,
    };
    Some(Widths {
        sets,
        rules,
        values,
    })
}

impl<'a> SequenceContext<'a> {
    /// A view over the subtable.
    ///
    /// Fails with [`ReadError::UnknownFormat`] for a format other than 1
    /// to 6, with [`ReadError::Truncated`] if the header and arrays do not
    /// fit, and with [`ReadError::InvalidValue`] for format 3 or 6 with
    /// no input glyphs (HarfBuzz rejects it).
    pub fn new(data: &'a [u8]) -> Result<SequenceContext<'a>, ReadError> {
        let format = be::u16_at(data, 0).ok_or(ReadError::truncated(2, data.len()))?;
        match format {
            1 | 4 => {
                let w = context_widths(format).ok_or(ReadError::UnknownFormat(format))?;
                let cw = if format == 1 { 2usize } else { 4 };
                let at = cw.saturating_add(2);
                super::need(data, at)?;
                let (_, offsets) = counted(data, at, w.sets, w.sets)?;
                Ok(SequenceContext::Glyphs {
                    coverage: coverage_at(data, 2, cw),
                    rule_sets: RuleSets(Sets {
                        base: data,
                        offsets,
                        w,
                    }),
                })
            }
            2 | 5 => {
                let w = context_widths(format).ok_or(ReadError::UnknownFormat(format))?;
                let cw = if format == 2 { 2usize } else { 4 };
                let at = cw.saturating_mul(2).saturating_add(2);
                super::need(data, at)?;
                let (_, offsets) = counted(data, at, w.sets, w.sets)?;
                Ok(SequenceContext::Classes {
                    coverage: coverage_at(data, 2, cw),
                    class_def: class_def_at(data, cw.saturating_add(2), cw),
                    rule_sets: RuleSets(Sets {
                        base: data,
                        offsets,
                        w,
                    }),
                })
            }
            3 | 6 => {
                let width = if format == 3 { 2 } else { 3 };
                let truncated = |n| ReadError::truncated(n, data.len());
                let glyphs = be::u16_at(data, 2).ok_or(truncated(6))?;
                let lookups = be::u16_at(data, 4).ok_or(truncated(6))?;
                if glyphs == 0 {
                    return Err(ReadError::InvalidValue);
                }
                let n = usize::from(glyphs).saturating_mul(width);
                let m = usize::from(lookups).saturating_mul(4);
                let needed = n.saturating_add(m).saturating_add(6);
                let offsets = be::slice_at(data, 6, n).ok_or(truncated(needed))?;
                let records =
                    be::slice_at(data, n.saturating_add(6), m).ok_or(truncated(needed))?;
                Ok(SequenceContext::Coverages {
                    coverages: CoverageArray::new(data, offsets, width),
                    lookups: LookupRecords::new(records),
                })
            }
            _ => Err(ReadError::UnknownFormat(format)),
        }
    }
}

/// A chained sequence context subtable (`GSUB` type 6, `GPOS` type 8).
#[derive(Clone, Copy, Debug)]
pub enum ChainedSequenceContext<'a> {
    /// Format 1 (16-bit glyphs) and 4 (24-bit glyphs and offsets).
    Glyphs {
        /// The coverage of the first input glyph.
        coverage: Coverage<'a>,
        /// The rule sets, by coverage index.
        rule_sets: ChainRuleSets<'a>,
    },
    /// Format 2 and 5 (24-bit offsets; classes stay 16-bit).
    Classes {
        /// The coverage of the first input glyph.
        coverage: Coverage<'a>,
        /// The classes of the backtrack glyphs.
        backtrack_class_def: ClassDef<'a>,
        /// The classes of the input glyphs.
        input_class_def: ClassDef<'a>,
        /// The classes of the lookahead glyphs.
        lookahead_class_def: ClassDef<'a>,
        /// The rule sets, by class of the first input glyph.
        rule_sets: ChainRuleSets<'a>,
    },
    /// Format 3.
    Coverages {
        /// The backtrack coverages, in glyph sequence order.
        backtrack: CoverageArray<'a>,
        /// The input coverages (at least one).
        input: CoverageArray<'a>,
        /// The lookahead coverages.
        lookahead: CoverageArray<'a>,
        /// The lookups to apply.
        lookups: LookupRecords<'a>,
    },
}

impl<'a> ChainedSequenceContext<'a> {
    /// A view over the subtable.
    ///
    /// Fails with [`ReadError::UnknownFormat`] for a format other than 1
    /// to 5, with [`ReadError::Truncated`] if the header and arrays do not
    /// fit, and with [`ReadError::InvalidValue`] for format 3 with no
    /// input coverages (HarfBuzz rejects it).
    pub fn new(data: &'a [u8]) -> Result<ChainedSequenceContext<'a>, ReadError> {
        let format = be::u16_at(data, 0).ok_or(ReadError::truncated(2, data.len()))?;
        match format {
            1 | 4 => {
                let w = chain_widths(format).ok_or(ReadError::UnknownFormat(format))?;
                let cw = if format == 1 { 2usize } else { 4 };
                let at = cw.saturating_add(2);
                super::need(data, at)?;
                let (_, offsets) = counted(data, at, w.sets, w.sets)?;
                Ok(ChainedSequenceContext::Glyphs {
                    coverage: coverage_at(data, 2, cw),
                    rule_sets: ChainRuleSets(Sets {
                        base: data,
                        offsets,
                        w,
                    }),
                })
            }
            2 | 5 => {
                let w = chain_widths(format).ok_or(ReadError::UnknownFormat(format))?;
                let cw = if format == 2 { 2usize } else { 4 };
                let at = cw.saturating_mul(4).saturating_add(2);
                super::need(data, at)?;
                let (_, offsets) = counted(data, at, 2, w.sets)?;
                let class =
                    |i: usize| class_def_at(data, cw.saturating_mul(i).saturating_add(2), cw);
                Ok(ChainedSequenceContext::Classes {
                    coverage: coverage_at(data, 2, cw),
                    backtrack_class_def: class(1),
                    input_class_def: class(2),
                    lookahead_class_def: class(3),
                    rule_sets: ChainRuleSets(Sets {
                        base: data,
                        offsets,
                        w,
                    }),
                })
            }
            3 => {
                let (_, backtrack) = counted(data, 2, 2, 2)?;
                let at = backtrack.len().saturating_add(4);
                let (_, input) = counted(data, at, 2, 2)?;
                let at = at.saturating_add(2).saturating_add(input.len());
                let (_, lookahead) = counted(data, at, 2, 2)?;
                let at = at.saturating_add(2).saturating_add(lookahead.len());
                let (_, lookups) = counted(data, at, 2, 4)?;
                if input.is_empty() {
                    return Err(ReadError::InvalidValue);
                }
                Ok(ChainedSequenceContext::Coverages {
                    backtrack: CoverageArray::new(data, backtrack, 2),
                    input: CoverageArray::new(data, input, 2),
                    lookahead: CoverageArray::new(data, lookahead, 2),
                    lookups: LookupRecords::new(lookups),
                })
            }
            _ => Err(ReadError::UnknownFormat(format)),
        }
    }
}
