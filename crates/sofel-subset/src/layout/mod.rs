//! GDEF, GSUB and GPOS: the layout closure and subsetting, as HarfBuzz
//! 15.1.0's `hb-subset-plan-layout.cc`, `hb-ot-layout-common.hh`,
//! `hb-ot-layout-gsubgpos.hh`, `OT/Layout/GSUB/`, `OT/Layout/GPOS/` and
//! `OT/Layout/GDEF/GDEF.hh` (ADR 0022, stage 3).

pub(crate) mod closure;
pub(crate) mod common;
pub(crate) mod context;
pub(crate) mod data;
pub(crate) mod gdef;
pub(crate) mod gpos;
pub(crate) mod gsub;
pub(crate) mod plan;

use alloc::collections::BTreeMap;

use crate::bits::BitSet;
use data::D;

/// HarfBuzz's limits (`src/hb-limits.hh`) used by the layout code.
pub(crate) mod limits {
    /// `HB_MAX_NESTING_LEVEL`.
    pub const MAX_NESTING_LEVEL: u32 = 64;
    /// `HB_CLOSURE_MAX_STAGES`.
    pub const CLOSURE_MAX_STAGES: u32 = 12;
    /// `HB_MAX_SCRIPTS`.
    pub const MAX_SCRIPTS: u32 = 500;
    /// `HB_MAX_LANGSYS`.
    pub const MAX_LANGSYS: u32 = 2000;
    /// `HB_MAX_LANGSYS_FEATURE_COUNT`.
    pub const MAX_LANGSYS_FEATURE_COUNT: u32 = 50000;
    /// `HB_MAX_FEATURE_INDICES`.
    pub const MAX_FEATURE_INDICES: u32 = 8000;
    /// `HB_MAX_LOOKUP_VISIT_COUNT`.
    pub const MAX_LOOKUP_VISIT_COUNT: u32 = 35000;
}

/// What the plan decided for one of GSUB and GPOS (the `gsub_*` and
/// `gpos_*` members of `hb_subset_plan_t`).
#[derive(Clone, Debug, Default)]
pub(crate) struct LayoutPlan {
    /// Old lookup index to new (`gsub_lookups`).
    pub lookups: BTreeMap<u32, u32>,
    /// Old feature index to new, duplicates left out (`gsub_features`).
    pub features: BTreeMap<u32, u32>,
    /// Old feature index to new, duplicates mapped to the feature they
    /// duplicate (`gsub_features_w_duplicates`).
    pub features_w_dups: BTreeMap<u32, u32>,
    /// Script index to the indices of its kept language systems
    /// (`gsub_langsys`).
    pub langsys: BTreeMap<u32, BitSet>,
}

/// The kinds of lookup subtables of GSUB and GPOS.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    SingleSubst,
    MultipleSubst,
    AlternateSubst,
    LigatureSubst,
    Context,
    ChainContext,
    ReverseChain,
    SinglePos,
    PairPos,
    CursivePos,
    MarkBasePos,
    MarkLigPos,
    MarkMarkPos,
    Unknown,
}

/// The extension lookup type of GSUB (7) or GPOS (9).
pub(crate) fn extension_type(gsub: bool) -> u16 {
    if gsub { 7 } else { 9 }
}

/// The kind of a lookup type of GSUB or GPOS.
pub(crate) fn kind(gsub: bool, lookup_type: u16) -> Kind {
    match (gsub, lookup_type) {
        (true, 1) => Kind::SingleSubst,
        (true, 2) => Kind::MultipleSubst,
        (true, 3) => Kind::AlternateSubst,
        (true, 4) => Kind::LigatureSubst,
        (true, 5) | (false, 7) => Kind::Context,
        (true, 6) | (false, 8) => Kind::ChainContext,
        (true, 8) => Kind::ReverseChain,
        (false, 1) => Kind::SinglePos,
        (false, 2) => Kind::PairPos,
        (false, 3) => Kind::CursivePos,
        (false, 4) => Kind::MarkBasePos,
        (false, 5) => Kind::MarkLigPos,
        (false, 6) => Kind::MarkMarkPos,
        _ => Kind::Unknown,
    }
}

/// A GSUB or GPOS table (`GSUBGPOS`), versions 1.0 and 1.1.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Gsubgpos<'a> {
    pub d: D<'a>,
    pub gsub: bool,
}

impl<'a> Gsubgpos<'a> {
    /// The table read from `data`.
    pub fn new(data: &'a [u8], gsub: bool) -> Gsubgpos<'a> {
        Gsubgpos { d: D(data), gsub }
    }

    /// The version as `major << 16 | minor`.
    pub fn version(self) -> u32 {
        self.d.u32(0)
    }

    /// The ScriptList.
    pub fn script_list(self) -> D<'a> {
        self.d.off16(4)
    }

    /// The FeatureList.
    pub fn feature_list(self) -> D<'a> {
        self.d.off16(6)
    }

    /// The LookupList.
    pub fn lookup_list(self) -> D<'a> {
        self.d.off16(8)
    }

    /// The FeatureVariations table, empty below version 1.1.
    pub fn feature_variations(self) -> D<'a> {
        if self.version() >= 0x0001_0001 {
            self.d.off32(10)
        } else {
            D(&[])
        }
    }

    /// The number of scripts.
    pub fn script_count(self) -> usize {
        usize::from(self.script_list().u16(0))
    }

    /// The tag of script `i`.
    pub fn script_tag(self, i: usize) -> u32 {
        if i >= self.script_count() {
            return 0;
        }
        self.script_list()
            .u32(2usize.saturating_add(i.saturating_mul(6)))
    }

    /// Script `i`.
    pub fn script(self, i: usize) -> D<'a> {
        if i >= self.script_count() {
            return D(&[]);
        }
        let l = self.script_list();
        l.off16(6usize.saturating_add(i.saturating_mul(6)))
    }

    /// The number of features.
    pub fn feature_count(self) -> usize {
        usize::from(self.feature_list().u16(0))
    }

    /// The tag of feature `i` (`get_feature_tag`; 0 out of range).
    pub fn feature_tag(self, i: usize) -> u32 {
        if i >= self.feature_count() {
            return 0;
        }
        self.feature_list()
            .u32(2usize.saturating_add(i.saturating_mul(6)))
    }

    /// Feature `i`.
    pub fn feature(self, i: usize) -> D<'a> {
        if i >= self.feature_count() {
            return D(&[]);
        }
        let l = self.feature_list();
        l.off16(6usize.saturating_add(i.saturating_mul(6)))
    }

    /// The number of lookups.
    pub fn lookup_count(self) -> usize {
        usize::from(self.lookup_list().u16(0))
    }

    /// Lookup `i` (empty out of range).
    pub fn lookup(self, i: usize) -> D<'a> {
        if i >= self.lookup_count() {
            return D(&[]);
        }
        self.lookup_list()
            .off16(2usize.saturating_add(i.saturating_mul(2)))
    }

    /// The offset of `d` from the start of the table: views are suffixes
    /// of the table, so this identifies a structure as HarfBuzz's pointer
    /// arithmetic does.
    pub fn delta(self, d: D<'a>) -> usize {
        self.d.0.len().saturating_sub(d.0.len())
    }
}

/// A lookup's subtables with extensions resolved: `(kind, subtable)`.
pub(crate) fn subtables<'a>(lookup: D<'a>, gsub: bool) -> alloc::vec::Vec<(Kind, D<'a>)> {
    let ty = lookup.u16(0);
    let n = usize::from(lookup.u16(4));
    (0..n)
        .filter_map(|i| {
            let st = lookup.off16(6usize.saturating_add(i.saturating_mul(2)));
            resolve(ty, st, gsub)
        })
        .collect()
}

/// The kind and data of a subtable of lookup type `ty`, through an
/// extension subtable (format 1 only).
pub(crate) fn resolve(ty: u16, st: D<'_>, gsub: bool) -> Option<(Kind, D<'_>)> {
    if ty == extension_type(gsub) {
        if st.u16(0) != 1 {
            return None;
        }
        let inner = st.u16(2);
        if inner == extension_type(gsub) {
            return None;
        }
        return Some((kind(gsub, inner), st.off32(4)));
    }
    Some((kind(gsub, ty), st))
}

/// A Feature's FeatureParams table as HarfBuzz keeps it after sanitizing
/// with the feature's tag: empty when the offset is null or the table
/// fails its check (`FeatureParams::sanitize`).
pub(crate) fn feature_params(feature: D<'_>, tag: u32) -> D<'_> {
    let p = feature.off16(0);
    if p.is_null() {
        return p;
    }
    let len = p.0.len();
    let ok = if tag == u32::from_be_bytes(*b"size") {
        let design = p.u16(0);
        let subfamily = p.u16(2);
        let name = p.u16(4);
        let start = p.u16(6);
        let end = p.u16(8);
        len >= 10
            && design != 0
            && ((subfamily == 0 && name == 0 && start == 0 && end == 0)
                || !(design < start || design > end || !(256..=32767).contains(&name)))
    } else if tag & 0xFFFF_0000 == u32::from_be_bytes(*b"ss\0\0") {
        len >= 4
    } else if tag & 0xFFFF_0000 == u32::from_be_bytes(*b"cv\0\0") {
        len >= 14 && len >= 14usize.saturating_add(usize::from(p.u16(12)).saturating_mul(3))
    } else {
        true
    };
    if ok { p } else { D(&[]) }
}

/// The size of FeatureParams for `tag` as HarfBuzz copies it; `None`
/// for tags without parameters (`FeatureParams::subset`).
pub(crate) fn feature_params_size(params: D<'_>, tag: u32) -> Option<usize> {
    if tag == u32::from_be_bytes(*b"size") {
        Some(10)
    } else if tag & 0xFFFF_0000 == u32::from_be_bytes(*b"ss\0\0") {
        Some(4)
    } else if tag & 0xFFFF_0000 == u32::from_be_bytes(*b"cv\0\0") {
        Some(14usize.saturating_add(usize::from(params.u16(12)).saturating_mul(3)))
    } else {
        None
    }
}
