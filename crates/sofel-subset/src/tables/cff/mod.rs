//! CFF (`src/hb-subset-cff1.cc`, `src/hb-subset-cff-common.{hh,cc}`,
//! `src/hb-ot-cff1-table.{hh,cc}`): the subsetting plan (`cff1_subset_plan`)
//! and the serialization (`accelerator_subset_t::serialize`), in
//! HarfBuzz's object order; and the `seac` closure of the plan
//! (`_add_cff_seac_components`).

mod cff2;
pub(crate) mod cs;
mod cs2;
mod dict;
mod projection;
mod specialize;

pub(crate) use cff2::{
    instance_metrics, subset as subset2, supports_subroutines, uses_subroutines,
};

use alloc::borrow::Cow;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use alloc::vec::Vec;

use sofel_font::read::Tag;
use sofel_font::read::be::{u8_at, u16_at};
use sofel_font::read::cff::hb::{
    DictKind, FdSelectCounts, HbDict, HbFdSelect, HbIndex, Sanitizer, UNDEF_SID, interpret_dict,
};
use sofel_font::read::cff::{NUM_STANDARD_STRINGS, Version};
use sofel_font::{Font, HbCff1};
use sofel_write::{Errors, ObjIdx, Serializer, Whence};

use crate::error::SubsetError;
use crate::input::Flags;
use crate::plan::Plan;
use crate::set::INVALID;
use crate::tables::Tables;

use cs::{Env, Flatten, Kind, Parsed, Programs, Seac, SubrSubset};
use dict::{Dict, DictOp};

/// `CFF_UNDEF_CODE`.
const UNDEF_CODE: u32 = 0xFFFF_FFFF;

/// HarfBuzz's subset accelerator (`OT::cff1::accelerator_subset_t`):
/// [`HbCff1`] and what the subsetter reads besides.
struct Acc<'a> {
    hb: HbCff1<'a>,
    data: &'a [u8],
    names: &'a [u8],
    top_bytes: &'a [u8],
    top: Dict,
    offsets: HbDict,
    strings: HbIndex<'a>,
    gsubrs: HbIndex<'a>,
    fd_select: HbFdSelect<'a>,
    fd_count: u32,
    /// The Font DICTs of a CID-keyed font, with their bytes.
    font_dicts: Vec<(Dict, &'a [u8])>,
    /// Each Font DICT's Private DICT, its bytes and its local subroutines.
    privates: Vec<(Dict, &'a [u8], HbIndex<'a>)>,
}

/// The Private DICT bytes at `offset` with `size` bytes
/// (`StructAtOffsetOrNull<UnsizedByteStr>`).
pub(super) fn private_bytes<'a>(
    data: &'a [u8],
    sc: &mut Sanitizer,
    offset: i32,
    size: u32,
) -> Option<&'a [u8]> {
    let at = sc
        .struct_at(0, offset)
        .filter(|&p| sc.check_array(p, size, 1));
    match at {
        Some(p) => data
            .get(usize::try_from(p).ok()?..)?
            .get(..usize::try_from(size).ok()?),
        None if size != 0 => None,
        None => Some(&[]),
    }
}

impl<'a> Acc<'a> {
    /// `None` exactly when HarfBuzz's accelerator is not valid.
    fn new(font: &Font<'a>) -> Option<Acc<'a>> {
        let hb = HbCff1::new(font)?;
        let data = hb.data();
        let mut sc = Sanitizer::unlimited(data.len());
        let header_size = i64::from(*data.get(2)?);
        let names_index = HbIndex::sanitize(data, header_size, Version::Cff, &mut sc)?;
        let names_len = usize::try_from(names_index.size_offset()).ok()?;
        let names_at = usize::try_from(header_size).ok()?;
        let names = data.get(names_at..)?.get(..names_len)?;
        let top_index = sc
            .struct_at(names_index.position(), names_index.size_offset())
            .and_then(|p| HbIndex::sanitize(data, p, Version::Cff, &mut sc))?;
        let top_bytes = top_index.get(0)?;
        let top = dict::parse(top_bytes, dict::Kind::Top)?;
        let offsets = interpret_dict(top_bytes, DictKind::Cff1Top)?;
        let strings = sc
            .struct_at(top_index.position(), top_index.size_offset())
            .and_then(|p| HbIndex::sanitize(data, p, Version::Cff, &mut sc))?;
        let gsubrs = hb.global_subrs();
        let num_glyphs = hb.num_glyphs();
        let mut font_dicts = Vec::new();
        let mut privates = Vec::new();
        let (fd_select, fd_count) = if hb.is_cid() {
            let fd_array = sc
                .struct_at(0, offsets.fd_array)
                .and_then(|p| HbIndex::sanitize(data, p, Version::Cff, &mut sc))?;
            let counts = FdSelectCounts {
                fd_count: fd_array.len(),
                glyphs: num_glyphs,
                face_glyphs: num_glyphs,
            };
            let fd_select = sc
                .struct_at(0, offsets.fd_select)
                .and_then(|p| HbFdSelect::sanitize(data, p, Version::Cff, counts, &mut sc))?;
            for i in 0..fd_array.len() {
                let bytes = fd_array.get(i)?;
                let fd = dict::parse(bytes, dict::Kind::Font)?;
                let info = interpret_dict(bytes, DictKind::Cff1Font)?;
                let p = private_bytes(data, &mut sc, info.private_offset, info.private_size)?;
                privates.push((dict::parse(p, dict::Kind::Private)?, p, hb.local_subrs(i)));
                font_dicts.push((fd, bytes));
            }
            (fd_select, fd_array.len())
        } else {
            let p = private_bytes(data, &mut sc, offsets.private_offset, offsets.private_size)?;
            privates.push((dict::parse(p, dict::Kind::Private)?, p, hb.local_subrs(0)));
            (HbFdSelect::null(), 1)
        };
        Some(Acc {
            hb,
            data,
            names,
            top_bytes,
            top,
            offsets,
            strings,
            gsubrs,
            fd_select,
            fd_count,
            font_dicts,
            privates,
        })
    }

    fn is_cid(&self) -> bool {
        self.hb.is_cid()
    }

    fn num_glyphs(&self) -> u32 {
        self.hb.num_glyphs()
    }

    fn char_string(&self, glyph: u32) -> &'a [u8] {
        self.hb.char_string(glyph).unwrap_or(&[])
    }

    fn local_subrs(&self, fd: u32) -> HbIndex<'a> {
        self.privates
            .get(fd as usize)
            .map_or(HbIndex::null(), |p| p.2)
    }

    /// `subset_get_fd`.
    fn fd(&self, glyph: u32) -> u32 {
        if glyph < self.num_glyphs() {
            self.fd_select.fd(glyph)
        } else {
            0
        }
    }

    fn is_predef_charset(&self) -> bool {
        self.offsets.charset <= 2
    }

    fn is_predef_encoding(&self) -> bool {
        self.offsets.encoding <= 1
    }

    /// The encoding's format byte position (non-predefined encodings).
    fn encoding_at(&self) -> Option<usize> {
        if self.is_predef_encoding() || self.is_cid() {
            return None;
        }
        usize::try_from(self.offsets.encoding).ok()
    }

    /// `glyph_to_code` with a custom encoding (`Encoding::get_code`).
    fn glyph_to_code(&self, glyph: u32) -> u32 {
        if glyph >= self.num_glyphs() {
            return UNDEF_CODE;
        }
        let Some(p) = self.encoding_at() else {
            return 0;
        };
        let d = self.data;
        let format = u8_at(d, p).unwrap_or(0) & 0x7F;
        let n = u32::from(u8_at(d, p.saturating_add(1)).unwrap_or(0));
        let g = glyph.saturating_sub(1);
        match format {
            0 => {
                if g < n {
                    u8_at(d, p.saturating_add(2).saturating_add(g as usize)).map_or(0, u32::from)
                } else {
                    UNDEF_CODE
                }
            }
            1 => {
                let mut g = g;
                for i in 0..n as usize {
                    let r = p.saturating_add(2).saturating_add(i.saturating_mul(2));
                    let first = u32::from(u8_at(d, r).unwrap_or(0));
                    let n_left = u32::from(u8_at(d, r.saturating_add(1)).unwrap_or(0));
                    if g <= n_left {
                        let code = first.saturating_add(g);
                        return if code < 0x100 { code } else { UNDEF_CODE };
                    }
                    g = g.saturating_sub(n_left.saturating_add(1));
                }
                UNDEF_CODE
            }
            _ => 0,
        }
    }

    /// `Encoding::get_supplement_codes`.
    fn supplement_codes(&self, sid: u32) -> Vec<u32> {
        let Some(p) = self.encoding_at() else {
            return Vec::new();
        };
        let d = self.data;
        let format = u8_at(d, p).unwrap_or(0);
        if format & 0x80 == 0 {
            return Vec::new();
        }
        let n = usize::from(u8_at(d, p.saturating_add(1)).unwrap_or(0));
        let size = if format & 0x7F == 0 {
            n
        } else {
            n.saturating_mul(2)
        };
        let s = p.saturating_add(2).saturating_add(size);
        let count = usize::from(u8_at(d, s).unwrap_or(0));
        (0..count)
            .filter_map(|i| {
                let r = s.saturating_add(1).saturating_add(i.saturating_mul(3));
                let code = u32::from(u8_at(d, r)?);
                let glyph = u32::from(u16_at(d, r.saturating_add(1))?);
                (glyph == sid).then_some(code)
            })
            .collect()
    }
}

/// `_cff1_subrs_may_end_in_seac`.
fn subrs_may_end_in_seac(subrs: HbIndex<'_>) -> bool {
    (0..subrs.len()).any(|i| subrs.get(i).unwrap_or(&[]).contains(&cs::ENDCHAR_BYTE))
}

/// `_cff1_charstring_may_end_in_seac`.
fn charstring_may_end_in_seac(s: &[u8], subrs_may_end_in_seac: bool) -> bool {
    if subrs_may_end_in_seac && s.iter().any(|&b| b == 10 || b == 29) {
        return true;
    }
    let Some((&14, rest)) = s.split_last() else {
        return false;
    };
    let end = rest.len();
    if end == 0 {
        return false;
    }
    let at = |back: usize| end.checked_sub(back).and_then(|i| s.get(i)).copied();
    let last = at(1).unwrap_or(0);
    if (32..=246).contains(&last) {
        return true;
    }
    if at(2).is_some_and(|b| (247..=254).contains(&b)) {
        return true;
    }
    if at(3) == Some(28) || at(5) == Some(255) {
        return true;
    }
    if last == 10 || last == 29 {
        return true;
    }
    at(2) == Some(12)
}

/// The `seac` closure of the plan (`_add_cff_seac_components` with
/// `accelerator_subset_t::get_seac_components`).
pub(crate) struct SeacFinder<'a> {
    hb: HbCff1<'a>,
    subrs_may_end_in_seac: bool,
}

impl<'a> SeacFinder<'a> {
    /// `None` when the font has no CFF table HarfBuzz reads.
    pub fn new(font: &Font<'a>) -> Option<SeacFinder<'a>> {
        let hb = HbCff1::new(font)?;
        // CID-keyed fonts have no seac (`get_seac_components`), so only
        // the subroutines of name-keyed fonts matter.
        let may = !hb.is_cid()
            && (subrs_may_end_in_seac(hb.global_subrs())
                || subrs_may_end_in_seac(hb.local_subrs(0)));
        Some(SeacFinder {
            hb,
            subrs_may_end_in_seac: may,
        })
    }

    /// The base and accent glyphs of `glyph`'s `seac`.
    pub fn components(&self, glyph: u32) -> Option<(u32, u32)> {
        if glyph >= self.hb.num_glyphs() || self.hb.is_cid() {
            return None;
        }
        let s = self.hb.char_string(glyph).unwrap_or(&[]);
        if !charstring_may_end_in_seac(s, self.subrs_may_end_in_seac) {
            return None;
        }
        let mut env = Env::new(s, self.hb.global_subrs(), self.hb.local_subrs(0));
        let mut seac = Seac::default();
        if !cs::interpret(&mut env, &mut seac) {
            return None;
        }
        let (b, a) = seac.codes?;
        let base = self.hb.std_code_to_glyph(b);
        let accent = self.hb.std_code_to_glyph(a);
        (base != 0 && accent != 0).then_some((base, accent))
    }
}

/// `remap_sid_t`.
#[derive(Default)]
struct SidMap {
    map: BTreeMap<u32, u32>,
    vector: Vec<u32>,
}

impl SidMap {
    fn add(&mut self, sid: u32) -> u32 {
        if sid < NUM_STANDARD_STRINGS || sid == UNDEF_SID {
            return sid;
        }
        let k = sid.wrapping_sub(NUM_STANDARD_STRINGS);
        let next = u32::try_from(self.vector.len()).unwrap_or(u32::MAX);
        let v = *self.map.entry(k).or_insert_with(|| {
            self.vector.push(k);
            next
        });
        v.wrapping_add(NUM_STANDARD_STRINGS)
    }

    fn get(&self, sid: u32) -> u32 {
        if sid < NUM_STANDARD_STRINGS || sid == UNDEF_SID {
            return sid;
        }
        let k = sid.wrapping_sub(NUM_STANDARD_STRINGS);
        self.map
            .get(&k)
            .copied()
            .unwrap_or(INVALID)
            .wrapping_add(NUM_STANDARD_STRINGS)
    }
}

/// `range_list_t::complete`: the glyph of each `(code, glyph)` range
/// becomes its count minus one; whether any needs two bytes.
fn complete(ranges: &mut [(u32, u32)], last_glyph: u32) -> bool {
    let mut last = last_glyph;
    let mut all = 0u32;
    for r in ranges.iter_mut().rev() {
        let n_left = last.wrapping_sub(r.1).wrapping_sub(1);
        all |= n_left;
        last = r.1;
        r.1 = n_left;
    }
    all >= 0x100
}

/// `hb_inc_bimap_t` of the Font DICTs.
pub(super) struct FdMap(BTreeMap<u32, u32>);

impl FdMap {
    pub(super) fn identity(n: u32) -> FdMap {
        FdMap((0..n).map(|i| (i, i)).collect())
    }
    pub(super) fn has(&self, fd: u32) -> bool {
        self.0.contains_key(&fd)
    }
    pub(super) fn get(&self, fd: u32) -> u32 {
        self.0.get(&fd).copied().unwrap_or(INVALID)
    }
}

/// `subr_remap_t`: old subroutine numbers to new ones and the new bias.
#[derive(Default)]
struct Remap {
    map: BTreeMap<u32, u32>,
    back: Vec<u32>,
    bias: i32,
}

impl Remap {
    fn new(closure: &BTreeSet<u32>) -> Remap {
        let back: Vec<u32> = closure.iter().copied().collect();
        let map = back.iter().copied().zip(0u32..).collect();
        let bias = cs::bias(u32::try_from(back.len()).unwrap_or(u32::MAX));
        Remap { map, back, bias }
    }

    fn biased(&self, old: u32) -> i32 {
        let new = self.map.get(&old).copied().unwrap_or(INVALID);
        i32::from_ne_bytes(new.to_ne_bytes()).wrapping_sub(self.bias)
    }
}

/// The plan's `(new, old)` list as HarfBuzz's subset plans walk it: from
/// new glyph 1, an old glyph for each new glyph, the new one itself for
/// the glyphs not kept.
fn old_glyphs(plan: &Plan) -> impl Iterator<Item = (u32, u32)> + '_ {
    let mut list = plan
        .glyphs
        .new_to_old
        .iter()
        .copied()
        .filter(|&(n, _)| n != 0)
        .peekable();
    (1..plan.num_output_glyphs).map(move |g| match list.peek() {
        Some(&(n, o)) if n == g => {
            list.next();
            (g, o)
        }
        _ => (g, g),
    })
}

/// The subsetting plan (`cff1_subset_plan`).
struct CffPlan {
    num_glyphs: u32,
    drop_hints: bool,
    desubroutinize: bool,
    min_charstrings_off_size: u32,
    top_ops: Vec<DictOp>,
    top_names: [u32; dict::NAME_COUNT],
    fdselect_ranges: Vec<(u32, u32)>,
    fdmap: FdMap,
    sidmap: SidMap,
    subset_charset: bool,
    charset_format: u8,
    charset_ranges: Vec<(u32, u32)>,
    subset_encoding: bool,
    enc_format: u8,
    enc_num_codes: u32,
    enc_ranges: Vec<(u32, u32)>,
    enc_supp: Vec<(u32, u32)>,
    charstrings: Vec<Vec<u8>>,
    globalsubrs: Vec<Vec<u8>>,
    localsubrs: Vec<Vec<Vec<u8>>>,
    /// `(old Font DICT, FontName)` of each kept Font DICT.
    fontdicts: Vec<(Option<usize>, u32)>,
}

/// `hb_plan_subset_cff_fdselect`: the kept Font DICT count, the
/// FDSelect ranges `(fd, first glyph)`, the Font DICT mapping and the
/// FDSelect format.
pub(super) struct FdPlan {
    pub count: u32,
    pub ranges: Vec<(u32, u32)>,
    pub fdmap: FdMap,
    pub format: u8,
}

/// `hb_plan_subset_cff_fdselect` for the source FDSelect `src` over
/// `src_num` glyphs and `fd_count` Font DICTs; `None` where HarfBuzz
/// fails.
pub(super) fn plan_fdselect(
    plan: &Plan,
    src: &HbFdSelect<'_>,
    src_num: u32,
    fd_count: u32,
) -> Option<FdPlan> {
    let n = plan.num_output_glyphs;
    let mut set = BTreeSet::new();
    let mut ranges = Vec::new();
    let mut prev = UNDEF_CODE;
    let list = &plan.glyphs.new_to_old;
    let mut it = 0usize;
    let cur = |i: usize| list.get(i).copied().unwrap_or((0, 0));
    let mut gid = 0u32;
    while gid < n {
        let mut item = cur(it);
        let old = if gid == item.0 {
            it = it.saturating_add(1);
            let o = item.1;
            item = cur(it);
            o
        } else {
            gid
        };
        let (fd, end) = if old < src_num {
            let (fd, end) = src.fd_range(old);
            (fd, end.min(src_num))
        } else {
            (0, INVALID)
        };
        if fd != prev {
            set.insert(fd);
            prev = fd;
            ranges.push((fd, gid));
            if gid == old {
                gid = item.0.wrapping_sub(1).min(end.wrapping_sub(1));
            }
        }
        gid = gid.wrapping_add(1);
    }
    let count = u32::try_from(set.len()).ok()?;
    let fdmap = if count == fd_count {
        FdMap::identity(fd_count)
    } else {
        FdMap(set.iter().copied().zip(0u32..).collect())
    };
    for r in &mut ranges {
        r.0 = fdmap.get(r.0);
    }
    let format = if count > 0xFF {
        if src.format() != 4 {
            return None;
        }
        4
    } else {
        3
    };
    Some(FdPlan {
        count,
        ranges,
        fdmap,
        format,
    })
}

/// `hb_serialize_cff_fdselect` (formats 3 and 4).
pub(super) fn fdselect(s: &mut Serializer, num_glyphs: u32, format: u8, ranges: &[(u32, u32)]) {
    s.u8(format);
    let lo16 = |v: u32| u16::try_from(v & 0xFFFF).unwrap_or(0);
    if format == 4 {
        s.u32(u32::try_from(ranges.len()).unwrap_or(u32::MAX));
        for &(fd, first) in ranges {
            s.u32(first);
            s.u16(lo16(fd));
        }
        s.u32(num_glyphs);
    } else {
        s.u16(lo16(u32::try_from(ranges.len()).unwrap_or(u32::MAX)));
        for &(fd, first) in ranges {
            s.u16(lo16(first));
            s.u8(u8::try_from(fd & 0xFF).unwrap_or(0));
        }
        s.u16(lo16(num_glyphs));
    }
}

/// `drop_hints_param_t`.
#[derive(Default)]
struct DropState {
    seen_moveto: bool,
    ends_in_hint: bool,
    all_dropped: bool,
}

/// `drop_hints_in_str`.
fn drop_hints_in_str(progs: &mut Programs<'_>, id: (Kind, u32), drop: &mut DropState) -> bool {
    let count = progs.get(id.0, id.1).map_or(0, |p| p.values.len());
    let mut seen_hint = false;
    for pos in 0..count {
        let Some(tok) = progs
            .get(id.0, id.1)
            .and_then(|p| p.values.get(pos))
            .copied()
        else {
            break;
        };
        let mut has_hint = false;
        let mark = |progs: &mut Programs<'_>, pos: usize| {
            if let Some(t) = progs.get(id.0, id.1).and_then(|p| p.values.get_mut(pos)) {
                t.hinting = true;
            }
        };
        match tok.op {
            cs::CALLSUBR => {
                has_hint = drop_hints_in_subr(progs, id, pos, (Kind::Local, tok.subr_num), drop);
            }
            cs::CALLGSUBR => {
                has_hint = drop_hints_in_subr(progs, id, pos, (Kind::Global, tok.subr_num), drop);
            }
            cs::RMOVETO | cs::HMOVETO | cs::VMOVETO => drop.seen_moveto = true,
            cs::HINTMASK | cs::CNTRMASK if drop.seen_moveto => mark(progs, pos),
            cs::HINTMASK | cs::CNTRMASK | cs::HSTEMHM | cs::VSTEMHM | cs::HSTEM | cs::VSTEM => {
                has_hint = true;
                mark(progs, pos);
                if progs.get(id.0, id.1).is_some_and(|p| p.at_end(pos)) {
                    drop.ends_in_hint = true;
                }
            }
            cs::DOTSECTION => mark(progs, pos),
            _ => {}
        }
        if has_hint {
            if let Some(p) = progs.get(id.0, id.1) {
                for t in p.values.iter_mut().take(pos).rev() {
                    if t.hinting {
                        break;
                    }
                    t.hinting = true;
                }
            }
            seen_hint = true;
        }
    }
    drop.all_dropped = true;
    if let Some(p) = progs.get(id.0, id.1) {
        for t in &p.values {
            if t.op == cs::RETURN {
                break;
            }
            if !t.hinting {
                drop.all_dropped = false;
                break;
            }
        }
    }
    seen_hint
}

/// `drop_hints_in_subr`.
fn drop_hints_in_subr(
    progs: &mut Programs<'_>,
    id: (Kind, u32),
    pos: usize,
    subr: (Kind, u32),
    drop: &mut DropState,
) -> bool {
    drop.ends_in_hint = false;
    let has_hint = drop_hints_in_str(progs, subr, drop);
    let Some(p) = progs.get(id.0, id.1) else {
        return has_hint;
    };
    if drop.ends_in_hint {
        if let Some(t) = p.values.get_mut(pos) {
            t.hinting = true;
        }
        if !p.at_end(pos) {
            drop.ends_in_hint = false;
        }
    } else if drop.all_dropped
        && let Some(t) = p.values.get_mut(pos)
    {
        t.hinting = true;
    }
    has_hint
}

/// `collect_subr_refs_in_str`: adds the subroutines that `p` calls to
/// the closures, without the calls marked as hints when `drop_hints`.
fn collect_subr_refs(
    p: &Parsed,
    gsubrs: &[Parsed],
    lsubrs: &[Parsed],
    drop_hints: bool,
    global: &mut BTreeSet<u32>,
    local: &mut BTreeSet<u32>,
) {
    if !p.has_calls {
        return;
    }
    for t in &p.values {
        if drop_hints && t.hinting {
            continue;
        }
        let (subrs, global_call) = match t.op {
            cs::CALLSUBR => (lsubrs, false),
            cs::CALLGSUBR => (gsubrs, true),
            _ => continue,
        };
        let added = if global_call {
            global.insert(t.subr_num)
        } else {
            local.insert(t.subr_num)
        };
        if added && let Some(s) = subrs.get(t.subr_num as usize) {
            collect_subr_refs(s, gsubrs, lsubrs, drop_hints, global, local);
        }
    }
}

/// The parsed programs of every glyph of a font, as HarfBuzz's
/// preprocessing leaves them in its `cff_subset_accelerator_t`: each
/// glyph parsed in glyph order, its hints marked, and the tokens
/// compacted.
#[derive(Clone, Debug)]
pub(crate) struct CffCache {
    /// By old glyph id.
    charstrings: Vec<Parsed>,
    gsubrs: Vec<Parsed>,
    lsubrs: Vec<Vec<Parsed>>,
}

/// The empty parsed programs of `acc`'s subroutines.
fn empty_subrs(acc: &Acc<'_>) -> (Vec<Parsed>, Vec<Vec<Parsed>>) {
    let gsubrs = vec![Parsed::default(); acc.gsubrs.len() as usize];
    let lsubrs = (0..acc.fd_count)
        .map(|fd| vec![Parsed::default(); acc.local_subrs(fd).len() as usize])
        .collect();
    (gsubrs, lsubrs)
}

/// One glyph of `subr_subsetter_t::subset`'s first loop: parses glyph
/// `old` into `cs`, adds the subroutines it runs to the closures, and
/// with `analyze_hints` marks its hints (`drop_hints_in_str`). `false`
/// when the program cannot be parsed.
#[allow(clippy::too_many_arguments)]
fn parse_glyph(
    acc: &Acc<'_>,
    old: u32,
    cs: &mut Parsed,
    gsubrs: &mut [Parsed],
    lsubrs: &mut [Vec<Parsed>],
    gclosure: &mut BTreeSet<u32>,
    lclosures: &mut [BTreeSet<u32>],
    analyze_hints: bool,
) -> bool {
    let fd = acc.fd(old);
    if fd >= acc.fd_count {
        return false;
    }
    let mut env = Env::new(acc.char_string(old), acc.gsubrs, acc.local_subrs(fd));
    let (Some(ls), Some(lc)) = (lsubrs.get_mut(fd as usize), lclosures.get_mut(fd as usize)) else {
        return false;
    };
    let mut ss = SubrSubset {
        programs: Programs {
            charstring: cs,
            gsubrs,
            lsubrs: ls,
        },
        global_closure: gclosure,
        local_closure: lc,
        current: (Kind::CharString, 0),
    };
    if !cs::interpret(&mut env, &mut ss) {
        return false;
    }
    ss.complete(&mut env);
    if analyze_hints {
        let mut drop = DropState::default();
        if drop_hints_in_str(&mut ss.programs, (Kind::CharString, 0), &mut drop) {
            ss.programs.charstring.hint_dropped = true;
        }
    }
    true
}

/// [`CffCache`] of `font`, or `None` when HarfBuzz's accelerator is not
/// valid or a glyph cannot be parsed (HarfBuzz's preprocessing then
/// fails and leaves no accelerator).
pub(crate) fn prepare(font: &Font<'_>) -> Option<CffCache> {
    let acc = Acc::new(font)?;
    let n = acc.num_glyphs();
    let mut charstrings = vec![Parsed::default(); n as usize];
    let (mut gsubrs, mut lsubrs) = empty_subrs(&acc);
    let mut gclosure = BTreeSet::new();
    let mut lclosures = vec![BTreeSet::new(); acc.fd_count as usize];
    for (g, cs) in (0..n).zip(charstrings.iter_mut()) {
        // populating the accelerator analyzes the hints of every glyph.
        let ok = parse_glyph(
            &acc,
            g,
            cs,
            &mut gsubrs,
            &mut lsubrs,
            &mut gclosure,
            &mut lclosures,
            true,
        );
        if !ok {
            return None;
        }
        cs.compact();
    }
    gsubrs.iter_mut().for_each(Parsed::compact);
    lsubrs.iter_mut().flatten().for_each(Parsed::compact);
    Some(CffCache {
        charstrings,
        gsubrs,
        lsubrs,
    })
}

/// The subroutine subsetter (`subr_subsetter_t`) after `subset`: the
/// parsed programs, borrowed from a [`CffCache`] or parsed for the plan.
struct Subrs<'c> {
    /// By new glyph id when parsed for the plan.
    charstrings: Vec<Parsed>,
    /// By old glyph id (`cached_charstrings`).
    cached: Option<&'c [Parsed]>,
    gsubrs: Cow<'c, [Parsed]>,
    lsubrs: Cow<'c, [Vec<Parsed>]>,
    global_remap: Remap,
    local_remaps: Vec<Remap>,
}

/// `subr_subsetter_t::subset`.
fn subset_subrs<'c>(plan: &'c Plan, acc: &Acc<'_>, tag: Tag) -> Result<Subrs<'c>, SubsetError> {
    let fail = SubsetError::Failed { tag };
    let drop_hints = plan.flags.has(Flags::NO_HINTING);
    let fd_count = acc.fd_count as usize;
    let mut gclosure = BTreeSet::new();
    let mut lclosures = vec![BTreeSet::new(); fd_count];
    let cache = plan.prepared.as_ref().and_then(|p| p.cff());
    let mut subrs = if let Some(c) = cache {
        Subrs {
            charstrings: Vec::new(),
            cached: Some(&c.charstrings),
            gsubrs: Cow::Borrowed(&c.gsubrs),
            lsubrs: Cow::Borrowed(&c.lsubrs),
            global_remap: Remap::default(),
            local_remaps: Vec::new(),
        }
    } else {
        let mut charstrings = vec![Parsed::default(); plan.num_output_glyphs as usize];
        let (mut gsubrs, mut lsubrs) = empty_subrs(acc);
        for &(new, old) in &plan.glyphs.new_to_old {
            let cs = charstrings.get_mut(new as usize).ok_or(fail)?;
            let ok = parse_glyph(
                acc,
                old,
                cs,
                &mut gsubrs,
                &mut lsubrs,
                &mut gclosure,
                &mut lclosures,
                drop_hints,
            );
            if !ok {
                return Err(fail);
            }
        }
        Subrs {
            charstrings,
            cached: None,
            gsubrs: Cow::Owned(gsubrs),
            lsubrs: Cow::Owned(lsubrs),
            global_remap: Remap::default(),
            local_remaps: Vec::new(),
        }
    };
    // With a cache the closures are computed from the parsed programs;
    // when dropping hints they are computed again without the hints.
    if cache.is_some() || drop_hints {
        gclosure.clear();
        lclosures.iter_mut().for_each(BTreeSet::clear);
        for &(new, old) in &plan.glyphs.new_to_old {
            let fd = acc.fd(old) as usize;
            if fd >= fd_count {
                return Err(fail);
            }
            let (Some(cs), Some(ls), Some(lc)) = (
                subrs.charstring(new, old),
                subrs.lsubrs.get(fd),
                lclosures.get_mut(fd),
            ) else {
                return Err(fail);
            };
            collect_subr_refs(cs, &subrs.gsubrs, ls, drop_hints, &mut gclosure, lc);
        }
    }
    subrs.global_remap = Remap::new(&gclosure);
    subrs.local_remaps = lclosures.iter().map(Remap::new).collect();
    Ok(subrs)
}

impl Subrs<'_> {
    /// The parsed charstring of new glyph `new` (`get_parsed_charstring`).
    fn charstring(&self, new: u32, old: u32) -> Option<&Parsed> {
        match self.cached {
            Some(c) => c.get(old as usize),
            None => self.charstrings.get(new as usize),
        }
    }

    /// `encode_str`.
    fn encode(&self, p: &Parsed, src: &[u8], fd: u32, hinting: bool) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(w) = p.prefix.filter(|_| !hinting && p.hint_dropped) {
            cs::encode_num_cs(&mut out, w);
        }
        for t in p.values.iter().filter(|t| hinting || !t.hinting) {
            match t.op {
                cs::CALLSUBR => {
                    let n = self
                        .local_remaps
                        .get(fd as usize)
                        .map_or(0, |r| r.biased(t.subr_num));
                    cs::encode_int(&mut out, n);
                }
                cs::CALLGSUBR => cs::encode_int(&mut out, self.global_remap.biased(t.subr_num)),
                _ => {}
            }
            out.extend_from_slice(src.get(t.start..t.end).unwrap_or(&[]));
        }
        out
    }
}

/// `cff1_subset_plan::create`.
fn create(plan: &Plan, acc: &Acc<'_>, tag: Tag) -> Result<CffPlan, SubsetError> {
    let fail = SubsetError::Failed { tag };
    if plan.glyphs.old_gid(0) != Some(0) {
        return Err(fail);
    }
    let flags = plan.flags;
    let num_glyphs = plan.num_output_glyphs;
    let drop_hints = flags.has(Flags::NO_HINTING);
    let desubroutinize = flags.has(Flags::DESUBROUTINIZE);
    let identity_charset = flags.has(Flags::CFF_IDENTITY_CHARSET) && acc.is_cid();
    let min_charstrings_off_size = if flags.has(Flags::IFTB_REQUIREMENTS) {
        4
    } else {
        0
    };
    let subset_charset = !acc.is_predef_charset()
        || identity_charset
        || plan.glyphs.new_to_old.iter().any(|&(n, o)| n != o);
    let subset_encoding = !acc.is_cid() && !acc.is_predef_encoding();

    let mut top_ops = acc.top.ops.clone();
    if subset_encoding && !acc.top.has_op(dict::ENCODING) {
        top_ops.push(DictOp::empty(dict::ENCODING));
    }
    if subset_charset && !acc.top.has_op(dict::CHARSET) {
        top_ops.push(DictOp::empty(dict::CHARSET));
    }

    let (subset_fdcount, fdselect_ranges, fdmap) = if acc.is_cid() {
        let fp = plan_fdselect(plan, &acc.fd_select, acc.num_glyphs(), acc.fd_count).ok_or(fail)?;
        (fp.count, fp.ranges, fp.fdmap)
    } else {
        (1, Vec::new(), FdMap([(0, 0)].into_iter().collect()))
    };

    // collect_sids_in_dicts
    let mut sidmap = SidMap::default();
    let mut top_names = [UNDEF_SID; dict::NAME_COUNT];
    for (slot, &sid) in top_names.iter_mut().zip(&acc.top.names) {
        if sid != UNDEF_SID {
            *slot = sidmap.add(sid);
        }
    }
    if acc.is_cid() {
        for (i, (fd, _)) in (0u32..).zip(&acc.font_dicts) {
            if fdmap.has(i) {
                sidmap.add(fd.font_name);
            }
        }
    }
    if sidmap.vector.len() > 0x8000 {
        return Err(fail);
    }

    // plan_subset_charset
    let mut charset_ranges = Vec::new();
    let mut charset_format = 0u8;
    if subset_charset {
        let mut last_sid = UNDEF_CODE.wrapping_sub(1);
        for (glyph, old) in old_glyphs(plan) {
            let sid = if identity_charset {
                glyph
            } else {
                let sid = acc.hb.sid(old);
                if acc.is_cid() { sid } else { sidmap.add(sid) }
            };
            if sid != last_sid.wrapping_add(1) {
                charset_ranges.push((sid, glyph));
            }
            last_sid = sid;
        }
        complete(&mut charset_ranges, num_glyphs);
        let size0 = u64::from(num_glyphs.saturating_sub(1)).saturating_mul(2);
        let format1_count: u64 = charset_ranges
            .iter()
            .map(|r| u64::from(r.1 / 0x100).saturating_add(1))
            .sum();
        let size1 = format1_count.saturating_mul(3);
        let size2 = (charset_ranges.len() as u64).saturating_mul(4);
        charset_format = if size0 < size1.min(size2) {
            0
        } else if size1 <= size2 {
            1
        } else {
            2
        };
    }

    // Programs.
    let fd_count = acc.fd_count;
    let mut globalsubrs = Vec::new();
    let mut localsubrs: Vec<Vec<Vec<u8>>> = Vec::new();
    let charstrings: Vec<Vec<u8>> = if desubroutinize {
        let mut out = Vec::new();
        for i in 0..num_glyphs {
            let Some(old) = plan.glyphs.old_gid(i) else {
                out.push(vec![cs::ENDCHAR_BYTE]);
                continue;
            };
            let fd = acc.fd(old);
            if fd >= fd_count {
                return Err(fail);
            }
            let mut env = Env::new(acc.char_string(old), acc.gsubrs, acc.local_subrs(fd));
            let mut f = Flatten {
                out: Vec::new(),
                drop_hints,
            };
            if !cs::interpret(&mut env, &mut f) {
                return Err(fail);
            }
            out.push(f.out);
        }
        out
    } else {
        let subrs = subset_subrs(plan, acc, tag)?;
        let identity_programs = !drop_hints
            && flags.has(Flags::NOTDEF_OUTLINE)
            && num_glyphs == acc.num_glyphs()
            && plan.glyphs.new_to_old.len() == num_glyphs as usize
            && plan.glyphs.new_to_old.iter().all(|&(n, o)| n == o);
        let copy = identity_programs
            && subrs.global_remap.back.len() == acc.gsubrs.len() as usize
            && (0..fd_count).filter(|&fd| fdmap.has(fd)).all(|fd| {
                subrs
                    .local_remaps
                    .get(fd as usize)
                    .is_some_and(|r| r.back.len() == acc.local_subrs(fd).len() as usize)
            });
        let index_items = |ix: HbIndex<'_>| -> Vec<Vec<u8>> {
            (0..ix.len())
                .map(|i| ix.get(i).unwrap_or(&[]).to_vec())
                .collect()
        };
        if copy {
            globalsubrs = index_items(acc.gsubrs);
            localsubrs = (0..fd_count)
                .map(|fd| {
                    if fdmap.has(fd) {
                        index_items(acc.local_subrs(fd))
                    } else {
                        Vec::new()
                    }
                })
                .collect();
            (0..acc.num_glyphs())
                .map(|g| acc.char_string(g).to_vec())
                .collect()
        } else {
            let hinting = !drop_hints;
            let mut out = vec![Vec::new(); num_glyphs as usize];
            let mut last = 0u32;
            for &(gid, old) in &plan.glyphs.new_to_old {
                while last < gid {
                    if let Some(o) = out.get_mut(last as usize) {
                        *o = vec![cs::ENDCHAR_BYTE];
                    }
                    last = last.saturating_add(1);
                }
                last = last.saturating_add(1);
                let fd = acc.fd(old);
                if fd >= fd_count {
                    return Err(fail);
                }
                let p = subrs.charstring(gid, old).ok_or(fail)?;
                let e = subrs.encode(p, acc.char_string(old), fd, hinting);
                if let Some(o) = out.get_mut(gid as usize) {
                    *o = e;
                }
            }
            while last < num_glyphs {
                if let Some(o) = out.get_mut(last as usize) {
                    *o = vec![cs::ENDCHAR_BYTE];
                }
                last = last.saturating_add(1);
            }
            globalsubrs = subrs
                .global_remap
                .back
                .iter()
                .map(|&old| {
                    let p = subrs.gsubrs.get(old as usize);
                    let src = acc.gsubrs.get(old).unwrap_or(&[]);
                    p.map_or_else(Vec::new, |p| subrs.encode(p, src, 0, hinting))
                })
                .collect();
            localsubrs = (0..fd_count)
                .map(|fd| {
                    if !fdmap.has(fd) {
                        return Vec::new();
                    }
                    let ls = acc.local_subrs(fd);
                    let parsed = subrs.lsubrs.get(fd as usize);
                    subrs
                        .local_remaps
                        .get(fd as usize)
                        .map_or_else(Vec::new, |r| {
                            r.back
                                .iter()
                                .map(|&old| {
                                    let p = parsed.and_then(|v| v.get(old as usize));
                                    let src = ls.get(old).unwrap_or(&[]);
                                    p.map_or_else(Vec::new, |p| subrs.encode(p, src, fd, hinting))
                                })
                                .collect()
                        })
                })
                .collect();
            out
        }
    };

    // plan_subset_encoding
    let mut enc_ranges = Vec::new();
    let mut enc_supp = Vec::new();
    let mut enc_num_codes = num_glyphs.saturating_sub(1);
    let mut enc_format = 0u8;
    if subset_encoding {
        let mut last_code = UNDEF_CODE.wrapping_sub(1);
        let mut end = num_glyphs.max(1);
        for (glyph, old) in old_glyphs(plan) {
            let code = acc.glyph_to_code(old);
            if code == UNDEF_CODE {
                enc_num_codes = glyph.saturating_sub(1);
                end = glyph;
                break;
            }
            if code != last_code.wrapping_add(1) {
                enc_ranges.push((code, glyph));
            }
            last_code = code;
            let sid = acc.hb.sid(old);
            for c in acc.supplement_codes(sid) {
                enc_supp.push((c, sid));
            }
        }
        complete(&mut enc_ranges, end);
        let size0 = u64::from(enc_num_codes).saturating_add(1);
        let size1 = (enc_ranges.len() as u64)
            .saturating_mul(2)
            .saturating_add(1);
        enc_format = if enc_num_codes <= 0xFF && size0 < size1 {
            0
        } else {
            1
        };
    }

    let fontdicts: Vec<(Option<usize>, u32)> = if acc.is_cid() {
        (0u32..)
            .zip(&acc.font_dicts)
            .filter(|(i, _)| fdmap.has(*i))
            .map(|(i, (d, _))| (Some(i as usize), sidmap.get(d.font_name)))
            .collect()
    } else {
        vec![(None, UNDEF_SID)]
    };
    if charstrings.len() != num_glyphs as usize || fontdicts.len() != subset_fdcount as usize {
        return Err(fail);
    }
    Ok(CffPlan {
        num_glyphs,
        drop_hints,
        desubroutinize,
        min_charstrings_off_size,
        top_ops,
        top_names,
        fdselect_ranges,
        fdmap,
        sidmap,
        subset_charset,
        charset_format,
        charset_ranges,
        subset_encoding,
        enc_format,
        enc_num_codes,
        enc_ranges,
        enc_supp,
        charstrings,
        globalsubrs,
        localsubrs,
        fontdicts,
    })
}

/// `hb_bit_storage`.
fn bit_storage(v: u32) -> u32 {
    32u32.saturating_sub(v.leading_zeros())
}

/// `CFFIndex::serialize_header` (16-bit count): the count, the offset
/// size and the offsets of items of `sizes` bytes.
pub(super) fn index_header(s: &mut Serializer, sizes: &[usize], min_off_size: u32) -> bool {
    index_header_n(s, sizes, min_off_size, false)
}

/// `CFFIndex::serialize_header` with a 32-bit count (CFF2) when `wide`.
pub(super) fn index_header_n(
    s: &mut Serializer,
    sizes: &[usize],
    min_off_size: u32,
    wide: bool,
) -> bool {
    let data_size: u64 = sizes.iter().map(|&l| l as u64).sum();
    let data = u32::try_from(data_size).unwrap_or(u32::MAX);
    let off_size = (bit_storage(data.saturating_add(1)).saturating_add(7) / 8).max(min_off_size);
    let ok = if wide {
        let at = s.zeros(4);
        s.check_assign_u32(at, sizes.len() as u64, Errors::ARRAY_OVERFLOW)
    } else {
        let at = s.zeros(2);
        s.check_assign_u16(at, sizes.len() as u64, Errors::ARRAY_OVERFLOW)
    };
    if !ok {
        return false;
    }
    if sizes.is_empty() {
        return true;
    }
    s.u8(u8::try_from(off_size).unwrap_or(4));
    let mut offset = 1u32;
    let put = |s: &mut Serializer, v: u32| {
        let b = v.to_be_bytes();
        s.extend(
            b.get(4usize.saturating_sub(off_size as usize)..)
                .unwrap_or(&[]),
        );
    };
    for &l in sizes {
        put(s, offset);
        offset = offset.wrapping_add(u32::try_from(l).unwrap_or(u32::MAX));
    }
    put(s, offset);
    true
}

/// `CFFIndex::serialize` of `items`.
pub(super) fn index(s: &mut Serializer, items: &[Vec<u8>], min_off_size: u32) -> bool {
    index_n(s, items, min_off_size, false)
}

/// `CFFIndex::serialize` with a 32-bit count (CFF2) when `wide`.
pub(crate) fn index_n(
    s: &mut Serializer,
    items: &[Vec<u8>],
    min_off_size: u32,
    wide: bool,
) -> bool {
    let sizes: Vec<usize> = items.iter().map(Vec::len).collect();
    if !index_header_n(s, &sizes, min_off_size, wide) {
        return false;
    }
    for it in items {
        s.extend(it);
    }
    !s.in_error()
}

pub(super) fn op_bytes(op: u32) -> Vec<u8> {
    let mut v = Vec::new();
    cs::encode_op(&mut v, op);
    v
}

/// `UnsizedByteStr::serialize_int2`.
pub(super) fn int2(s: &mut Serializer, v: i64) -> bool {
    s.u8(28);
    let at = s.zeros(2);
    let fits = (-32768..=32767).contains(&v);
    s.set_u16(at, u16::try_from(v & 0xFFFF).unwrap_or(0));
    fits || s.err(Errors::INT_OVERFLOW)
}

/// `Dict::serialize_int2_op`.
pub(super) fn int2_op(s: &mut Serializer, op: u32, v: i64) -> bool {
    let ok = int2(s, v);
    s.extend(&op_bytes(op));
    ok && !s.in_error()
}

/// `Dict::serialize_link4_op`.
pub(super) fn link4_op(s: &mut Serializer, op: u32, link: ObjIdx, whence: Whence) {
    let at = s.extend(&[29, 0, 0, 0, 0]).saturating_add(1);
    s.extend(&op_bytes(op));
    s.add_link(at, 4, true, link, whence, 0);
}

/// `Dict::serialize_link2_op`.
pub(super) fn link2_op(s: &mut Serializer, op: u32, link: ObjIdx) {
    let at = s.extend(&[28, 0, 0]).saturating_add(1);
    s.extend(&op_bytes(op));
    s.add_link(at, 2, true, link, Whence::Head, 0);
}

fn sid_value(sid: u32) -> i64 {
    i64::from(sid)
}

/// The links the Top DICT writes (`cff1_sub_table_info_t`).
#[derive(Default)]
struct Info {
    charset: ObjIdx,
    encoding: ObjIdx,
    char_strings: ObjIdx,
    fd_array: ObjIdx,
    fd_select: ObjIdx,
    private: (usize, ObjIdx),
}

/// `accelerator_subset_t::serialize`.
fn serialize(s: &mut Serializer, acc: &Acc<'_>, p: &CffPlan) -> bool {
    let mut info = Info::default();
    let bytes = |d: &[u8], o: &DictOp| -> Vec<u8> { d.get(o.start..o.end).unwrap_or(&[]).to_vec() };

    // CharStrings first, so that they are laid out last.
    s.push();
    if !index(s, &p.charstrings, p.min_charstrings_off_size) {
        s.pop_discard();
        return false;
    }
    info.char_strings = s.pop_pack(false);

    // Private DICTs and local subroutines.
    let mut privates = vec![(0usize, 0 as ObjIdx); p.fontdicts.len()];
    for (i, (pd, pbytes, _)) in acc.privates.iter().enumerate().rev() {
        let i32_ = u32::try_from(i).unwrap_or(u32::MAX);
        if !p.fdmap.has(i32_) {
            continue;
        }
        let mut subrs_link = 0;
        let local = p.localsubrs.get(i).map_or(&[][..], Vec::as_slice);
        if !local.is_empty() {
            s.push();
            if !index(s, local, 0) {
                s.pop_discard();
                return false;
            }
            subrs_link = s.pop_pack(true);
        }
        s.push();
        for o in &pd.ops {
            if p.drop_hints && dict::is_hint_op(o.op) {
                continue;
            }
            if o.op == dict::SUBRS {
                if !p.desubroutinize && subrs_link != 0 {
                    link2_op(s, o.op, subrs_link);
                }
                continue;
            }
            s.extend(&bytes(pbytes, o));
        }
        if s.in_error() {
            s.pop_discard();
            return false;
        }
        let size = s.len();
        let link = s.pop_pack(true);
        if let Some(slot) = privates.get_mut(p.fdmap.get(i32_) as usize) {
            *slot = (size, link);
        }
    }
    if !acc.is_cid() {
        info.private = privates.first().copied().unwrap_or((0, 0));
    }

    // FDArray.
    if acc.is_cid() {
        s.push();
        s.push();
        let mut sizes = Vec::new();
        for ((fd, font_name), &(size, link)) in p.fontdicts.iter().zip(&privates) {
            let start = s.len();
            if let Some((d, b)) = fd.and_then(|i| acc.font_dicts.get(i)) {
                for o in &d.ops {
                    match o.op {
                        dict::FONT_NAME => {
                            int2_op(s, o.op, sid_value(*font_name));
                        }
                        dict::PRIVATE => {
                            int2(s, i64::try_from(size).unwrap_or(i64::MAX));
                            link4_op(s, o.op, link, Whence::Absolute);
                        }
                        _ => {
                            s.extend(&bytes(b, o));
                        }
                    }
                }
            }
            sizes.push(s.len().saturating_sub(start));
        }
        s.pop_pack(false);
        if !index_header(s, &sizes, 0) {
            s.pop_discard();
            return false;
        }
        info.fd_array = s.pop_pack(false);
    }

    // FDSelect.
    if acc.is_cid() {
        s.push();
        fdselect(s, p.num_glyphs, 3, &p.fdselect_ranges);
        info.fd_select = s.pop_pack(true);
    }

    // Charset.
    if p.subset_charset {
        s.push();
        s.u8(p.charset_format);
        let lo16 = |v: u32| u16::try_from(v & 0xFFFF).unwrap_or(0);
        match p.charset_format {
            0 => {
                for &(sid, n_left) in &p.charset_ranges {
                    let mut sid = sid;
                    for _ in 0..=n_left {
                        s.u16(lo16(sid));
                        sid = sid.wrapping_add(1);
                    }
                }
            }
            1 => {
                for &(sid, n_left) in &p.charset_ranges {
                    let (mut sid, mut left) = (sid, n_left);
                    loop {
                        s.u16(lo16(sid));
                        s.u8(u8::try_from(left.min(0xFF)).unwrap_or(0xFF));
                        if left <= 0xFF {
                            break;
                        }
                        sid = sid.wrapping_add(0x100);
                        left = left.saturating_sub(0x100);
                    }
                }
            }
            _ => {
                let mut all = 0u32;
                for &(sid, n_left) in &p.charset_ranges {
                    all |= n_left;
                    s.u16(lo16(sid));
                    s.u16(lo16(n_left));
                }
                if all > 0xFFFF {
                    s.pop_discard();
                    return false;
                }
            }
        }
        info.charset = s.pop_pack(true);
    }

    // Encoding.
    if p.subset_encoding {
        s.push();
        let supp = if p.enc_supp.is_empty() { 0 } else { 0x80 };
        s.u8(p.enc_format | supp);
        let lo8 = |v: u32| u8::try_from(v & 0xFF).unwrap_or(0);
        if p.enc_format == 0 {
            s.u8(lo8(p.enc_num_codes));
            let mut glyph = 0u32;
            for &(code, n_left) in &p.enc_ranges {
                let mut code = code;
                for _ in 0..=n_left {
                    s.u8(lo8(code));
                    glyph = glyph.saturating_add(1);
                    code = code.wrapping_add(1);
                }
                if glyph > 0x100 || code > 0x100 {
                    s.pop_discard();
                    return false;
                }
            }
        } else {
            if p.enc_ranges.len() > 0xFF {
                s.err(Errors::OTHER);
                return false;
            }
            s.u8(lo8(u32::try_from(p.enc_ranges.len()).unwrap_or(0)));
            for &(code, n_left) in &p.enc_ranges {
                if code > 0xFF || n_left > 0xFF {
                    s.pop_discard();
                    return false;
                }
                s.u8(lo8(code));
                s.u8(lo8(n_left));
            }
        }
        if !p.enc_supp.is_empty() {
            s.u8(lo8(u32::try_from(p.enc_supp.len()).unwrap_or(0)));
            for &(code, sid) in &p.enc_supp {
                s.u8(lo8(code));
                s.u16(u16::try_from(sid & 0xFFFF).unwrap_or(0));
            }
        }
        info.encoding = s.pop_pack(true);
    }

    // Global subroutines.
    s.push();
    if !index(s, &p.globalsubrs, 0) {
        s.pop_discard();
        return false;
    }
    s.pop_pack(false);

    // String INDEX.
    s.push();
    if acc.strings.is_empty() || p.sidmap.vector.is_empty() {
        s.u16(0);
    } else {
        let items: Vec<Vec<u8>> = p
            .sidmap
            .vector
            .iter()
            .map(|&k| acc.strings.get(k).unwrap_or(&[]).to_vec())
            .collect();
        if !index(s, &items, 0) {
            s.pop_discard();
            return false;
        }
    }
    s.pop_pack(true);

    // Header and Name INDEX.
    s.extend(&[1, 0, 4, 4]);
    s.extend(acc.names);

    // Top DICT.
    s.push();
    for o in &p.top_ops {
        let ok = match o.op {
            dict::CHARSET if info.charset != 0 => {
                link4_op(s, o.op, info.charset, Whence::Absolute);
                true
            }
            dict::ENCODING if info.encoding != 0 => {
                link4_op(s, o.op, info.encoding, Whence::Absolute);
                true
            }
            dict::PRIVATE => {
                let ok = int2(s, i64::try_from(info.private.0).unwrap_or(i64::MAX));
                link4_op(s, o.op, info.private.1, Whence::Absolute);
                ok
            }
            op if dict::name_index(op).is_some() => {
                let sid = dict::name_index(op)
                    .and_then(|i| p.top_names.get(i))
                    .copied()
                    .unwrap_or(UNDEF_SID);
                int2_op(s, op, sid_value(sid))
            }
            dict::ROS => {
                let len = o.end.saturating_sub(o.start);
                if len < o.last_arg.saturating_add(3) {
                    false
                } else {
                    let a = int2(s, sid_value(p.top_names[dict::REGISTRY]));
                    let b = int2(s, sid_value(p.top_names[dict::ORDERING]));
                    let supp = acc
                        .top_bytes
                        .get(o.start.saturating_add(o.last_arg)..o.end)
                        .unwrap_or(&[]);
                    s.extend(supp);
                    a && b
                }
            }
            dict::CHAR_STRINGS => {
                link4_op(s, o.op, info.char_strings, Whence::Absolute);
                true
            }
            dict::FD_ARRAY => {
                link4_op(s, o.op, info.fd_array, Whence::Absolute);
                true
            }
            dict::FD_SELECT => {
                link4_op(s, o.op, info.fd_select, Whence::Absolute);
                true
            }
            _ => {
                s.extend(&bytes(acc.top_bytes, o));
                true
            }
        };
        if !ok || s.in_error() {
            s.pop_discard();
            return false;
        }
    }
    let top_size = s.len();
    s.pop_pack(false);
    index_header(s, &[top_size], 0) && !s.in_error()
}

/// `cff1::accelerator_subset_t::subset`.
pub(crate) fn subset(
    plan: &Plan,
    font: &Font<'_>,
    tag: Tag,
    _src: &[u8],
    s: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let acc = Acc::new(font).ok_or(SubsetError::Failed { tag })?;
    let p = create(plan, &acc, tag)?;
    if !serialize(s, &acc, &p) {
        return Err(SubsetError::Failed { tag });
    }
    Ok(true)
}
