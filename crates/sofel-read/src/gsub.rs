//! The `GSUB` (glyph substitution) table (ADR 0020).
//!
//! The header, lists and lookups are those of [`crate::layout`];
//! [`subtable`] reads a lookup's subtable as a [`SubstSubtable`],
//! following an extension (type 7) to its type. Every lookup type 1 to 8
//! is read in all formats of HarfBuzz 15.1.0, the 24-bit ones of
//! ISO/IEC 14496-22 5th edition included (single substitution 3 and 4;
//! multiple, alternate and ligature substitution 2; context 4 to 6;
//! chained context 4 and 5; reverse chained single substitution 2).
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/gsub>

use crate::be;
use crate::layout::context::coverage_at;
use crate::layout::walk::{Layout, Walker, layout};
use crate::layout::{
    ChainedSequenceContext, Coverage, CoverageArray, GlyphArray, Lookup, SequenceContext, counted,
    follow, need, uint,
};
use crate::{Diagnostic, GlyphId, Kind, ReadError, Tag};

/// The `GSUB` tag.
pub const TAG: Tag = Tag::new(b"GSUB");

/// Lookup type 1, single substitution.
pub const SINGLE: u16 = 1;
/// Lookup type 2, multiple substitution.
pub const MULTIPLE: u16 = 2;
/// Lookup type 3, alternate substitution.
pub const ALTERNATE: u16 = 3;
/// Lookup type 4, ligature substitution.
pub const LIGATURE: u16 = 4;
/// Lookup type 5, contextual substitution.
pub const CONTEXT: u16 = 5;
/// Lookup type 6, chained contexts substitution.
pub const CHAIN_CONTEXT: u16 = 6;
/// Lookup type 7, extension substitution.
pub const EXTENSION: u16 = 7;
/// Lookup type 8, reverse chained contexts single substitution.
pub const REVERSE_CHAIN_SINGLE: u16 = 8;

/// A `GSUB` subtable, after following an extension.
#[derive(Clone, Copy, Debug)]
pub enum SubstSubtable<'a> {
    /// Single substitution format 1 (16-bit delta, ids modulo 0x10000)
    /// and 3 (24-bit delta, modulo 0x1000000): the substitute of a
    /// covered glyph `g` is `(g + delta) & mask`.
    SingleDelta {
        /// The glyphs to substitute.
        coverage: Coverage<'a>,
        /// `deltaGlyphID`, unsigned.
        delta: u32,
        /// 0xFFFF or 0xFFFFFF.
        mask: u32,
    },
    /// Single substitution format 2 and 4 (24-bit ids).
    SingleList {
        /// The glyphs to substitute.
        coverage: Coverage<'a>,
        /// The substitutes, by coverage index.
        substitutes: GlyphArray<'a>,
    },
    /// Multiple substitution, formats 1 and 2.
    Multiple {
        /// The glyphs to substitute.
        coverage: Coverage<'a>,
        /// The sequences, by coverage index.
        sequences: GlyphArrays<'a>,
    },
    /// Alternate substitution, formats 1 and 2.
    Alternate {
        /// The glyphs to substitute.
        coverage: Coverage<'a>,
        /// The alternate sets, by coverage index.
        alternate_sets: GlyphArrays<'a>,
    },
    /// Ligature substitution, formats 1 and 2.
    Ligature {
        /// The first glyphs of the ligatures.
        coverage: Coverage<'a>,
        /// The ligature sets, by coverage index.
        ligature_sets: LigatureSets<'a>,
    },
    /// Contextual substitution.
    Context(SequenceContext<'a>),
    /// Chained contexts substitution.
    ChainContext(ChainedSequenceContext<'a>),
    /// Reverse chained contexts single substitution, formats 1 and 2.
    ReverseChainSingle {
        /// The glyphs to substitute.
        coverage: Coverage<'a>,
        /// The backtrack coverages.
        backtrack: CoverageArray<'a>,
        /// The lookahead coverages.
        lookahead: CoverageArray<'a>,
        /// The substitutes, by coverage index.
        substitutes: GlyphArray<'a>,
    },
}

/// An array of offsets, from the start of a subtable, to arrays of glyph
/// ids with a 16-bit count (Sequence and AlternateSet tables).
#[derive(Clone, Copy, Debug)]
pub struct GlyphArrays<'a> {
    base: &'a [u8],
    offsets: &'a [u8],
    width: usize,
}

impl<'a> GlyphArrays<'a> {
    /// The number of arrays.
    pub fn len(&self) -> usize {
        self.offsets.len().checked_div(self.width).unwrap_or(0)
    }
    /// Whether there are no arrays.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    /// The offset of array `i` from the start of the subtable.
    pub fn offset(&self, i: usize) -> Option<u32> {
        uint(self.offsets, i.checked_mul(self.width)?, self.width)
    }
    /// Array `i`; [`ReadError::OutOfBounds`] for an index past the
    /// offsets.
    pub fn get(&self, i: usize) -> Result<Option<GlyphArray<'a>>, ReadError> {
        let o = self.offset(i).ok_or(ReadError::OutOfBounds)?;
        let w = self.width;
        follow(self.base, o, |d| GlyphArray::counted(d, 0, 2, w))
    }
}

/// An array of offsets to LigatureSet tables, from the start of the
/// subtable.
#[derive(Clone, Copy, Debug)]
pub struct LigatureSets<'a> {
    base: &'a [u8],
    offsets: &'a [u8],
    width: usize,
}

impl<'a> LigatureSets<'a> {
    /// The number of sets.
    pub fn len(&self) -> usize {
        self.offsets.len().checked_div(self.width).unwrap_or(0)
    }
    /// Whether there are no sets.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    /// The offset of set `i` from the start of the subtable.
    pub fn offset(&self, i: usize) -> Option<u32> {
        uint(self.offsets, i.checked_mul(self.width)?, self.width)
    }
    /// Set `i`; [`ReadError::OutOfBounds`] for an index past the offsets.
    pub fn get(&self, i: usize) -> Result<Option<LigatureSet<'a>>, ReadError> {
        let o = self.offset(i).ok_or(ReadError::OutOfBounds)?;
        let w = self.width;
        follow(self.base, o, |d| LigatureSet::new(d, w))
    }
}

/// A view over a LigatureSet table: a 16-bit count and offsets (16 or 24
/// bits) to Ligature tables.
#[derive(Clone, Copy, Debug)]
pub struct LigatureSet<'a> {
    data: &'a [u8],
    offsets: &'a [u8],
    width: usize,
}

impl<'a> LigatureSet<'a> {
    /// A view over a set whose offsets and glyph ids are `width` bytes
    /// wide. Fails with [`ReadError::Truncated`] if it does not fit.
    pub fn new(data: &'a [u8], width: usize) -> Result<LigatureSet<'a>, ReadError> {
        let (_, offsets) = counted(data, 0, 2, width)?;
        Ok(LigatureSet {
            data,
            offsets,
            width,
        })
    }
    /// The number of ligatures.
    pub fn len(&self) -> usize {
        self.offsets.len().checked_div(self.width).unwrap_or(0)
    }
    /// Whether there are no ligatures.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    /// The offset of ligature `i` from the start of the set.
    pub fn offset(&self, i: usize) -> Option<u32> {
        uint(self.offsets, i.checked_mul(self.width)?, self.width)
    }
    /// Ligature `i`; [`ReadError::OutOfBounds`] for an index past the
    /// offsets.
    pub fn get(&self, i: usize) -> Result<Option<Ligature<'a>>, ReadError> {
        let o = self.offset(i).ok_or(ReadError::OutOfBounds)?;
        let w = self.width;
        follow(self.data, o, |d| Ligature::new(d, w))
    }
}

/// A Ligature table.
#[derive(Clone, Copy, Debug)]
pub struct Ligature<'a> {
    /// The ligature glyph.
    pub glyph: GlyphId,
    /// The components after the first (`componentCount - 1`; none when
    /// the count is 0).
    pub components: GlyphArray<'a>,
}

impl<'a> Ligature<'a> {
    /// A view over a ligature whose glyph ids are `width` bytes wide.
    /// Fails with [`ReadError::Truncated`] if it does not fit.
    pub fn new(data: &'a [u8], width: usize) -> Result<Ligature<'a>, ReadError> {
        let header = width.saturating_add(2);
        let truncated = |n| ReadError::truncated(n, data.len());
        let glyph = uint(data, 0, width).ok_or(truncated(header))?;
        let count = be::u16_at(data, width).ok_or(truncated(header))?;
        let n = usize::from(count.saturating_sub(1)).saturating_mul(width);
        let components =
            be::slice_at(data, header, n).ok_or(truncated(header.saturating_add(n)))?;
        Ok(Ligature {
            glyph: GlyphId(glyph),
            components: GlyphArray::new(components, width),
        })
    }
}

/// `(coverage offset width, glyph id width)` of a format that is 1 for
/// 16-bit ids and `wide` for 24-bit ones.
fn widths(format: u16, wide: u16) -> Option<(usize, usize)> {
    if format == 1 {
        Some((2, 2))
    } else if format == wide {
        Some((4, 3))
    } else {
        None
    }
}

/// Reads the subtable at the start of `data` as lookup type `ty` (not an
/// extension).
pub fn parse(ty: u16, data: &[u8]) -> Result<SubstSubtable<'_>, ReadError> {
    let format = be::u16_at(data, 0).ok_or(ReadError::truncated(2, data.len()))?;
    let unknown = ReadError::UnknownFormat(format);
    match ty {
        SINGLE => match format {
            1 | 3 => {
                let (cw, dw) = if format == 1 {
                    (2usize, 2usize)
                } else {
                    (4, 3)
                };
                need(data, cw.saturating_add(dw).saturating_add(2))?;
                Ok(SubstSubtable::SingleDelta {
                    coverage: coverage_at(data, 2, cw),
                    delta: uint(data, cw.saturating_add(2), dw).unwrap_or(0),
                    mask: if format == 1 { 0xFFFF } else { 0xFF_FFFF },
                })
            }
            2 | 4 => {
                let (cw, gw) = if format == 2 {
                    (2usize, 2usize)
                } else {
                    (4, 3)
                };
                need(data, cw.saturating_add(2))?;
                Ok(SubstSubtable::SingleList {
                    coverage: coverage_at(data, 2, cw),
                    substitutes: GlyphArray::counted(data, cw.saturating_add(2), gw, gw)?,
                })
            }
            _ => Err(unknown),
        },
        MULTIPLE | ALTERNATE => {
            let (cw, gw) = widths(format, 2).ok_or(unknown)?;
            need(data, cw.saturating_add(2))?;
            let (_, offsets) = counted(data, cw.saturating_add(2), gw, gw)?;
            let coverage = coverage_at(data, 2, cw);
            let arrays = GlyphArrays {
                base: data,
                offsets,
                width: gw,
            };
            Ok(if ty == MULTIPLE {
                SubstSubtable::Multiple {
                    coverage,
                    sequences: arrays,
                }
            } else {
                SubstSubtable::Alternate {
                    coverage,
                    alternate_sets: arrays,
                }
            })
        }
        LIGATURE => {
            let (cw, gw) = widths(format, 2).ok_or(unknown)?;
            need(data, cw.saturating_add(2))?;
            let (_, offsets) = counted(data, cw.saturating_add(2), gw, gw)?;
            Ok(SubstSubtable::Ligature {
                coverage: coverage_at(data, 2, cw),
                ligature_sets: LigatureSets {
                    base: data,
                    offsets,
                    width: gw,
                },
            })
        }
        CONTEXT => SequenceContext::new(data).map(SubstSubtable::Context),
        CHAIN_CONTEXT => ChainedSequenceContext::new(data).map(SubstSubtable::ChainContext),
        REVERSE_CHAIN_SINGLE => {
            let (cw, gw) = widths(format, 2).ok_or(unknown)?;
            need(data, cw.saturating_add(2))?;
            let at = cw.saturating_add(2);
            let (_, backtrack) = counted(data, at, 2, gw)?;
            let at = at.saturating_add(2).saturating_add(backtrack.len());
            let (_, lookahead) = counted(data, at, 2, gw)?;
            let at = at.saturating_add(2).saturating_add(lookahead.len());
            Ok(SubstSubtable::ReverseChainSingle {
                coverage: coverage_at(data, 2, cw),
                backtrack: CoverageArray::new(data, backtrack, gw),
                lookahead: CoverageArray::new(data, lookahead, gw),
                substitutes: GlyphArray::counted(data, at, gw, gw)?,
            })
        }
        _ => Err(ReadError::InvalidValue),
    }
}

/// The data and type of the subtable an extension subtable points to.
pub(crate) fn extension_target(data: &[u8], extension: u16) -> Result<(u16, &[u8]), ReadError> {
    let format = be::u16_at(data, 0).ok_or(ReadError::truncated(2, data.len()))?;
    if format != 1 {
        return Err(ReadError::UnknownFormat(format));
    }
    need(data, 8)?;
    let ty = be::u16_at(data, 2).unwrap_or(0);
    if ty == extension {
        return Err(ReadError::InvalidValue);
    }
    let offset = be::u32_at(data, 4).unwrap_or(0);
    let target = follow(data, offset, Ok)?.ok_or(ReadError::OutOfBounds)?;
    Ok((ty, target))
}

/// Subtable `i` of `lookup`, following an extension (type 7) to its type.
///
/// Fails with [`ReadError::OutOfBounds`] for a null subtable offset or
/// one past the data, [`ReadError::InvalidValue`] for an unknown lookup
/// type or an extension that points to an extension, and otherwise as
/// the subtable's view does. HarfBuzz applies nothing for a subtable
/// that fails. It also drops a whole extension lookup whose subtables
/// point to different types; see [`Lookup::extension_types_agree`].
pub fn subtable<'a>(lookup: &Lookup<'a>, i: usize) -> Result<SubstSubtable<'a>, ReadError> {
    let data = lookup.subtable_data(i).ok_or(ReadError::OutOfBounds)?;
    match lookup.lookup_type() {
        EXTENSION => {
            let (ty, target) = extension_target(data, EXTENSION)?;
            parse(ty, target)
        }
        ty => parse(ty, data),
    }
}

/// The lookup type after following extensions, from the first subtable
/// (HarfBuzz `SubstLookup::get_type` semantics; 8 means the lookup is
/// applied in reverse).
pub fn effective_type(lookup: &Lookup<'_>) -> u16 {
    lookup.effective_type(EXTENSION)
}

/// Checks a `GSUB` table (ADR 0020): an unknown version and truncation of
/// the header (and nothing else then), offsets whose tables do not fit,
/// unknown formats and lookup types, ScriptList and LangSys records out
/// of tag order, feature and lookup indices not below the counts,
/// Coverage tables out of order, overlapping ClassDef ranges, arrays
/// whose length is not the number of glyphs covered, extensions of
/// different types or pointing to extensions, sequence lookup records
/// past the input, ligatures of no components, reserved lookup flags
/// (Warning), and mark filtering
/// sets not below `mark_sets`, the number of mark glyph sets in `GDEF`
/// (unchecked when `None`).
pub fn validate(data: &[u8], mark_sets: Option<u16>, report: &mut dyn FnMut(Diagnostic)) {
    layout(
        TAG,
        data,
        Layout {
            extension: EXTENSION,
            max_type: REVERSE_CHAIN_SINGLE,
            mark_sets,
            subtable: &mut validate_subtable,
        },
        report,
    );
}

pub(crate) fn inspect_partial_lookup(
    data: &[u8],
    at: usize,
    lookup: &Lookup<'_>,
    mark_sets: Option<u16>,
    report: &mut dyn FnMut(Diagnostic),
) {
    let mut walker = Walker::new(TAG, data, report);
    let mut spec = Layout {
        extension: EXTENSION,
        max_type: REVERSE_CHAIN_SINGLE,
        mark_sets,
        subtable: &mut validate_subtable,
    };
    crate::layout::walk::one_lookup(&mut walker, at, lookup, &mut spec);
}

fn validate_subtable(w: &mut Walker<'_, '_>, field: usize, at: usize, ty: u16) {
    match ty {
        CONTEXT => return w.context(field, at),
        CHAIN_CONTEXT => return w.chain_context(field, at),
        _ => {}
    }
    let view = match parse(ty, w.tail(at)) {
        Ok(v) => v,
        Err(e) => return w.error(field, at, e),
    };
    let format = w.uint(at, 2);
    let (cw, gw) = if format == 1 || (ty == SINGLE && format == 2) {
        (2, 2)
    } else {
        (4, 3)
    };
    let covered = w.coverage(at.saturating_add(2), at, cw);
    let count_at = at.saturating_add(2).saturating_add(cw);
    match view {
        SubstSubtable::SingleDelta { .. } | SubstSubtable::Context(_) => {}
        SubstSubtable::ChainContext(_) => {}
        SubstSubtable::SingleList { substitutes, .. } => {
            w.count_matches(count_at, covered, substitutes.len());
        }
        SubstSubtable::Multiple { sequences: a, .. }
        | SubstSubtable::Alternate {
            alternate_sets: a, ..
        } => {
            w.count_matches(count_at, covered, a.len());
            for i in 0..a.len() {
                let f = count_at
                    .saturating_add(gw)
                    .saturating_add(i.saturating_mul(gw));
                w.follow(f, at, gw, |d| GlyphArray::counted(d, 0, 2, gw));
            }
        }
        SubstSubtable::Ligature { ligature_sets, .. } => {
            w.count_matches(count_at, covered, ligature_sets.len());
            for i in 0..ligature_sets.len() {
                let f = count_at
                    .saturating_add(gw)
                    .saturating_add(i.saturating_mul(gw));
                let Some((set_at, _, available)) = w.offset_prefix(f, at, gw, 0, gw) else {
                    continue;
                };
                for j in 0..available {
                    let f = set_at
                        .saturating_add(2)
                        .saturating_add(j.saturating_mul(gw));
                    if let Some((lig_at, _)) = w.follow(f, set_at, gw, |d| Ligature::new(d, gw))
                        && w.uint(lig_at.saturating_add(gw), 2) == 0
                    {
                        // HarfBuzz applies no ligature of no components.
                        w.emit(lig_at.saturating_add(gw), Kind::EmptyInputSequence);
                    }
                }
            }
        }
        SubstSubtable::ReverseChainSingle {
            backtrack,
            lookahead,
            substitutes,
            ..
        } => {
            let mut p = count_at;
            for a in [backtrack, lookahead] {
                for i in 0..a.len() {
                    w.coverage(
                        p.saturating_add(2).saturating_add(i.saturating_mul(gw)),
                        at,
                        gw,
                    );
                }
                p = p.saturating_add(2).saturating_add(a.offsets().len());
            }
            w.count_matches(p, covered, substitutes.len());
        }
    }
}
