//! GSUB style coverage for FreeType's HarfBuzz-backed automatic hinter.

use alloc::collections::BTreeSet;
use alloc::vec;
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::gpos;
use sofel_font::read::gsub::{self, SubstSubtable};
use sofel_font::read::layout::{
    ChainedSequenceContext, FeatureList, LayoutTable, LookupList, ScriptList, SequenceContext,
};

use crate::Script;
use crate::layout::Layout;
use crate::layout::would::{WouldApply, lookup_would_substitute};

#[derive(Clone, Copy)]
struct LookupContext<'a> {
    font: &'a Font<'a>,
    accelerated: &'a Layout<'a>,
    scripts: ScriptList<'a>,
    features: FeatureList<'a>,
    lookups: LookupList<'a>,
}
use crate::ot_tags_from_script_and_language_max;

/// One style from the pinned FreeType automatic-hinter tables.
#[derive(Clone, Copy, Debug)]
pub struct AutoHintStyleRequest<'a> {
    /// Index into FreeType's ordered style table.
    pub index: u16,
    /// ISO 15924 script tag.
    pub script: [u8; 4],
    /// Unicode ranges considered for direct cmap coverage.
    pub ranges: &'a [(u32, u32)],
    /// Unicode ranges of non-base glyphs.
    pub nonbase_ranges: &'a [(u32, u32)],
    /// OpenType feature tag, or `None` for a default style.
    pub feature: Option<[u8; 4]>,
    /// UTF-8 strings used to determine whether a feature has blue zones.
    pub blue_strings: &'a [&'a [u8]],
}

/// Assigns FreeType's automatic-hinter styles in the order of its style
/// classes, extending direct cmap assignments through GSUB coverage.
pub fn auto_hint_gsub_styles(
    font: &Font<'_>,
    requests: &[AutoHintStyleRequest<'_>],
    fallback_style: u16,
    default_style: u16,
) -> Vec<u16> {
    const UNASSIGNED: u16 = 0x1fff;
    let mut styles = vec![UNASSIGNED; font.num_glyphs() as usize];
    let layout = font
        .table(Tag::new(b"GSUB"))
        .and_then(|data| LayoutTable::new(data).ok());
    let tables = layout.and_then(|gsub| {
        Some((
            gsub.script_list().ok()??,
            gsub.feature_list().ok()??,
            gsub.lookup_list().ok()??,
        ))
    });
    let accelerated = Layout::new(font);

    for request in requests {
        if request.feature.is_none() {
            assign_cmap_style(font, &mut styles, request);
        } else if let Some((scripts, features, lookups)) = tables {
            let context = LookupContext {
                font,
                accelerated: &accelerated,
                scripts,
                features,
                lookups,
            };
            assign_gsub_style(&context, &mut styles, request, false);
        }
    }
    if let Some((scripts, features, lookups)) = tables {
        for request in requests.iter().filter(|request| request.feature.is_none()) {
            let context = LookupContext {
                font,
                accelerated: &accelerated,
                scripts,
                features,
                lookups,
            };
            assign_gsub_style(&context, &mut styles, request, false);
        }
        if let Some(request) = requests.get(usize::from(default_style)) {
            let context = LookupContext {
                font,
                accelerated: &accelerated,
                scripts,
                features,
                lookups,
            };
            assign_gsub_style(&context, &mut styles, request, true);
        }
    }
    for codepoint in 0x30..=0x39 {
        if let Some(glyph) = font.nominal_glyph_raw(codepoint)
            && let Some(style) = styles.get_mut(glyph.0 as usize)
        {
            *style |= 0x8000;
        }
    }
    for style in &mut styles {
        if *style & 0x1fff == UNASSIGNED {
            *style = (*style & !0x1fff) | fallback_style;
        }
    }
    styles
}

fn assign_cmap_style(font: &Font<'_>, styles: &mut [u16], request: &AutoHintStyleRequest<'_>) {
    for &(first, last) in request.ranges {
        for codepoint in first..=last {
            if let Some(glyph) = font.nominal_glyph_raw(codepoint)
                && glyph.0 != 0
                && let Some(style) = styles.get_mut(glyph.0 as usize)
                && *style & 0x1fff == 0x1fff
            {
                *style = request.index | 0x2000;
            }
        }
    }
    for &(first, last) in request.nonbase_ranges {
        for codepoint in first..=last {
            if let Some(glyph) = font.nominal_glyph_raw(codepoint)
                && let Some(style) = styles.get_mut(glyph.0 as usize)
                && *style & 0x1fff == request.index
            {
                *style |= 0x4000;
            }
        }
    }
}

fn assign_gsub_style(
    context: &LookupContext<'_>,
    styles: &mut [u16],
    request: &AutoHintStyleRequest<'_>,
    include_default_script: bool,
) {
    let LookupContext {
        font,
        accelerated,
        scripts,
        features,
        lookups,
    } = *context;
    let Some(script) = Script::from_iso15924(&request.script) else {
        return;
    };
    let (mut script_tags, _) = ot_tags_from_script_and_language_max(Some(script), None, 3, 0);
    if include_default_script && !script_tags.contains(&Tag::new(b"DFLT")) {
        script_tags.push(Tag::new(b"DFLT"));
    }
    if !include_default_script && script_tags.first() == Some(&Tag::new(b"DFLT")) {
        return;
    }
    let feature = request.feature.map(|tag| Tag::new(&tag));
    let lookup_indices = collect_lookup_indices(scripts, features, &script_tags, feature);
    if lookup_indices.is_empty() {
        return;
    }
    if feature.is_some()
        && !request.blue_strings.iter().any(|bytes| {
            core::str::from_utf8(bytes).is_ok_and(|string| {
                string
                    .chars()
                    .filter_map(|ch| font.nominal_glyph_raw(ch as u32))
                    .any(|glyph| {
                        let one = [glyph.0];
                        lookup_indices.iter().any(|&index| {
                            u16::try_from(index).is_ok_and(|index| {
                                lookup_would_substitute(
                                    &accelerated.gsub,
                                    index,
                                    &mut WouldApply {
                                        glyphs: &one,
                                        zero_context: true,
                                        buffer: None,
                                    },
                                )
                            })
                        })
                    })
            })
        })
    {
        return;
    }
    let excluded = if feature.is_some() {
        collect_gpos_inputs(font, &script_tags, feature)
    } else {
        BTreeSet::new()
    };
    let mut glyphs = if feature.is_some() {
        BTreeSet::new()
    } else {
        styles
            .iter()
            .enumerate()
            .filter_map(|(glyph, &style)| {
                ((style & 0x1fff) == request.index)
                    .then(|| u32::try_from(glyph).ok())
                    .flatten()
            })
            .collect()
    };
    let mut previous = usize::MAX;
    for _ in 0..5 {
        if glyphs.len() == previous {
            break;
        }
        previous = glyphs.len();
        for &index in &lookup_indices {
            let Ok(Some(lookup)) = lookups.lookup(index) else {
                continue;
            };
            add_lookup_outputs(&lookup, &mut glyphs, feature.is_none(), styles.len());
            if feature.is_none() {
                for i in 0..lookup.num_subtables() {
                    if let Ok(subtable) = gsub::subtable(&lookup, i) {
                        add_context_outputs(subtable, lookups, &mut glyphs, styles.len());
                    }
                }
            } else {
                for i in 0..lookup.num_subtables() {
                    if let Ok(subtable) = gsub::subtable(&lookup, i) {
                        for nested in nested_lookups(subtable, &glyphs) {
                            if let Ok(Some(lookup)) = lookups.lookup(nested) {
                                add_lookup_outputs(&lookup, &mut glyphs, false, styles.len());
                            }
                        }
                    }
                }
            }
        }
    }
    for glyph in glyphs.difference(&excluded) {
        if let Some(style) = styles.get_mut(*glyph as usize)
            && *style == 0x1fff
        {
            *style = request.index;
        }
    }
}

fn collect_lookup_indices(
    scripts: ScriptList<'_>,
    features: FeatureList<'_>,
    script_tags: &[Tag],
    feature: Option<Tag>,
) -> BTreeSet<usize> {
    let mut lookup_indices = BTreeSet::new();
    for &tag in script_tags {
        let Some(index) = scripts.find(tag) else {
            continue;
        };
        let Ok(Some(script)) = scripts.script(index) else {
            continue;
        };
        let languages = core::iter::once(script.default_lang_sys().ok().flatten())
            .chain((0..script.num_lang_sys()).map(|i| script.lang_sys(i).ok().flatten()));
        for language in languages.flatten() {
            let indices = core::iter::once(language.required_feature_index())
                .chain((0..language.num_features()).filter_map(|i| language.feature_index(i)));
            for index in indices {
                if feature.is_some_and(|wanted| features.tag(usize::from(index)) != Some(wanted)) {
                    continue;
                }
                let Ok(Some(record)) = features.feature(usize::from(index)) else {
                    continue;
                };
                for i in 0..record.num_lookups() {
                    if let Some(index) = record.lookup_index(i) {
                        lookup_indices.insert(usize::from(index));
                    }
                }
            }
        }
    }
    lookup_indices
}

fn collect_gpos_inputs(
    font: &Font<'_>,
    script_tags: &[Tag],
    feature: Option<Tag>,
) -> BTreeSet<u32> {
    let Some(table) = font
        .table(Tag::new(b"GPOS"))
        .and_then(|data| LayoutTable::new(data).ok())
    else {
        return BTreeSet::new();
    };
    let Some((scripts, features, lookups)) = table
        .script_list()
        .ok()
        .flatten()
        .zip(table.feature_list().ok().flatten())
        .zip(table.lookup_list().ok().flatten())
        .map(|((a, b), c)| (a, b, c))
    else {
        return BTreeSet::new();
    };
    let mut inputs = BTreeSet::new();
    for index in collect_lookup_indices(scripts, features, script_tags, feature) {
        let Ok(Some(lookup)) = lookups.lookup(index) else {
            continue;
        };
        for i in 0..lookup.num_subtables() {
            let Ok(subtable) = gpos::subtable(&lookup, i) else {
                continue;
            };
            for (first, last) in crate::layout::pos_coverage(&subtable).ranges() {
                inputs.extend(first.0..=last.0);
            }
        }
    }
    inputs
}

fn add_context_outputs(
    subtable: SubstSubtable<'_>,
    lookups: LookupList<'_>,
    glyphs: &mut BTreeSet<u32>,
    limit: usize,
) {
    use sofel_font::read::layout::LookupRecords;

    fn records(
        lookups: LookupList<'_>,
        records: LookupRecords<'_>,
        positions: &[BTreeSet<u32>],
        glyphs: &mut BTreeSet<u32>,
        limit: usize,
    ) {
        if positions.iter().any(BTreeSet::is_empty) {
            return;
        }
        for record in records.iter() {
            let Some(active) = positions.get(usize::from(record.sequence_index)) else {
                continue;
            };
            let Ok(Some(lookup)) = lookups.lookup(usize::from(record.lookup_index)) else {
                continue;
            };
            let mut result = active.clone();
            add_lookup_outputs(&lookup, &mut result, true, limit);
            glyphs.extend(result);
        }
    }

    let available = glyphs.clone();
    let covered = |coverage: sofel_font::read::layout::Coverage<'_>| {
        available
            .iter()
            .copied()
            .filter(|&glyph| coverage.get(sofel_font::read::GlyphId(glyph)).is_some())
            .collect::<BTreeSet<_>>()
    };
    let classes = |class_def: sofel_font::read::layout::ClassDef<'_>, value: u32| {
        available
            .iter()
            .copied()
            .filter(|&glyph| class_def.get(sofel_font::read::GlyphId(glyph)) == value)
            .collect::<BTreeSet<_>>()
    };
    match subtable {
        SubstSubtable::Context(SequenceContext::Glyphs {
            coverage,
            rule_sets,
        }) => {
            for i in 0..rule_sets.len() {
                let Some(first) = coverage.glyph(i).map(|glyph| glyph.0) else {
                    continue;
                };
                if !available.contains(&first) {
                    continue;
                }
                let Ok(Some(set)) = rule_sets.get(i) else {
                    continue;
                };
                for j in 0..set.len() {
                    let Ok(Some(rule)) = set.get(j) else { continue };
                    let positions = core::iter::once(core::iter::once(first).collect())
                        .chain(
                            (0..rule.input.len()).map(|k| rule.input.get(k).into_iter().collect()),
                        )
                        .collect::<Vec<BTreeSet<u32>>>();
                    if positions
                        .iter()
                        .all(|set| set.iter().all(|glyph| available.contains(glyph)))
                    {
                        records(lookups, rule.lookups, &positions, glyphs, limit);
                    }
                }
            }
        }
        SubstSubtable::Context(SequenceContext::Classes {
            coverage,
            class_def,
            rule_sets,
        }) => {
            for i in 0..rule_sets.len() {
                let first = covered(coverage)
                    .intersection(&classes(class_def, u32::try_from(i).unwrap_or(u32::MAX)))
                    .copied()
                    .collect::<BTreeSet<_>>();
                if first.is_empty() {
                    continue;
                }
                let Ok(Some(set)) = rule_sets.get(i) else {
                    continue;
                };
                for j in 0..set.len() {
                    let Ok(Some(rule)) = set.get(j) else { continue };
                    let positions = core::iter::once(first.clone())
                        .chain((0..rule.input.len()).map(|k| {
                            rule.input
                                .get(k)
                                .map_or_else(BTreeSet::new, |value| classes(class_def, value))
                        }))
                        .collect::<Vec<_>>();
                    records(lookups, rule.lookups, &positions, glyphs, limit);
                }
            }
        }
        SubstSubtable::Context(SequenceContext::Coverages {
            coverages,
            lookups: records_list,
        }) => {
            let positions = (0..coverages.len())
                .map(|i| {
                    coverages
                        .get(i)
                        .ok()
                        .flatten()
                        .map_or_else(BTreeSet::new, covered)
                })
                .collect::<Vec<_>>();
            records(lookups, records_list, &positions, glyphs, limit);
        }
        SubstSubtable::ChainContext(ChainedSequenceContext::Glyphs {
            coverage,
            rule_sets,
        }) => {
            for i in 0..rule_sets.len() {
                let Some(first) = coverage.glyph(i).map(|glyph| glyph.0) else {
                    continue;
                };
                if !available.contains(&first) {
                    continue;
                }
                let Ok(Some(set)) = rule_sets.get(i) else {
                    continue;
                };
                for j in 0..set.len() {
                    let Ok(Some(rule)) = set.get(j) else { continue };
                    if (0..rule.backtrack.len()).all(|k| {
                        rule.backtrack
                            .get(k)
                            .is_some_and(|glyph| available.contains(&glyph))
                    }) && (0..rule.lookahead.len()).all(|k| {
                        rule.lookahead
                            .get(k)
                            .is_some_and(|glyph| available.contains(&glyph))
                    }) {
                        let positions = core::iter::once(core::iter::once(first).collect())
                            .chain(
                                (0..rule.input.len())
                                    .map(|k| rule.input.get(k).into_iter().collect()),
                            )
                            .collect::<Vec<BTreeSet<u32>>>();
                        if positions
                            .iter()
                            .all(|set| set.iter().all(|glyph| available.contains(glyph)))
                        {
                            records(lookups, rule.lookups, &positions, glyphs, limit);
                        }
                    }
                }
            }
        }
        SubstSubtable::ChainContext(ChainedSequenceContext::Classes {
            coverage,
            backtrack_class_def,
            input_class_def,
            lookahead_class_def,
            rule_sets,
        }) => {
            for i in 0..rule_sets.len() {
                let first = covered(coverage)
                    .intersection(&classes(
                        input_class_def,
                        u32::try_from(i).unwrap_or(u32::MAX),
                    ))
                    .copied()
                    .collect::<BTreeSet<_>>();
                if first.is_empty() {
                    continue;
                }
                let Ok(Some(set)) = rule_sets.get(i) else {
                    continue;
                };
                for j in 0..set.len() {
                    let Ok(Some(rule)) = set.get(j) else { continue };
                    if (0..rule.backtrack.len()).all(|k| {
                        rule.backtrack
                            .get(k)
                            .is_some_and(|value| !classes(backtrack_class_def, value).is_empty())
                    }) && (0..rule.lookahead.len()).all(|k| {
                        rule.lookahead
                            .get(k)
                            .is_some_and(|value| !classes(lookahead_class_def, value).is_empty())
                    }) {
                        let positions = core::iter::once(first.clone())
                            .chain((0..rule.input.len()).map(|k| {
                                rule.input.get(k).map_or_else(BTreeSet::new, |value| {
                                    classes(input_class_def, value)
                                })
                            }))
                            .collect::<Vec<_>>();
                        records(lookups, rule.lookups, &positions, glyphs, limit);
                    }
                }
            }
        }
        SubstSubtable::ChainContext(ChainedSequenceContext::Coverages {
            backtrack,
            input,
            lookahead,
            lookups: records_list,
        }) if (0..backtrack.len()).all(|i| {
            backtrack
                .get(i)
                .ok()
                .flatten()
                .is_some_and(|coverage| !covered(coverage).is_empty())
        }) && (0..lookahead.len()).all(|i| {
            lookahead
                .get(i)
                .ok()
                .flatten()
                .is_some_and(|coverage| !covered(coverage).is_empty())
        }) =>
        {
            let positions = (0..input.len())
                .map(|i| {
                    input
                        .get(i)
                        .ok()
                        .flatten()
                        .map_or_else(BTreeSet::new, covered)
                })
                .collect::<Vec<_>>();
            records(lookups, records_list, &positions, glyphs, limit);
        }
        _ => {}
    }
}

fn nested_lookups(subtable: SubstSubtable<'_>, glyphs: &BTreeSet<u32>) -> BTreeSet<usize> {
    let mut nested = BTreeSet::new();
    let mut add = |records: sofel_font::read::layout::LookupRecords<'_>| {
        nested.extend(
            records
                .iter()
                .map(|record| usize::from(record.lookup_index)),
        );
    };
    match subtable {
        SubstSubtable::Context(SequenceContext::Glyphs {
            coverage,
            rule_sets,
        }) => {
            for i in 0..rule_sets.len() {
                if !coverage.glyph(i).is_some_and(|g| glyphs.contains(&g.0)) {
                    continue;
                }
                if let Ok(Some(set)) = rule_sets.get(i) {
                    for j in 0..set.len() {
                        if let Ok(Some(rule)) = set.get(j) {
                            add(rule.lookups);
                        }
                    }
                }
            }
        }
        SubstSubtable::Context(SequenceContext::Classes {
            coverage,
            class_def,
            rule_sets,
        }) => {
            for i in 0..rule_sets.len() {
                if !glyphs.iter().any(|&g| {
                    coverage.get(sofel_font::read::GlyphId(g)).is_some()
                        && class_def.get(sofel_font::read::GlyphId(g))
                            == u32::try_from(i).unwrap_or(u32::MAX)
                }) {
                    continue;
                }
                if let Ok(Some(set)) = rule_sets.get(i) {
                    for j in 0..set.len() {
                        if let Ok(Some(rule)) = set.get(j) {
                            add(rule.lookups);
                        }
                    }
                }
            }
        }
        SubstSubtable::Context(SequenceContext::Coverages { coverages, lookups }) => {
            if coverages.get(0).ok().flatten().is_some_and(|coverage| {
                glyphs
                    .iter()
                    .any(|&g| coverage.get(sofel_font::read::GlyphId(g)).is_some())
            }) {
                add(lookups);
            }
        }
        SubstSubtable::ChainContext(ChainedSequenceContext::Coverages {
            input, lookups, ..
        }) => {
            if input.get(0).ok().flatten().is_some_and(|coverage| {
                glyphs
                    .iter()
                    .any(|&g| coverage.get(sofel_font::read::GlyphId(g)).is_some())
            }) {
                add(lookups);
            }
        }
        SubstSubtable::ChainContext(ChainedSequenceContext::Glyphs {
            coverage,
            rule_sets,
        }) => {
            for i in 0..rule_sets.len() {
                if !coverage.glyph(i).is_some_and(|g| glyphs.contains(&g.0)) {
                    continue;
                }
                if let Ok(Some(set)) = rule_sets.get(i) {
                    for j in 0..set.len() {
                        if let Ok(Some(rule)) = set.get(j) {
                            add(rule.lookups);
                        }
                    }
                }
            }
        }
        SubstSubtable::ChainContext(ChainedSequenceContext::Classes {
            coverage,
            input_class_def,
            rule_sets,
            ..
        }) => {
            for i in 0..rule_sets.len() {
                if !glyphs.iter().any(|&g| {
                    coverage.get(sofel_font::read::GlyphId(g)).is_some()
                        && input_class_def.get(sofel_font::read::GlyphId(g))
                            == u32::try_from(i).unwrap_or(u32::MAX)
                }) {
                    continue;
                }
                if let Ok(Some(set)) = rule_sets.get(i) {
                    for j in 0..set.len() {
                        if let Ok(Some(rule)) = set.get(j) {
                            add(rule.lookups);
                        }
                    }
                }
            }
        }
        _ => {}
    }
    nested
}

fn add_lookup_outputs(
    lookup: &sofel_font::read::layout::Lookup<'_>,
    glyphs: &mut BTreeSet<u32>,
    only_reachable: bool,
    limit: usize,
) {
    for i in 0..lookup.num_subtables() {
        let Ok(subtable) = gsub::subtable(lookup, i) else {
            continue;
        };
        match subtable {
            SubstSubtable::SingleDelta {
                coverage,
                delta,
                mask,
            } => {
                for (first, last) in coverage.ranges() {
                    for glyph in first.0..=last.0.min(u32::try_from(limit).unwrap_or(u32::MAX)) {
                        if !only_reachable || glyphs.contains(&glyph) {
                            glyphs.insert(glyph.wrapping_add(delta) & mask);
                        }
                    }
                }
            }
            SubstSubtable::SingleList {
                coverage,
                substitutes,
            } => {
                for (first, last) in coverage.ranges() {
                    for glyph in first.0..=last.0.min(u32::try_from(limit).unwrap_or(u32::MAX)) {
                        if (!only_reachable || glyphs.contains(&glyph))
                            && let Some(index) = coverage.get(sofel_font::read::GlyphId(glyph))
                            && let Some(replacement) = substitutes.get(index as usize)
                        {
                            glyphs.insert(replacement);
                        }
                    }
                }
            }
            SubstSubtable::Multiple {
                coverage,
                sequences,
            }
            | SubstSubtable::Alternate {
                coverage,
                alternate_sets: sequences,
            } => {
                for (first, last) in coverage.ranges() {
                    for glyph in first.0..=last.0.min(u32::try_from(limit).unwrap_or(u32::MAX)) {
                        if (!only_reachable || glyphs.contains(&glyph))
                            && let Some(index) = coverage.get(sofel_font::read::GlyphId(glyph))
                            && let Ok(Some(sequence)) = sequences.get(index as usize)
                        {
                            for i in 0..sequence.len() {
                                if let Some(replacement) = sequence.get(i) {
                                    glyphs.insert(replacement);
                                }
                            }
                        }
                    }
                }
            }
            SubstSubtable::Ligature {
                coverage,
                ligature_sets,
            } => {
                for (first, last) in coverage.ranges() {
                    for glyph in first.0..=last.0.min(u32::try_from(limit).unwrap_or(u32::MAX)) {
                        if (!only_reachable || glyphs.contains(&glyph))
                            && let Some(index) = coverage.get(sofel_font::read::GlyphId(glyph))
                            && let Ok(Some(set)) = ligature_sets.get(index as usize)
                        {
                            for i in 0..set.len() {
                                if let Ok(Some(ligature)) = set.get(i)
                                    && (!only_reachable
                                        || (0..ligature.components.len()).all(|j| {
                                            ligature
                                                .components
                                                .get(j)
                                                .is_some_and(|g| glyphs.contains(&g))
                                        }))
                                {
                                    glyphs.insert(ligature.glyph.0);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
