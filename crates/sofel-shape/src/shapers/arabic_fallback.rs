//! The Arabic fallback forms (ADR 0024, arabic): for fonts whose `GSUB`
//! lacks the form features, single and ligature substitution lookups
//! built from the Unicode presentation forms and the cmap, or for
//! Windows-1256-encoded fonts the hand-made lookups of HarfBuzz. They are
//! built with the plan and applied after `rlig` as HarfBuzz applies them,
//! with an empty `GDEF`.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shaper-arabic-fallback.hh` and
//! `src/hb-ot-shaper-arabic-win1256.hh`, MIT; see THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::gsub::SubstSubtable;

use crate::buffer::{Buffer, Mask};
use crate::digest::Digest;
use crate::font::ShapeFont;
use crate::layout::apply::{ApplyContext, apply_synthesized_gsub};
use crate::layout::{GSUB, add_coverage, subst_coverage};
use crate::plan::ShapePlan;
use crate::tables::arabic::{
    LIGATURE_3_TABLE, LIGATURE_MARK_TABLE, LIGATURE_TABLE, SHAPING_TABLE, SHAPING_TABLE_FIRST,
};

/// `LookupFlag::IgnoreMarks`.
const IGNORE_MARKS: u16 = 0x0008;
/// The lookup types.
const SINGLE: u16 = 1;
const LIGATURE: u16 = 4;

/// The ligatures of a first glyph: `(first, [(other components,
/// ligature)])`.
type LigatureSet16 = (u16, Vec<(Vec<u16>, u16)>);
/// A row of the ligature tables: `(first, [(other components, ligature)])`.
type LigatureRow<'t, const N: usize> = (u16, &'t [([u16; N], u16)]);
/// A lookup of `hb-ot-shaper-arabic-win1256.hh`: `(feature, flag, type,
/// subtables)`.
type Win1256Lookup = ([u8; 4], u16, u16, Vec<Vec<u8>>);

/// A synthesized lookup: its mask, flag, type and subtables as `GSUB`
/// bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FallbackLookup {
    mask: Mask,
    flag: u16,
    ty: u16,
    subtables: Vec<Vec<u8>>,
}

/// `arabic_fallback_plan_t`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct FallbackPlan {
    pub(crate) lookups: Vec<FallbackLookup>,
}

fn push16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn len16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// Coverage format 1 of sorted glyphs.
fn coverage(glyphs: &[u16]) -> Vec<u8> {
    let mut c = Vec::new();
    push16(&mut c, 1);
    push16(&mut c, len16(glyphs.len()));
    glyphs.iter().for_each(|&g| push16(&mut c, g));
    c
}

/// Single substitution format 2 of `(glyph, substitute)` sorted by
/// glyph.
pub(crate) fn single_subtable(pairs: &[(u16, u16)]) -> Vec<u8> {
    let header = 6usize.saturating_add(pairs.len().saturating_mul(2));
    let mut s = Vec::new();
    push16(&mut s, 2);
    push16(&mut s, len16(header));
    push16(&mut s, len16(pairs.len()));
    pairs.iter().for_each(|p| push16(&mut s, p.1));
    let glyphs: Vec<u16> = pairs.iter().map(|p| p.0).collect();
    s.extend(coverage(&glyphs));
    s
}

/// Ligature substitution format 1: for each first glyph (sorted), its
/// ligatures as `(other components, ligature)`.
pub(crate) fn ligature_subtable(sets: &[LigatureSet16]) -> Vec<u8> {
    let set_tables: Vec<Vec<u8>> = sets
        .iter()
        .map(|(_, ligs)| {
            let ligatures: Vec<Vec<u8>> = ligs
                .iter()
                .map(|(comps, lig)| {
                    let mut l = Vec::new();
                    push16(&mut l, *lig);
                    push16(&mut l, len16(comps.len().saturating_add(1)));
                    comps.iter().for_each(|&c| push16(&mut l, c));
                    l
                })
                .collect();
            let mut t = Vec::new();
            push16(&mut t, len16(ligatures.len()));
            let mut off = 2usize.saturating_add(ligatures.len().saturating_mul(2));
            for l in &ligatures {
                push16(&mut t, len16(off));
                off = off.saturating_add(l.len());
            }
            ligatures.iter().for_each(|l| t.extend_from_slice(l));
            t
        })
        .collect();
    let mut s = Vec::new();
    push16(&mut s, 1);
    let header = 6usize.saturating_add(sets.len().saturating_mul(2));
    let sets_len: usize = set_tables.iter().map(Vec::len).sum();
    push16(&mut s, len16(header.saturating_add(sets_len)));
    push16(&mut s, len16(sets.len()));
    let mut off = header;
    for t in &set_tables {
        push16(&mut s, len16(off));
        off = off.saturating_add(t.len());
    }
    set_tables.iter().for_each(|t| s.extend_from_slice(t));
    let firsts: Vec<u16> = sets.iter().map(|s| s.0).collect();
    s.extend(coverage(&firsts));
    s
}

/// `hb_font_get_glyph` for glyphs that fit 16 bits.
fn glyph16(font: &Font<'_>, u: u32) -> Option<u16> {
    font.nominal_glyph_raw(u)
        .and_then(|g| u16::try_from(g.0).ok())
}

/// `arabic_fallback_synthesize_lookup_single`.
fn synthesize_single(font: &Font<'_>, feature_index: usize) -> Option<Vec<u8>> {
    let mut pairs: Vec<(u16, u16)> = SHAPING_TABLE
        .iter()
        .zip(SHAPING_TABLE_FIRST..)
        .filter_map(|(forms, u)| {
            let s = u32::from(*forms.get(feature_index)?);
            if s == 0 {
                return None;
            }
            let ug = font.nominal_glyph_raw(u)?.0;
            let sg = font.nominal_glyph_raw(s)?.0;
            if ug == sg {
                return None;
            }
            Some((u16::try_from(ug).ok()?, u16::try_from(sg).ok()?))
        })
        .collect();
    if pairs.is_empty() {
        return None;
    }
    // A stable sort by glyph, as hb_stable_sort.
    pairs.sort_by_key(|p| p.0);
    Some(single_subtable(&pairs))
}

/// `arabic_fallback_synthesize_lookup_ligature`.
fn synthesize_ligature<const N: usize>(
    font: &Font<'_>,
    table: &[LigatureRow<'_, N>],
) -> Option<Vec<u8>> {
    let mut firsts: Vec<(u16, usize)> = table
        .iter()
        .enumerate()
        .filter_map(|(i, (first, _))| Some((glyph16(font, u32::from(*first))?, i)))
        .collect();
    firsts.sort_by_key(|f| f.0);
    let mut n = 0usize;
    let sets: Vec<LigatureSet16> = firsts
        .iter()
        .map(|&(g, i)| {
            let ligs: Vec<(Vec<u16>, u16)> = table
                .get(i)
                .map_or(&[][..], |t| t.1)
                .iter()
                .filter_map(|(comps, lig)| {
                    let lg = glyph16(font, u32::from(*lig))?;
                    let cg: Option<Vec<u16>> = comps
                        .iter()
                        .map(|&c| {
                            if c == 0 {
                                None
                            } else {
                                glyph16(font, u32::from(c))
                            }
                        })
                        .collect();
                    Some((cg?, lg))
                })
                .collect();
            n = n.saturating_add(ligs.len());
            (g, ligs)
        })
        .collect();
    if n == 0 {
        return None;
    }
    Some(ligature_subtable(&sets))
}

/// The features of the fallback lookups, in order (`arabic_fallback_features`).
const FEATURES: [[u8; 4]; 7] = [
    *b"init", *b"medi", *b"fina", *b"isol", *b"rlig", *b"rlig", *b"rlig",
];

/// `hb_font_get_glyph`'s signature of a Windows-1256-encoded font.
const WIN1256_SIGNATURE: [(u32, u32); 5] = [
    (0x0627, 199), // ALEF
    (0x0644, 225), // LAM
    (0x0649, 236), // ALEF MAKSURA
    (0x064A, 237), // YEH
    (0x0652, 250), // SUKUN
];

fn pairs(glyphs: &[u16], subs: &[u16]) -> Vec<(u16, u16)> {
    glyphs.iter().copied().zip(subs.iter().copied()).collect()
}

/// The lookups of `hb-ot-shaper-arabic-win1256.hh`, in manifest order:
/// `(feature, flag, type, subtables)`.
fn win1256_lookups() -> [Win1256Lookup; 5] {
    let initmedi = single_subtable(&pairs(
        &[
            198, 200, 201, 202, 203, 204, 205, 206, 211, 212, 213, 214, 223, 225, 227, 228, 236,
            237,
        ],
        &[
            162, 4, 5, 5, 6, 7, 9, 11, 13, 14, 15, 26, 140, 141, 142, 143, 154, 154,
        ],
    ));
    let init = single_subtable(&pairs(&[218, 219, 221, 222, 229], &[27, 30, 128, 131, 144]));
    let medi = single_subtable(&pairs(&[218, 219, 221, 222, 229], &[28, 31, 129, 138, 149]));
    let fina = single_subtable(&pairs(
        &[
            194, 195, 197, 198, 199, 201, 204, 205, 206, 218, 219, 229, 236, 237,
        ],
        &[2, 1, 3, 181, 0, 159, 8, 10, 12, 29, 127, 152, 160, 156],
    ));
    let medifina_lam_alef = single_subtable(&pairs(&[165, 178, 180, 252], &[170, 179, 185, 255]));
    let lam_alef = ligature_subtable(&[(
        225,
        alloc::vec![
            (alloc::vec![199], 165),
            (alloc::vec![195], 178),
            (alloc::vec![194], 180),
            (alloc::vec![197], 252),
        ],
    )]);
    let shadda = ligature_subtable(&[(
        248,
        alloc::vec![
            (alloc::vec![243], 172),
            (alloc::vec![245], 173),
            (alloc::vec![246], 175),
        ],
    )]);
    [
        (*b"rlig", IGNORE_MARKS, LIGATURE, alloc::vec![lam_alef]),
        (
            *b"init",
            IGNORE_MARKS,
            SINGLE,
            alloc::vec![initmedi.clone(), init],
        ),
        (
            *b"medi",
            IGNORE_MARKS,
            SINGLE,
            alloc::vec![initmedi, medi, medifina_lam_alef.clone()],
        ),
        (
            *b"fina",
            IGNORE_MARKS,
            SINGLE,
            alloc::vec![fina, medifina_lam_alef],
        ),
        (*b"rlig", 0, LIGATURE, alloc::vec![shadda]),
    ]
}

impl FallbackPlan {
    /// `arabic_fallback_plan_create`: from the presentation forms, else as
    /// a Windows-1256 font, else empty.
    pub(crate) fn new(plan: &ShapePlan<'_>, font: &Font<'_>) -> FallbackPlan {
        let unicode: Vec<FallbackLookup> = FEATURES
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                let mask = plan.map.one_mask(Tag(*f));
                if mask == 0 {
                    return None;
                }
                let (flag, ty, sub) = match i {
                    0..=3 => (IGNORE_MARKS, SINGLE, synthesize_single(font, i)?),
                    4 => (
                        IGNORE_MARKS,
                        LIGATURE,
                        synthesize_ligature(font, &LIGATURE_3_TABLE)?,
                    ),
                    5 => (
                        IGNORE_MARKS,
                        LIGATURE,
                        synthesize_ligature(font, &LIGATURE_TABLE)?,
                    ),
                    _ => (
                        0,
                        LIGATURE,
                        synthesize_ligature(font, &LIGATURE_MARK_TABLE)?,
                    ),
                };
                Some(FallbackLookup {
                    mask,
                    flag,
                    ty,
                    subtables: alloc::vec![sub],
                })
            })
            .collect();
        if !unicode.is_empty() {
            return FallbackPlan { lookups: unicode };
        }
        let is_win1256 = WIN1256_SIGNATURE
            .iter()
            .all(|&(u, g)| font.nominal_glyph_raw(u).is_some_and(|x| x.0 == g));
        if !is_win1256 {
            return FallbackPlan::default();
        }
        let lookups = win1256_lookups()
            .into_iter()
            .filter_map(|(f, flag, ty, subtables)| {
                let mask = plan.map.one_mask(Tag(f));
                (mask != 0).then_some(FallbackLookup {
                    mask,
                    flag,
                    ty,
                    subtables,
                })
            })
            .collect();
        FallbackPlan { lookups }
    }

    /// `arabic_fallback_plan_shape`.
    pub(crate) fn shape(
        &self,
        plan: &ShapePlan<'_>,
        font: &ShapeFont<'_, '_>,
        buffer: &mut Buffer,
    ) {
        for l in &self.lookups {
            let subtables: Vec<(SubstSubtable<'_>, Digest)> = l
                .subtables
                .iter()
                .filter_map(|b| sofel_font::read::gsub::parse(l.ty, b).ok())
                .map(|s| {
                    let mut d = Digest::default();
                    add_coverage(&mut d, &subst_coverage(&s));
                    (s, d)
                })
                .collect();
            let mut digest = Digest::default();
            subtables.iter().for_each(|(_, d)| digest.union(d));
            let mut c = ApplyContext::new(GSUB, plan, font, buffer);
            c.has_glyph_classes = false;
            c.set_lookup_mask(l.mask);
            apply_synthesized_gsub(&mut c, u32::from(l.flag), &subtables, &digest);
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn single_and_ligature_subtables_parse() {
        let s = single_subtable(&[(3, 7), (5, 9)]);
        match sofel_font::read::gsub::parse(SINGLE, &s).unwrap() {
            SubstSubtable::SingleList {
                coverage,
                substitutes,
            } => {
                assert_eq!(coverage.get(sofel_font::read::GlyphId(5)), Some(1));
                assert_eq!(substitutes.get(1), Some(9));
            }
            other => panic!("{other:?}"),
        }
        let l = ligature_subtable(&[
            (4, alloc::vec![(alloc::vec![5, 6], 10)]),
            (8, alloc::vec![]),
        ]);
        match sofel_font::read::gsub::parse(LIGATURE, &l).unwrap() {
            SubstSubtable::Ligature {
                coverage,
                ligature_sets: sets,
            } => {
                assert_eq!(coverage.get(sofel_font::read::GlyphId(8)), Some(1));
                let set = sets.get(0).unwrap().unwrap();
                let lig = set.get(0).unwrap().unwrap();
                assert_eq!(lig.glyph.0, 10);
                assert_eq!(lig.components.get(1), Some(6));
            }
            other => panic!("{other:?}"),
        }
    }
}
