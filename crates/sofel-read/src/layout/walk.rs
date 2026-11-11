//! The validator walk shared by `GDEF`, `GSUB` and `GPOS` (ADR 0007,
//! ADR 0020). Positions are absolute: from the start of the table.
//!
//! Every followed offset and every scanned record spends from a budget
//! proportional to the size of the table (HarfBuzz's sanitizer budget:
//! 64 operations per byte, at least 16384), so that validation ends in
//! time linear in the input even when subtables are shared by many
//! offsets. When the budget runs out the walk stops with
//! [`Kind::WorkLimitExceeded`]; HarfBuzz rejects such a table too. The
//! walk also stops so after 1000 diagnostics, since a shared subtable
//! reports its problems once per offset that reaches it.

use super::context::{ChainRule, Rule, chain_widths, context_widths};
use super::{ClassDef, Coverage, Lookup, LookupList, counted, header_size, lookup_flag, uint};
use crate::be::{self, to_u32, to_usize};
use crate::device::Device;
use crate::{Diagnostic, Kind, ReadError, Tag};

const OPS_FACTOR: usize = 64;
const OPS_MIN: usize = 16384;
const OPS_MAX: usize = 0x3FFF_FFFF;
/// The most diagnostics one table reports before the walk stops.
const MAX_REPORTS: usize = 1000;

/// The state of a layout table validation.
pub(crate) struct Walker<'d, 'r> {
    tag: Tag,
    pub(crate) data: &'d [u8],
    report: &'r mut dyn FnMut(Diagnostic),
    budget: usize,
    exhausted: bool,
    reports: usize,
    /// The number of lookups, for checking lookup indices; `None` when
    /// unknown.
    pub(crate) lookup_count: Option<usize>,
}

impl<'d, 'r> Walker<'d, 'r> {
    pub(crate) fn new(
        tag: Tag,
        data: &'d [u8],
        report: &'r mut dyn FnMut(Diagnostic),
    ) -> Walker<'d, 'r> {
        let budget = data
            .len()
            .saturating_mul(OPS_FACTOR)
            .clamp(OPS_MIN, OPS_MAX);
        Walker {
            tag,
            data,
            report,
            budget,
            exhausted: false,
            reports: 0,
            lookup_count: None,
        }
    }

    pub(crate) fn emit(&mut self, at: usize, kind: Kind) {
        if self.exhausted {
            return;
        }
        self.reports = self.reports.saturating_add(1);
        if self.reports > MAX_REPORTS {
            self.exhausted = true;
            (self.report)(Diagnostic::new(self.tag, 0, Kind::WorkLimitExceeded));
            return;
        }
        (self.report)(Diagnostic::new(self.tag, to_u32(at), kind));
    }

    /// Spends `n` units of the budget; false (and one report) once it is
    /// exhausted.
    pub(crate) fn spend(&mut self, n: usize) -> bool {
        if self.exhausted {
            return false;
        }
        match self.budget.checked_sub(n.max(1)) {
            Some(b) => {
                self.budget = b;
                true
            }
            None => {
                self.exhausted = true;
                (self.report)(Diagnostic::new(self.tag, 0, Kind::WorkLimitExceeded));
                false
            }
        }
    }

    /// An unsigned integer of `width` bytes at `at`; 0 when it is not there.
    pub(crate) fn uint(&self, at: usize, width: usize) -> u32 {
        uint(self.data, at, width).unwrap_or(0)
    }

    /// The bytes from `at` to the end of the table.
    pub(crate) fn tail(&self, at: usize) -> &'d [u8] {
        be::tail_at(self.data, at).unwrap_or(&[])
    }

    /// Reports `e`, the failure to build the table at `at` pointed to by
    /// the offset at `field`.
    pub(crate) fn error(&mut self, field: usize, at: usize, e: ReadError) {
        let available = to_u32(self.data.len());
        let kind = match e {
            ReadError::UnknownFormat(f) => Kind::UnknownFormat {
                found: u32::from(f),
            },
            ReadError::UnknownVersion(found) => Kind::UnknownVersion { found },
            ReadError::Truncated { needed, .. } => Kind::OffsetOutOfBounds {
                offset: to_u32(at),
                length: to_u32(needed),
                available,
            },
            ReadError::OutOfBounds | ReadError::InvalidValue => Kind::OffsetOutOfBounds {
                offset: to_u32(at),
                length: 0,
                available,
            },
        };
        self.emit(field, kind);
    }

    /// The table at `base` plus the offset of `width` bytes at `field`,
    /// built by `make`; `None` for a null offset, and with a report when
    /// it cannot be built.
    pub(crate) fn follow<T>(
        &mut self,
        field: usize,
        base: usize,
        width: usize,
        make: impl FnOnce(&'d [u8]) -> Result<T, ReadError>,
    ) -> Option<(usize, T)> {
        let offset = self.uint(field, width);
        if offset == 0 || !self.spend(1) {
            return None;
        }
        let at = base.saturating_add(to_usize(offset));
        let data: &'d [u8] = self.data;
        match be::tail_at(data, at)
            .ok_or(ReadError::OutOfBounds)
            .and_then(make)
        {
            Ok(t) => Some((at, t)),
            Err(e) => {
                self.error(field, at, e);
                None
            }
        }
    }

    /// Checks an offset of `width` bytes at `field` to a table of at
    /// least `length` bytes that is not read further.
    pub(crate) fn opaque(&mut self, field: usize, base: usize, width: usize, length: usize) {
        self.follow(field, base, width, |d| super::need(d, length));
    }

    /// Reports a count at `field` that is not the number of glyphs of
    /// the coverage table that goes with it.
    pub(crate) fn count_matches(&mut self, field: usize, coverage: Option<usize>, count: usize) {
        if let Some(c) = coverage
            && c != count
        {
            self.emit(
                field,
                Kind::CoverageCountMismatch {
                    coverage: to_u32(c),
                    count: to_u32(count),
                },
            );
        }
    }

    /// Validates the Coverage table pointed to by the offset at `field`;
    /// the number of glyphs it covers when it could be read.
    pub(crate) fn coverage(&mut self, field: usize, base: usize, width: usize) -> Option<usize> {
        let (at, c) = self.follow(field, base, width, Coverage::new)?;
        if !self.spend(c.num_records()) {
            return None;
        }
        let w = if c.format() >= 3 { 3 } else { 2 };
        let first = at.saturating_add(2).saturating_add(w);
        match c.format() {
            1 | 3 => {
                let mut previous = None;
                for i in 0..c.num_records() {
                    let g = c.glyph(i)?;
                    let at = first.saturating_add(i.saturating_mul(w));
                    match previous {
                        Some(p) if g < p => {
                            self.emit(at, Kind::CoverageNotSorted { index: to_u32(i) });
                        }
                        Some(p) if g == p => self.emit(
                            at,
                            Kind::CoverageDuplicate {
                                index: to_u32(i),
                                glyph: g.0,
                            },
                        ),
                        _ => {}
                    }
                    previous = Some(g);
                }
                Some(c.num_records())
            }
            _ => {
                let size = w.saturating_mul(3);
                let mut total = 0u32;
                let mut last = None;
                for i in 0..c.num_records() {
                    let (start, end, index) = c.range(i)?;
                    let at = first.saturating_add(i.saturating_mul(size));
                    if start > end {
                        self.emit(
                            at,
                            Kind::StartAfterEnd {
                                index: to_u32(i),
                                start: start.0,
                                end: end.0,
                            },
                        );
                        continue;
                    }
                    if last.is_some_and(|l| start <= l) {
                        self.emit(at, Kind::CoverageNotSorted { index: to_u32(i) });
                    }
                    if index != total {
                        self.emit(
                            at.saturating_add(w.saturating_mul(2)),
                            Kind::CoverageIndexMismatch {
                                index: to_u32(i),
                                expected: total,
                                found: index,
                            },
                        );
                    }
                    total = total.saturating_add(end.0.wrapping_sub(start.0).saturating_add(1));
                    last = Some(end);
                }
                Some(to_usize(total))
            }
        }
    }

    /// Validates the ClassDef table pointed to by the offset at `field`.
    pub(crate) fn class_def(&mut self, field: usize, base: usize, width: usize) {
        let Some((at, c)) = self.follow(field, base, width, ClassDef::new) else {
            return;
        };
        if !matches!(c.format(), 2 | 4) || !self.spend(c.num_records()) {
            return;
        }
        let (first, size) = if c.format() == 2 {
            (at.saturating_add(4), 6)
        } else {
            (at.saturating_add(5), 8)
        };
        let mut last = None;
        for i in 0..c.num_records() {
            let Some((start, end, _)) = c.range(i) else {
                return;
            };
            let at = first.saturating_add(i.saturating_mul(size));
            if start > end {
                self.emit(
                    at,
                    Kind::StartAfterEnd {
                        index: to_u32(i),
                        start: start.0,
                        end: end.0,
                    },
                );
                continue;
            }
            if last.is_some_and(|l| start <= l) {
                self.emit(at, Kind::ClassRangesOverlap { index: to_u32(i) });
            }
            last = Some(end);
        }
    }

    /// Validates the Device or VariationIndex table pointed to by the
    /// offset at `field`.
    pub(crate) fn device(&mut self, field: usize, base: usize) {
        self.follow(field, base, 2, Device::new);
    }

    /// Checks that `records` (SequenceLookupRecords at `at`) apply
    /// lookups that exist at positions of an input of `input` glyphs.
    pub(crate) fn lookup_records(&mut self, at: usize, records: &[u8], input: usize) {
        let n = records.len() / 4;
        if !self.spend(n) {
            return;
        }
        for i in 0..n {
            let r = at.saturating_add(i.saturating_mul(4));
            let sequence = usize::from(be::u16_at(records, i.saturating_mul(4)).unwrap_or(0));
            let lookup = usize::from(
                be::u16_at(records, i.saturating_mul(4).saturating_add(2)).unwrap_or(0),
            );
            if sequence >= input {
                self.emit(
                    r,
                    Kind::SequenceIndexOutOfRange {
                        index: to_u32(sequence),
                        count: to_u32(input),
                    },
                );
            }
            if let Some(count) = self.lookup_count
                && lookup >= count
            {
                self.emit(
                    r.saturating_add(2),
                    Kind::LookupIndexOutOfRange {
                        index: to_u32(lookup),
                        count: to_u32(count),
                    },
                );
            }
        }
    }

    /// Validates the rule sets of a (chained) sequence context subtable
    /// at `at`: `count` offsets of `w.sets` bytes at `first`.
    fn rule_sets(&mut self, at: usize, first: usize, count: usize, chained: bool) {
        let format = be::u16_at(self.data, at).unwrap_or(0);
        let w = if chained {
            chain_widths(format)
        } else {
            context_widths(format)
        };
        let Some(w) = w else { return };
        for i in 0..count {
            let field = first.saturating_add(i.saturating_mul(w.sets));
            let Some((set_at, (n, _))) =
                self.follow(field, at, w.sets, |d| counted(d, 0, 2, w.rules))
            else {
                continue;
            };
            for j in 0..n {
                let field = set_at
                    .saturating_add(2)
                    .saturating_add(j.saturating_mul(w.rules));
                if chained {
                    let Some((rule_at, rule)) =
                        self.follow(field, set_at, w.rules, |d| ChainRule::new(d, w.values))
                    else {
                        continue;
                    };
                    let input_at = rule_at
                        .saturating_add(2)
                        .saturating_add(rule.backtrack.data().len());
                    let input = usize::from(be::u16_at(self.data, input_at).unwrap_or(0));
                    let records_at = input_at
                        .saturating_add(4)
                        .saturating_add(rule.input.data().len())
                        .saturating_add(rule.lookahead.data().len())
                        .saturating_add(2);
                    self.lookup_records(records_at, rule.lookups.data(), input);
                } else {
                    let Some((rule_at, rule)) =
                        self.follow(field, set_at, w.rules, |d| Rule::new(d, w.values))
                    else {
                        continue;
                    };
                    let input = usize::from(be::u16_at(self.data, rule_at).unwrap_or(0));
                    let records_at = rule_at
                        .saturating_add(4)
                        .saturating_add(rule.input.data().len());
                    self.lookup_records(records_at, rule.lookups.data(), input);
                }
            }
        }
    }

    /// Validates the coverage offsets of `count` entries of `width`
    /// bytes at `first`, from the subtable at `at`.
    fn coverages(&mut self, at: usize, first: usize, count: usize, width: usize) {
        for i in 0..count {
            self.coverage(first.saturating_add(i.saturating_mul(width)), at, width);
        }
    }

    /// Validates a sequence context subtable (`GSUB` 5, `GPOS` 7) at `at`,
    /// pointed to by the offset at `field`.
    pub(crate) fn context(&mut self, field: usize, at: usize) {
        let format = be::u16_at(self.data, at).unwrap_or(0);
        match super::SequenceContext::new(self.tail(at)) {
            Err(ReadError::InvalidValue) => {
                return self.emit(at.saturating_add(2), Kind::EmptyInputSequence);
            }
            Err(e) => return self.error(field, at, e),
            Ok(_) => {}
        }
        match format {
            1 | 4 => {
                let (cw, sw) = if format == 1 {
                    (2usize, 2usize)
                } else {
                    (4, 3)
                };
                let covered = self.coverage(at.saturating_add(2), at, cw);
                let count_at = at.saturating_add(2).saturating_add(cw);
                let count = to_usize(self.uint(count_at, sw));
                self.count_matches(count_at, covered, count);
                self.rule_sets(at, count_at.saturating_add(sw), count, false);
            }
            2 | 5 => {
                let (cw, sw) = if format == 2 {
                    (2usize, 2usize)
                } else {
                    (4, 3)
                };
                self.coverage(at.saturating_add(2), at, cw);
                self.class_def(at.saturating_add(2).saturating_add(cw), at, cw);
                let count_at = at.saturating_add(2).saturating_add(cw.saturating_mul(2));
                let count = to_usize(self.uint(count_at, sw));
                self.rule_sets(at, count_at.saturating_add(sw), count, false);
            }
            _ => {
                let width = if format == 3 { 2 } else { 3 };
                let glyphs = to_usize(self.uint(at.saturating_add(2), 2));
                let lookups = to_usize(self.uint(at.saturating_add(4), 2));
                self.coverages(at, at.saturating_add(6), glyphs, width);
                let records_at = at
                    .saturating_add(6)
                    .saturating_add(glyphs.saturating_mul(width));
                let records =
                    be::slice_at(self.data, records_at, lookups.saturating_mul(4)).unwrap_or(&[]);
                self.lookup_records(records_at, records, glyphs);
            }
        }
    }

    /// Validates a chained sequence context subtable (`GSUB` 6, `GPOS` 8)
    /// at `at`, pointed to by the offset at `field`.
    pub(crate) fn chain_context(&mut self, field: usize, at: usize) {
        let format = be::u16_at(self.data, at).unwrap_or(0);
        match super::ChainedSequenceContext::new(self.tail(at)) {
            Err(ReadError::InvalidValue) => {
                let backtrack = to_usize(self.uint(at.saturating_add(2), 2));
                let input_at = at
                    .saturating_add(4)
                    .saturating_add(backtrack.saturating_mul(2));
                return self.emit(input_at, Kind::EmptyInputSequence);
            }
            Err(e) => return self.error(field, at, e),
            Ok(_) => {}
        }
        match format {
            1 | 4 => {
                let (cw, sw) = if format == 1 {
                    (2usize, 2usize)
                } else {
                    (4, 3)
                };
                let covered = self.coverage(at.saturating_add(2), at, cw);
                let count_at = at.saturating_add(2).saturating_add(cw);
                let count = to_usize(self.uint(count_at, sw));
                self.count_matches(count_at, covered, count);
                self.rule_sets(at, count_at.saturating_add(sw), count, true);
            }
            2 | 5 => {
                let cw = if format == 2 { 2usize } else { 4 };
                self.coverage(at.saturating_add(2), at, cw);
                for i in 1..=3usize {
                    self.class_def(
                        at.saturating_add(2).saturating_add(cw.saturating_mul(i)),
                        at,
                        cw,
                    );
                }
                let count_at = at.saturating_add(2).saturating_add(cw.saturating_mul(4));
                let count = to_usize(self.uint(count_at, 2));
                self.rule_sets(at, count_at.saturating_add(2), count, true);
            }
            _ => {
                let mut p = at.saturating_add(2);
                let mut input = 0;
                for k in 0..3 {
                    let n = to_usize(self.uint(p, 2));
                    self.coverages(at, p.saturating_add(2), n, 2);
                    if k == 1 {
                        input = n;
                    }
                    p = p.saturating_add(2).saturating_add(n.saturating_mul(2));
                }
                let n = to_usize(self.uint(p, 2));
                let records = be::slice_at(self.data, p.saturating_add(2), n.saturating_mul(4))
                    .unwrap_or(&[]);
                self.lookup_records(p.saturating_add(2), records, input);
            }
        }
    }
}

/// What a `GSUB` or `GPOS` validation needs to know about its table.
pub(crate) struct Layout<'f> {
    /// The lookup type of an extension subtable.
    pub(crate) extension: u16,
    /// The largest lookup type.
    pub(crate) max_type: u16,
    /// The number of mark glyph sets in `GDEF`; `None` when unknown.
    pub(crate) mark_sets: Option<u16>,
    /// Validates the subtable of a type at a position, pointed to by the
    /// offset at a field: `(walker, field, at, type)`.
    pub(crate) subtable: &'f mut dyn FnMut(&mut Walker<'_, '_>, usize, usize, u16),
}

/// Validates a `GSUB` or `GPOS` table: the header, the lists and every
/// lookup, with `spec.subtable` for the subtables.
pub(crate) fn layout(
    tag: Tag,
    data: &[u8],
    mut spec: Layout<'_>,
    report: &mut dyn FnMut(Diagnostic),
) {
    let table = match super::LayoutTable::new(data) {
        Ok(t) => t,
        Err(ReadError::UnknownVersion(found)) => {
            return report(Diagnostic::new(tag, 0, Kind::UnknownVersion { found }));
        }
        Err(_) => {
            let needed = be::u32_at(data, 0).map_or(4, header_size);
            return report(Diagnostic::new(
                tag,
                0,
                Kind::Truncated {
                    needed: to_u32(needed),
                    available: to_u32(data.len()),
                },
            ));
        }
    };
    let mut w = Walker::new(tag, data, report);
    let wide = table.version() >= super::VERSION_1_2;
    let field = |narrow: usize, offset_wide: usize| -> (usize, usize) {
        if wide && be::u32_at(data, offset_wide).is_some_and(|o| o != 0) {
            (offset_wide, 4)
        } else {
            (narrow, 2)
        }
    };
    if let Some(off) = table.feature_variations_offset()
        && off != 0
        && to_usize(off) >= data.len()
    {
        w.emit(
            10,
            Kind::OffsetOutOfBounds {
                offset: off,
                length: 0,
                available: to_u32(data.len()),
            },
        );
    }

    let (lf, lw) = field(8, 22);
    let lookups = w.follow(lf, 0, lw, |d| LookupList::new(d, lw == 4));
    w.lookup_count = Some(lookups.map_or(0, |(_, l)| l.len()));
    let (ff, fw) = field(6, 18);
    let feature_count = features(&mut w, ff, fw);
    let (sf, sw) = field(4, 14);
    scripts(&mut w, sf, sw, feature_count);
    if let Some((at, list)) = lookups {
        let width = if list.is_wide() { 4 } else { 2 };
        for i in 0..list.len() {
            let field = at.saturating_add(2).saturating_add(i.saturating_mul(width));
            if let Some((lookup_at, lookup)) = w.follow(field, at, width, Lookup::new) {
                one_lookup(&mut w, lookup_at, &lookup, &mut spec);
            }
        }
    }
}

fn features(w: &mut Walker<'_, '_>, field: usize, width: usize) -> usize {
    let Some((at, list)) = w.follow(field, 0, width, super::FeatureList::new) else {
        return 0;
    };
    if !w.spend(list.len()) {
        return list.len();
    }
    for i in 0..list.len() {
        let record = at.saturating_add(2).saturating_add(i.saturating_mul(6));
        let Some((fat, feature)) = w.follow(record.saturating_add(4), at, 2, super::Feature::new)
        else {
            continue;
        };
        one_feature(w, fat, &feature);
    }
    list.len()
}

pub(crate) fn one_feature(w: &mut Walker<'_, '_>, fat: usize, feature: &super::Feature<'_>) {
    if feature.feature_params_offset() != 0 {
        w.opaque(fat, fat, 2, 1);
    }
    if !w.spend(feature.num_lookups()) {
        return;
    }
    if let Some(count) = w.lookup_count {
        for j in 0..feature.num_lookups() {
            let index = usize::from(feature.lookup_index(j).unwrap_or(0));
            if index >= count {
                w.emit(
                    fat.saturating_add(4).saturating_add(j.saturating_mul(2)),
                    Kind::LookupIndexOutOfRange {
                        index: to_u32(index),
                        count: to_u32(count),
                    },
                );
            }
        }
    }
}

fn sorted_tags(w: &mut Walker<'_, '_>, first: usize, tags: impl Iterator<Item = Tag>) {
    let mut previous: Option<Tag> = None;
    for (i, t) in tags.enumerate() {
        if previous.is_some_and(|p| t <= p) {
            let at = first.saturating_add(i.saturating_mul(6));
            w.emit(at, Kind::TagsNotSorted { index: to_u32(i) });
        }
        previous = Some(t);
    }
}

fn scripts(w: &mut Walker<'_, '_>, field: usize, width: usize, features: usize) {
    let Some((at, list)) = w.follow(field, 0, width, super::ScriptList::new) else {
        return;
    };
    if !w.spend(list.len()) {
        return;
    }
    sorted_tags(
        w,
        at.saturating_add(2),
        (0..list.len()).filter_map(|i| list.tag(i)),
    );
    for i in 0..list.len() {
        let record = at.saturating_add(2).saturating_add(i.saturating_mul(6));
        let Some((sat, script)) = w.follow(record.saturating_add(4), at, 2, super::Script::new)
        else {
            continue;
        };
        one_script(w, sat, &script, features);
    }
}

pub(crate) fn one_script(
    w: &mut Walker<'_, '_>,
    sat: usize,
    script: &super::Script<'_>,
    features: usize,
) {
    if !w.spend(script.num_lang_sys()) {
        return;
    }
    sorted_tags(
        w,
        sat.saturating_add(4),
        (0..script.num_lang_sys()).filter_map(|i| script.lang_sys_tag(i)),
    );
    lang_sys(w, sat, sat, features);
    for j in 0..script.num_lang_sys() {
        let field = sat
            .saturating_add(4)
            .saturating_add(j.saturating_mul(6))
            .saturating_add(4);
        lang_sys(w, field, sat, features);
    }
}

fn lang_sys(w: &mut Walker<'_, '_>, field: usize, base: usize, features: usize) {
    let Some((at, ls)) = w.follow(field, base, 2, super::LangSys::new) else {
        return;
    };
    let required = ls.required_feature_index();
    if required != 0xFFFF && usize::from(required) >= features {
        w.emit(
            at.saturating_add(2),
            Kind::FeatureIndexOutOfRange {
                index: u32::from(required),
                count: to_u32(features),
            },
        );
    }
    if !w.spend(ls.num_features()) {
        return;
    }
    for j in 0..ls.num_features() {
        let index = usize::from(ls.feature_index(j).unwrap_or(0));
        if index >= features {
            w.emit(
                at.saturating_add(6).saturating_add(j.saturating_mul(2)),
                Kind::FeatureIndexOutOfRange {
                    index: to_u32(index),
                    count: to_u32(features),
                },
            );
        }
    }
}

pub(crate) fn one_lookup(
    w: &mut Walker<'_, '_>,
    at: usize,
    lookup: &Lookup<'_>,
    spec: &mut Layout<'_>,
) {
    let flag = lookup.flag();
    if flag & lookup_flag::RESERVED != 0 {
        w.emit(
            at.saturating_add(2),
            Kind::ReservedBitsSet {
                found: u32::from(flag),
            },
        );
    }
    let n = lookup.num_subtables();
    if let (Some(set), Some(count)) = (lookup.mark_filtering_set(), spec.mark_sets)
        && set >= count
    {
        w.emit(
            at.saturating_add(6).saturating_add(n.saturating_mul(2)),
            Kind::MarkSetOutOfRange {
                index: u32::from(set),
                count: u32::from(count),
            },
        );
    }
    let ty = lookup.lookup_type();
    if ty == 0 || (ty > spec.max_type && ty != spec.extension) {
        return w.emit(
            at,
            Kind::UnknownLookupType {
                found: u32::from(ty),
            },
        );
    }
    let mut first = None;
    for i in 0..n {
        let field = at.saturating_add(6).saturating_add(i.saturating_mul(2));
        let offset = w.uint(field, 2);
        if offset == 0 || !w.spend(1) {
            continue;
        }
        let sub = at.saturating_add(to_usize(offset));
        if ty != spec.extension {
            if sub >= w.data.len() {
                w.error(field, sub, ReadError::OutOfBounds);
                continue;
            }
            (spec.subtable)(w, field, sub, ty);
            continue;
        }
        if let Err(e) = super::need(w.tail(sub), 8) {
            w.error(field, sub, e);
            continue;
        }
        let format = w.uint(sub, 2);
        if format != 1 {
            w.emit(sub, Kind::UnknownFormat { found: format });
            continue;
        }
        let ext = u16::try_from(w.uint(sub.saturating_add(2), 2)).unwrap_or(0);
        if ext == spec.extension {
            w.emit(sub.saturating_add(2), Kind::NestedExtension);
            continue;
        }
        if ext == 0 || ext > spec.max_type {
            w.emit(
                sub.saturating_add(2),
                Kind::UnknownLookupType {
                    found: u32::from(ext),
                },
            );
            continue;
        }
        match first {
            None => first = Some(ext),
            Some(f) if f != ext => w.emit(
                sub.saturating_add(2),
                Kind::ExtensionTypeMismatch {
                    expected: u32::from(f),
                    found: u32::from(ext),
                },
            ),
            Some(_) => {}
        }
        let target = sub.saturating_add(to_usize(w.uint(sub.saturating_add(4), 4)));
        if target == sub {
            continue;
        }
        if target >= w.data.len() {
            w.error(sub.saturating_add(4), target, ReadError::OutOfBounds);
            continue;
        }
        (spec.subtable)(w, sub.saturating_add(4), target, ext);
    }
}
