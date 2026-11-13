//! The subsetting plan ([`Plan`]): the closure and the glyph mapping, as
//! HarfBuzz's `src/hb-subset-plan.cc`.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::varc::Varc;

use crate::bits::BitSet;
use crate::cmap_accel::CmapAccel;
use crate::error::SubsetError;
use crate::input::{AxisRange, Flags, Input, SetKind};
use crate::instance::{self, VarPlan};
use crate::layout::{self, Gsubgpos, LayoutPlan};
use crate::limits;
use crate::prepared::{Accel, Key, Prepared};
use crate::set::{INVALID, RangeSet};
use crate::tables::cff::SeacFinder;
use crate::tables::colr::{self, ColorPlan};
use crate::tables::cpal;
use crate::tables::glyf::GlyfAccel;

/// The tables whose subsetting or closure this version does not write
/// yet: present and kept, they make [`Plan::new`] return
/// [`SubsetError::Unsupported`].
const PENDING: [(&[u8; 4], &str); 1] = [(b"VARC", "variable composites (ADR 0022 stage 10)")];

/// The tables `_should_drop_table` drops when every axis is pinned.
pub(crate) const PINNED_DROP: [&[u8; 4]; 8] = [
    b"avar", b"fvar", b"gvar", b"GVAR", b"HVAR", b"VVAR", b"MVAR", b"cvar",
];

/// An identity avar v1 map needs no rebasing when an axis range changes.
/// The glyph mapping: new ids in ascending order with their old ids, and
/// the reverse lookup (ADR 0022's `GlyphMap`).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct GlyphMap {
    /// `(new, old)`, ascending by new id (`new_to_old_gid_list`).
    pub new_to_old: Vec<(u32, u32)>,
    /// Old id to new id: dense for up to 65536 source glyphs, else sorted.
    dense: Vec<u32>,
    sparse: Vec<(u32, u32)>,
}

impl GlyphMap {
    fn new(new_to_old: Vec<(u32, u32)>, source_glyphs: u32) -> GlyphMap {
        let mut m = GlyphMap {
            new_to_old,
            dense: Vec::new(),
            sparse: Vec::new(),
        };
        if source_glyphs <= 65536 {
            m.dense = alloc::vec![INVALID; source_glyphs as usize];
            for &(n, o) in &m.new_to_old {
                if let Some(d) = m.dense.get_mut(o as usize) {
                    *d = n;
                }
            }
        } else {
            m.sparse = m.new_to_old.iter().map(|&(n, o)| (o, n)).collect();
            m.sparse.sort_unstable();
        }
        m
    }

    /// The new id of an old glyph.
    pub fn new_gid(&self, old: u32) -> Option<u32> {
        if self.sparse.is_empty() {
            self.dense
                .get(old as usize)
                .copied()
                .filter(|&n| n != INVALID)
        } else {
            self.sparse
                .binary_search_by_key(&old, |p| p.0)
                .ok()
                .and_then(|i| self.sparse.get(i))
                .map(|p| p.1)
        }
    }

    /// The old id of a new glyph.
    pub fn old_gid(&self, new: u32) -> Option<u32> {
        self.new_to_old
            .binary_search_by_key(&new, |p| p.0)
            .ok()
            .and_then(|i| self.new_to_old.get(i))
            .map(|p| p.1)
    }
}

/// A subsetting plan (`hb_subset_plan_t`): what to keep and how glyphs are
/// renumbered. Immutable once made.
#[derive(Clone, Debug)]
pub struct Plan {
    pub(crate) flags: Flags,
    pub(crate) num_output_glyphs: u32,
    pub(crate) glyphs: GlyphMap,
    /// `plan->unicodes`: the kept characters and variation selectors.
    pub(crate) unicodes: RangeSet,
    /// `(character, new glyph or INVALID)`, ascending by character.
    pub(crate) unicode_to_new_gid: Vec<(u32, u32)>,
    /// Character to old glyph (`codepoint_to_glyph`).
    pub(crate) codepoint_to_glyph: BTreeMap<u32, u32>,
    pub(crate) glyphset: RangeSet,
    pub(crate) glyphs_requested: RangeSet,
    pub(crate) name_overrides: BTreeMap<(u16, u16, u16, u16), Vec<u8>>,
    pub(crate) name_ids: RangeSet,
    pub(crate) name_languages: RangeSet,
    pub(crate) drop_tables: RangeSet,
    pub(crate) no_subset_tables: RangeSet,
    pub(crate) os2_min_cp: u32,
    pub(crate) os2_max_cp: u32,
    pub(crate) force_long_loca: bool,
    /// The axis location and what instancing recomputes.
    pub(crate) var: VarPlan,
    /// Source gvar/GVAR has no shared tuple or glyph variation records.
    pub(crate) inert_gvar: bool,
    /// The number of glyphs of the source font.
    pub(crate) source_glyphs: u32,
    pub(crate) layout: PlanLayout,
    /// The glyphs after the cmap closure (`_glyphset_cmaped`).
    pub(crate) glyphset_cmaped: RangeSet,
    /// The glyphs after the MATH closure (`_glyphset_mathed`).
    pub(crate) glyphset_mathed: RangeSet,
    /// Glyphs after recursive VARC component closure.
    pub(crate) glyphset_varced: RangeSet,
    pub(crate) color: ColorPlan,
    /// The preprocessed data the plan was made with (`accelerator`).
    pub(crate) prepared: Option<Prepared>,
}

/// The layout part of the plan (the `gsub_*`, `gpos_*`,
/// `_glyphset_gsub`, `glyph_map_gsub` and `used_mark_sets_map` members of
/// `hb_subset_plan_t`).
#[derive(Clone, Debug, Default)]
pub(crate) struct PlanLayout {
    /// The glyphs after the GSUB closure (`_glyphset_gsub`).
    pub glyphset_gsub: BitSet,
    /// Old to new ids of `glyphset_gsub`, [`INVALID`] elsewhere
    /// (`glyph_map_gsub_flat`).
    pub gsub_map_flat: Vec<u32>,
    pub gsub: LayoutPlan,
    pub gpos: LayoutPlan,
    /// Old mark glyph set index to new (`used_mark_sets_map`).
    pub used_mark_sets: BTreeMap<u32, u32>,
    pub layout_features: RangeSet,
    pub layout_scripts: RangeSet,
    /// Every axis is pinned and GDEF has a variation store: its
    /// variation indices map to `HB_OT_LAYOUT_NO_VARIATIONS_INDEX`.
    pub varidx_pinned: bool,
    /// Rounded GDEF variation deltas at a fully pinned location.
    pub pinned_layout_deltas: BTreeMap<u32, i32>,
    pub pinned_base_deltas: BTreeMap<u32, i32>,
    /// Used rows and remapped indices of an unpinned GDEF store, shared
    /// by GDEF caret values and retained GPOS PairPos records.
    pub instanced_gdef_store: Option<Vec<u8>>,
    pub gdef_var_items: Vec<(u16, Vec<u32>)>,
    pub gdef_var_map: BTreeMap<u32, u32>,
    /// Condition-specific private rows and unrounded sign-preserving defaults.
    pub gdef_condition_map: BTreeMap<(u32, i16), (u32, i16)>,
}

/// The checks of the layout tables this version cannot write yet.
fn layout_unsupported(
    font: &Font<'_>,
    drop: &RangeSet,
    no_subset: &RangeSet,
    pinned: bool,
) -> Result<(), SubsetError> {
    let kept = |t: &[u8; 4]| {
        let tag = Tag::new(t);
        font.table(tag)
            .filter(|_| !drop.contains(tag.to_u32()) && !no_subset.contains(tag.to_u32()))
    };
    for t in [b"GSUB", b"GPOS"] {
        if kept(t).is_some_and(|d| sofel_font::read::be::u32_at(d, 0).unwrap_or(0) >= 0x0001_0002) {
            return Err(SubsetError::Unsupported {
                tag: Tag::new(t),
                reason: "GSUB and GPOS 1.2 (ADR 0022 stage 3)",
            });
        }
    }
    // Pinning every axis drops the GDEF variation store.
    let retained = |t: &[u8; 4]| {
        let tag = Tag::new(t);
        font.table(tag).is_some() && !drop.contains(tag.to_u32())
    };
    let gsub_may_reference_store = retained(b"GSUB")
        && font.table(Tag::new(b"GSUB")).is_some_and(|d| {
            let table = layout::Gsubgpos::new(d, true);
            // The GSUB writer only emits format-1 axis ranges, which never
            // reference GDEF. A copied GSUB bypasses that writer.
            table.version() >= 0x0001_0001
                && sofel_font::read::be::u32_at(d, 10) != Some(0)
                && (table.feature_variations_view().is_none()
                    || (no_subset.contains(Tag::new(b"GSUB").to_u32())
                        && !table.has_only_axis_range_conditions()))
        });
    // A VariationIndex Device necessarily contains the format word 0x8000.
    // Scan the whole retained table so an unknown or malformed subtable
    // cannot make a real reference look absent. Other 0x8000 words may
    // conservatively keep the table unsupported.
    let may_reference_gdef_store = |d: &[u8]| d.windows(2).any(|word| word == [0x80, 0x00]);
    let gpos_may_reference_store = retained(b"GPOS")
        && font
            .table(Tag::new(b"GPOS"))
            .is_some_and(may_reference_gdef_store);
    let copied_gpos_may_reference_store =
        gpos_may_reference_store && no_subset.contains(Tag::new(b"GPOS").to_u32());
    if !pinned
        && kept(b"GDEF").is_some_and(layout::gdef::has_var_store)
        && (copied_gpos_may_reference_store || gsub_may_reference_store)
    {
        return Err(SubsetError::Unsupported {
            tag: Tag::new(b"GDEF"),
            reason: "the GDEF item variation store (ADR 0022 stage 10)",
        });
    }
    Ok(())
}

impl Plan {
    /// Computes the closure and the glyph mapping
    /// (`hb_subset_plan_create_or_fail`).
    ///
    /// # Errors
    ///
    /// [`SubsetError::NoGlyphs`] for a font without glyphs,
    /// [`SubsetError::InvalidInput`] for a glyph mapping HarfBuzz refuses,
    /// [`SubsetError::Limit`] when the glyf closure exceeds HarfBuzz's
    /// budget, and [`SubsetError::Unsupported`] for tables and settings
    /// not subset yet.
    pub fn new(font: &Font<'_>, input: &Input) -> Result<Plan, SubsetError> {
        Plan::build(font, input, None)
    }

    /// [`Plan::new`] with the data `prepared` holds for `font`
    /// (`hb_subset_plan_create_or_fail` on a face from
    /// `hb_subset_preprocess`): the same plan, made faster, whose
    /// [`Plan::execute`] also uses `prepared` and gives the same bytes.
    ///
    /// # Errors
    ///
    /// As [`Plan::new`], and [`SubsetError::InvalidInput`] when
    /// `prepared` was made from a font with another glyph count or other
    /// lengths of the cmap, DMAP or CFF table.
    pub fn with_prepared(
        font: &Font<'_>,
        prepared: &Prepared,
        input: &Input,
    ) -> Result<Plan, SubsetError> {
        if prepared.accel().key != Key::of(font) {
            return Err(SubsetError::InvalidInput);
        }
        Plan::build(font, input, Some(prepared))
    }

    fn build(
        font: &Font<'_>,
        input: &Input,
        prepared: Option<&Prepared>,
    ) -> Result<Plan, SubsetError> {
        let num_glyphs = font.num_glyphs();
        if num_glyphs == 0 {
            return Err(SubsetError::NoGlyphs);
        }
        let flags = input.flags();
        let drop = input.set(SetKind::DropTables);
        // normalize_axes_location
        let mut norm = instance::normalize(font, input.axes());
        let other_variation_tags = [
            b"gvar", b"GVAR", b"HVAR", b"VVAR", b"MVAR", b"cvar", b"CFF2", b"GDEF", b"GSUB",
            b"GPOS", b"BASE", b"MATH", b"COLR", b"STAT",
        ];
        // A gvar/GVAR with no shared tuples and no glyph records has no
        // deltas to rebase. Its axis range can change as metadata alone.
        let inert_gvar = [b"gvar", b"GVAR"].iter().all(|tag| {
            font.table(Tag::new(tag)).is_none_or(|data| {
                sofel_font::read::gvar::Gvar::new(data, Tag::new(tag)).is_ok_and(|table| {
                    table.shared_tuple_count() == 0
                        && (0..=table.glyph_count()).all(|gid| table.glyph_offset(gid) == Some(0))
                })
            })
        });
        let identity_avar2 = norm.avar2
            && font.table(Tag::new(b"CFF2")).is_none()
            && (norm.user.len() == 1 || norm.user.values().all(|r| r.min == r.max))
            && font.table(Tag::new(b"avar")).is_some_and(|data| {
                crate::tables::avar::is_empty_identity_v2(data, &norm.axis_tags, |tag| {
                    norm.user.values().all(|r| r.min == r.max) || norm.user.contains_key(&tag)
                })
            });
        if norm.avar2 && !norm.all_axes_pinned && !identity_avar2 {
            norm.old_intermediates = norm.axes_location.clone();
            let ranges = crate::tables::avar::avar2_reachable_ranges(
                font,
                &norm,
                font.table(Tag::new(b"CFF2")).is_none(),
            )?;
            norm.avar2_reachable = ranges.reachable;
            norm.avar2_self_contained = ranges.self_contained;
            if font.table(Tag::new(b"VARC")).is_some() && !drop.contains(Tag::new(b"VARC").to_u32())
            {
                norm.avar2_reachable.clear();
            }
            norm.retained_axis_indices = norm
                .axis_tags
                .iter()
                .enumerate()
                .filter_map(|(i, tag)| (!norm.avar2_self_contained.contains_key(tag)).then_some(i))
                .collect();
            norm.axes_location = norm
                .avar2_self_contained
                .iter()
                .map(|(&tag, &coord)| {
                    (
                        tag,
                        crate::input::AxisRange {
                            min: coord,
                            default: coord,
                            max: coord,
                        },
                    )
                })
                .collect();
            norm.normalized_coords = if norm.avar2_self_contained.is_empty() {
                Vec::new()
            } else {
                norm.axis_tags
                    .iter()
                    .map(|tag| {
                        norm.avar2_self_contained.get(tag).map_or(0, |&value| {
                            crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(
                                value * 16384.0,
                            ))
                        })
                    })
                    .collect()
            };
            norm.pinned_at_default = norm.avar2_self_contained.values().all(|&v| v == 0.0);
        }
        let avar2_range = norm.avar2
            && !norm.all_axes_pinned
            && norm.avar2_self_contained.is_empty()
            && (!identity_avar2 || norm.user.values().any(|r| r.min < r.max));
        let point_pins = !norm.user.is_empty() && norm.user.values().all(|r| r.min == r.max);
        // During a complete glyf instance, HarfBuzz writes hmtx/vmtx from
        // the four instanced phantom points and drops HVAR/VVAR.
        let pinned_glyf = norm.all_axes_pinned
            && font.table(Tag::new(b"glyf")).is_some()
            && !drop.contains(Tag::new(b"glyf").to_u32())
            && !input
                .set(SetKind::NoSubsetTables)
                .contains(Tag::new(b"glyf").to_u32())
            && font.table(Tag::new(b"CFF2")).is_none();
        let needs_rebase = |tag: &&[u8; 4]| {
            if avar2_range {
                // avar2 maps the restricted user range into the original variation space.
                return false;
            }
            let handled = match *tag {
                b"GSUB" | b"GPOS" | b"GDEF" | b"BASE" | b"COLR" => {
                    let static_table = font.table(Tag::new(tag)).is_some_and(|data| match *tag {
                        b"GSUB" | b"GPOS" => {
                            can_instance_layout_conditions(data, &norm.axis_tags, norm.avar2)
                                || layout::Gsubgpos::new(data, *tag == b"GSUB")
                                    .feature_variations_view()
                                    .is_some_and(|fv| fv.version() == 0x0001_0001)
                        }
                        b"BASE" => sofel_font::read::base::Base::new(data).is_ok_and(|table| {
                            table.item_var_store_offset().unwrap_or(0) == 0
                                || (!norm.avar2 && table.variation_store().is_ok())
                        }),
                        b"COLR" => !norm.avar2 && sofel_font::read::colr::Colr::new(data).is_ok(),
                        b"GDEF" => sofel_font::read::gdef::Gdef::new(data).is_ok_and(|t| {
                            t.item_var_store_offset().unwrap_or(0) == 0
                                || t.variation_store().is_ok()
                        }),
                        _ => false,
                    });
                    (norm.all_axes_pinned || static_table)
                        && !input
                            .set(SetKind::NoSubsetTables)
                            .contains(Tag::new(tag).to_u32())
                }
                b"HVAR" | b"VVAR" => {
                    pinned_glyf
                        || (!norm.all_axes_pinned
                            && (!norm.avar2 || !norm.avar2_self_contained.is_empty())
                            && font.table(Tag::new(tag)).is_some_and(|data| {
                                sofel_font::read::be::u32_at(data, 8).is_some()
                            })
                            && !input
                                .set(SetKind::NoSubsetTables)
                                .contains(Tag::new(tag).to_u32()))
                }
                b"cvar" => {
                    norm.all_axes_pinned || !norm.avar2 || !norm.avar2_self_contained.is_empty()
                }
                b"MVAR" => {
                    norm.all_axes_pinned
                        || ((!norm.avar2 || !norm.avar2_self_contained.is_empty())
                            && !input
                                .set(SetKind::NoSubsetTables)
                                .contains(Tag::new(tag).to_u32())
                            && font.table(Tag::new(tag)).is_some_and(|data| {
                                sofel_font::read::mvar::Mvar::new(data).is_ok()
                            }))
                }
                b"gvar" => {
                    !norm.avar2
                        || inert_gvar
                        || avar2_range
                        || point_pins
                        || pinned_glyf
                        || !norm.avar2_self_contained.is_empty()
                }
                b"GVAR" => inert_gvar,
                _ => false,
            };
            !handled && font.table(Tag::new(tag)).is_some()
        };
        let no_other_varc_rebase = !other_variation_tags.iter().any(|tag| {
            needs_rebase(tag) && (**tag != *b"gvar" || !drop.contains(Tag::new(tag).to_u32()))
        });
        let varc_unrelated_axis_change = if norm.user.len() == 1
            && (norm.pinned_at_default || no_other_varc_rebase)
        {
            let axes = instance::axis_infos(font);
            axes.iter().enumerate().any(|(index, info)| {
                norm.user.get(&info.tag).is_some_and(|range| {
                    no_other_varc_rebase
                        || (range.min == range.default && range.default == range.max)
                }) && u16::try_from(index).ok().is_some_and(|index| {
                    font.table(Tag::new(b"VARC"))
                        .is_some_and(|data| crate::tables::varc_complex::axis_unused(data, index))
                })
            })
        } else {
            false
        };
        if !norm.user.is_empty() {
            if norm.avar2
                && !norm.all_axes_pinned
                && !identity_avar2
                && !avar2_range
                && norm.avar2_self_contained.is_empty()
            {
                return Err(SubsetError::Unsupported {
                    tag: Tag::new(b"avar"),
                    reason: "pinning the axes of avar version 2 (deferred by ADR 0022)",
                });
            }
            if avar2_range {
                // An avar2 restriction stays in the output mapping. Only
                // self-contained pins apply deltas to the source outlines.
                norm.normalized_coords.clear();
            }
            let varc_tag = Tag::new(b"VARC");
            let varc_retained = font.table(varc_tag).is_some() && !drop.contains(varc_tag.to_u32());
            let no_other_variation_data = !other_variation_tags.iter().any(|tag| {
                if *tag == b"STAT" && !varc_retained {
                    return false;
                }
                if !needs_rebase(tag) {
                    return false;
                }
                let tag = Tag::new(tag);
                !drop.contains(tag.to_u32())
            });
            let simple_avar = (point_pins && drop.contains(Tag::new(b"avar").to_u32()))
                || norm.avar2
                || (norm.avar2 && norm.all_axes_pinned)
                || font.table(Tag::new(b"avar")).is_none_or(|data| {
                    sofel_font::read::avar::Avar::new(data).is_ok_and(|table| {
                        table.major_version() == 1
                            && table.axis_count() == norm.axis_tags.len()
                            && (0..table.axis_count()).all(|axis| {
                                table
                                    .segment_map(axis)
                                    .is_some_and(|map| sofel_font::var::map_float(&map, 0.0) == 0.0)
                            })
                    })
                });
            let metadata_only = no_other_variation_data && simple_avar;
            let gvar_full_pin = norm.all_axes_pinned
                && font.table(Tag::new(b"gvar")).is_some()
                && font.table(Tag::new(b"GVAR")).is_none()
                && font.table(Tag::new(b"glyf")).is_some()
                && font.table(Tag::new(b"avar")).is_none()
                && !other_variation_tags.iter().any(|tag| {
                    **tag != *b"gvar" && **tag != *b"GVAR" && font.table(Tag::new(tag)).is_some()
                });
            let kept = |tag: &[u8; 4]| {
                let tag = Tag::new(tag);
                font.table(tag).filter(|_| !drop.contains(tag.to_u32()))
            };
            let cff2_instance = (norm.all_axes_pinned
                || (!norm.avar2
                    && simple_avar
                    && other_variation_tags
                        .iter()
                        .filter(|tag| **tag != b"CFF2" && **tag != b"STAT")
                        .all(|tag| kept(tag).is_none() || !needs_rebase(tag))))
                && kept(b"CFF2").is_some()
                && [b"glyf", b"GLYF", b"gvar", b"GVAR", b"cvar", b"VARC"]
                    .iter()
                    .all(|t| kept(t).is_none());
            if !(norm.all_axes_pinned && norm.pinned_at_default)
                && !metadata_only
                && !gvar_full_pin
                && !cff2_instance
            {
                return Err(SubsetError::Unsupported {
                    tag: Tag::new(b"fvar"),
                    reason: "instancing variation data needs ADR 0061",
                });
            }
        }
        let pinned = norm.all_axes_pinned;
        let no_subset = input.set(SetKind::NoSubsetTables);
        let varc_tag = Tag::new(b"VARC");
        if font.table(varc_tag).is_some()
            && !drop.contains(varc_tag.to_u32())
            && no_subset.contains(varc_tag.to_u32())
            && (!flags.has(Flags::RETAIN_GIDS) || !norm.user.is_empty())
        {
            return Err(SubsetError::Unsupported {
                tag: varc_tag,
                reason: "VARC passthrough needs retained glyph IDs and unchanged axes",
            });
        }
        for (tag, reason) in PENDING {
            let t = Tag::new(tag);
            let passthrough = no_subset.contains(t.to_u32());
            let dropped = drop.contains(t.to_u32()) || (pinned && PINNED_DROP.contains(&tag));
            if font.table(t).is_some() && !dropped && !passthrough {
                if tag == b"VARC" {
                    let source = font.table(t).unwrap_or(&[]);
                    Varc::new(source).map_err(|_| SubsetError::Malformed { tag: t })?;
                    continue;
                }
                return Err(SubsetError::Unsupported { tag: t, reason });
            }
        }

        let full_instance = pinned && !norm.normalized_coords.is_empty();
        layout_unsupported(font, drop, no_subset, pinned)?;
        let colr_table = font.table(Tag::new(b"COLR"));
        let colr_kept = !drop.contains(Tag::new(b"COLR").to_u32())
            && !no_subset.contains(Tag::new(b"COLR").to_u32());
        let cmap = CmapAccel::new(font);
        let glyphs_in = input.set(SetKind::Glyphs);
        let mut p = Plan {
            flags,
            num_output_glyphs: 0,
            glyphs: GlyphMap::default(),
            unicodes: RangeSet::new(),
            unicode_to_new_gid: Vec::new(),
            codepoint_to_glyph: BTreeMap::new(),
            glyphset: RangeSet::new(),
            glyphs_requested: glyphs_in.clone(),
            name_overrides: input.name_overrides().clone(),
            name_ids: input.set(SetKind::NameIds).clone(),
            name_languages: input.set(SetKind::NameLanguages).clone(),
            drop_tables: drop.clone(),
            no_subset_tables: no_subset.clone(),
            os2_min_cp: INVALID,
            os2_max_cp: INVALID,
            force_long_loca: flags.has(Flags::IFTB_REQUIREMENTS),
            var: VarPlan {
                n: norm,
                ..VarPlan::default()
            },
            inert_gvar,
            source_glyphs: num_glyphs,
            layout: PlanLayout {
                layout_features: input.set(SetKind::LayoutFeatures).clone(),
                layout_scripts: input.set(SetKind::LayoutScripts).clone(),
                ..PlanLayout::default()
            },
            glyphset_cmaped: RangeSet::new(),
            glyphset_mathed: RangeSet::new(),
            glyphset_varced: RangeSet::new(),
            color: ColorPlan::default(),
            prepared: prepared.cloned(),
        };

        // _populate_unicodes_to_retain
        let unicodes = unicode_closure(
            input.set(SetKind::Unicodes),
            !flags.has(Flags::NO_BIDI_CLOSURE),
        );
        let u = populate_unicodes(
            &cmap,
            &unicodes,
            glyphs_in,
            num_glyphs,
            prepared.map(Prepared::accel),
        );
        let mut glyphset_gsub = u.glyphset_gsub;
        let list = u.list;
        p.codepoint_to_glyph = u.codepoint_to_glyph;
        p.unicodes = u.unicodes;
        p.os2_min_cp = u.os2_min_cp;
        p.os2_max_cp = u.os2_max_cp;

        // _populate_gids_to_retain
        glyphset_gsub.add(0);
        cmap.closure_glyphs(&p.unicodes, &mut glyphset_gsub);
        p.glyphset_cmaped = glyphset_gsub.clone();
        let tag_kept = |t: &[u8; 4]| !drop.contains(Tag::new(t).to_u32());
        if tag_kept(b"MATH") {
            let math = font.table(Tag::new(b"MATH")).unwrap_or(&[]);
            crate::tables::math::closure(math, &mut glyphset_gsub);
            glyphset_gsub.truncate(num_glyphs);
        }
        p.glyphset_mathed = glyphset_gsub.clone();
        // layout_populate_gids_to_retain
        let mut gsub_glyphs = BitSet::from_range_set(&glyphset_gsub);
        let table =
            |t: &[u8; 4], gsub: bool| Gsubgpos::new(font.table(Tag::new(t)).unwrap_or(&[]), gsub);
        let no_closure = flags.has(Flags::NO_LAYOUT_CLOSURE);
        let mut condition_deltas = BTreeMap::new();
        if !p.var.n.normalized_coords.is_empty() {
            let location =
                sofel_font::var::Location::from_normalized_2_14(font, &p.var.n.normalized_coords);
            let instance = font.instance(&location);
            for (tag, is_gsub) in [(b"GSUB", true), (b"GPOS", false)] {
                if !tag_kept(tag) {
                    continue;
                }
                let Some(data) = font.table(Tag::new(tag)) else {
                    continue;
                };
                let t = layout::Gsubgpos::new(data, is_gsub);
                let all = (0..t.feature_count())
                    .map(|i| (u32::try_from(i).unwrap_or(u32::MAX), 0))
                    .collect();
                for (index, _) in layout::plan::feature_variation_condition_values(t, &all) {
                    let outer = u16::try_from(index >> 16).map_err(|_| SubsetError::Limit {
                        tag: Tag::new(b"GDEF"),
                    })?;
                    let inner = u16::try_from(index & 0xffff).map_err(|_| SubsetError::Limit {
                        tag: Tag::new(b"GDEF"),
                    })?;
                    let delta = instance
                        .item_delta::<sofel_font::var::HbF64>(
                            sofel_font::var::StoreKind::Gdef,
                            outer,
                            inner,
                        )
                        .map_err(|_| SubsetError::Malformed {
                            tag: Tag::new(b"GDEF"),
                        })?;
                    condition_deltas.insert(index, delta);
                }
            }
        }
        for (tag, is_gsub, lp) in [
            (b"GSUB", true, &mut p.layout.gsub),
            (b"GPOS", false, &mut p.layout.gpos),
        ] {
            if !tag_kept(tag) {
                continue;
            }
            let t = table(tag, is_gsub);
            let fixed = pinned
                || (!p.var.n.normalized_coords.is_empty()
                    && t.feature_variations_view().is_some()
                    && layout_conditions_match(t.d.0, &p.var.n.axis_tags, |axis, min, max| {
                        condition_is_constant(p.var.n.axes_location.get(axis), min, max)
                    }));
            *lp = layout::plan::closure_glyphs_lookups_features(
                t,
                num_glyphs,
                no_closure,
                &p.layout.layout_features,
                &p.layout.layout_scripts,
                &mut gsub_glyphs,
                layout::plan::LayoutLocation {
                    coords: &p.var.n.normalized_coords,
                    fixed,
                    retained_conditions: if !fixed && !p.var.n.normalized_coords.is_empty() {
                        t.feature_variations_view()
                            .and_then(|fv| layout::plan::retained_conditions(fv, &p.var.n))
                    } else {
                        None
                    },
                    condition_deltas: &condition_deltas,
                },
            );
            if fixed && !pinned {
                lp.universal_record = layout::plan::applied_record(
                    t.feature_variations_view(),
                    &p.var.n.normalized_coords,
                )
                .ok()
                .flatten();
            }
        }
        for (t, lp) in [(b"GSUB", &p.layout.gsub), (b"GPOS", &p.layout.gpos)] {
            if let Some(reason) = lp.unsupported {
                return Err(SubsetError::Unsupported {
                    tag: Tag::new(t),
                    reason,
                });
            }
        }
        gsub_glyphs.truncate(num_glyphs);
        let mut cur = gsub_glyphs.to_range_set();
        // _colr_closure
        if tag_kept(b"COLR") {
            p.color = colr::closure(
                colr_table.unwrap_or(&[]),
                &mut cur,
                full_instance,
                colr_kept,
                (&p, font),
                num_glyphs,
            )?;
        }
        p.color.glyphset_colred = cur.clone();
        if tag_kept(b"VARC")
            && let Some(data) = font.table(Tag::new(b"VARC"))
        {
            let varc = Varc::new(data).map_err(|_| SubsetError::Malformed {
                tag: Tag::new(b"VARC"),
            })?;
            let mut pending: Vec<u32> = cur.iter().collect();
            let mut next = 0usize;
            while let Some(&glyph) = pending.get(next) {
                next = next.saturating_add(1);
                if let Some(components) = varc.components(sofel_font::read::GlyphId(glyph)) {
                    for component in components {
                        let component = component.ok_or(SubsetError::Malformed {
                            tag: Tag::new(b"VARC"),
                        })?;
                        let child = component.glyph.0;
                        if !cur.contains(child) {
                            cur.add(child);
                            pending.push(child);
                        }
                    }
                }
            }
        }
        if !p.var.n.user.is_empty()
            && !varc_unrelated_axis_change
            && tag_kept(b"VARC")
            && font.table(Tag::new(b"VARC")).is_some_and(|data| {
                Varc::new(data).is_ok_and(|varc| {
                    cur.iter()
                        .any(|glyph| varc.components(sofel_font::read::GlyphId(glyph)).is_some())
                })
            })
        {
            return Err(SubsetError::Unsupported {
                tag: Tag::new(b"fvar"),
                reason: "instancing variation data needs ADR 0061",
            });
        }
        p.glyphset_varced = cur.clone();
        // _nameid_closure
        if tag_kept(b"STAT") {
            stat_name_ids(
                font.table(Tag::new(b"STAT")).unwrap_or(&[]),
                p.var.user(),
                &mut p.name_ids,
            );
        }
        if !pinned {
            fvar_name_ids(
                font.table(Tag::new(b"fvar")).unwrap_or(&[]),
                p.var.user(),
                &mut p.name_ids,
            );
        }
        if tag_kept(b"CPAL") {
            cpal::collect_name_ids(
                font.table(Tag::new(b"CPAL")).unwrap_or(&[]),
                &p.color.palettes,
                &mut p.name_ids,
            );
        }
        if tag_kept(b"GPOS") {
            layout::plan::collect_name_ids(
                table(b"GPOS", false),
                &p.layout.gpos,
                pinned,
                &mut p.name_ids,
            );
        }
        if tag_kept(b"GSUB") {
            layout::plan::collect_name_ids(
                table(b"GSUB", true),
                &p.layout.gsub,
                pinned,
                &mut p.name_ids,
            );
        }
        p.layout.glyphset_gsub = gsub_glyphs;
        let glyf = GlyfAccel::new(font);
        if glyf.has_data() {
            let mut budget: i64 = i64::try_from(glyf.table_len())
                .unwrap_or(i64::MAX)
                .saturating_mul(limits::SUBSET_GLYF_MAX_BYTES_FACTOR);
            let ops = i64::try_from(cur.len())
                .unwrap_or(i64::MAX)
                .saturating_mul(limits::MAX_COMPOSITE_OPERATIONS_PER_GLYPH);
            for gid in cur.iter() {
                add_gid_and_children(&glyf, gid, &mut p.glyphset, ops, &mut budget, 0);
            }
            if budget < 0 {
                return Err(SubsetError::Limit {
                    tag: Tag::new(b"glyf"),
                });
            }
        } else {
            p.glyphset.union(&cur);
        }
        // _add_cff_seac_components: nothing to add when every glyph is
        // kept already, or when the prepared value found no seac.
        if cur.len() != u64::from(num_glyphs)
            && prepared.is_none_or(|p| p.accel().has_seac)
            && let Some(cff) = SeacFinder::new(font)
        {
            for (base, accent) in cur.iter().filter_map(|g| cff.components(g)) {
                p.glyphset.add(base);
                p.glyphset.add(accent);
            }
        }
        p.glyphset.truncate(num_glyphs);

        // _create_old_gid_to_new_gid_map
        let (new_to_old, n) = glyph_map(
            &p.glyphset,
            input.glyph_mapping(),
            flags.has(Flags::RETAIN_GIDS),
        )?;
        p.num_output_glyphs =
            if flags.has(Flags::RETAIN_GIDS) && flags.has(Flags::RETAIN_NUM_GLYPHS) {
                num_glyphs
            } else {
                n
            };
        if new_to_old.iter().any(|&(new, _)| new > 0xFFFF) {
            for name in [b"GSUB", b"GPOS"] {
                let tag = Tag::new(name);
                if drop.contains(tag.to_u32()) || no_subset.contains(tag.to_u32()) {
                    continue;
                }
                let Some(data) = font.table(tag) else {
                    continue;
                };
                let gsub = name == b"GSUB";
                let table = layout::Gsubgpos::new(data, gsub);
                let plan = if gsub { &p.layout.gsub } else { &p.layout.gpos };
                let supported = plan.lookups.keys().all(|&i| {
                    layout::subtables(table.lookup(i as usize), gsub)
                        .iter()
                        .all(|(kind, _)| {
                            matches!(kind, layout::Kind::SingleSubst | layout::Kind::SinglePos)
                        })
                });
                if !supported {
                    return Err(SubsetError::Unsupported {
                        tag,
                        reason: "layout subsetting with retained glyph IDs above 65535",
                    });
                }
            }
        }
        p.glyphs = GlyphMap::new(new_to_old, num_glyphs);
        let cff2 = Tag::new(b"CFF2");
        if font.table(cff2).is_some()
            && !drop.contains(cff2.to_u32())
            && !no_subset.contains(cff2.to_u32())
            && !full_instance
            && !flags.has(Flags::DESUBROUTINIZE)
            && crate::tables::cff::uses_subroutines(font, p.glyphs.new_to_old.as_slice())
            && !crate::tables::cff::supports_subroutines(
                font,
                p.glyphs.new_to_old.as_slice(),
                p.num_output_glyphs,
                flags.has(Flags::NO_HINTING),
            )
        {
            return Err(SubsetError::Unsupported {
                tag: cff2,
                reason: "CFF2 subroutines used by retained glyphs (ADR 0022 stage 10)",
            });
        }
        // _create_glyph_map_gsub
        let flat = match p.layout.glyphset_gsub.max() {
            INVALID => 0,
            m => m.saturating_add(1) as usize,
        };
        let mut gsub_map = alloc::vec![INVALID; flat];
        for g in p.layout.glyphset_gsub.iter() {
            if let Some(slot) = gsub_map.get_mut(g as usize) {
                *slot = p.glyphs.new_gid(g).unwrap_or(INVALID);
            }
        }
        p.layout.gsub_map_flat = gsub_map;
        p.unicode_to_new_gid = list
            .into_iter()
            .map(|(cp, g)| (cp, p.glyphs.new_gid(g).unwrap_or(INVALID)))
            .collect();
        // collect_layout_variation_indices: when pinned, the variation
        // indices of a GDEF store all map to "no variations".
        p.layout.varidx_pinned = pinned
            && tag_kept(b"GDEF")
            && font
                .table(Tag::new(b"GDEF"))
                .is_some_and(layout::gdef::has_var_store);
        if tag_kept(b"GDEF")
            && let Some(gdef) = font.table(Tag::new(b"GDEF"))
        {
            p.layout.used_mark_sets = layout::gdef::used_mark_sets(gdef, &p.layout.glyphset_gsub);
            let copied_gdef = no_subset.contains(Tag::new(b"GDEF").to_u32());
            let gpos_data = font.table(Tag::new(b"GPOS")).unwrap_or(&[]);
            let gpos_has_var_format = tag_kept(b"GPOS")
                && !no_subset.contains(Tag::new(b"GPOS").to_u32())
                && gpos_data.windows(2).any(|word| word == [0x80, 0x00]);
            let math_data = font.table(Tag::new(b"MATH")).unwrap_or(&[]);
            let copied_math = no_subset.contains(Tag::new(b"MATH").to_u32());
            let math_has_var_format = tag_kept(b"MATH")
                && (!copied_gdef || !copied_math)
                && math_data.windows(2).any(|word| word == [0x80, 0x00]);
            if (!pinned || !p.var.n.pinned_at_default)
                && layout::gdef::has_var_store(gdef)
                && (!copied_gdef || gpos_has_var_format || math_has_var_format)
            {
                let mut indices = if gpos_has_var_format {
                    layout::gpos::retained_variation_indices(
                        gpos_data,
                        &p.layout.gpos,
                        &p.layout.glyphset_gsub,
                        num_glyphs,
                    )
                } else {
                    Some(BTreeSet::new())
                };
                let Some(ref mut selected) = indices else {
                    return Err(SubsetError::Unsupported {
                        tag: Tag::new(b"GDEF"),
                        reason: "the GDEF item variation store (ADR 0022 stage 10)",
                    });
                };
                for (tag, layout) in [
                    (Tag::new(b"GSUB"), &p.layout.gsub),
                    (Tag::new(b"GPOS"), &p.layout.gpos),
                ] {
                    if tag_kept(&tag.0)
                        && !no_subset.contains(tag.to_u32())
                        && let Some(data) = font.table(tag)
                    {
                        selected.extend(layout::plan::feature_variation_var_indices(
                            layout::Gsubgpos::new(data, tag == Tag::new(b"GSUB")),
                            &layout.features_w_dups,
                        ));
                    }
                }
                if math_has_var_format {
                    let Some(math_indices) = crate::tables::math::retained_variation_indices(
                        math_data,
                        &p.glyphset_mathed,
                        &p.glyphset_cmaped,
                    ) else {
                        return Err(SubsetError::Unsupported {
                            tag: Tag::new(b"GDEF"),
                            reason: "unsupported MATH VariationIndex (ADR 0022 stage 10)",
                        });
                    };
                    selected.extend(math_indices);
                }
                let Some((items, map)) = layout::gdef::retained_variation_indices(
                    gdef,
                    &p.layout.glyphset_gsub,
                    selected,
                ) else {
                    return Err(SubsetError::Unsupported {
                        tag: Tag::new(b"GDEF"),
                        reason: "the GDEF item variation store (ADR 0022 stage 10)",
                    });
                };
                if copied_math && !copied_gdef && math_has_var_format {
                    // Even records for glyphs outside the subset survive in a
                    // copied MATH. Every original index must still address its row.
                    let all = RangeSet::all();
                    let all_indices =
                        crate::tables::math::retained_variation_indices(math_data, &all, &all);
                    if !all_indices.is_some_and(|indices| {
                        indices.iter().all(|index| map.get(index) == Some(index))
                    }) {
                        return Err(SubsetError::Unsupported {
                            tag: Tag::new(b"GDEF"),
                            reason: "copied MATH needs unchanged GDEF variation indices",
                        });
                    }
                }
                if pinned {
                    p.layout.pinned_layout_deltas = p.pinned_deltas(
                        font,
                        Tag::new(b"GDEF"),
                        sofel_font::var::StoreKind::Gdef,
                        &map,
                    )?;
                } else if copied_gdef {
                    // HarfBuzz remaps subset GPOS and MATH references even when it
                    // copies the original GDEF table unchanged.
                    p.layout.gdef_var_map = map;
                } else if p.var.instancing() {
                    let tag = Tag::new(b"GDEF");
                    let store = sofel_font::read::gdef::Gdef::new(gdef)
                        .ok()
                        .and_then(|table| table.variation_store().ok().flatten())
                        .ok_or(SubsetError::Malformed { tag })?;
                    // HarfBuzz omits the inner maps when no layout record uses
                    // a variation index. Instancing then processes every row.
                    let items = if items.is_empty() {
                        (0..store.data_count())
                            .map(|outer| {
                                let data = store
                                    .item_data(outer)
                                    .ok_or(SubsetError::Malformed { tag })?;
                                Ok((outer, (0..u32::from(data.item_count)).collect()))
                            })
                            .collect::<Result<Vec<_>, SubsetError>>()?
                    } else {
                        items
                    };
                    let mut projected = BTreeMap::new();
                    let location = sofel_font::var::Location::from_normalized_2_14(
                        font,
                        &p.var.n.normalized_coords,
                    );
                    let instance = font.instance(&location);
                    let mut conditions = Vec::new();
                    for (layout_tag, lp) in [
                        (Tag::new(b"GSUB"), &p.layout.gsub),
                        (Tag::new(b"GPOS"), &p.layout.gpos),
                    ] {
                        if !tag_kept(&layout_tag.0) {
                            continue;
                        }
                        let Some(data) = font.table(layout_tag) else {
                            continue;
                        };
                        let source = layout::Gsubgpos::new(data, layout_tag == Tag::new(b"GSUB"));
                        for (index, default) in layout::plan::feature_variation_condition_values(
                            source,
                            &lp.features_w_dups,
                        ) {
                            let outer = u16::try_from(index >> 16)
                                .map_err(|_| SubsetError::Limit { tag })?;
                            let inner = u16::try_from(index & 0xffff)
                                .map_err(|_| SubsetError::Limit { tag })?;
                            let delta = instance
                                .item_delta::<sofel_font::var::HbF64>(
                                    sofel_font::var::StoreKind::Gdef,
                                    outer,
                                    inner,
                                )
                                .map_err(|_| SubsetError::Malformed { tag })?;
                            conditions.push((index, default, f64::from(default) + delta));
                        }
                    }
                    let mut condition_map = BTreeMap::new();
                    let bytes = crate::tables::hvar::instance::subset_gdef_conditions(
                        &p,
                        font,
                        store,
                        &items,
                        &mut projected,
                        &conditions,
                        &mut condition_map,
                    )?;
                    p.layout.gdef_condition_map = condition_map;
                    p.layout.pinned_layout_deltas =
                        p.pinned_deltas(font, tag, sofel_font::var::StoreKind::Gdef, &map)?;
                    p.layout.gdef_var_map = map
                        .into_iter()
                        .filter_map(|(old, compact)| projected.get(&compact).map(|&new| (old, new)))
                        .collect();
                    p.layout.instanced_gdef_store = Some(bytes);
                } else {
                    p.layout.gdef_var_items = items;
                    p.layout.gdef_var_map = map;
                }
            }
        }
        if p.var.instancing()
            && !p.var.n.pinned_at_default
            && tag_kept(b"BASE")
            && !no_subset.contains(Tag::new(b"BASE").to_u32())
            && let Some(data) = font.table(Tag::new(b"BASE"))
            && sofel_font::read::base::Base::new(data)
                .is_ok_and(|base| base.variation_store().ok().flatten().is_some())
        {
            let tag = Tag::new(b"BASE");
            let (_, indices) = crate::tables::base::retained_variation_indices(data, &p)
                .ok_or(SubsetError::Malformed { tag })?;
            p.layout.pinned_base_deltas =
                p.pinned_deltas(font, tag, sofel_font::var::StoreKind::Base, &indices)?;
        }
        p.instance_metrics(font)?;
        Ok(p)
    }

    fn pinned_deltas(
        &self,
        font: &Font<'_>,
        tag: Tag,
        kind: sofel_font::var::StoreKind,
        indices: &BTreeMap<u32, u32>,
    ) -> Result<BTreeMap<u32, i32>, SubsetError> {
        use sofel_font::var::{HbF32, Location};
        let location = Location::from_normalized_2_14(font, &self.var.n.normalized_coords);
        let instance = font.instance(&location);
        indices
            .keys()
            .map(|&index| {
                let [a, b, c, d] = index.to_be_bytes();
                let delta = instance
                    .item_delta::<HbF32>(
                        kind,
                        u16::from_be_bytes([a, b]),
                        u16::from_be_bytes([c, d]),
                    )
                    .map_err(|_| SubsetError::Malformed { tag })?;
                Ok((
                    index,
                    crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(delta)),
                ))
            })
            .collect()
    }

    /// The number of glyphs of the subset font.
    pub fn num_output_glyphs(&self) -> u32 {
        self.num_output_glyphs
    }

    /// `(old, new)` glyph ids, ascending by old id
    /// (`hb_subset_plan_old_to_new_glyph_mapping`).
    pub fn old_to_new_glyphs(&self) -> Vec<(u32, u32)> {
        let mut v: Vec<(u32, u32)> = self
            .glyphs
            .new_to_old
            .iter()
            .map(|&(n, o)| (o, n))
            .collect();
        v.sort_unstable();
        v
    }

    /// `(new, old)` glyph ids, ascending by new id
    /// (`hb_subset_plan_new_to_old_glyph_mapping`).
    pub fn new_to_old_glyphs(&self) -> &[(u32, u32)] {
        &self.glyphs.new_to_old
    }

    /// The new id of an old glyph.
    pub fn new_glyph(&self, old: u32) -> Option<u32> {
        self.glyphs.new_gid(old)
    }

    /// The old id of a new glyph.
    pub fn old_glyph(&self, new: u32) -> Option<u32> {
        self.glyphs.old_gid(new)
    }

    /// `(character, old glyph)` of the kept characters, ascending
    /// (`hb_subset_plan_unicode_to_old_glyph_mapping`).
    pub fn unicode_to_old_glyphs(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.codepoint_to_glyph.iter().map(|(&c, &g)| (c, g))
    }

    /// The kept glyphs (old ids).
    pub fn glyph_set(&self) -> &RangeSet {
        &self.glyphset
    }

    /// The flags of the input.
    pub fn flags(&self) -> Flags {
        self.flags
    }

    /// Whether the table `tag` is subset (present, not dropped by
    /// `_should_drop_table`, not copied as it is).
    pub(crate) fn subsets_table(&self, font: &Font<'_>, tag: Tag) -> bool {
        font.table(tag).is_some()
            && !self.should_drop(tag)
            && !self.no_subset_tables.contains(tag.to_u32())
    }

    /// The metrics instancing recomputes: from the glyf outlines (which
    /// HarfBuzz computes while writing glyf) and from CFF2
    /// (`update_instance_metrics_map_from_cff2`).
    fn instance_metrics(&mut self, font: &Font<'_>) -> Result<(), SubsetError> {
        let n = self.num_output_glyphs as usize;
        self.var.metrics.bounds_width = alloc::vec![u32::MAX; n];
        self.var.metrics.bounds_height = alloc::vec![u32::MAX; n];
        if !self.var.instancing() {
            return Ok(());
        }
        crate::tables::cff::instance_metrics(
            font,
            &self.glyphs.new_to_old,
            &self.var.n.normalized_coords,
            &mut self.var.metrics,
        );
        let glyf_tag = crate::tables::glyf::instanced_tag(self, font);
        if let Some(tag) = glyf_tag {
            let (glyphs, metrics) = crate::tables::glyf::instance(
                &self.glyphs.new_to_old,
                self.flags,
                self.num_output_glyphs,
                font,
                &self.var.n,
            )
            .ok_or(SubsetError::Failed { tag })?;
            self.var.metrics = metrics;
            self.var.glyf = Some(glyphs);
        }
        Ok(())
    }
}

/// `STAT::collect_name_ids`: the design axes' names, the names of the
/// axis values inside the axis ranges of `user` and the elided fallback
/// name.
fn stat_name_ids(stat: &[u8], user: &BTreeMap<Tag, crate::input::AxisRange>, names: &mut RangeSet) {
    let Ok(stat) = sofel_font::read::stat::Stat::new(stat) else {
        return;
    };
    stat.design_axes()
        .for_each(|a| names.add(u32::from(a.name_id)));
    for i in 0..stat.axis_value_count() {
        let Some((v, _)) = stat.axis_value(i) else {
            continue;
        };
        if crate::tables::stat::keep_axis_value(&stat, &v, user) {
            names.add(u32::from(v.name_id().unwrap_or(0xFFFF)));
        }
    }
    names.add(u32::from(stat.elided_fallback_name_id()));
}

/// `fvar::collect_name_ids` for the axes and named instances that remain.
fn fvar_name_ids(fvar: &[u8], user: &BTreeMap<Tag, AxisRange>, names: &mut RangeSet) {
    let Ok(fvar) = sofel_font::read::fvar::Fvar::new(fvar) else {
        return;
    };
    for axis in fvar.axes() {
        let pinned = user
            .get(&axis.tag)
            .is_some_and(|range| range.min == range.default && range.default == range.max);
        if !pinned {
            names.add(u32::from(axis.name_id));
        }
    }
    for index in 0..fvar.instance_count() {
        if !crate::instance::fvar_instance_kept(&fvar, index, user) {
            continue;
        }
        let Some(instance) = fvar.instance(index) else {
            continue;
        };
        names.add(u32::from(instance.subfamily_name_id));
        if let Some(postscript) = instance.postscript_name_id.filter(|id| *id != 0xFFFF) {
            names.add(u32::from(postscript));
        }
    }
}

/// What `_populate_unicodes_to_retain` collects.
pub(crate) struct Unicodes {
    /// `unicode_to_new_gid_list` (with old glyphs), ascending.
    pub list: Vec<(u32, u32)>,
    pub codepoint_to_glyph: BTreeMap<u32, u32>,
    /// `plan->unicodes`, with the variation selectors.
    pub unicodes: RangeSet,
    /// The glyphs of `list` and the requested glyphs below the count.
    pub glyphset_gsub: RangeSet,
    pub os2_min_cp: u32,
    pub os2_max_cp: u32,
}

/// `_populate_unicodes_to_retain` for the characters `unicodes` (after
/// `_unicode_closure`) and the requested glyphs `glyphs`, with the
/// prepared value's map when there is one (`plan->accelerator`).
pub(crate) fn populate_unicodes(
    cmap: &CmapAccel<'_>,
    unicodes: &RangeSet,
    glyphs: &RangeSet,
    num_glyphs: u32,
    accel: Option<&Accel>,
) -> Unicodes {
    let mut glyphset_gsub = RangeSet::new();
    let mut codepoint_to_glyph = BTreeMap::new();
    let mut list: Vec<(u32, u32)> = Vec::new();
    let mut fill = |cp: u32, gid: u32, list: &mut Vec<(u32, u32)>| {
        if gid != INVALID {
            codepoint_to_glyph.insert(cp, gid);
            list.push((cp, gid));
        }
    };
    if glyphs.is_empty() && unicodes.len() < u64::from(num_glyphs) {
        // The direct lookup, for few characters and no glyphs.
        for cp in unicodes.iter() {
            let gid = match accel {
                Some(a) => a.unicode_to_gid.get(&cp).copied(),
                None => cmap.nominal_glyph(cp),
            };
            fill(cp, gid.unwrap_or(INVALID), &mut list);
        }
    } else {
        let mut storage = (RangeSet::new(), BTreeMap::new());
        let (cmap_unicodes, mapping) = match accel {
            Some(a) => (&a.unicodes, &a.unicode_to_gid),
            None => {
                cmap.collect_mapping(&mut storage.0, &mut storage.1);
                (&storage.0, &storage.1)
            }
        };
        let get = |cp: u32| mapping.get(&cp).copied().unwrap_or(INVALID);
        match accel {
            Some(a)
                if unicodes.len() < cmap_unicodes.len() && glyphs.len() < cmap_unicodes.len() =>
            {
                // The reverse map: the characters of the requested
                // glyphs, then the requested characters not added yet.
                for gid in glyphs.iter() {
                    for &cp in a.gid_to_unicodes.get(&gid).into_iter().flatten() {
                        fill(cp, gid, &mut list);
                    }
                }
                let added: alloc::collections::BTreeSet<u32> = list.iter().map(|p| p.0).collect();
                for cp in unicodes.iter().filter(|cp| !added.contains(cp)) {
                    fill(cp, get(cp), &mut list);
                }
                list.sort_unstable();
            }
            _ => {
                // The walk over the mapping.
                for (a, b) in cmap_unicodes.ranges() {
                    for cp in a..=b {
                        let gid = get(cp);
                        if !unicodes.contains(cp) && !glyphs.contains(gid) {
                            continue;
                        }
                        fill(cp, gid, &mut list);
                    }
                }
            }
        }
        // The requested glyphs that the cmap does not map.
        for (a, b) in glyphs.ranges() {
            if a >= num_glyphs {
                break;
            }
            glyphset_gsub.add_range(a, b.min(num_glyphs.saturating_sub(1)));
        }
    }
    let mut kept = RangeSet::new();
    list.iter().for_each(|&(cp, g)| {
        kept.add(cp);
        glyphset_gsub.add(g);
    });
    let (os2_min_cp, os2_max_cp) = (kept.min(), kept.max());
    // Variation selectors have no glyph but are kept; the OS/2 range is
    // taken before they are added.
    let mut selectors = RangeSet::new();
    cmap.collect_variation_selectors(&mut selectors);
    selectors
        .iter()
        .filter(|s| unicodes.contains(*s))
        .for_each(|s| kept.add(s));
    Unicodes {
        list,
        codepoint_to_glyph,
        unicodes: kept,
        glyphset_gsub,
        os2_min_cp,
        os2_max_cp,
    }
}

/// `_unicode_closure`: the characters and, unless disabled or inverted,
/// their mirrored characters.
pub(crate) fn unicode_closure(unicodes: &RangeSet, bidi: bool) -> RangeSet {
    let mut out = unicodes.clone();
    if !bidi || out.is_inverted() {
        return out;
    }
    for cp in unicodes.iter() {
        let m = sofel_unicode::mirroring(cp);
        if m != cp {
            out.add(m);
        }
    }
    out
}

/// `_glyf_add_gid_and_children`.
fn add_gid_and_children(
    glyf: &GlyfAccel<'_>,
    gid: u32,
    keep: &mut RangeSet,
    ops: i64,
    budget: &mut i64,
    depth: u32,
) -> i64 {
    if keep.contains(gid) {
        return ops;
    }
    keep.add(gid);
    let glyph = glyf.glyph(gid, false);
    *budget = budget.saturating_sub(i64::try_from(glyph.bytes.len()).unwrap_or(i64::MAX));
    if *budget < 0 {
        return ops;
    }
    if depth > limits::MAX_NESTING_LEVEL {
        return ops;
    }
    let mut ops = ops.saturating_sub(1);
    if ops < 0 {
        return ops;
    }
    for c in glyph.components() {
        ops = add_gid_and_children(glyf, c.gid, keep, ops, budget, depth.saturating_add(1));
    }
    ops
}

/// `_create_old_gid_to_new_gid_map`: the `(new, old)` list and the glyph
/// count.
fn glyph_map(
    keep: &RangeSet,
    requested: &BTreeMap<u32, u32>,
    retain: bool,
) -> Result<(Vec<(u32, u32)>, u32), SubsetError> {
    if !requested.is_empty() {
        let values: alloc::collections::BTreeSet<u32> = requested.values().copied().collect();
        if values.len() != requested.len() || retain {
            return Err(SubsetError::InvalidInput);
        }
        let mut list = Vec::new();
        let mut remaining = Vec::new();
        let mut max_glyph = 0u32;
        for old in keep.iter() {
            if old == 0 {
                list.push((0, 0));
                continue;
            }
            match requested.get(&old) {
                None => remaining.push(old),
                Some(0) => return Err(SubsetError::InvalidInput),
                Some(&n) => {
                    max_glyph = max_glyph.max(n);
                    list.push((n, old));
                }
            }
        }
        list.sort_unstable();
        let max_num = (1u32 << 24) - 1;
        if max_glyph >= max_num
            || remaining.len() as u64
                > u64::from(max_num.saturating_sub(max_glyph.saturating_add(1)))
        {
            return Err(SubsetError::InvalidInput);
        }
        for old in remaining {
            max_glyph = max_glyph.saturating_add(1);
            list.push((max_glyph, old));
        }
        return Ok((list, max_glyph.saturating_add(1)));
    }
    if !retain {
        let list: Vec<(u32, u32)> = keep.iter().zip(0u32..).map(|(o, n)| (n, o)).collect();
        let n = u32::try_from(list.len()).unwrap_or(u32::MAX);
        return Ok((list, n));
    }
    let list: Vec<(u32, u32)> = keep.iter().map(|o| (o, o)).collect();
    let max = keep.max();
    Ok((list, max.wrapping_add(1)))
}

fn condition_is_constant(range: Option<&crate::input::AxisRange>, min: i16, max: i16) -> bool {
    let (low, high) = range.map_or((-1.0, 1.0), |r| (r.min, r.max));
    let (min, max) = (f32::from(min) / 16384.0, f32::from(max) / 16384.0);
    max < min || high < min || low > max || low == high || (min == low && max == high)
}

fn can_instance_layout_conditions(data: &[u8], axis_tags: &[Tag], avar2: bool) -> bool {
    !avar2 && layout_conditions_match(data, axis_tags, |_, _, _| true)
        || sofel_font::read::layout::LayoutTable::new(data)
            .is_ok_and(|t| t.feature_variations_offset().unwrap_or(0) == 0)
}

fn layout_conditions_match(
    data: &[u8],
    axis_tags: &[Tag],
    accept: impl Fn(&Tag, i16, i16) -> bool,
) -> bool {
    let Ok(variations) =
        sofel_font::read::layout::LayoutTable::new(data).and_then(|t| t.feature_variations())
    else {
        return false;
    };
    let Some(fv) = variations else { return true };
    fv.version() == 0x0001_0000
        && (0..fv.len()).all(|i| {
            let Some(record) = fv.record(i) else {
                return false;
            };
            let Ok(conditions) = record.conditions() else {
                return false;
            };
            conditions.is_none_or(|conditions| {
                (0..conditions.len()).all(|j| {
                    conditions
                        .condition(j)
                        .ok()
                        .flatten()
                        .and_then(|c| c.axis_range())
                        .is_some_and(|(axis, min, max)| {
                            axis_tags
                                .get(usize::from(axis))
                                .is_some_and(|tag| accept(tag, min, max))
                        })
                })
            })
        })
}
