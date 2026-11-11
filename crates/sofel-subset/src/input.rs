//! The subsetting input ([`Input`]), HarfBuzz's `hb_subset_input_t`.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;

use crate::set::RangeSet;

/// The subsetting flags, with HarfBuzz's bit values (`hb_subset_flags_t`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct Flags(pub u32);

impl Flags {
    /// No flags.
    pub const DEFAULT: Flags = Flags(0);
    /// Drop hinting: instructions and the hinting tables.
    pub const NO_HINTING: Flags = Flags(0x1);
    /// Keep the glyph ids; dropped glyphs become empty.
    pub const RETAIN_GIDS: Flags = Flags(0x2);
    /// Remove CFF subroutines.
    pub const DESUBROUTINIZE: Flags = Flags(0x4);
    /// Keep non-Unicode name records.
    pub const NAME_LEGACY: Flags = Flags(0x8);
    /// Set the overlap flag of the first contour or component.
    pub const SET_OVERLAPS_FLAG: Flags = Flags(0x10);
    /// Copy tables without a subsetter instead of dropping them.
    pub const PASSTHROUGH_UNRECOGNIZED: Flags = Flags(0x20);
    /// Keep the outline of `.notdef`.
    pub const NOTDEF_OUTLINE: Flags = Flags(0x40);
    /// Keep post's glyph names.
    pub const GLYPH_NAMES: Flags = Flags(0x80);
    /// Keep OS/2's Unicode ranges as they are.
    pub const NO_PRUNE_UNICODE_RANGES: Flags = Flags(0x100);
    /// Skip the GSUB closure.
    pub const NO_LAYOUT_CLOSURE: Flags = Flags(0x200);
    /// Optimize IUP deltas when instancing.
    pub const OPTIMIZE_IUP_DELTAS: Flags = Flags(0x400);
    /// Skip adding the mirrored characters.
    pub const NO_BIDI_CLOSURE: Flags = Flags(0x800);
    /// Incremental font transfer requirements (HarfBuzz's experimental API).
    pub const IFTB_REQUIREMENTS: Flags = Flags(0x1000);
    /// With `RETAIN_GIDS`, keep the source's glyph count (experimental API).
    pub const RETAIN_NUM_GLYPHS: Flags = Flags(0x2000);
    /// Convert CFF2 to CFF when every axis is pinned.
    pub const DOWNGRADE_CFF2: Flags = Flags(0x4000);
    /// Use the glyph ids as the CIDs of a CID-keyed CFF.
    pub const CFF_IDENTITY_CHARSET: Flags = Flags(0x8000);
    /// Every defined bit.
    pub const ALL: Flags = Flags(0xFFFF);

    /// Whether every bit of `f` is set.
    pub fn has(self, f: Flags) -> bool {
        self.0 & f.0 == f.0
    }
}

impl core::ops::BitOr for Flags {
    type Output = Flags;
    fn bitor(self, o: Flags) -> Flags {
        Flags(self.0 | o.0)
    }
}

/// The input's sets, numbered as HarfBuzz's `hb_subset_sets_t`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum SetKind {
    /// Glyph ids to keep.
    Glyphs = 0,
    /// Characters to keep.
    Unicodes = 1,
    /// Tables copied without subsetting.
    NoSubsetTables = 2,
    /// Tables dropped.
    DropTables = 3,
    /// Name ids kept.
    NameIds = 4,
    /// Name language ids kept.
    NameLanguages = 5,
    /// Layout features kept.
    LayoutFeatures = 6,
    /// Layout scripts kept.
    LayoutScripts = 7,
}

impl SetKind {
    /// Every kind, in HarfBuzz's order.
    pub const ALL: [SetKind; 8] = [
        SetKind::Glyphs,
        SetKind::Unicodes,
        SetKind::NoSubsetTables,
        SetKind::DropTables,
        SetKind::NameIds,
        SetKind::NameLanguages,
        SetKind::LayoutFeatures,
        SetKind::LayoutScripts,
    ];

    /// The kind numbered `v` in HarfBuzz.
    pub fn from_u32(v: u32) -> Option<SetKind> {
        SetKind::ALL.get(usize::try_from(v).ok()?).copied()
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
struct Sets {
    glyphs: RangeSet,
    unicodes: RangeSet,
    no_subset_tables: RangeSet,
    drop_tables: RangeSet,
    name_ids: RangeSet,
    name_languages: RangeSet,
    layout_features: RangeSet,
    layout_scripts: RangeSet,
}

impl Sets {
    fn get(&self, kind: SetKind) -> &RangeSet {
        match kind {
            SetKind::Glyphs => &self.glyphs,
            SetKind::Unicodes => &self.unicodes,
            SetKind::NoSubsetTables => &self.no_subset_tables,
            SetKind::DropTables => &self.drop_tables,
            SetKind::NameIds => &self.name_ids,
            SetKind::NameLanguages => &self.name_languages,
            SetKind::LayoutFeatures => &self.layout_features,
            SetKind::LayoutScripts => &self.layout_scripts,
        }
    }
    fn get_mut(&mut self, kind: SetKind) -> &mut RangeSet {
        match kind {
            SetKind::Glyphs => &mut self.glyphs,
            SetKind::Unicodes => &mut self.unicodes,
            SetKind::NoSubsetTables => &mut self.no_subset_tables,
            SetKind::DropTables => &mut self.drop_tables,
            SetKind::NameIds => &mut self.name_ids,
            SetKind::NameLanguages => &mut self.name_languages,
            SetKind::LayoutFeatures => &mut self.layout_features,
            SetKind::LayoutScripts => &mut self.layout_scripts,
        }
    }
}

/// An axis range in user coordinates (HarfBuzz's `Triple`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AxisRange {
    /// The minimum.
    pub min: f32,
    /// The default.
    pub default: f32,
    /// The maximum.
    pub max: f32,
}

/// A range of one value.
fn point(v: f32) -> AxisRange {
    AxisRange {
        min: v,
        default: v,
        max: v,
    }
}

/// `hb_ot_var_find_axis_info`: the first axis tagged `tag`.
fn find_axis(font: &Font<'_>, tag: Tag) -> Option<crate::instance::AxisInfo> {
    crate::instance::axis_infos(font)
        .into_iter()
        .find(|a| a.tag == tag)
}

/// The subsetting input: an immutable value; each `with_*` returns a new
/// one (HarfBuzz's `hb_subset_input_t`).
#[derive(Clone, PartialEq, Debug)]
pub struct Input {
    sets: Sets,
    flags: Flags,
    glyph_map: BTreeMap<u32, u32>,
    axes: BTreeMap<Tag, AxisRange>,
    name_overrides: BTreeMap<(u16, u16, u16, u16), Vec<u8>>,
}

/// Tables dropped by default (`hb_subset_input_t`'s constructor).
pub const DEFAULT_DROP_TABLES: [&[u8; 4]; 17] = [
    b"morx", b"mort", b"kerx", b"kern", b"JSTF", b"DSIG", b"EBDT", b"EBLC", b"EBSC", b"SVG ",
    b"PCLT", b"LTSH", b"Feat", b"Glat", b"Gloc", b"Silf", b"Sill",
];

/// Tables copied without subsetting by default.
pub const DEFAULT_NO_SUBSET_TABLES: [&[u8; 4]; 5] = [b"gasp", b"fpgm", b"prep", b"VDMX", b"DSIG"];

/// Layout features kept by default (fontTools' list, as HarfBuzz).
pub const DEFAULT_LAYOUT_FEATURES: &[&[u8; 4]] = &[
    b"rvrn", b"ccmp", b"liga", b"locl", b"mark", b"mkmk", b"rlig", b"frac", b"numr", b"dnom",
    b"calt", b"clig", b"curs", b"kern", b"rclt", b"valt", b"vert", b"vkrn", b"vpal", b"vrt2",
    b"ltra", b"ltrm", b"rtla", b"rtlm", b"rand", b"jalt", b"chws", b"vchw", b"halt", b"vhal",
    b"palt", b"Harf", b"HARF", b"Buzz", b"BUZZ", b"init", b"medi", b"fina", b"isol", b"med2",
    b"fin2", b"fin3", b"cswh", b"mset", b"stch", b"ljmo", b"vjmo", b"tjmo", b"abvs", b"blws",
    b"abvm", b"blwm", b"nukt", b"akhn", b"rphf", b"rkrf", b"pref", b"blwf", b"half", b"abvf",
    b"pstf", b"cfar", b"vatu", b"cjct", b"init", b"pres", b"abvs", b"blws", b"psts", b"haln",
    b"dist", b"abvm", b"blwm",
];

fn tag_set(tags: &[&[u8; 4]]) -> RangeSet {
    RangeSet::from_values(tags.iter().map(|t| Tag::new(t).to_u32()))
}

impl Default for Input {
    fn default() -> Self {
        Input::new()
    }
}

impl Input {
    /// HarfBuzz's default input: no glyphs or characters, name ids 0 to 6
    /// in language 0x409, the default dropped, copied and layout feature
    /// tables, every script and no flags.
    pub fn new() -> Input {
        let mut sets = Sets::default();
        sets.name_ids.add_range(0, 6);
        sets.name_languages.add(0x409);
        sets.drop_tables = tag_set(&DEFAULT_DROP_TABLES);
        sets.no_subset_tables = tag_set(&DEFAULT_NO_SUBSET_TABLES);
        sets.layout_features = tag_set(DEFAULT_LAYOUT_FEATURES);
        sets.layout_scripts = RangeSet::all();
        Input {
            sets,
            flags: Flags::DEFAULT,
            glyph_map: BTreeMap::new(),
            axes: BTreeMap::new(),
            name_overrides: BTreeMap::new(),
        }
    }

    /// `hb_subset_input_keep_everything`: every glyph, character, name,
    /// language, feature and script; no dropped tables; and the flags
    /// `NOTDEF_OUTLINE`, `GLYPH_NAMES`, `NAME_LEGACY`,
    /// `NO_PRUNE_UNICODE_RANGES` and `PASSTHROUGH_UNRECOGNIZED`.
    pub fn keep_everything(self) -> Input {
        let all = [
            SetKind::Unicodes,
            SetKind::Glyphs,
            SetKind::NameIds,
            SetKind::NameLanguages,
            SetKind::LayoutFeatures,
            SetKind::LayoutScripts,
        ];
        let mut s = all
            .iter()
            .fold(self, |s, &k| s.with_set(k, RangeSet::all()));
        s.sets.drop_tables.clear();
        s.with_flags(
            Flags::NOTDEF_OUTLINE
                | Flags::GLYPH_NAMES
                | Flags::NAME_LEGACY
                | Flags::NO_PRUNE_UNICODE_RANGES
                | Flags::PASSTHROUGH_UNRECOGNIZED,
        )
    }

    /// The set of `kind`.
    pub fn set(&self, kind: SetKind) -> &RangeSet {
        self.sets.get(kind)
    }

    /// The input with the set of `kind` replaced.
    pub fn with_set(mut self, kind: SetKind, set: RangeSet) -> Input {
        *self.sets.get_mut(kind) = set;
        self
    }

    /// The input with the set of `kind` changed by `f`.
    pub fn map_set(mut self, kind: SetKind, f: impl FnOnce(&mut RangeSet)) -> Input {
        f(self.sets.get_mut(kind));
        self
    }

    /// The flags.
    pub fn flags(&self) -> Flags {
        self.flags
    }

    /// The input with these flags.
    pub fn with_flags(mut self, flags: Flags) -> Input {
        self.flags = flags;
        self
    }

    /// The requested glyph mapping (old id to new id).
    pub fn glyph_mapping(&self) -> &BTreeMap<u32, u32> {
        &self.glyph_map
    }

    /// The input with a requested glyph mapping
    /// (`hb_subset_input_old_to_new_glyph_mapping`).
    pub fn with_glyph_mapping(mut self, map: BTreeMap<u32, u32>) -> Input {
        self.glyph_map = map;
        self
    }

    /// The axis locations and ranges.
    pub fn axes(&self) -> &BTreeMap<Tag, AxisRange> {
        &self.axes
    }

    /// The range of axis `tag`, if set (`hb_subset_input_get_axis_range`).
    pub fn axis_range(&self, tag: Tag) -> Option<AxisRange> {
        self.axes.get(&tag).copied()
    }

    /// The input with `tag` set to `range`, unchecked (used by the
    /// functions that check the font's `fvar`).
    pub(crate) fn with_axis_unchecked(mut self, tag: Tag, range: AxisRange) -> Input {
        self.axes.insert(tag, range);
        self
    }

    /// The input with every axis of `font` pinned to its default
    /// (`hb_subset_input_pin_all_axes_to_default`); `None`, as HarfBuzz
    /// fails, when the font has no axes (no valid `fvar`).
    pub fn with_all_axes_pinned_to_default(&self, font: &Font<'_>) -> Option<Input> {
        let axes = crate::instance::axis_infos(font);
        if axes.is_empty() {
            return None;
        }
        Some(axes.iter().fold(self.clone(), |input, a| {
            input.with_axis_unchecked(a.tag, point(a.default))
        }))
    }

    /// The input with axis `tag` pinned to its default
    /// (`hb_subset_input_pin_axis_to_default`); `None` when `font` has no
    /// such axis.
    pub fn with_axis_pinned_to_default(&self, tag: Tag, font: &Font<'_>) -> Option<Input> {
        let a = find_axis(font, tag)?;
        Some(self.clone().with_axis_unchecked(tag, point(a.default)))
    }

    /// The input with axis `tag` pinned to `value`, clamped to the axis
    /// range (`hb_subset_input_pin_axis_location`); `None` when `font`
    /// has no such axis.
    pub fn with_axis_pinned(&self, tag: Tag, value: f32, font: &Font<'_>) -> Option<Input> {
        let a = find_axis(font, tag)?;
        let v = crate::instance::clamp_f64(f64::from(value), f64::from(a.min), f64::from(a.max));
        let v = crate::instance::f64_to_f32(v);
        Some(self.clone().with_axis_unchecked(tag, point(v)))
    }

    /// The input with the range of axis `tag` restricted
    /// (`hb_subset_input_set_axis_range`): `None` (or NaN) for a value
    /// takes the axis's own; the minimum and maximum are clamped to the
    /// axis range and the default to the new range. `None` when `font`
    /// has no such axis or when `min` is above `max`.
    pub fn with_axis_range(
        &self,
        tag: Tag,
        min: Option<f32>,
        max: Option<f32>,
        default: Option<f32>,
        font: &Font<'_>,
    ) -> Option<Input> {
        let a = find_axis(font, tag)?;
        let or = |v: Option<f32>, d: f32| v.filter(|v| !v.is_nan()).unwrap_or(d);
        let (min, max, def) = (or(min, a.min), or(max, a.max), or(default, a.default));
        if min > max {
            return None;
        }
        let new_min = crate::instance::clamp_f32(min, a.min, a.max);
        let new_max = crate::instance::clamp_f32(max, a.min, a.max);
        let new_default = crate::instance::clamp_f32(def, new_min, new_max);
        Some(self.clone().with_axis_unchecked(
            tag,
            AxisRange {
                min: new_min,
                default: new_default,
                max: new_max,
            },
        ))
    }

    /// The name records to override, by (platform, encoding, language, name
    /// id); an empty value removes the record.
    pub fn name_overrides(&self) -> &BTreeMap<(u16, u16, u16, u16), Vec<u8>> {
        &self.name_overrides
    }

    /// The input with a name record overridden
    /// (`hb_subset_input_override_name_table`); `text` is UTF-8, and
    /// empty to remove the record.
    pub fn with_name_override(mut self, ids: (u16, u16, u16, u16), text: Vec<u8>) -> Input {
        self.name_overrides.insert(ids, text);
        self
    }
}
