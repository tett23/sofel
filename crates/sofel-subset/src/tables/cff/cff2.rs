//! CFF2 with every axis pinned or with subroutines flattened (`src/hb-subset-cff2.cc`,
//! `src/hb-subset-cff2-to-cff1.hh`, `src/hb-ot-cff2-table.hh`): the
//! subsetting plan (`cff2_subset_plan::create` on its instancing path),
//! the serialization as CFF2 (`accelerator_subset_t::serialize`) and as
//! CFF with `DOWNGRADE_CFF2` (`serialize_cff2_to_cff1`), in HarfBuzz's
//! object order; and the metrics HarfBuzz takes from CFF2 when it
//! instances (`update_instance_metrics_map_from_cff2` of
//! `src/hb-subset-plan-var.cc`).
//!
//! CFF2 kept with its variations (no axis pinned) is stage 10 of ADR 0022.
//! Literal calls from retained glyphs use the subroutine-preserving path.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use alloc::vec::Vec;

use sofel_font::read::cff::hb::{
    DictKind, DictToken, HbFdSelect, HbIndex, Sanitizer, dict_token, interpret_dict,
};
use sofel_font::read::{GlyphId, Tag};
use sofel_font::{Font, GlyphExtents, HbCff2};
use sofel_geom::float::hb_clamp_to_i32;
use sofel_write::{Errors, ObjIdx, Serializer, Whence};

use super::cs::{ENDCHAR_BYTE, bias, encode_int, encode_op};
use super::cs2::{
    self, Command, Env, Flatten, HintTrace, Programs, Rounding, Source, Store, Usage, c_round,
};
use super::dict::{self, Dict};
use super::specialize;
use super::{
    FdMap, fdselect, index, index_header, index_header_n, index_n, int2, link2_op, link4_op,
    plan_fdselect, private_bytes,
};
use crate::error::SubsetError;
use crate::input::Flags;
use crate::instance::Metrics;
use crate::plan::Plan;
use crate::tables::Tables;
use crate::tables::hmtx::MetricsAccel;

/// The name of the CFF font a downgrade writes (`CFF1_DEFAULT_FONT_NAME`).
const CFF1_FONT_NAME: &[u8] = b"CFF1Font";

/// The charstring interpreter's view of a CFF2 font: HarfBuzz's
/// accelerator and each Font DICT's local subroutines and `vsindex`.
struct Fonts<'a> {
    hb: HbCff2<'a>,
    privates: Vec<(HbIndex<'a>, u32)>,
    store: Store<'a>,
}

impl<'a> Fonts<'a> {
    fn new(hb: HbCff2<'a>) -> Fonts<'a> {
        let n = hb.fd_array().len();
        Fonts {
            privates: (0..n).map(|fd| hb.private(fd)).collect(),
            store: Store {
                data: hb.data(),
                at: hb.var_store().and_then(|p| usize::try_from(p).ok()),
            },
            hb,
        }
    }

    fn fd_count(&self) -> u32 {
        self.hb.fd_array().len()
    }

    /// `subset_get_fd`.
    fn fd(&self, glyph: u32) -> u32 {
        if glyph < self.hb.num_glyphs() {
            self.hb.fd(glyph)
        } else {
            0
        }
    }

    fn char_string(&self, glyph: u32) -> &'a [u8] {
        self.hb.char_string(glyph).unwrap_or(&[])
    }

    fn programs(&self, fd: u32) -> Programs<'a> {
        let (lsubrs, ivs) = self
            .privates
            .get(fd as usize)
            .copied()
            .unwrap_or((HbIndex::null(), 0));
        Programs {
            gsubrs: self.hb.global_subrs(),
            lsubrs,
            store: self.store,
            ivs,
        }
    }

    /// `get_extents_at` of `glyph` at `coords`, unscaled.
    fn extents(&self, glyph: u32, coords: &[i32]) -> Option<GlyphExtents> {
        if glyph >= self.hb.num_glyphs() {
            return None;
        }
        let p = self.programs(self.hb.fd(glyph));
        cs2::extents(self.char_string(glyph), p, coords).map(
            |(x_bearing, y_bearing, width, height)| GlyphExtents {
                x_bearing,
                y_bearing,
                width,
                height,
            },
        )
    }
}

/// Whether the CFF2 variation-preserving writer must retain a called
/// subroutine for the selected glyphs.
pub(crate) fn uses_subroutines(font: &Font<'_>, glyphs: &[(u32, u32)]) -> bool {
    let Some(hb) = HbCff2::new(font) else {
        return true;
    };
    if hb.global_subrs().is_empty()
        && (0..hb.fd_array().len()).all(|fd| hb.private(fd).0.is_empty())
    {
        return false;
    }
    let fonts = Fonts::new(hb);
    glyphs.iter().any(|&(_, old)| {
        let fd = fonts.fd(old);
        let mut env = Env::new(fonts.char_string(old), fonts.programs(fd), &[]);
        let mut usage = Usage::default();
        !cs2::interpret(&mut env, &mut usage) || env.called_subr()
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct CallSite {
    start: usize,
    end: usize,
    local: bool,
    old: u32,
}

#[derive(Default)]
struct KeptSubrs {
    glyphs: Vec<Vec<u8>>,
    globals: Vec<Vec<u8>>,
    locals: Vec<Vec<Vec<u8>>>,
}

fn rewrite_calls(
    bytes: &[u8],
    sites: &[CallSite],
    dropped: &[HintTrace],
    global: &BTreeMap<u32, u32>,
    local: &BTreeMap<u32, u32>,
) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut cursor = 0;
    let mut events: Vec<(usize, usize, Option<&CallSite>)> = sites
        .iter()
        .map(|s| (s.start, s.end, Some(s)))
        .chain(dropped.iter().map(|s| (s.start, s.end, None)))
        .collect();
    events.sort_unstable_by_key(|e| e.0);
    for (start, end, call) in events {
        out.extend_from_slice(bytes.get(cursor..start)?);
        if let Some(site) = call {
            let (map, count) = if site.local {
                (local, local.len())
            } else {
                (global, global.len())
            };
            let next = *map.get(&site.old)?;
            encode_int(
                &mut out,
                i32::try_from(next)
                    .ok()?
                    .checked_sub(bias(u32::try_from(count).ok()?))?,
            );
            out.push(if site.local { 10 } else { 29 });
        }
        cursor = end;
    }
    out.extend_from_slice(bytes.get(cursor..)?);
    Some(out)
}

/// Only complete number operands may change interpretation from data to code:
/// operators in the disputed bytes can otherwise create a different call or hint.
fn mask_difference_is_operands(mut bytes: &[u8]) -> bool {
    while let Some((&first, rest)) = bytes.split_first() {
        let extra = match first {
            32..=246 => 0,
            247..=254 => 1,
            28 => 2,
            255 => 4,
            _ => return false,
        };
        let Some(tail) = rest.get(extra..) else {
            return false;
        };
        bytes = tail;
    }
    true
}

fn keep_subrs(
    fonts: &Fonts<'_>,
    glyphs: &[(u32, u32)],
    output_count: u32,
    drop_hints: bool,
) -> Option<KeptSubrs> {
    let fd_count = usize::try_from(fonts.fd_count()).ok()?;
    let mut global_used = BTreeSet::new();
    let mut local_used = vec![BTreeSet::new(); fd_count];
    let mut glyph_sites = Vec::new();
    let mut subr_sites: BTreeMap<(usize, Source), BTreeMap<usize, CallSite>> = BTreeMap::new();
    let mut subr_drops: BTreeMap<(usize, Source), BTreeMap<usize, HintTrace>> = BTreeMap::new();
    let mut visited = BTreeSet::new();
    let mut mask_sizes = BTreeMap::new();
    let mut contextual_masks = BTreeSet::new();
    for &(new, old) in glyphs {
        let fd = usize::try_from(fonts.fd(old)).ok()?;
        let p = fonts.programs(u32::try_from(fd).ok()?);
        let mut env = Env::new(fonts.char_string(old), p, &[]);
        env.enable_call_trace(drop_hints);
        if !cs2::interpret(&mut env, &mut Usage::default()) {
            return None;
        }
        for mask in env.mask_trace() {
            if mask.source != Source::Glyph {
                let first = mask_sizes
                    .entry((fd, mask.source, mask.at))
                    .or_insert(mask.size);
                if *first != mask.size {
                    if drop_hints {
                        let program = match mask.source {
                            Source::Global(old) => p.gsubrs.get(old)?,
                            Source::Local(old) => p.lsubrs.get(old)?,
                            Source::Glyph => return None,
                        };
                        let bytes = program.get(mask.at.checked_add(1)?..)?;
                        let lo = usize::try_from((*first).min(mask.size)).ok()?;
                        let hi = usize::try_from((*first).max(mask.size)).ok()?;
                        if !mask_difference_is_operands(bytes.get(lo..hi)?) {
                            return None;
                        }
                    }
                    contextual_masks.insert((fd, mask.source));
                }
            }
        }
        let mut direct = BTreeMap::new();
        let mut direct_drops = BTreeMap::new();
        if drop_hints {
            for mut hint in env.hint_trace().iter().copied() {
                // HarfBuzz caches the first parse of a subroutine. Its first
                // caller fixes the mask width used when encoding without hints.
                if hint.source != Source::Glyph
                    && let Some(at) = hint.mask_at
                {
                    let size = *mask_sizes.get(&(fd, hint.source, at))?;
                    hint.end = at
                        .checked_add(1)?
                        .checked_add(usize::try_from(size).ok()?)?;
                }

                if !hint.can_follow_call
                    && env
                        .call_trace()?
                        .iter()
                        .any(|call| call.source == hint.source && call.end <= hint.start)
                {
                    return None;
                }
                let drops = if hint.source == Source::Glyph {
                    &mut direct_drops
                } else {
                    subr_drops.entry((fd, hint.source)).or_default()
                };
                if drops
                    .insert(hint.start, hint)
                    .is_some_and(|old| old != hint)
                {
                    return None;
                }
            }
        }
        for trace in env.call_trace()? {
            let target = match trace.target {
                Source::Global(old) => p.gsubrs.get(old)?,
                Source::Local(old) => p.lsubrs.get(old)?,
                Source::Glyph => return None,
            };
            let drops_cover_target = subr_drops.get(&(fd, trace.target)).is_some_and(|drops| {
                drops
                    .values()
                    .try_fold(0, |cursor, drop| (drop.start == cursor).then_some(drop.end))
                    == Some(target.len())
            });
            if drop_hints && (target.is_empty() || drops_cover_target) {
                let drops = if trace.source == Source::Glyph {
                    &mut direct_drops
                } else {
                    subr_drops.entry((fd, trace.source)).or_default()
                };
                drops.insert(
                    trace.start,
                    HintTrace {
                        source: trace.source,
                        start: trace.start,
                        end: trace.end,
                        can_follow_call: false,
                        mask_at: None,
                    },
                );
                continue;
            }
            let (local, old) = match trace.target {
                Source::Global(old) => {
                    global_used.insert(old);
                    (false, old)
                }
                Source::Local(old) => {
                    local_used.get_mut(fd)?.insert(old);
                    (true, old)
                }
                Source::Glyph => return None,
            };
            visited.insert((fd, trace.target));
            let site = CallSite {
                start: trace.start,
                end: trace.end,
                local,
                old,
            };
            let sites = if trace.source == Source::Glyph {
                &mut direct
            } else {
                subr_sites.entry((fd, trace.source)).or_default()
            };
            if sites
                .insert(trace.start, site)
                .is_some_and(|old| old != site)
            {
                return None;
            }
        }
        glyph_sites.push((
            new,
            old,
            fd,
            direct.into_values().collect::<Vec<_>>(),
            direct_drops.into_values().collect::<Vec<_>>(),
        ));
    }
    // An unchanged program can keep both interpretations. Rewriting a call
    // could replace bytes consumed as mask data by another invocation.
    if contextual_masks
        .iter()
        .any(|key| subr_sites.contains_key(key))
    {
        return None;
    }
    let gmap: BTreeMap<_, _> = global_used.iter().copied().zip(0u32..).collect();
    let lmaps: Vec<BTreeMap<_, _>> = local_used
        .iter()
        .map(|set| set.iter().copied().zip(0u32..).collect())
        .collect();
    let mut result = KeptSubrs {
        glyphs: vec![Vec::new(); output_count as usize],
        globals: Vec::new(),
        locals: vec![Vec::new(); fd_count],
    };
    for (new, old, fd, sites, dropped) in glyph_sites {
        *result.glyphs.get_mut(new as usize)? = rewrite_calls(
            fonts.char_string(old),
            &sites,
            &dropped,
            &gmap,
            lmaps.get(fd)?,
        )?;
    }
    for old in global_used {
        let mut encoded = None;
        for fd in 0..fd_count {
            if visited.contains(&(fd, Source::Global(old))) {
                let sites = subr_sites.get(&(fd, Source::Global(old)));
                let dropped = subr_drops.get(&(fd, Source::Global(old)));
                let bytes = fonts.programs(u32::try_from(fd).ok()?).gsubrs.get(old)?;
                let value = rewrite_calls(
                    bytes,
                    &sites.map_or(Vec::new(), |s| s.values().copied().collect()),
                    &dropped.map_or(Vec::new(), |s| s.values().copied().collect()),
                    &gmap,
                    lmaps.get(fd)?,
                )?;
                if drop_hints && value.is_empty() {
                    return None;
                }
                if encoded.as_ref().is_some_and(|v| v != &value) {
                    return None;
                }
                encoded = Some(value);
            }
        }
        result.globals.push(encoded?);
    }
    for (fd, used) in local_used.iter().enumerate() {
        let p = fonts.programs(u32::try_from(fd).ok()?);
        for &old in used {
            let sites = subr_sites.get(&(fd, Source::Local(old)));
            let dropped = subr_drops.get(&(fd, Source::Local(old)));
            let value = rewrite_calls(
                p.lsubrs.get(old)?,
                &sites.map_or(Vec::new(), |s| s.values().copied().collect()),
                &dropped.map_or(Vec::new(), |s| s.values().copied().collect()),
                &gmap,
                lmaps.get(fd)?,
            )?;
            if drop_hints && value.is_empty() {
                return None;
            }
            result.locals.get_mut(fd)?.push(value);
        }
    }
    Some(result)
}

pub(crate) fn supports_subroutines(
    font: &Font<'_>,
    glyphs: &[(u32, u32)],
    output_count: u32,
    drop_hints: bool,
) -> bool {
    HbCff2::new(font)
        .is_some_and(|hb| keep_subrs(&Fonts::new(hb), glyphs, output_count, drop_hints).is_some())
}

/// HarfBuzz's subset accelerator (`OT::cff2::accelerator_subset_t`):
/// the DICTs as its subsetter records them.
struct Acc<'a> {
    fonts: Fonts<'a>,
    top: Dict,
    top_bytes: &'a [u8],
    /// The Font DICTs with their bytes.
    font_dicts: Vec<(Dict, &'a [u8])>,
    /// Each Font DICT's Private DICT and its bytes.
    privates: Vec<(Dict, &'a [u8])>,
}

impl<'a> Acc<'a> {
    /// `None` exactly when HarfBuzz's accelerator is not valid.
    fn new(font: &Font<'a>) -> Option<Acc<'a>> {
        let hb = HbCff2::new(font)?;
        let data = hb.data();
        let top_at = usize::from(*data.get(2)?);
        let top_size = usize::from(u16::from_be_bytes([*data.get(3)?, *data.get(4)?]));
        let top_bytes = data.get(top_at..)?.get(..top_size)?;
        let top = dict::parse(top_bytes, dict::Kind::Cff2Top)?;
        let mut sc = Sanitizer::unlimited(data.len());
        let fd_array = hb.fd_array();
        let mut font_dicts = Vec::new();
        let mut privates = Vec::new();
        for i in 0..fd_array.len() {
            let bytes = fd_array.get(i)?;
            let fd = dict::parse(bytes, dict::Kind::Cff2Font)?;
            let info = interpret_dict(bytes, DictKind::Cff2Font)?;
            let p = private_bytes(data, &mut sc, info.private_offset, info.private_size)?;
            privates.push((dict::parse(p, dict::Kind::Cff2Private)?, p));
            font_dicts.push((fd, bytes));
        }
        Some(Acc {
            fonts: Fonts::new(hb),
            top,
            top_bytes,
            font_dicts,
            privates,
        })
    }

    fn fd_select(&self) -> Option<HbFdSelect<'a>> {
        let f = self.fonts.hb.fd_select();
        (f.format() != 0xFF).then_some(f)
    }
}

/// The subsetting plan (`cff2_subset_plan`) for a full instance or a
/// variation-preserving desubroutinized font.
struct Cff2Plan {
    num_glyphs: u32,
    drop_hints: bool,
    min_charstrings_off_size: u32,
    /// The desubroutinized charstrings, empty for the glyphs not kept.
    charstrings: Vec<Vec<u8>>,
    /// The captured commands of each charstring.
    commands: Vec<Vec<Command>>,
    subset_fdcount: u32,
    /// `(format, ranges)` of the FDSelect, when the font has one.
    fdselect: Option<(u8, Vec<(u32, u32)>)>,
    fdmap: FdMap,
    keep_variations: bool,
    subrs: Option<KeptSubrs>,
}

/// `cff2_subset_plan::create` on the full-instance or desubroutinized path.
fn create(plan: &Plan, acc: &Acc<'_>, tag: Tag) -> Result<Cff2Plan, SubsetError> {
    let fail = SubsetError::Failed { tag };
    if plan.glyphs.old_gid(0) != Some(0) {
        return Err(fail);
    }
    let num_glyphs = plan.num_output_glyphs;
    let drop_hints = plan.flags.has(Flags::NO_HINTING);
    let coords = plan.var.n.normalized_coords.as_slice();
    let keep_variations = coords.is_empty();
    let fonts = &acc.fonts;
    let glyph = |i: u32| -> Result<Option<(u32, u32)>, SubsetError> {
        let Some(old) = plan.glyphs.old_gid(i) else {
            return Ok(None);
        };
        let fd = fonts.fd(old);
        if fd >= fonts.fd_count() {
            return Err(fail);
        }
        Ok(Some((old, fd)))
    };
    // Which charstrings round absolute coordinates: those with a blend
    // a kept operator takes, without flex1.
    let mut round = vec![false; num_glyphs as usize];
    let mut called = false;
    for i in 0..num_glyphs {
        let Some((old, fd)) = glyph(i)? else {
            continue;
        };
        let mut env = Env::new(fonts.char_string(old), fonts.programs(fd), &[]);
        let mut u = Usage {
            drop_hints,
            ..Usage::default()
        };
        if !cs2::interpret(&mut env, &mut u) {
            return Err(fail);
        }
        called |= env.called_subr();
        if let Some(r) = round.get_mut(i as usize) {
            *r = !keep_variations && u.has_blend && !u.has_flex1;
        }
    }
    let mut charstrings = vec![Vec::new(); num_glyphs as usize];
    let mut commands = vec![Vec::new(); num_glyphs as usize];
    for i in 0..num_glyphs {
        let Some((old, fd)) = glyph(i)? else {
            continue;
        };
        let rounds = round.get(i as usize).copied().unwrap_or(false);
        let mut env = Env::new(fonts.char_string(old), fonts.programs(fd), coords);
        env.round_blends = !rounds;
        let mut f = Flatten {
            drop_hints,
            rounding: rounds.then(Rounding::default),
            commands: Some(Vec::new()),
            ..Flatten::default()
        };
        if !cs2::interpret(&mut env, &mut f) {
            return Err(fail);
        }
        if let Some(c) = charstrings.get_mut(i as usize) {
            *c = f.out;
        }
        if let Some(c) = commands.get_mut(i as usize) {
            *c = f.commands.unwrap_or_default();
        }
    }
    let (subset_fdcount, fdselect, fdmap) = match acc.fd_select() {
        Some(src) => {
            let fp =
                plan_fdselect(plan, &src, fonts.hb.num_glyphs(), fonts.fd_count()).ok_or(fail)?;
            (fp.count, Some((fp.format, fp.ranges)), fp.fdmap)
        }
        None => (1, None, FdMap::identity(1)),
    };
    let subrs = if called && keep_variations && !plan.flags.has(Flags::DESUBROUTINIZE) {
        Some(
            keep_subrs(fonts, &plan.glyphs.new_to_old, num_glyphs, drop_hints).ok_or(
                SubsetError::Unsupported {
                    tag,
                    reason: "CFF2 call operand cannot be renumbered",
                },
            )?,
        )
    } else {
        None
    };
    if let Some(kept) = &subrs {
        charstrings.clone_from(&kept.glyphs);
    }
    Ok(Cff2Plan {
        num_glyphs,
        drop_hints,
        min_charstrings_off_size: if plan.flags.has(Flags::IFTB_REQUIREMENTS) {
            4
        } else {
            0
        },
        charstrings,
        commands,
        subset_fdcount,
        fdselect,
        fdmap,
        keep_variations,
        subrs,
    })
}

/// `encode_num_tp`: a number of a Top or Private DICT, an integer form
/// or a BCD real written as fontTools writes it.
fn encode_num_tp(out: &mut Vec<u8>, v: f64) {
    // `number_t::in_int_range`.
    #[allow(clippy::cast_possible_truncation)]
    let t = v as i16;
    if (-32768.0..=32767.0).contains(&v) && f64::from(t) == v {
        encode_int(out, i32::from(t));
        return;
    }
    let nibbles = bcd_nibbles(&format_g8(v));
    out.push(30);
    for pair in nibbles.chunks(2) {
        let hi = pair.first().copied().unwrap_or(0x0F);
        let lo = pair.get(1).copied().unwrap_or(0x0F);
        out.push(hi.wrapping_shl(4) | (lo & 0x0F));
    }
}

/// `snprintf (buf, 16, "%.8G", v)` in the C locale.
fn format_g8(v: f64) -> Vec<u8> {
    use alloc::string::String;
    use core::fmt::Write;
    if v.is_nan() {
        return b"NAN".to_vec();
    }
    if v.is_infinite() {
        return if v < 0.0 {
            b"-INF".to_vec()
        } else {
            b"INF".to_vec()
        };
    }
    // Eight significant digits, correctly rounded: d.dddddddE±x.
    let mut e = String::new();
    let _ = write!(e, "{v:.7e}");
    let (mantissa, exp) = e.split_once('e').unwrap_or((e.as_str(), "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let negative = mantissa.starts_with('-');
    let digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    let mut out = Vec::new();
    if negative {
        out.push(b'-');
    }
    if (-4..8).contains(&exp) {
        // %f with 7 - exp decimals, trailing zeros removed.
        let mut int_part = Vec::new();
        let mut frac = Vec::new();
        if exp >= 0 {
            let cut = usize::try_from(exp).unwrap_or(0).saturating_add(1);
            int_part.extend_from_slice(digits.get(..cut).unwrap_or(&digits));
            frac.extend_from_slice(digits.get(cut..).unwrap_or(&[]));
        } else {
            int_part.push(b'0');
            let zeros = usize::try_from(exp.saturating_neg().saturating_sub(1)).unwrap_or(0);
            frac.extend(core::iter::repeat_n(b'0', zeros));
            frac.extend_from_slice(&digits);
        }
        while frac.last() == Some(&b'0') {
            frac.pop();
        }
        out.extend_from_slice(&int_part);
        if !frac.is_empty() {
            out.push(b'.');
            out.extend_from_slice(&frac);
        }
    } else {
        let mut frac: Vec<u8> = digits.get(1..).unwrap_or(&[]).to_vec();
        while frac.last() == Some(&b'0') {
            frac.pop();
        }
        out.push(digits.first().copied().unwrap_or(b'0'));
        if !frac.is_empty() {
            out.push(b'.');
            out.extend_from_slice(&frac);
        }
        out.push(b'E');
        out.push(if exp < 0 { b'-' } else { b'+' });
        let a = exp.unsigned_abs();
        let mut s = String::new();
        let _ = write!(s, "{a:02}");
        out.extend_from_slice(s.as_bytes());
    }
    out
}

/// The rewriting of `encode_num_tp` from the printed number to nibbles.
fn bcd_nibbles(printed: &[u8]) -> Vec<u8> {
    use alloc::string::String;
    use core::fmt::Write;
    let mut s: Vec<u8> = printed.to_vec();
    if s.starts_with(b"0.") {
        s.remove(0);
    } else if s.starts_with(b"-0.") {
        s.remove(1);
    } else if s.len() > 3 && s.ends_with(b"000") {
        let zeros = s.iter().rev().take_while(|&&c| c == b'0').count();
        s.truncate(s.len().saturating_sub(zeros));
        let mut e = String::new();
        let _ = write!(e, "E{zeros}");
        s.extend_from_slice(e.as_bytes());
    } else if let (Some(dot), Some(e)) = (
        s.iter().position(|&c| c == b'.'),
        s.iter().position(|&c| c == b'E'),
    ) && dot < e
    {
        let exponent: i32 = core::str::from_utf8(s.get(e.saturating_add(1)..).unwrap_or(&[]))
            .ok()
            .and_then(|t| t.parse().ok())
            .unwrap_or(0);
        let moved = i32::try_from(e.saturating_sub(dot).saturating_sub(1)).unwrap_or(0);
        let new_exponent = exponent.saturating_sub(moved);
        let mut t: Vec<u8> = s.get(..dot).unwrap_or(&[]).to_vec();
        t.extend_from_slice(s.get(dot.saturating_add(1)..e).unwrap_or(&[]));
        if new_exponent == 1 {
            t.push(b'0');
        } else {
            let mut x = String::new();
            let _ = write!(x, "E{new_exponent}");
            t.extend_from_slice(x.as_bytes());
        }
        s = t;
    }
    let sign = usize::from(s.first() == Some(&b'-'));
    if s.get(sign) == Some(&b'.') && s.get(sign.saturating_add(1)) == Some(&b'0') {
        // ".00123" → "123E-5": the exponent is the count of the
        // fraction's digits.
        let fraction = s.len().saturating_sub(sign).saturating_sub(1);
        let digits: Vec<u8> = s
            .get(sign.saturating_add(1)..)
            .unwrap_or(&[])
            .iter()
            .copied()
            .skip_while(|&c| c == b'0')
            .collect();
        let mut t: Vec<u8> = s.get(..sign).unwrap_or(&[]).to_vec();
        t.extend_from_slice(&digits);
        let mut x = String::new();
        let _ = write!(x, "E-{fraction}");
        t.extend_from_slice(x.as_bytes());
        s = t;
    }
    let mut nibbles = Vec::new();
    let mut i = 0usize;
    while let Some(&c) = s.get(i) {
        i = i.saturating_add(1);
        match c {
            b'E' => {
                if s.get(i) == Some(&b'-') {
                    i = i.saturating_add(1);
                    nibbles.push(0x0C);
                } else {
                    if s.get(i) == Some(&b'+') {
                        i = i.saturating_add(1);
                    }
                    nibbles.push(0x0B);
                }
                if s.get(i) == Some(&b'0') {
                    i = i.saturating_add(1);
                }
            }
            b'.' => nibbles.push(0x0A),
            b'-' => nibbles.push(0x0E),
            c => nibbles.push(c.wrapping_sub(b'0')),
        }
    }
    nibbles.push(0x0F);
    if nibbles.len() % 2 != 0 {
        nibbles.push(0x0F);
    }
    nibbles
}

/// `cff2_private_blend_encoder_param_t` without partial instancing: the
/// state kept across the operators of one Private DICT.
struct BlendParam<'a, 'c> {
    store: Store<'a>,
    coords: &'c [i32],
    seen_blend: bool,
    ivs: u32,
    region_count: u32,
    scalars: Vec<f32>,
}

impl BlendParam<'_, '_> {
    /// `blend_deltas`.
    fn blend_deltas(&self, deltas: &[f64]) -> f64 {
        if self.scalars.len() != deltas.len() {
            return 0.0;
        }
        self.scalars
            .iter()
            .zip(deltas)
            .fold(0.0, |v, (&s, &d)| v + f64::from(s) * d)
    }
}

/// `cff2_private_dict_blend_opset_t` over one recorded operator's bytes
/// (`cff2_private_dict_op_serializer_t` when pinned): the operands with
/// their blends evaluated, then the operator; `vsindex` is dropped.
fn reinterpret_private_op(out: &mut Vec<u8>, bytes: &[u8], param: &mut BlendParam<'_, '_>) -> bool {
    /// The Private DICT operators written with their operands.
    const HINTS: [u32; 13] = [10, 11, 265, 266, 267, 274, 273, 6, 7, 8, 9, 268, 269];
    let mut stack: Vec<f64> = Vec::new();
    let mut off = 0usize;
    while off < bytes.len() {
        let Some((tok, next)) = dict_token(bytes, off) else {
            return false;
        };
        off = next;
        let op = match tok {
            DictToken::Number(v) => {
                if stack.len() >= 513 {
                    return false;
                }
                stack.push(v);
                continue;
            }
            DictToken::Operator(op) => op,
        };
        match op {
            dict::VSINDEX => {
                let Some(ivs) = stack.pop().and_then(|v| {
                    (v >= f64::from(i32::MIN) && v <= f64::from(i32::MAX))
                        .then(|| super::cs::to_int(v))
                        .and_then(|i| u32::try_from(i).ok())
                }) else {
                    return false;
                };
                param.ivs = ivs;
                stack.clear();
                continue;
            }
            dict::BLEND => {
                if !process_blend(&mut stack, param) {
                    return false;
                }
                continue;
            }
            op if HINTS.contains(&op) => {}
            _ => stack.clear(),
        }
        for &v in &stack {
            encode_num_tp(out, v);
        }
        encode_op(out, op);
        stack.clear();
    }
    true
}

/// `cff2_private_dict_blend_opset_t::process_blend` when pinned: each
/// blended value becomes its rounded value at the coordinates.
fn process_blend(stack: &mut Vec<f64>, param: &mut BlendParam<'_, '_>) -> bool {
    if !param.seen_blend {
        param.region_count = param.store.region_count(param.ivs);
        param.scalars = param
            .store
            .scalars(param.ivs, param.coords, param.region_count);
        param.seen_blend = true;
    }
    let k = param.region_count;
    let Some(n) = stack.pop().and_then(|v| {
        (v >= f64::from(i32::MIN) && v <= f64::from(i32::MAX))
            .then(|| super::cs::to_int(v))
            .and_then(|i| u32::try_from(i).ok())
    }) else {
        return false;
    };
    let count = stack.len();
    let Some(total) = k
        .checked_add(1)
        .and_then(|k1| k1.checked_mul(n))
        .map(|t| t as usize)
        .filter(|&t| t <= count)
    else {
        return false;
    };
    let start = count.saturating_sub(total);
    let (n, k) = (n as usize, k as usize);
    for i in 0..n {
        let from = start.saturating_add(n).saturating_add(i.saturating_mul(k));
        let d = param.blend_deltas(stack.get(from..from.saturating_add(k)).unwrap_or(&[]));
        if let Some(v) = stack.get_mut(start.saturating_add(i)) {
            *v = f64::from(hb_clamp_to_i32(c_round(*v + d)));
        }
    }
    stack.truncate(count.saturating_sub(k.saturating_mul(n)));
    true
}

/// A Private DICT (`PrivateDict::serialize` with
/// `cff2_private_dict_op_serializer_t`): hints dropped with
/// `drop_hints`, no `Subrs` (the charstrings are desubroutinized), and
/// blends evaluated only for a pinned instance.
fn private_dict(
    s: &mut Serializer,
    acc: &Acc<'_>,
    fd: usize,
    drop_hints: bool,
    coords: &[i32],
    keep_variations: bool,
    subrs_link: ObjIdx,
) -> bool {
    let Some((d, bytes)) = acc.privates.get(fd) else {
        return false;
    };
    let mut param = BlendParam {
        store: acc.fonts.store,
        coords,
        seen_blend: false,
        ivs: 0,
        region_count: 0,
        scalars: Vec::new(),
    };
    for o in &d.ops {
        if drop_hints && dict::is_hint_op(o.op) {
            continue;
        }
        if o.op == dict::SUBRS {
            if subrs_link != 0 {
                link2_op(s, o.op, subrs_link);
            }
            continue;
        }
        if keep_variations {
            s.extend(bytes.get(o.start..o.end).unwrap_or(&[]));
            continue;
        }
        let mut out = Vec::new();
        let ok = reinterpret_private_op(
            &mut out,
            bytes.get(o.start..o.end).unwrap_or(&[]),
            &mut param,
        );
        s.extend(&out);
        if !ok {
            return false;
        }
    }
    !s.in_error()
}

/// The Private DICTs (in reverse order, as HarfBuzz packs them), with
/// `extra` appended to each: `(size, link)` by new Font DICT.
fn privates(
    s: &mut Serializer,
    acc: &Acc<'_>,
    p: &Cff2Plan,
    coords: &[i32],
    extra: &[u8],
) -> Option<Vec<(usize, ObjIdx)>> {
    let mut infos = vec![(0usize, 0 as ObjIdx); p.subset_fdcount as usize];
    for i in (0..acc.privates.len()).rev() {
        let fd = u32::try_from(i).unwrap_or(u32::MAX);
        if !p.fdmap.has(fd) {
            continue;
        }
        let mut subrs_link = 0;
        if let Some(local) = p.subrs.as_ref().and_then(|k| k.locals.get(i))
            && !local.is_empty()
        {
            s.push();
            if !index_n(s, local, 0, true) {
                s.pop_discard();
                return None;
            }
            subrs_link = s.pop_pack(true);
        }
        s.push();
        if !private_dict(
            s,
            acc,
            i,
            p.drop_hints,
            coords,
            p.keep_variations,
            subrs_link,
        ) {
            s.pop_discard();
            return None;
        }
        s.extend(extra);
        let size = s.len();
        let link = s.pop_pack(true);
        if let Some(slot) = infos.get_mut(p.fdmap.get(fd) as usize) {
            *slot = (size, link);
        }
    }
    Some(infos)
}

/// The FDArray (`FDArray::serialize` with `cff_font_dict_op_serializer_t`).
fn fd_array(
    s: &mut Serializer,
    acc: &Acc<'_>,
    p: &Cff2Plan,
    infos: &[(usize, ObjIdx)],
    wide: bool,
) -> Option<ObjIdx> {
    s.push();
    s.push();
    let mut sizes = Vec::new();
    let kept = (0u32..)
        .zip(&acc.font_dicts)
        .filter(|(i, _)| p.fdmap.has(*i))
        .map(|(_, d)| d);
    for ((d, b), &(size, link)) in kept.zip(infos) {
        let start = s.len();
        for o in &d.ops {
            if o.op == dict::PRIVATE {
                int2(s, i64::try_from(size).unwrap_or(i64::MAX));
                link4_op(s, o.op, link, Whence::Absolute);
            } else {
                s.extend(b.get(o.start..o.end).unwrap_or(&[]));
            }
        }
        sizes.push(s.len().saturating_sub(start));
    }
    s.pop_pack(false);
    if !index_header_n(s, &sizes, 0, wide) {
        s.pop_discard();
        return None;
    }
    Some(s.pop_pack(false))
}

/// `accelerator_subset_t::serialize` without subroutines, preserving the
/// Variation Store when no axes are pinned.
fn serialize2(s: &mut Serializer, acc: &Acc<'_>, p: &Cff2Plan, coords: &[i32]) -> bool {
    s.push();
    if !index_n(s, &p.charstrings, p.min_charstrings_off_size, true) {
        s.pop_discard();
        return false;
    }
    let char_strings = s.pop_pack(false);
    let Some(infos) = privates(s, acc, p, coords, &[]) else {
        return false;
    };
    let mut fd_select_link = 0;
    if let Some((format, ranges)) = &p.fdselect {
        s.push();
        fdselect(s, p.num_glyphs, *format, ranges);
        fd_select_link = s.pop_pack(true);
    }
    let Some(fd_array_link) = fd_array(s, acc, p, &infos, true) else {
        return false;
    };
    let var_store_link = if p.keep_variations
        && let Some(at) = acc.fonts.store.at
    {
        let size = usize::from(acc.fonts.store.size());
        let Some(data) = acc
            .fonts
            .store
            .data
            .get(at..at.saturating_add(2).saturating_add(size))
        else {
            return false;
        };
        s.push();
        s.extend(data);
        s.pop_pack(false)
    } else {
        0
    };
    let header = s.extend(&[2, 0, 5, 0, 0]);
    let start = s.len();
    for o in &acc.top.ops {
        match o.op {
            dict::VSTORE if p.keep_variations => {
                link4_op(s, o.op, var_store_link, Whence::Absolute)
            }
            dict::VSTORE => {}
            dict::CHAR_STRINGS => link4_op(s, o.op, char_strings, Whence::Absolute),
            dict::FD_ARRAY => link4_op(s, o.op, fd_array_link, Whence::Absolute),
            dict::FD_SELECT => link4_op(s, o.op, fd_select_link, Whence::Absolute),
            _ => {
                s.extend(acc.top_bytes.get(o.start..o.end).unwrap_or(&[]));
            }
        }
        if s.in_error() {
            return false;
        }
    }
    let size = s.len().saturating_sub(start);
    if !s.check_assign_u16(header.saturating_add(3), size as u64, Errors::INT_OVERFLOW) {
        return false;
    }
    index_n(
        s,
        p.subrs.as_ref().map_or(&[][..], |k| k.globals.as_slice()),
        0,
        true,
    )
}

/// `serialize_cff2_to_cff1`: the instance as a CID-keyed CFF font.
fn serialize1(s: &mut Serializer, acc: &Acc<'_>, p: &Cff2Plan, plan: &Plan) -> bool {
    let metrics = &plan.var.metrics;
    let width = |g: u32| metrics.hmtx.get(&g).map_or(0, |m| m.0);
    let widths: Vec<u32> = (0..p.num_glyphs).map(width).collect();
    let (default_width, nominal_width) = specialize::optimize_widths(&widths);

    // CharStrings, with the widths and endchar.
    s.push();
    let mut items = Vec::with_capacity(p.charstrings.len());
    for (i, cs) in (0u32..).zip(&p.charstrings) {
        let mut out = Vec::new();
        let w = width(i);
        if w != default_width {
            encode_int(
                &mut out,
                w.cast_signed().wrapping_sub(nominal_width.cast_signed()),
            );
        }
        match p.commands.get(i as usize) {
            Some(c) if !c.is_empty() => {
                let mut c = c.clone();
                specialize::specialize(&mut c, 48);
                specialize::encode(&c, &mut out);
            }
            _ => out.extend_from_slice(cs),
        }
        if !matches!(out.last(), Some(&ENDCHAR_BYTE | &0x0B)) {
            out.push(ENDCHAR_BYTE);
        }
        items.push(out);
    }
    if !index(s, &items, p.min_charstrings_off_size) {
        s.pop_discard();
        return false;
    }
    let char_strings = s.pop_pack(false);

    // Private DICTs with the widths.
    let mut widths_ops = Vec::new();
    encode_int(&mut widths_ops, default_width.cast_signed());
    encode_op(&mut widths_ops, dict::DEFAULT_WIDTH_X);
    encode_int(&mut widths_ops, nominal_width.cast_signed());
    encode_op(&mut widths_ops, dict::NOMINAL_WIDTH_X);
    let coords = plan.var.n.normalized_coords.as_slice();
    let Some(infos) = privates(s, acc, p, coords, &widths_ops) else {
        return false;
    };
    let Some(fd_array_link) = fd_array(s, acc, p, &infos, false) else {
        return false;
    };

    // FDSelect: CFF requires one.
    s.push();
    match &p.fdselect {
        Some((format, ranges)) => fdselect(s, p.num_glyphs, *format, ranges),
        None => fdselect(s, p.num_glyphs, 3, &[(0, 0)]),
    }
    let fd_select_link = s.pop_pack(true);

    // An identity charset (format 2).
    s.push();
    s.u8(2);
    if p.num_glyphs > 1 {
        let n_left = p.num_glyphs.saturating_sub(2);
        if n_left > 0xFFFF {
            s.pop_discard();
            return false;
        }
        s.u16(1);
        s.u16(u16::try_from(n_left).unwrap_or(0));
    }
    let charset_link = s.pop_pack(true);

    // Global subroutines.
    s.push();
    if !index(s, &[], 0) {
        s.pop_discard();
        return false;
    }
    s.pop_pack(false);

    // String INDEX: the registry and ordering of ROS.
    s.push();
    if !index(s, &[b"Adobe".to_vec(), b"Identity".to_vec()], 0) {
        s.pop_discard();
        return false;
    }
    s.pop_pack(false);

    // Header and Name INDEX.
    s.extend(&[1, 0, 4, 4]);
    if !index_header(s, &[CFF1_FONT_NAME.len()], 0) {
        return false;
    }
    s.extend(CFF1_FONT_NAME);

    // Top DICT.
    s.push();
    let mut head = Vec::new();
    for v in [391, 392, 0] {
        encode_int(&mut head, v);
    }
    encode_op(&mut head, dict::ROS);
    let hm = &metrics.head_maxp;
    for v in [hm.x_min, hm.y_min, hm.x_max, hm.y_max] {
        encode_int(&mut head, v);
    }
    encode_op(&mut head, dict::FONT_BBOX);
    s.extend(&head);
    if charset_link != 0 {
        link4_op(s, dict::CHARSET, charset_link, Whence::Absolute);
    }
    if fd_select_link != 0 {
        link4_op(s, dict::FD_SELECT, fd_select_link, Whence::Absolute);
    }
    if fd_array_link != 0 {
        link4_op(s, dict::FD_ARRAY, fd_array_link, Whence::Absolute);
    }
    for o in &acc.top.ops {
        match o.op {
            dict::VSTORE | dict::FD_ARRAY | dict::FD_SELECT => {}
            dict::CHAR_STRINGS => link4_op(s, o.op, char_strings, Whence::Absolute),
            _ => {
                s.extend(acc.top_bytes.get(o.start..o.end).unwrap_or(&[]));
            }
        }
        if s.in_error() {
            s.pop_discard();
            return false;
        }
    }
    let top_size = s.len();
    s.pop_pack(false);
    index_header(s, &[top_size], 0) && !s.in_error()
}

/// `cff2::accelerator_subset_t::subset` with every axis pinned or all
/// subroutines flattened. With
/// `DOWNGRADE_CFF2` the CFF table is added to `tables` and CFF2 is not
/// needed.
pub(crate) fn subset(
    plan: &Plan,
    font: &Font<'_>,
    tag: Tag,
    _src: &[u8],
    s: &mut Serializer,
    tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let acc = Acc::new(font).ok_or(SubsetError::Failed { tag })?;
    let p = create(plan, &acc, tag)?;
    if plan.flags.has(Flags::DOWNGRADE_CFF2) {
        if serialize1(s, &acc, &p, plan) && !s.in_error() {
            let done = core::mem::replace(s, Serializer::new());
            // Keep the room the serializer needed (see `room`).
            s.zeros(done.peak_len());
            let bytes = done
                .end()
                .ok()
                .and_then(|packed| packed.resolve().ok())
                .ok_or(SubsetError::Failed { tag })?;
            tables.insert(Tag::new(b"CFF "), bytes);
        }
        return Ok(false);
    }
    Ok(serialize2(
        s,
        &acc,
        &p,
        plan.var.n.normalized_coords.as_slice(),
    ))
}

/// `update_instance_metrics_map_from_cff2`: the boxes, advances and side
/// bearings of the kept glyphs at the instance, from CFF2 and hmtx and
/// vmtx (whose HVAR and VVAR deltas are 0 at the default location). Does
/// nothing without a CFF2 table HarfBuzz reads.
pub(crate) fn instance_metrics(
    font: &Font<'_>,
    new_to_old: &[(u32, u32)],
    coords: &[i32],
    m: &mut Metrics,
) {
    let Some(hb) = HbCff2::new(font) else {
        return;
    };
    let fonts = Fonts::new(hb);
    let hmtx = MetricsAccel::dual(font, false);
    let vmtx = MetricsAccel::dual(font, true);
    let clamp_advance = |v: u32| v.min(0xFFFF);
    for &(new, old) in new_to_old {
        let Some(e) = fonts.extents(old, coords) else {
            continue;
        };
        let has_bounds = !(e.x_bearing == 0 && e.width == 0 && e.height == 0 && e.y_bearing == 0);
        if has_bounds {
            let h = &mut m.head_maxp;
            h.x_min = h.x_min.min(e.x_bearing);
            h.x_max = h.x_max.max(e.x_bearing.saturating_add(e.width));
            h.y_max = h.y_max.max(e.y_bearing);
            h.y_min = h.y_min.min(e.y_bearing.saturating_add(e.height));
        }
        if hmtx.has_data() {
            let lsb = if has_bounds {
                e.x_bearing
            } else {
                hmtx.side_bearing(old)
            };
            m.hmtx.insert(new, (clamp_advance(hmtx.advance(old)), lsb));
            if has_bounds && let Some(w) = m.bounds_width.get_mut(new as usize) {
                *w = e.width.cast_unsigned();
            }
        }
        if vmtx.has_data() {
            let tsb = if has_bounds {
                let (_, vorg_y) = font
                    .default_instance()
                    .v_origin(GlyphId(old), |g| fonts.extents(g.0, coords));
                vorg_y.saturating_sub(e.y_bearing)
            } else {
                vmtx.side_bearing(old)
            };
            m.vmtx.insert(new, (clamp_advance(vmtx.advance(old)), tsb));
            if has_bounds && let Some(h) = m.bounds_height.get_mut(new as usize) {
                *h = e.height.saturating_neg().cast_unsigned();
            }
        }
    }
    m.head_maxp.settle();
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::{bcd_nibbles, encode_num_tp, format_g8};

    #[test]
    fn printf_g8() {
        // The values of the C library's `printf ("%.8G")`.
        for (v, s) in [
            (0.039625, "0.039625"),
            (0.00001, "1E-05"),
            (123_456_789.0, "1.2345679E+08"),
            (0.5, "0.5"),
            (12_345_678.5, "12345678"),
            (12_345_677.5, "12345678"),
            (-0.06, "-0.06"),
            (1.5e-7, "1.5E-07"),
            (99_999_999.5, "1E+08"),
            (32768.25, "32768.25"),
            (-40000.125, "-40000.125"),
            (1e20, "1E+20"),
            (0.000_123_456_789, "0.00012345679"),
        ] {
            assert_eq!(format_g8(v), s.as_bytes(), "{v}");
        }
    }

    #[test]
    fn bcd_as_fonttools_writes_it() {
        let n = |s: &str| bcd_nibbles(s.as_bytes());
        // ".039625" → "39625E-6".
        assert_eq!(n("0.039625"), [3, 9, 6, 2, 5, 0xC, 6, 0xF]);
        // "-.06" → "-6E-2".
        assert_eq!(n("-0.06"), [0xE, 6, 0xC, 2, 0xF, 0xF]);
        // Trailing zeros of an integer become an exponent.
        assert_eq!(n("40000"), [4, 0xB, 4, 0xF]);
        // The decimal point moves into the exponent; an exponent of 1
        // becomes a zero digit, one of 0 stays as "E".
        assert_eq!(n("1.2345679E+08"), [1, 2, 3, 4, 5, 6, 7, 9, 0, 0xF]);
        assert_eq!(
            n("1.2345679E+09"),
            [1, 2, 3, 4, 5, 6, 7, 9, 0xB, 2, 0xF, 0xF]
        );
        assert_eq!(n("1.5E+01"), [1, 5, 0xB, 0xF]);
        assert_eq!(n("1E-05"), [1, 0xC, 5, 0xF]);
        assert_eq!(n("32768.25"), [3, 2, 7, 6, 8, 0xA, 2, 5, 0xF, 0xF]);
        let mut out = alloc::vec::Vec::new();
        encode_num_tp(&mut out, 0.039625);
        assert_eq!(out, [30, 0x39, 0x62, 0x5C, 0x6F]);
        out.clear();
        encode_num_tp(&mut out, -107.0);
        assert_eq!(out, [32]);
    }
}
