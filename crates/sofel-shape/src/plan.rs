//! Shaping plans (ADR 0020, シェーピングの計画): what to do for a font,
//! segment properties and features, decided once (`hb_shape_plan_t`,
//! `hb_ot_shape_plan_t`, `hb_ot_shape_planner_t`), and the shapers that
//! plug into the steps (`hb_ot_shaper_t`).
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shape.cc`,
//! `src/hb-ot-shaper.hh` and `src/hb-ot-shaper-default.cc`, and
//! HarfRust 0.15.0 `src/plan.rs` and `src/planner.rs`, MIT; see
//! THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::layout::LayoutTable;
use sofel_font::var::{HbF64, Instance, StoreKind};
use sofel_unicode::Script;

use crate::buffer::{Buffer, Mask};
use crate::direction::Direction;
use crate::feature::Feature;
use crate::font::ShapeFont;
use crate::layout::map::{Map, MapBuilder, flags};
use crate::layout::{GPOS, GSUB, Layout};
use crate::normalize::NormalizeContext;
use crate::props::ResolvedProperties;
use crate::tag::ot_tags_from_script_and_language;

/// The script-specific shaper a plan uses (`hb_ot_shaper_categorize`).
///
/// The shapers of ADR 0024 not implemented yet run the default shaper's
/// steps; the plan still reports which one HarfBuzz would pick.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Shaper {
    /// `_hb_ot_shaper_default`.
    Default,
    /// `_hb_ot_shaper_arabic` (Arabic and Syriac, horizontal).
    Arabic,
    /// `_hb_ot_shaper_hangul`.
    Hangul,
    /// `_hb_ot_shaper_hebrew`.
    Hebrew,
    /// `_hb_ot_shaper_indic`.
    Indic,
    /// `_hb_ot_shaper_khmer`.
    Khmer,
    /// `_hb_ot_shaper_myanmar`.
    Myanmar,
    /// `_hb_ot_shaper_myanmar_zawgyi` (the `Qaag` script).
    MyanmarZawgyi,
    /// `_hb_ot_shaper_thai` (Thai and Lao).
    Thai,
    /// `_hb_ot_shaper_use`, the Universal Shaping Engine.
    Use,
}

/// The normalization a shaper asks for
/// (`hb_ot_shape_normalization_mode_t`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum NormalizationMode {
    None,
    Decomposed,
    ComposedDiacritics,
    ComposedDiacriticsNoShortCircuit,
    Auto,
}

/// When mark advances are zeroed
/// (`hb_ot_shape_zero_width_marks_type_t`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ZeroWidthMarks {
    None,
    ByGdefEarly,
    ByGdefLate,
}

/// Data a shaper keeps in the plan (`plan->data`, ADR 0024).
#[derive(Clone, Debug, Default)]
pub(crate) enum ShaperData {
    #[default]
    None,
    /// `hangul_shape_plan_t`.
    Hangul(crate::shapers::hangul::HangulPlan),
    /// `arabic_shape_plan_t`.
    Arabic(crate::shapers::arabic::ArabicPlan),
    /// `indic_shape_plan_t`.
    Indic(crate::shapers::indic::IndicPlan),
    /// `khmer_shape_plan_t`.
    Khmer(crate::shapers::khmer::KhmerPlan),
}

/// A hook that runs on the buffer.
pub(crate) type BufferHook = fn(&ShapePlan<'_>, &mut Buffer, &ShapeFont<'_, '_, '_, '_>);

/// A shaper's decomposition (`decompose`).
pub(crate) type DecomposeFn = fn(&NormalizeContext<'_, '_, '_, '_, '_>, u32) -> Option<(u32, u32)>;
/// A shaper's composition (`compose`).
pub(crate) type ComposeFn = fn(&NormalizeContext<'_, '_, '_, '_, '_>, u32, u32) -> Option<u32>;

/// The hooks and settings of a shaper (`hb_ot_shaper_t`), at the places
/// HarfBuzz calls them.
pub(crate) struct ShaperFuncs {
    pub(crate) collect_features: Option<fn(&mut Planner<'_, '_>)>,
    pub(crate) override_features: Option<fn(&mut Planner<'_, '_>)>,
    pub(crate) data_create: Option<fn(&ShapePlan<'_>, &Font<'_>) -> ShaperData>,
    pub(crate) preprocess_text: Option<BufferHook>,
    pub(crate) postprocess_glyphs: Option<BufferHook>,
    pub(crate) decompose: Option<DecomposeFn>,
    pub(crate) compose: Option<ComposeFn>,
    pub(crate) setup_masks: Option<BufferHook>,
    pub(crate) reorder_marks: Option<fn(&ShapePlan<'_>, &mut Buffer, usize, usize)>,
    pub(crate) gpos_tag: Option<Tag>,
    pub(crate) normalization_preference: NormalizationMode,
    pub(crate) zero_width_marks: ZeroWidthMarks,
    pub(crate) fallback_position: bool,
}

/// `_hb_ot_shaper_default`.
pub(crate) static DEFAULT_SHAPER: ShaperFuncs = ShaperFuncs {
    collect_features: None,
    override_features: None,
    data_create: None,
    preprocess_text: None,
    postprocess_glyphs: None,
    decompose: None,
    compose: None,
    setup_masks: None,
    reorder_marks: None,
    gpos_tag: None,
    normalization_preference: NormalizationMode::Auto,
    zero_width_marks: ZeroWidthMarks::ByGdefLate,
    fallback_position: true,
};

impl Shaper {
    /// The hooks of the shaper. The shapers of ADR 0024 not implemented
    /// yet run the default shaper's.
    pub(crate) fn funcs(self) -> &'static ShaperFuncs {
        use crate::shapers;
        match self {
            Shaper::Arabic => &shapers::arabic::ARABIC,
            Shaper::Hangul => &shapers::hangul::HANGUL,
            Shaper::Hebrew => &shapers::hebrew::HEBREW,
            Shaper::Indic => &shapers::indic::INDIC,
            Shaper::Khmer => &shapers::khmer::KHMER,
            Shaper::Myanmar => &shapers::myanmar::MYANMAR,
            Shaper::MyanmarZawgyi => &shapers::myanmar::MYANMAR_ZAWGYI,
            Shaper::Thai => &shapers::thai::THAI,
            _ => &DEFAULT_SHAPER,
        }
    }

    /// `hb_ot_shaper_categorize`.
    pub(crate) fn categorize(
        script: Option<Script>,
        direction: Direction,
        gsub_script: Option<Tag>,
    ) -> Shaper {
        let Some(script) = script else {
            return Shaper::Default;
        };
        let gsub = gsub_script.map_or(0, Tag::to_u32);
        let dflt = Tag(*b"DFLT").to_u32();
        let latn = Tag(*b"latn").to_u32();
        let default_or = |s: Shaper| {
            if gsub == dflt || gsub == latn {
                Shaper::Default
            } else {
                s
            }
        };
        match &script.tag() {
            b"Arab" | b"Syrc" => {
                if (gsub != dflt || script.tag() == *b"Arab") && direction.is_horizontal() {
                    Shaper::Arabic
                } else {
                    Shaper::Default
                }
            }
            b"Thai" | b"Laoo" => Shaper::Thai,
            b"Hang" => Shaper::Hangul,
            b"Hebr" => Shaper::Hebrew,
            b"Beng" | b"Deva" | b"Gujr" | b"Guru" | b"Knda" | b"Mlym" | b"Orya" | b"Taml"
            | b"Telu" => {
                if gsub & 0xFF == u32::from(b'3') && gsub != dflt && gsub != latn {
                    Shaper::Use
                } else {
                    default_or(Shaper::Indic)
                }
            }
            b"Khmr" => Shaper::Khmer,
            b"Mymr" => {
                if gsub == Tag(*b"mymr").to_u32() {
                    Shaper::Default
                } else {
                    default_or(Shaper::Myanmar)
                }
            }
            b"Qaag" => Shaper::MyanmarZawgyi,
            b"Tibt" | b"Mong" | b"Sinh" | b"Buhd" | b"Hano" | b"Tglg" | b"Tagb" | b"Limb"
            | b"Tale" | b"Bugi" | b"Khar" | b"Sylo" | b"Tfng" | b"Bali" | b"Nkoo" | b"Phag"
            | b"Cham" | b"Kali" | b"Lepc" | b"Rjng" | b"Saur" | b"Sund" | b"Egyp" | b"Java"
            | b"Kthi" | b"Mtei" | b"Lana" | b"Tavt" | b"Batk" | b"Brah" | b"Mand" | b"Cakm"
            | b"Plrd" | b"Shrd" | b"Takr" | b"Dupl" | b"Gran" | b"Khoj" | b"Sind" | b"Mahj"
            | b"Mani" | b"Modi" | b"Hmng" | b"Phlp" | b"Sidd" | b"Tirh" | b"Ahom" | b"Mult"
            | b"Adlm" | b"Bhks" | b"Marc" | b"Newa" | b"Gonm" | b"Soyo" | b"Zanb" | b"Dogr"
            | b"Gong" | b"Rohg" | b"Maka" | b"Medf" | b"Sogo" | b"Sogd" | b"Elym" | b"Nand"
            | b"Hmnp" | b"Wcho" | b"Chrs" | b"Diak" | b"Kits" | b"Yezi" | b"Cpmn" | b"Ougr"
            | b"Tnsa" | b"Toto" | b"Vith" | b"Kawi" | b"Nagm" | b"Gara" | b"Gukh" | b"Krai"
            | b"Onao" | b"Sunu" | b"Todr" | b"Tutg" | b"Berf" | b"Sidt" | b"Tayo" | b"Tols"
            | b"Jurc" | b"Pcun" | b"Seal" => default_or(Shaper::Use),
            _ => Shaper::Default,
        }
    }
}

/// `hb_ot_shape_planner_t`: what a shaper's `collect_features` and
/// `override_features` see.
pub(crate) struct Planner<'l, 'a> {
    pub(crate) props: &'l ResolvedProperties,
    pub(crate) map: MapBuilder<'l, 'a>,
    pub(crate) shaper: Shaper,
}

/// The chosen scripts and language systems of `GSUB` and `GPOS` (index
/// 0 and 1): `hb_ot_layout_table_select_script` and
/// `hb_ot_layout_script_select_language` for a list of script and
/// language tags.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LayoutSelection {
    /// Whether one of the script tags was found (not a fallback to
    /// `DFLT`, `dflt` or `latn`).
    pub found_script: [bool; 2],
    /// The ScriptRecord index; `None` when there is none.
    pub script_index: [Option<usize>; 2],
    /// The script tag chosen.
    pub chosen_script: [Option<Tag>; 2],
    /// The LangSysRecord index; `None` for the default language system.
    pub language_index: [Option<usize>; 2],
}

/// Selects the script and language system in `GSUB` and `GPOS` as a
/// plan does, for the given tags in preference order.
pub fn layout_selection(
    font: &Font<'_>,
    script_tags: &[Tag],
    language_tags: &[Tag],
) -> LayoutSelection {
    let layout = Layout::new(font);
    let b = MapBuilder::new(&layout, script_tags, language_tags);
    LayoutSelection {
        found_script: b.found_script,
        script_index: b.script_index,
        chosen_script: b.chosen_script,
        language_index: b.language_index,
    }
}

/// A shaping plan: everything about how to shape with a font, segment
/// properties and features that does not depend on the text. Immutable,
/// and shareable between threads.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Some items serve the shapers of ADR 0024.
pub struct ShapePlan<'a> {
    pub(crate) props: ResolvedProperties,
    pub(crate) user_features: Vec<Feature>,
    variations_index: [Option<u32>; 2],
    pub(crate) shaper: Shaper,
    pub(crate) map: Map,
    pub(crate) layout: Layout<'a>,
    pub(crate) data: ShaperData,

    pub(crate) frac_mask: Mask,
    pub(crate) numr_mask: Mask,
    pub(crate) dnom_mask: Mask,
    pub(crate) has_frac: bool,
    pub(crate) rtlm_mask: Mask,
    pub(crate) has_vert: bool,
    pub(crate) kern_mask: Mask,
    pub(crate) requested_kerning: bool,
    pub(crate) fallback_glyph_classes: bool,
    pub(crate) apply_gpos: bool,
    pub(crate) apply_fallback_kern: bool,
    pub(crate) zero_marks: bool,
    pub(crate) has_gpos_mark: bool,
    pub(crate) adjust_mark_positioning_when_zeroing: bool,
    pub(crate) fallback_mark_positioning: bool,
}

/// The features every plan starts with (`common_features`).
const COMMON_FEATURES: [([u8; 4], u32); 7] = [
    (*b"abvm", flags::GLOBAL),
    (*b"blwm", flags::GLOBAL),
    (*b"ccmp", flags::GLOBAL),
    (*b"locl", flags::GLOBAL),
    (*b"mark", flags::GLOBAL_MANUAL_JOINERS),
    (*b"mkmk", flags::GLOBAL_MANUAL_JOINERS),
    (*b"rlig", flags::GLOBAL),
];

/// The features of horizontal text (`horizontal_features`).
const HORIZONTAL_FEATURES: [([u8; 4], u32); 7] = [
    (*b"calt", flags::GLOBAL),
    (*b"clig", flags::GLOBAL),
    (*b"curs", flags::GLOBAL),
    (*b"dist", flags::GLOBAL),
    (*b"kern", flags::GLOBAL_HAS_FALLBACK),
    (*b"liga", flags::GLOBAL),
    (*b"rclt", flags::GLOBAL),
];

/// `hb_ot_shape_collect_features`.
fn collect_features(planner: &mut Planner<'_, '_>, user_features: &[Feature]) {
    let funcs = planner.shaper.funcs();
    let map = &mut planner.map;
    map.is_simple = true;
    map.enable_feature(Tag(*b"rvrn"), flags::NONE, 1);
    map.add_gsub_pause(None);
    match planner.props.direction {
        Direction::Ltr => {
            map.enable_feature(Tag(*b"ltra"), flags::NONE, 1);
            map.enable_feature(Tag(*b"ltrm"), flags::NONE, 1);
        }
        Direction::Rtl => {
            map.enable_feature(Tag(*b"rtla"), flags::NONE, 1);
            map.add_feature(Tag(*b"rtlm"), flags::NONE, 1);
        }
        Direction::Ttb | Direction::Btt => {}
    }
    map.add_feature(Tag(*b"frac"), flags::NONE, 1);
    map.add_feature(Tag(*b"numr"), flags::NONE, 1);
    map.add_feature(Tag(*b"dnom"), flags::NONE, 1);
    map.enable_feature(Tag(*b"rand"), flags::RANDOM, crate::layout::map::MAX_VALUE);
    map.enable_feature(Tag(*b"Harf"), flags::NONE, 1);
    map.enable_feature(Tag(*b"HARF"), flags::NONE, 1);
    if let Some(f) = funcs.collect_features {
        planner.map.is_simple = false;
        f(planner);
    }
    let map = &mut planner.map;
    map.enable_feature(Tag(*b"Buzz"), flags::NONE, 1);
    map.enable_feature(Tag(*b"BUZZ"), flags::NONE, 1);
    for (tag, f) in COMMON_FEATURES {
        map.add_feature(Tag(tag), f, 1);
    }
    if planner.props.direction.is_horizontal() {
        for (tag, f) in HORIZONTAL_FEATURES {
            map.add_feature(Tag(tag), f, 1);
        }
    } else {
        map.enable_feature(Tag(*b"vert"), flags::GLOBAL_SEARCH, 1);
    }
    if !user_features.is_empty() {
        map.is_simple = false;
    }
    for f in user_features {
        let flag = if f.is_global() {
            flags::GLOBAL
        } else {
            flags::NONE
        };
        map.add_feature(f.tag, flag, f.value);
    }
    if let Some(f) = funcs.override_features {
        f(planner);
    }
}

impl<'a> ShapePlan<'a> {
    /// Plans shaping with `font` for `props` and `features` (ADR 0020):
    /// the `GSUB` and `GPOS` script and language system, the shaper,
    /// the features and their mask bits, the lookups of each stage, and
    /// how to position (`hb_ot_shape_plan_t::init0` and
    /// `hb_ot_shape_planner_t::compile`, with the AAT tables and the
    /// `kern` table taken as absent). The lookups' subtables are read
    /// and their digests built here, once.
    pub fn new(
        instance: Instance<'_, 'a, '_, '_>,
        props: ResolvedProperties,
        features: &[Feature],
    ) -> ShapePlan<'a> {
        let font = instance.font();
        let layout = Layout::new(font);
        let variations_index = Self::variations_index_for(instance);
        let (script_tags, language_tags) =
            ot_tags_from_script_and_language(props.script, props.language.as_ref());
        let mut builder = MapBuilder::new(&layout, &script_tags, &language_tags);
        builder.variations_index = variations_index;
        let shaper = Shaper::categorize(props.script, props.direction, builder.chosen_script[GSUB]);
        let funcs = shaper.funcs();
        let mut planner = Planner {
            props: &props,
            map: builder,
            shaper,
        };
        collect_features(&mut planner, features);
        let map = planner.map.compile();

        let frac_mask = map.one_mask(Tag(*b"frac"));
        let numr_mask = map.one_mask(Tag(*b"numr"));
        let dnom_mask = map.one_mask(Tag(*b"dnom"));
        let kern_tag = if props.direction.is_horizontal() {
            Tag(*b"kern")
        } else {
            Tag(*b"vkrn")
        };
        let kern_mask = map.mask(kern_tag).0;
        let disable_gpos = funcs
            .gpos_tag
            .is_some_and(|t| Some(t) != map.chosen_script[GPOS]);
        let has_gpos = !disable_gpos && layout.gpos.has_data;
        let apply_gpos = has_gpos;
        let script_fallback_position = funcs.fallback_position;
        let adjust = !apply_gpos;
        let mut plan = ShapePlan {
            fallback_glyph_classes: !layout.gdef.has_glyph_classes(),
            props,
            user_features: features.to_vec(),
            variations_index,
            shaper,
            frac_mask,
            numr_mask,
            dnom_mask,
            has_frac: frac_mask != 0 || (numr_mask != 0 && dnom_mask != 0),
            rtlm_mask: map.one_mask(Tag(*b"rtlm")),
            has_vert: map.one_mask(Tag(*b"vert")) != 0,
            kern_mask,
            requested_kerning: kern_mask != 0,
            apply_gpos,
            apply_fallback_kern: script_fallback_position && !apply_gpos,
            zero_marks: funcs.zero_width_marks != ZeroWidthMarks::None,
            has_gpos_mark: map.one_mask(Tag(*b"mark")) != 0,
            adjust_mark_positioning_when_zeroing: adjust,
            fallback_mark_positioning: adjust && script_fallback_position,
            map,
            layout,
            data: ShaperData::None,
        };
        if let Some(f) = funcs.data_create {
            plan.data = f(&plan, font);
        }
        plan
    }

    /// The shaper HarfBuzz would pick (`hb_ot_shaper_categorize`).
    pub fn shaper(&self) -> Shaper {
        self.shaper
    }

    /// What the plan was made from, for the caller to compare when it
    /// keeps plans: the segment properties and the features.
    pub fn key(&self) -> (ResolvedProperties, Vec<Feature>, [Option<u32>; 2]) {
        (
            self.props.clone(),
            self.user_features.clone(),
            self.variations_index,
        )
    }

    /// The GSUB and GPOS FeatureVariations record indices for an instance.
    pub fn variations_index_for(instance: Instance<'_, '_, '_, '_>) -> [Option<u32>; 2] {
        let font = instance.font();
        let location = instance.location();
        let delta = |outer, inner| {
            if !location.hb_nonzero() {
                0.0
            } else {
                instance
                    .item_delta::<HbF64>(StoreKind::Gdef, outer, inner)
                    .unwrap_or(0.0)
            }
        };
        [Tag(*b"GSUB"), Tag(*b"GPOS")].map(|tag| {
            font.table(tag)
                .and_then(|data| LayoutTable::new(data).ok())
                .and_then(|table| table.feature_variations().ok().flatten())
                .and_then(|variations| {
                    crate::layout::variations::find_index(
                        variations,
                        location.coords_normalized(),
                        delta,
                    )
                })
        })
    }

    /// The hooks of the plan's shaper.
    pub(crate) fn funcs(&self) -> &'static ShaperFuncs {
        self.shaper.funcs()
    }

    /// The `GSUB` (0) and `GPOS` (1) script tags chosen.
    pub fn chosen_scripts(&self) -> [Option<Tag>; 2] {
        self.map.chosen_script
    }

    /// The features in the map, by tag, with their mask, shift and
    /// whether a lookup was found for them (for tests and dumps).
    pub fn feature_masks(&self) -> Vec<(Tag, u32, u32, bool)> {
        self.map
            .features
            .iter()
            .map(|f| (f.tag, f.mask, f.shift, !f.needs_fallback))
            .collect()
    }

    /// The lookups of `GSUB` (table 0) or `GPOS` (table 1) per stage, in
    /// application order: `(lookup index, mask)` lists.
    pub fn stage_lookups(&self, table: usize) -> Vec<Vec<(u16, u32)>> {
        let (Some(stages), Some(lookups)) =
            (self.map.stages.get(table), self.map.lookups.get(table))
        else {
            return Vec::new();
        };
        let mut start = 0usize;
        stages
            .iter()
            .map(|s| {
                let v = lookups
                    .get(start..s.last_lookup)
                    .unwrap_or_default()
                    .iter()
                    .map(|l| (l.index, l.mask))
                    .collect();
                start = s.last_lookup;
                v
            })
            .collect()
    }

    /// The global mask: the bits every character starts with.
    pub fn global_mask(&self) -> u32 {
        self.map.global_mask
    }
}
