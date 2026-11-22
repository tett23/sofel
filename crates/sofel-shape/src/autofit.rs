//! GSUB style coverage for FreeType's HarfBuzz-backed automatic hinter.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_font::read::Tag;
use sofel_font::read::feature_variations::FeatureVariations;
use sofel_font::read::gpos;
use sofel_font::read::gsub::{self, SubstSubtable};
use sofel_font::read::layout::{
    ChainedSequenceContext, FeatureList, LayoutTable, LookupList, ScriptList, SequenceContext,
};
use sofel_font::{GlyphExtents, var::Instance};

use crate::Script;
use crate::layout::Layout;
use crate::layout::would::{WouldApply, lookup_would_substitute};
use crate::shape::shape_with_extents;
use crate::{Feature, Scale, SegmentProperties, ShapePlan, UnicodeBuffer};

#[derive(Clone, Copy)]
struct LookupContext<'a> {
    font: &'a Font<'a>,
    accelerated: &'a Layout<'a>,
    scripts: ScriptList<'a>,
    features: FeatureList<'a>,
    lookups: LookupList<'a>,
    variations: Option<FeatureVariations<'a>>,
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
    /// UTF-8 standard-character candidates for stem width measurement.
    pub standard: &'a [u8],
}

/// One glyph of a blue-zone or standard-character cluster at font-unit scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoHintShapedGlyph {
    /// Result glyph ID.
    pub glyph: u32,
    /// Horizontal advance in font units.
    pub x_advance: i32,
    /// Vertical offset in font units.
    pub y_offset: i32,
}

/// A shaped cluster and the auto-hinter style that consumes it.
#[derive(Clone, Debug)]
pub struct AutoHintShapedCluster {
    /// Style index in FreeType's style table.
    pub style: u16,
    /// UTF-8 source text without spaces.
    pub text: Vec<u8>,
    /// Shaping result, empty when a feature has no glyph substitution.
    pub glyphs: Vec<AutoHintShapedGlyph>,
}

/// Computes the HarfBuzz-backed automatic hinter's style map and clusters.
pub fn auto_hint_shaping_data(
    instance: Instance<'_, '_, '_, '_>,
    extents: &dyn Fn(GlyphId) -> Option<GlyphExtents>,
    requests: &[AutoHintStyleRequest<'_>],
    fallback_style: u16,
    default_style: u16,
) -> (Vec<u16>, Vec<AutoHintShapedCluster>) {
    let glyph_styles =
        auto_hint_gsub_styles(instance.font(), requests, fallback_style, default_style);
    let mut clusters = Vec::new();
    for request in requests {
        if !glyph_styles
            .iter()
            .any(|&word| word & 0x1fff == request.index)
        {
            continue;
        }
        let mut seen = BTreeSet::new();
        for string in core::iter::once(request.standard)
            .chain(request.blue_strings.iter().copied())
            .chain(core::iter::once(b"0 1 2 3 4 5 6 7 8 9".as_slice()))
        {
            for text in string
                .split(|&byte| byte == b' ')
                .filter(|text| !text.is_empty() && *text != b"|")
            {
                if seen.insert(text.to_vec()) {
                    let glyphs = auto_hint_shape_cluster(instance, extents, text, request.feature);
                    clusters.push(AutoHintShapedCluster {
                        style: request.index,
                        text: text.to_vec(),
                        glyphs,
                    });
                }
            }
        }
    }
    (glyph_styles, clusters)
}

/// Builds per-style GSUB inverse entries used for accent adjustment.
/// Direct cmap entries are left to the outline cache and take precedence.
pub fn auto_hint_reverse_entries(
    font: &Font<'_>,
    requests: &[AutoHintStyleRequest<'_>],
    adjustment_codepoints: &[u32],
) -> Vec<(u16, u32, u32)> {
    let Some(table) = font
        .table(Tag::new(b"GSUB"))
        .and_then(|data| LayoutTable::new(data).ok())
    else {
        return Vec::new();
    };
    let Some((scripts, features, lookups)) = table
        .script_list()
        .ok()
        .flatten()
        .zip(table.feature_list().ok().flatten())
        .zip(table.lookup_list().ok().flatten())
        .map(|((a, b), c)| (a, b, c))
    else {
        return Vec::new();
    };
    let mut direct = BTreeMap::new();
    for &codepoint in adjustment_codepoints {
        if let Some(glyph) = font.nominal_glyph_raw(codepoint)
            && glyph.0 != 0
        {
            direct.insert(glyph.0, codepoint);
        }
    }
    let mut result = Vec::new();
    for request in requests {
        let Some(script) = Script::from_iso15924(&request.script) else {
            continue;
        };
        let (tags, _) = ot_tags_from_script_and_language_max(Some(script), None, 1, 0);
        let indices = collect_lookup_indices(
            scripts,
            features,
            table.feature_variations().ok().flatten(),
            &tags,
            None,
        );
        let mut replacements: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        let mut insertion_order = Vec::new();
        for index in indices {
            let Ok(Some(lookup)) = lookups.lookup(index) else {
                continue;
            };
            let mut valid = lookup.num_subtables() > 0;
            for i in 0..lookup.num_subtables() {
                valid &= matches!(
                    gsub::subtable(&lookup, i),
                    Ok(SubstSubtable::SingleDelta { .. }
                        | SubstSubtable::SingleList { .. }
                        | SubstSubtable::Alternate { .. })
                );
            }
            if !valid {
                continue;
            }
            for i in 0..lookup.num_subtables() {
                let Ok(subtable) = gsub::subtable(&lookup, i) else {
                    continue;
                };
                add_reverse_substitutions(subtable, &mut replacements, &mut insertion_order);
            }
        }
        let mut mapped = direct.clone();
        for glyph in ft_hash_iteration(&insertion_order) {
            if glyph & 0xffff_0000 != 0 {
                continue;
            }
            if let Some(&codepoint) = mapped.get(&glyph) {
                add_reverse_descendants(glyph, codepoint, &replacements, &mut mapped, 0);
            }
        }
        result.extend(
            mapped
                .into_iter()
                .filter(|(glyph, _)| !direct.contains_key(glyph))
                .map(|(glyph, codepoint)| (request.index, glyph, codepoint)),
        );
    }
    result
}

fn add_reverse_descendants(
    glyph: u32,
    codepoint: u32,
    replacements: &BTreeMap<u32, Vec<u32>>,
    mapped: &mut BTreeMap<u32, u32>,
    depth: usize,
) {
    if depth >= 64 {
        return;
    }
    if let Some(next) = replacements.get(&glyph) {
        for &replacement in next {
            if let alloc::collections::btree_map::Entry::Vacant(entry) = mapped.entry(replacement) {
                entry.insert(codepoint);
                add_reverse_descendants(
                    replacement,
                    codepoint,
                    replacements,
                    mapped,
                    depth.saturating_add(1),
                );
            }
        }
    }
}

fn add_reverse_substitutions(
    subtable: SubstSubtable<'_>,
    replacements: &mut BTreeMap<u32, Vec<u32>>,
    insertion_order: &mut Vec<u32>,
) {
    let mut add = |glyph: u32, replacement: u32| {
        if let alloc::collections::btree_map::Entry::Vacant(entry) = replacements.entry(glyph) {
            insertion_order.push(glyph);
            entry.insert(Vec::new());
        } else if let Some(list) = replacements.get(&glyph) {
            insertion_order.push(glyph | (u32::try_from(list.len()).unwrap_or(u32::MAX) << 16));
        }
        if let Some(list) = replacements.get_mut(&glyph) {
            list.push(replacement);
        }
    };
    match subtable {
        SubstSubtable::SingleDelta {
            coverage,
            delta,
            mask,
        } => {
            for glyph in ft_coverage_glyphs(coverage) {
                add(glyph, glyph.wrapping_add(delta) & mask);
            }
        }
        SubstSubtable::SingleList {
            coverage,
            substitutes,
        } => {
            for (index, glyph) in ft_coverage_glyphs(coverage).into_iter().enumerate() {
                if let Some(replacement) = substitutes.get(index) {
                    add(glyph, replacement);
                }
            }
        }
        SubstSubtable::Alternate {
            coverage,
            alternate_sets,
        } => {
            for (index, glyph) in ft_coverage_glyphs(coverage).into_iter().enumerate() {
                if let Ok(Some(set)) = alternate_sets.get(index) {
                    for i in 0..set.len() {
                        if let Some(replacement) = set.get(i) {
                            add(glyph, replacement);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// The coverage iterator in FreeType's `afgsub.c`, including its final-range
/// termination behavior for format 2.
fn ft_coverage_glyphs(coverage: sofel_font::read::layout::Coverage<'_>) -> Vec<u32> {
    match coverage.format() {
        1 => (0..coverage.num_records())
            .filter_map(|i| coverage.glyph(i).map(|g| g.0))
            .collect(),
        2 => (0..coverage.num_records())
            .flat_map(|i| {
                coverage
                    .range(i)
                    .into_iter()
                    .flat_map(move |(first, last, _)| {
                        let end = if i.saturating_add(1) == coverage.num_records() {
                            first.0
                        } else {
                            last.0
                        };
                        first.0..=end
                    })
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Numeric hash-table iteration order from FreeType's `fthash.c`.
// The table is always nonempty, and the index is reduced modulo its size.
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]
fn ft_hash_iteration(keys: &[u32]) -> Vec<u32> {
    fn hash(key: u32) -> u64 {
        let mut result = u64::from(key & 0xff);
        for shift in [8, 16, 24] {
            result = result
                .wrapping_mul(31)
                .wrapping_add(u64::from((key >> shift) & 0xff));
        }
        result
    }
    fn insert(table: &mut [Option<u32>], key: u32) -> bool {
        let mut index = (hash(key) % table.len() as u64) as usize;
        loop {
            match table[index] {
                Some(old) if old == key => return false,
                Some(_) => {
                    index = if index == 0 {
                        table.len() - 1
                    } else {
                        index - 1
                    }
                }
                None => {
                    table[index] = Some(key);
                    return true;
                }
            }
        }
    }
    let mut table = vec![None; 241];
    let mut used = 0;
    for &key in keys {
        if insert(&mut table, key) {
            if used >= table.len() / 3 {
                let old = table;
                table = vec![None; old.len() * 2];
                for key in old.into_iter().flatten() {
                    insert(&mut table, key);
                }
            }
            used += 1;
        }
    }
    table.into_iter().flatten().collect()
}

/// Shapes one auto-hinter cluster as FreeType's HarfBuzz path does.
pub fn auto_hint_shape_cluster(
    instance: Instance<'_, '_, '_, '_>,
    extents: &dyn Fn(GlyphId) -> Option<GlyphExtents>,
    text: &[u8],
    feature: Option<[u8; 4]>,
) -> Vec<AutoHintShapedGlyph> {
    let mut buffer = UnicodeBuffer::new();
    buffer.push_utf8(text, 0, None);
    let properties = SegmentProperties::default().guess(&buffer, None);
    let features = feature.map(|tag| Feature::new(Tag(tag), 1));
    let plan = ShapePlan::new(instance, properties.clone(), features.as_slice());
    let scale = Scale::unscaled(instance.font());
    let shaped = shape_with_extents(instance, &scale, &plan, buffer, extents);
    if feature.is_some() {
        let mut baseline = UnicodeBuffer::new();
        baseline.push_utf8(text, 0, None);
        let baseline_plan = ShapePlan::new(instance, properties, &[]);
        let unfeatured = shape_with_extents(instance, &scale, &baseline_plan, baseline, extents);
        if shaped
            .glyph_infos()
            .iter()
            .map(|g| g.glyph)
            .eq(unfeatured.glyph_infos().iter().map(|g| g.glyph))
        {
            return Vec::new();
        }
    }
    shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions())
        .map(|(info, pos)| AutoHintShapedGlyph {
            glyph: info.glyph,
            x_advance: pos.x_advance,
            y_offset: pos.y_offset,
        })
        .collect()
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
            gsub.feature_variations().ok().flatten(),
        ))
    });
    let accelerated = Layout::new(font);

    for request in requests {
        if request.feature.is_none() {
            assign_cmap_style(font, &mut styles, request);
        } else if let Some((scripts, features, lookups, variations)) = tables {
            let context = LookupContext {
                font,
                accelerated: &accelerated,
                scripts,
                features,
                lookups,
                variations,
            };
            assign_gsub_style(&context, &mut styles, request, false);
        }
    }
    if let Some((scripts, features, lookups, variations)) = tables {
        for request in requests.iter().filter(|request| request.feature.is_none()) {
            let context = LookupContext {
                font,
                accelerated: &accelerated,
                scripts,
                features,
                lookups,
                variations,
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
                variations,
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
        variations,
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
    let lookup_indices =
        collect_lookup_indices(scripts, features, variations, &script_tags, feature);
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
    for _ in 0..14 {
        if glyphs.len() == previous {
            break;
        }
        previous = glyphs.len();
        for &index in &lookup_indices {
            let Ok(Some(lookup)) = lookups.lookup(index) else {
                continue;
            };
            add_lookup_outputs(&lookup, &mut glyphs, feature.is_none(), None, styles.len());
            if feature.is_none() {
                for i in 0..lookup.num_subtables() {
                    if let Ok(subtable) = gsub::subtable(&lookup, i) {
                        add_context_outputs(subtable, lookups, &mut glyphs, styles.len(), 0);
                    }
                }
            } else {
                for i in 0..lookup.num_subtables() {
                    if let Ok(subtable) = gsub::subtable(&lookup, i) {
                        for nested in nested_lookups(subtable, &glyphs) {
                            if let Ok(Some(lookup)) = lookups.lookup(nested) {
                                add_lookup_outputs(&lookup, &mut glyphs, false, None, styles.len());
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
    variations: Option<FeatureVariations<'_>>,
    script_tags: &[Tag],
    feature: Option<Tag>,
) -> BTreeSet<usize> {
    let mut lookup_indices = BTreeSet::new();
    let mut selected_features = BTreeSet::new();
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
                selected_features.insert(index);
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
    if let Some(variations) = variations {
        for variation_index in 0..variations.len() {
            let Some(variation) = variations.record(variation_index) else {
                continue;
            };
            let Ok(Some(substitutions)) = variation.substitutions() else {
                continue;
            };
            for record_index in 0..substitutions.len() {
                let Some(record) = substitutions.record(record_index) else {
                    continue;
                };
                if !selected_features.contains(&record.feature_index()) {
                    continue;
                }
                let Ok(Some(data)) = record.feature() else {
                    continue;
                };
                let Some(count) = data
                    .get(2..4)
                    .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
                    .map(u16::from_be_bytes)
                else {
                    continue;
                };
                for index in 0..usize::from(count) {
                    let start = 4usize.saturating_add(index.saturating_mul(2));
                    if let Some(index) = data
                        .get(start..start.saturating_add(2))
                        .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
                        .map(u16::from_be_bytes)
                    {
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
    for index in collect_lookup_indices(
        scripts,
        features,
        table.feature_variations().ok().flatten(),
        script_tags,
        feature,
    ) {
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

fn coverage_glyph_at_index(
    coverage: sofel_font::read::layout::Coverage<'_>,
    index: usize,
) -> Option<u32> {
    if let Some(glyph) = coverage.glyph(index) {
        return Some(glyph.0);
    }
    let index = u32::try_from(index).ok()?;
    (0..coverage.num_records()).find_map(|record| {
        let (first, last, start) = coverage.range(record)?;
        let offset = index.checked_sub(start)?;
        let glyph = first.0.checked_add(offset)?;
        (glyph <= last.0).then_some(glyph)
    })
}

fn add_context_outputs(
    subtable: SubstSubtable<'_>,
    lookups: LookupList<'_>,
    glyphs: &mut BTreeSet<u32>,
    limit: usize,
    depth: u8,
) {
    use sofel_font::read::layout::LookupRecords;

    fn records(
        lookups: LookupList<'_>,
        records: LookupRecords<'_>,
        positions: &[BTreeSet<u32>],
        glyphs: &mut BTreeSet<u32>,
        limit: usize,
        depth: u8,
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
            let mut result = glyphs.clone();
            add_lookup_outputs(&lookup, &mut result, true, Some(active), limit);
            if depth < 8 {
                for index in 0..lookup.num_subtables() {
                    if let Ok(subtable) = gsub::subtable(&lookup, index) {
                        add_context_outputs(
                            subtable,
                            lookups,
                            &mut result,
                            limit,
                            depth.saturating_add(1),
                        );
                    }
                }
            }
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
                let Some(first) = coverage_glyph_at_index(coverage, i) else {
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
                        records(lookups, rule.lookups, &positions, glyphs, limit, depth);
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
                    records(lookups, rule.lookups, &positions, glyphs, limit, depth);
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
            records(lookups, records_list, &positions, glyphs, limit, depth);
        }
        SubstSubtable::ChainContext(ChainedSequenceContext::Glyphs {
            coverage,
            rule_sets,
        }) => {
            for i in 0..rule_sets.len() {
                let Some(first) = coverage_glyph_at_index(coverage, i) else {
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
                            records(lookups, rule.lookups, &positions, glyphs, limit, depth);
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
                        records(lookups, rule.lookups, &positions, glyphs, limit, depth);
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
            records(lookups, records_list, &positions, glyphs, limit, depth);
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
                if !coverage_glyph_at_index(coverage, i).is_some_and(|g| glyphs.contains(&g)) {
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
                if !coverage_glyph_at_index(coverage, i).is_some_and(|g| glyphs.contains(&g)) {
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
    allowed_first: Option<&BTreeSet<u32>>,
    limit: usize,
) {
    let reachable = |glyph: u32, glyphs: &BTreeSet<u32>| {
        !only_reachable
            || allowed_first.map_or_else(|| glyphs.contains(&glyph), |s| s.contains(&glyph))
    };
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
                        if reachable(glyph, glyphs) {
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
                        if reachable(glyph, glyphs)
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
                        if reachable(glyph, glyphs)
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
                        if reachable(glyph, glyphs)
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
            SubstSubtable::ReverseChainSingle {
                coverage,
                backtrack,
                lookahead,
                substitutes,
            } => {
                let context_matches = [backtrack, lookahead].into_iter().all(|coverages| {
                    (0..coverages.len()).all(|index| {
                        coverages.get(index).ok().flatten().is_some_and(|coverage| {
                            glyphs.iter().any(|&glyph| {
                                coverage.get(sofel_font::read::GlyphId(glyph)).is_some()
                            })
                        })
                    })
                });
                if context_matches {
                    let inputs = glyphs.iter().copied().collect::<Vec<_>>();
                    for glyph in inputs {
                        if reachable(glyph, glyphs)
                            && let Some(index) = coverage.get(sofel_font::read::GlyphId(glyph))
                            && let Some(replacement) = substitutes.get(index as usize)
                        {
                            glyphs.insert(replacement);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
