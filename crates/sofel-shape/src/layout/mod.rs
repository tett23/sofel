//! The OpenType layout tables as shaping reads them (ADR 0020, データ構造):
//! `GDEF` glyph properties and mark glyph sets, the `GSUB` and `GPOS`
//! script, language system and feature lists, and an accelerator per
//! lookup with its subtables resolved through extensions and a digest
//! of the glyphs each can start at.
//!
//! Where a table, offset or subtable cannot be read, the behaviour is
//! that of HarfBuzz's `Null` objects for neutered offsets: an empty
//! table, no lookups, no subtable.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-layout.cc`,
//! `src/hb-ot-layout-common.hh`, `src/hb-ot-layout-gsubgpos.hh`
//! (`hb_ot_layout_lookup_accelerator_t`) and `src/OT/Layout/GDEF/GDEF.hh`,
//! MIT; see THIRD_PARTY_NOTICES.md.

pub(crate) mod apply;
pub(crate) mod gpos;
pub(crate) mod gsub;
pub(crate) mod map;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::gdef::{self, Gdef as GdefView, MarkGlyphSets};
use sofel_font::read::gpos::{self as gpos_view, PosSubtable};
use sofel_font::read::gsub::{self as gsub_view, SubstSubtable};
use sofel_font::read::layout::{
    ChainedSequenceContext, ClassDef, Coverage, FeatureList, LangSys, LayoutTable, LookupList,
    ScriptList, SequenceContext, lookup_flag,
};
use sofel_font::read::{GlyphId, Tag};

use crate::buffer::gprops;
use crate::digest::Digest;

/// `GSUB`.
pub(crate) const GSUB: usize = 0;
/// `GPOS`.
pub(crate) const GPOS: usize = 1;

/// `HB_OT_TAG_DEFAULT_SCRIPT`.
const DFLT: Tag = Tag(*b"DFLT");
/// `HB_OT_TAG_DEFAULT_LANGUAGE`.
const DFLT_LANG: Tag = Tag(*b"dflt");
/// `HB_OT_TAG_LATIN_SCRIPT`.
const LATN: Tag = Tag(*b"latn");

/// The `GDEF` data shaping uses (`GDEF::accelerator_t`).
#[derive(Clone, Debug)]
pub(crate) struct Gdef<'a> {
    glyph_classes: Option<ClassDef<'a>>,
    mark_attach: Option<ClassDef<'a>>,
    mark_sets: Option<MarkGlyphSets<'a>>,
    mark_set_count: usize,
}

impl<'a> Gdef<'a> {
    fn empty() -> Gdef<'a> {
        Gdef {
            glyph_classes: None,
            mark_attach: None,
            mark_sets: None,
            mark_set_count: 0,
        }
    }

    fn new(font: &Font<'a>) -> Gdef<'a> {
        let len = |t: &[u8; 4]| font.table(Tag(*t)).map_or(0, <[u8]>::len);
        let Some(data) = font.table(gdef::TAG) else {
            return Gdef::empty();
        };
        if gdef::is_blocklisted(data.len(), len(b"GSUB"), len(b"GPOS")) {
            return Gdef::empty();
        }
        let Ok(g) = GdefView::new(data) else {
            return Gdef::empty();
        };
        let mark_sets = g.mark_glyph_sets().ok().flatten();
        Gdef {
            glyph_classes: g.glyph_class_def().ok().flatten(),
            mark_attach: g.mark_attach_class_def().ok().flatten(),
            mark_set_count: mark_sets.map_or(0, |m| m.len()),
            mark_sets,
        }
    }

    /// `hb_ot_layout_has_glyph_classes`.
    pub(crate) fn has_glyph_classes(&self) -> bool {
        self.glyph_classes.is_some()
    }

    /// `GDEF::get_glyph_props`: the class bits, and for marks the mark
    /// attachment class in the high byte.
    pub(crate) fn glyph_props(&self, glyph: u32) -> u16 {
        let class = self.glyph_classes.map_or(0, |c| c.get(GlyphId(glyph)));
        match class {
            1 => gprops::BASE_GLYPH,
            2 => gprops::LIGATURE,
            3 => {
                let attach = self.mark_attach.map_or(0, |c| c.get(GlyphId(glyph)));
                let low = u16::try_from(attach & 0xFF).unwrap_or(0);
                gprops::MARK | (low << 8)
            }
            _ => 0,
        }
    }

    /// `mark_set_covers`.
    pub(crate) fn mark_set_covers(&self, set: u32, glyph: u32) -> bool {
        let Ok(set) = u16::try_from(set) else {
            return false;
        };
        self.mark_sets
            .is_some_and(|m| m.covers(set, GlyphId(glyph)))
    }

    /// `sanitize_lookup_props`: drops a mark filtering set that does not
    /// exist.
    pub(crate) fn sanitize_lookup_props(&self, props: u32) -> u32 {
        let flag = u32::from(lookup_flag::USE_MARK_FILTERING_SET);
        if props & flag != 0
            && usize::try_from(props >> 16).unwrap_or(usize::MAX) >= self.mark_set_count
        {
            props & !flag
        } else {
            props
        }
    }
}

/// A subtable with the digest of the glyphs it may start at.
#[derive(Clone, Debug)]
pub(crate) struct Sub<S> {
    pub(crate) table: S,
    pub(crate) digest: Digest,
}

/// A lookup's accelerator (`hb_ot_layout_lookup_accelerator_t`).
#[derive(Clone, Debug)]
pub(crate) struct LookupAccel<S> {
    /// `get_props`: the lookup flag and the mark filtering set in the
    /// high 16 bits.
    pub(crate) props: u32,
    /// Whether it is a reverse chaining single substitution lookup.
    pub(crate) reverse: bool,
    pub(crate) subtables: Vec<Sub<S>>,
    pub(crate) digest: Digest,
}

/// Adds the glyphs of a coverage to a digest (`collect_coverage`).
pub(crate) fn add_coverage(d: &mut Digest, c: &Coverage<'_>) {
    for (a, b) in c.ranges() {
        d.add_range(a.0, b.0);
    }
}

fn context_coverage<'a>(c: &SequenceContext<'a>) -> Coverage<'a> {
    match c {
        SequenceContext::Glyphs { coverage, .. } | SequenceContext::Classes { coverage, .. } => {
            *coverage
        }
        SequenceContext::Coverages { coverages, .. } => {
            coverages.get(0).ok().flatten().unwrap_or(Coverage::empty())
        }
    }
}

fn chain_coverage<'a>(c: &ChainedSequenceContext<'a>) -> Coverage<'a> {
    match c {
        ChainedSequenceContext::Glyphs { coverage, .. }
        | ChainedSequenceContext::Classes { coverage, .. } => *coverage,
        ChainedSequenceContext::Coverages { input, .. } => {
            input.get(0).ok().flatten().unwrap_or(Coverage::empty())
        }
    }
}

/// The coverage of a `GSUB` subtable's first glyph (`get_coverage`).
fn subst_coverage<'a>(s: &SubstSubtable<'a>) -> Coverage<'a> {
    match s {
        SubstSubtable::SingleDelta { coverage, .. }
        | SubstSubtable::SingleList { coverage, .. }
        | SubstSubtable::Multiple { coverage, .. }
        | SubstSubtable::Alternate { coverage, .. }
        | SubstSubtable::Ligature { coverage, .. }
        | SubstSubtable::ReverseChainSingle { coverage, .. } => *coverage,
        SubstSubtable::Context(c) => context_coverage(c),
        SubstSubtable::ChainContext(c) => chain_coverage(c),
    }
}

/// The coverage of a `GPOS` subtable's first glyph (`get_coverage`).
fn pos_coverage<'a>(s: &PosSubtable<'a>) -> Coverage<'a> {
    match s {
        PosSubtable::Single { coverage, .. }
        | PosSubtable::SingleArray { coverage, .. }
        | PosSubtable::PairGlyphs { coverage, .. }
        | PosSubtable::PairClasses { coverage, .. }
        | PosSubtable::Cursive { coverage, .. } => *coverage,
        PosSubtable::MarkBase { mark_coverage, .. }
        | PosSubtable::MarkLig { mark_coverage, .. } => *mark_coverage,
        PosSubtable::MarkMark { mark1_coverage, .. } => *mark1_coverage,
        PosSubtable::Context(c) => context_coverage(c),
        PosSubtable::ChainContext(c) => chain_coverage(c),
    }
}

/// A `GSUB` or `GPOS` table: its lists for planning and its lookups'
/// accelerators.
#[derive(Clone, Debug)]
pub(crate) struct Table<'a, S> {
    /// `has_data`: the table is there.
    pub(crate) has_data: bool,
    scripts: Option<ScriptList<'a>>,
    features: Option<FeatureList<'a>>,
    lookup_count: usize,
    pub(crate) lookups: Vec<LookupAccel<S>>,
}

impl<'a, S> Table<'a, S> {
    fn empty() -> Table<'a, S> {
        Table {
            has_data: false,
            scripts: None,
            features: None,
            lookup_count: 0,
            lookups: Vec::new(),
        }
    }

    fn new(
        data: Option<&'a [u8]>,
        extension: u16,
        subtable: impl Fn(&sofel_font::read::layout::Lookup<'a>, usize) -> Option<S>,
        coverage: impl Fn(&S) -> Coverage<'a>,
        reverse_type: Option<u16>,
    ) -> Table<'a, S> {
        let Some(t) = data.and_then(|d| LayoutTable::new(d).ok()) else {
            return Table::empty();
        };
        let list: Option<LookupList<'a>> = t.lookup_list().ok().flatten();
        let lookup_count = list.map_or(0, |l| l.len());
        let lookups = (0..lookup_count)
            .map(|i| {
                let lookup = list.and_then(|l| l.lookup(i).ok().flatten());
                let Some(lookup) = lookup.filter(|l| l.extension_types_agree(extension)) else {
                    return LookupAccel {
                        props: 0,
                        reverse: false,
                        subtables: Vec::new(),
                        digest: Digest::default(),
                    };
                };
                let flag = u32::from(lookup.flag());
                let props = match lookup.mark_filtering_set() {
                    Some(set) => flag.wrapping_add(u32::from(set) << 16),
                    None => flag,
                };
                let subtables: Vec<Sub<S>> = (0..lookup.num_subtables())
                    .filter_map(|k| subtable(&lookup, k))
                    .map(|table| {
                        let mut digest = Digest::default();
                        add_coverage(&mut digest, &coverage(&table));
                        Sub { table, digest }
                    })
                    .collect();
                let mut digest = Digest::default();
                for s in &subtables {
                    digest.union(&s.digest);
                }
                LookupAccel {
                    props,
                    reverse: reverse_type.is_some_and(|r| lookup.effective_type(extension) == r),
                    subtables,
                    digest,
                }
            })
            .collect();
        Table {
            has_data: true,
            scripts: t.script_list().ok().flatten(),
            features: t.feature_list().ok().flatten(),
            lookup_count,
            lookups,
        }
    }

    /// `get_lookup_count`.
    pub(crate) fn lookup_count(&self) -> usize {
        self.lookup_count
    }

    /// `find_script_index`.
    fn find_script(&self, tag: Tag) -> Option<usize> {
        self.scripts.and_then(|s| s.find(tag))
    }

    /// `hb_ot_layout_table_select_script`: whether one of `tags` was
    /// found, the script index and the chosen tag.
    pub(crate) fn select_script(&self, tags: &[Tag]) -> (bool, Option<usize>, Option<Tag>) {
        if let Some((i, t)) = tags
            .iter()
            .find_map(|&t| self.find_script(t).map(|i| (i, t)))
        {
            return (true, Some(i), Some(t));
        }
        for t in [DFLT, DFLT_LANG, LATN] {
            if let Some(i) = self.find_script(t) {
                return (false, Some(i), Some(t));
            }
        }
        (false, None, None)
    }

    /// `hb_ot_layout_script_select_language`: the language system
    /// index; `None` for the default one.
    pub(crate) fn select_language(&self, script: Option<usize>, tags: &[Tag]) -> Option<usize> {
        let s = script.and_then(|i| self.scripts.and_then(|l| l.script(i).ok().flatten()))?;
        tags.iter()
            .chain(core::iter::once(&DFLT_LANG))
            .find_map(|&t| s.find_lang_sys(t))
    }

    /// `get_script (script).get_lang_sys (language)`.
    fn lang_sys(&self, script: Option<usize>, language: Option<usize>) -> Option<LangSys<'a>> {
        let s = script.and_then(|i| self.scripts.and_then(|l| l.script(i).ok().flatten()))?;
        match language {
            Some(i) => s.lang_sys(i).ok().flatten(),
            None => s.default_lang_sys().ok().flatten(),
        }
    }

    /// The tag of feature `index`; `None` for `HB_TAG_NONE`.
    pub(crate) fn feature_tag(&self, index: u16) -> Option<Tag> {
        self.features.and_then(|f| f.tag(usize::from(index)))
    }

    /// `hb_ot_layout_language_get_required_feature`.
    pub(crate) fn required_feature(
        &self,
        script: Option<usize>,
        language: Option<usize>,
    ) -> Option<u16> {
        self.lang_sys(script, language)
            .map(|l| l.required_feature_index())
            .filter(|&i| i != 0xFFFF)
    }

    /// `hb_ot_layout_collect_features_map`: feature tag to index, the
    /// first entry of a tag winning.
    pub(crate) fn features_map(
        &self,
        script: Option<usize>,
        language: Option<usize>,
    ) -> BTreeMap<Tag, u16> {
        let mut map = BTreeMap::new();
        if let Some(l) = self.lang_sys(script, language) {
            for i in (0..l.num_features()).rev() {
                let Some(index) = l.feature_index(i) else {
                    break;
                };
                let tag = self.feature_tag(index).unwrap_or(Tag([0; 4]));
                map.insert(tag, index);
            }
        }
        map
    }

    /// `hb_ot_layout_language_find_feature`.
    pub(crate) fn language_find_feature(
        &self,
        script: Option<usize>,
        language: Option<usize>,
        tag: Tag,
    ) -> Option<u16> {
        let l = self.lang_sys(script, language)?;
        (0..l.num_features())
            .filter_map(|i| l.feature_index(i))
            .find(|&f| self.feature_tag(f) == Some(tag))
    }

    /// `hb_ot_layout_table_find_feature`.
    pub(crate) fn find_feature(&self, tag: Tag) -> Option<u16> {
        let f = self.features?;
        (0..f.len())
            .find(|&i| f.tag(i) == Some(tag))
            .and_then(|i| u16::try_from(i).ok())
    }

    /// The lookup indices of feature `index` (none for an index past the
    /// list or a feature that cannot be read).
    pub(crate) fn feature_lookups(&self, index: u16) -> Vec<u16> {
        let Some(f) = self
            .features
            .and_then(|l| l.feature(usize::from(index)).ok().flatten())
        else {
            return Vec::new();
        };
        (0..f.num_lookups())
            .filter_map(|i| f.lookup_index(i))
            .collect()
    }

    /// The accelerator of lookup `index`.
    pub(crate) fn lookup(&self, index: usize) -> Option<&LookupAccel<S>> {
        self.lookups.get(index)
    }
}

/// The layout tables of a font.
#[derive(Clone, Debug)]
pub(crate) struct Layout<'a> {
    pub(crate) gdef: Gdef<'a>,
    pub(crate) gsub: Table<'a, SubstSubtable<'a>>,
    pub(crate) gpos: Table<'a, PosSubtable<'a>>,
}

impl<'a> Layout<'a> {
    pub(crate) fn new(font: &Font<'a>) -> Layout<'a> {
        Layout {
            gdef: Gdef::new(font),
            gsub: Table::new(
                font.table(gsub_view::TAG),
                gsub_view::EXTENSION,
                |l, i| gsub_view::subtable(l, i).ok(),
                subst_coverage,
                Some(gsub_view::REVERSE_CHAIN_SINGLE),
            ),
            gpos: Table::new(
                font.table(gpos_view::TAG),
                gpos_view::EXTENSION,
                |l, i| gpos_view::subtable(l, i).ok(),
                pos_coverage,
                None,
            ),
        }
    }
}
