//! The subsetting plan ([`Plan`]): the closure and the glyph mapping, as
//! HarfBuzz's `src/hb-subset-plan.cc`.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;

use crate::cmap_accel::CmapAccel;
use crate::error::SubsetError;
use crate::input::{AxisRange, Flags, Input, SetKind};
use crate::limits;
use crate::set::{INVALID, RangeSet};
use crate::tables::cff::SeacFinder;
use crate::tables::glyf::GlyfAccel;

/// The tables whose subsetting or closure this version does not write
/// yet: present and kept, they make [`Plan::new`] return
/// [`SubsetError::Unsupported`].
const PENDING: [(&[u8; 4], &str); 19] = [
    (b"GSUB", "layout subsetting (ADR 0022 stage 3)"),
    (b"GPOS", "layout subsetting (ADR 0022 stage 3)"),
    (b"GDEF", "layout subsetting (ADR 0022 stage 3)"),
    (b"BASE", "BASE subsetting (ADR 0022 stage 3)"),
    (b"MATH", "MATH subsetting (ADR 0022 stage 8)"),
    (b"COLR", "color subsetting (ADR 0022 stage 9)"),
    (b"CPAL", "color subsetting (ADR 0022 stage 9)"),
    (b"CBLC", "color subsetting (ADR 0022 stage 9)"),
    (b"CBDT", "color subsetting (ADR 0022 stage 9)"),
    (b"sbix", "color subsetting (ADR 0022 stage 9)"),
    (b"CFF2", "CFF2 subsetting (ADR 0022 stages 7 and 10)"),
    (b"gvar", "variable font tables (ADR 0022 stage 10)"),
    (b"GVAR", "variable font tables (ADR 0022 stage 10)"),
    (b"HVAR", "variable font tables (ADR 0022 stage 10)"),
    (b"VVAR", "variable font tables (ADR 0022 stage 10)"),
    (b"VARC", "variable composites (ADR 0022 stage 10)"),
    (b"fvar", "instancing (ADR 0022 stage 6)"),
    (b"avar", "instancing (ADR 0022 stage 6)"),
    (b"STAT", "STAT subsetting (ADR 0022 stage 6)"),
];

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
    pub(crate) name_ids: RangeSet,
    pub(crate) name_languages: RangeSet,
    pub(crate) drop_tables: RangeSet,
    pub(crate) no_subset_tables: RangeSet,
    pub(crate) os2_min_cp: u32,
    pub(crate) os2_max_cp: u32,
    pub(crate) force_long_loca: bool,
    pub(crate) axes: BTreeMap<Tag, AxisRange>,
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
        let num_glyphs = font.num_glyphs();
        if num_glyphs == 0 {
            return Err(SubsetError::NoGlyphs);
        }
        let flags = input.flags();
        if !input.axes().is_empty() {
            return Err(SubsetError::Unsupported {
                tag: Tag::new(b"fvar"),
                reason: "pinning axes (ADR 0022 stage 6)",
            });
        }
        if !input.name_overrides().is_empty() {
            return Err(SubsetError::Unsupported {
                tag: Tag::new(b"name"),
                reason: "name overrides",
            });
        }
        let drop = input.set(SetKind::DropTables);
        let no_subset = input.set(SetKind::NoSubsetTables);
        for (tag, reason) in PENDING {
            let t = Tag::new(tag);
            let passthrough = no_subset.contains(t.to_u32())
                || (matches!(tag, b"fvar" | b"avar" | b"STAT") && input.axes().is_empty());
            if font.table(t).is_some() && !drop.contains(t.to_u32()) && !passthrough {
                return Err(SubsetError::Unsupported { tag: t, reason });
            }
        }

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
            name_ids: input.set(SetKind::NameIds).clone(),
            name_languages: input.set(SetKind::NameLanguages).clone(),
            drop_tables: drop.clone(),
            no_subset_tables: no_subset.clone(),
            os2_min_cp: INVALID,
            os2_max_cp: INVALID,
            force_long_loca: flags.has(Flags::IFTB_REQUIREMENTS),
            axes: input.axes().clone(),
        };

        // _populate_unicodes_to_retain
        let mut glyphset_gsub = RangeSet::new();
        let unicodes = unicode_closure(
            input.set(SetKind::Unicodes),
            !flags.has(Flags::NO_BIDI_CLOSURE),
        );
        let mut list: Vec<(u32, u32)> = Vec::new();
        if glyphs_in.is_empty() && unicodes.len() < u64::from(num_glyphs) {
            for cp in unicodes.iter() {
                if let Some(g) = cmap.nominal_glyph(cp) {
                    p.codepoint_to_glyph.insert(cp, g);
                    list.push((cp, g));
                }
            }
        } else {
            let mut cmap_unicodes = RangeSet::new();
            let mut mapping = BTreeMap::new();
            cmap.collect_mapping(&mut cmap_unicodes, &mut mapping);
            for (a, b) in cmap_unicodes.ranges() {
                for cp in a..=b {
                    let gid = mapping.get(&cp).copied().unwrap_or(INVALID);
                    if !unicodes.contains(cp) && !glyphs_in.contains(gid) {
                        continue;
                    }
                    if gid == INVALID {
                        continue;
                    }
                    p.codepoint_to_glyph.insert(cp, gid);
                    list.push((cp, gid));
                }
            }
            for (a, b) in glyphs_in.ranges() {
                if a >= num_glyphs {
                    break;
                }
                glyphset_gsub.add_range(a, b.min(num_glyphs.saturating_sub(1)));
            }
        }
        list.iter().for_each(|&(cp, g)| {
            p.unicodes.add(cp);
            glyphset_gsub.add(g);
        });
        p.os2_min_cp = p.unicodes.min();
        p.os2_max_cp = p.unicodes.max();
        let mut selectors = RangeSet::new();
        cmap.collect_variation_selectors(&mut selectors);
        selectors
            .iter()
            .filter(|s| unicodes.contains(*s))
            .for_each(|s| p.unicodes.add(s));

        // _populate_gids_to_retain
        glyphset_gsub.add(0);
        cmap.closure_glyphs(&p.unicodes, &mut glyphset_gsub);
        glyphset_gsub.truncate(num_glyphs);
        let cur = glyphset_gsub;
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
        // kept already.
        if cur.len() != u64::from(num_glyphs)
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
        p.glyphs = GlyphMap::new(new_to_old, num_glyphs);
        p.unicode_to_new_gid = list
            .into_iter()
            .map(|(cp, g)| (cp, p.glyphs.new_gid(g).unwrap_or(INVALID)))
            .collect();
        Ok(p)
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
}

/// `_unicode_closure`: the characters and, unless disabled or inverted,
/// their mirrored characters.
fn unicode_closure(unicodes: &RangeSet, bidi: bool) -> RangeSet {
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
