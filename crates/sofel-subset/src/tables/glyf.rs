//! glyf and loca (and GLYF and LOCA), with head's `indexToLocFormat`, as
//! HarfBuzz's `src/OT/glyf/` subsetting.

use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::be::{u16_at, u24_at, u32_at};
use sofel_write::Serializer;

use crate::error::SubsetError;
use crate::input::Flags;
use crate::plan::Plan;
use crate::tables::Tables;

const GLYF: Tag = Tag::new(b"glyf");
const GLYF_WIDE: Tag = Tag::new(b"GLYF");
const LOCA: Tag = Tag::new(b"loca");
const LOCA_WIDE: Tag = Tag::new(b"LOCA");
const HEAD: Tag = Tag::new(b"head");

const HEADER_SIZE: usize = 10;

const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
const WE_HAVE_A_SCALE: u16 = 0x0008;
const MORE_COMPONENTS: u16 = 0x0020;
const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;
const WE_HAVE_INSTRUCTIONS: u16 = 0x0100;
const OVERLAP_COMPOUND: u16 = 0x0400;
const GID_IS_24BIT: u16 = 0x2000;

const FLAG_REPEAT: u8 = 0x08;
const FLAG_X_SHORT: u8 = 0x02;
const FLAG_Y_SHORT: u8 = 0x04;
const FLAG_X_SAME: u8 = 0x10;
const FLAG_Y_SAME: u8 = 0x20;
const FLAG_OVERLAP_SIMPLE: u8 = 0x40;

/// HarfBuzz's head view: `indexToLocFormat` and `glyphDataFormat`, zero
/// when head fails the sanitizer (version 1, the magic number).
pub(crate) fn head_formats(font: &Font<'_>) -> (i16, i16) {
    let Some(h) = font.table(HEAD).filter(|h| h.len() >= 54) else {
        return (0, 0);
    };
    let ok = u16_at(h, 0) == Some(1) && u32_at(h, 12) == Some(0x5F0F_3CF5);
    if !ok {
        return (0, 0);
    }
    let f = |at| sofel_font::read::be::i16_at(h, at).unwrap_or(0);
    (f(50), f(52))
}

/// `glyf_accelerator_t`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GlyfAccel<'a> {
    glyf: &'a [u8],
    loca: &'a [u8],
    short: bool,
    extended: bool,
    num_glyphs: u32,
}

/// What kind of glyph.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Empty,
    Simple,
    Composite,
}

/// A glyph's bytes (`glyf_impl::Glyph`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Glyph<'a> {
    pub bytes: &'a [u8],
    pub kind: Kind,
    pub extended: bool,
}

/// A component record of a composite glyph.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Component {
    /// Where the record starts in the glyph.
    pub at: usize,
    pub size: usize,
    pub flags: u16,
    pub gid: u32,
}

impl<'a> GlyfAccel<'a> {
    pub fn new(font: &Font<'a>) -> GlyfAccel<'a> {
        let (loc, data_format) = head_formats(font);
        let empty = GlyfAccel {
            glyf: &[],
            loca: &[],
            short: true,
            extended: false,
            num_glyphs: 0,
        };
        if !(loc <= 1 && data_format <= 1) {
            return empty;
        }
        let short = loc == 0;
        let wide = (font.table(GLYF_WIDE), font.table(LOCA_WIDE));
        let (extended, glyf, loca) = match wide {
            (Some(g), Some(l)) if !g.is_empty() && !l.is_empty() => (true, g, l),
            _ => (
                false,
                font.table(GLYF).unwrap_or(&[]),
                font.table(LOCA).unwrap_or(&[]),
            ),
        };
        let entry = if short { 2 } else { 4 };
        let n = u32::try_from(
            loca.len()
                .checked_div(entry)
                .unwrap_or(0)
                .max(1)
                .saturating_sub(1),
        )
        .unwrap_or(u32::MAX);
        GlyfAccel {
            glyf,
            loca,
            short,
            extended,
            num_glyphs: n.min(font.num_glyphs()),
        }
    }

    pub fn has_data(&self) -> bool {
        self.num_glyphs != 0
    }

    pub fn table_len(&self) -> usize {
        self.glyf.len()
    }

    /// `glyph_for_gid`.
    pub fn glyph(&self, gid: u32, trim: bool) -> Glyph<'a> {
        let empty = Glyph {
            bytes: &[],
            kind: Kind::Empty,
            extended: self.extended,
        };
        if gid >= self.num_glyphs {
            return Glyph {
                extended: false,
                ..empty
            };
        }
        let g = gid as usize;
        let (start, end) = if self.short {
            (
                u16_at(self.loca, g.saturating_mul(2))
                    .map_or(0, |v| usize::from(v).saturating_mul(2)),
                u16_at(self.loca, g.saturating_mul(2).saturating_add(2))
                    .map_or(0, |v| usize::from(v).saturating_mul(2)),
            )
        } else {
            (
                u32_at(self.loca, g.saturating_mul(4)).map_or(0, |v| v as usize),
                u32_at(self.loca, g.saturating_mul(4).saturating_add(4)).map_or(0, |v| v as usize),
            )
        };
        if start > end || end > self.glyf.len() {
            return empty;
        }
        let glyph = Glyph::new(self.glyf.get(start..end).unwrap_or(&[]), self.extended);
        if trim {
            Glyph::new(glyph.trim_padding(), self.extended)
        } else {
            glyph
        }
    }
}

impl<'a> Glyph<'a> {
    pub fn new(bytes: &'a [u8], extended: bool) -> Glyph<'a> {
        let n = if bytes.len() >= HEADER_SIZE {
            sofel_font::read::be::i16_at(bytes, 0).unwrap_or(0)
        } else {
            0
        };
        let kind = match n {
            0 => Kind::Empty,
            n if n > 0 => Kind::Simple,
            _ => Kind::Composite,
        };
        Glyph {
            bytes,
            kind,
            extended,
        }
    }

    fn contours(&self) -> usize {
        sofel_font::read::be::i16_at(self.bytes, 0).map_or(0, |n| usize::try_from(n).unwrap_or(0))
    }

    /// `SimpleGlyph::instruction_len_offset`.
    fn ins_len_at(&self) -> usize {
        HEADER_SIZE.saturating_add(2usize.saturating_mul(self.contours()))
    }

    /// `SimpleGlyph::instructions_length`.
    fn simple_ins_len(&self) -> usize {
        let at = self.ins_len_at();
        let Some(n) = u16_at(self.bytes, at) else {
            return 0;
        };
        let n = usize::from(n);
        if at.saturating_add(2).saturating_add(n) > self.bytes.len() {
            0
        } else {
            n
        }
    }

    /// The component records (`composite_iter_t`).
    pub fn components(&self) -> impl Iterator<Item = Component> + 'a {
        let bytes = self.bytes;
        let extended = self.extended;
        let mut at = (self.kind == Kind::Composite).then_some(HEADER_SIZE);
        core::iter::from_fn(move || {
            let c = component_at(bytes, at?, extended)?;
            at = (c.flags & MORE_COMPONENTS != 0).then_some(c.at.saturating_add(c.size));
            Some(c)
        })
    }

    /// `trim_padding`.
    pub fn trim_padding(&self) -> &'a [u8] {
        match self.kind {
            Kind::Composite | Kind::Empty => self.bytes,
            Kind::Simple => {
                let b = self.bytes;
                let mut p = self.ins_len_at();
                if p.saturating_add(2) >= b.len() {
                    return &[];
                }
                let num_coordinates = u16_at(b, p.saturating_sub(2))
                    .map_or(0usize, usize::from)
                    .saturating_add(1);
                let num_instructions = u16_at(b, p).map_or(0, usize::from);
                p = p.saturating_add(2).saturating_add(num_instructions);
                let mut coord_bytes = 0usize;
                let mut coords = 0usize;
                while p < b.len() {
                    let flag = b.get(p).copied().unwrap_or(0);
                    p = p.saturating_add(1);
                    let mut repeat = 1usize;
                    if flag & FLAG_REPEAT != 0 {
                        if p >= b.len() {
                            return &[];
                        }
                        repeat = usize::from(b.get(p).copied().unwrap_or(0)).saturating_add(1);
                        p = p.saturating_add(1);
                    }
                    let xb: usize = if flag & FLAG_X_SHORT != 0 {
                        1
                    } else if flag & FLAG_X_SAME == 0 {
                        2
                    } else {
                        0
                    };
                    let yb = if flag & FLAG_Y_SHORT != 0 {
                        1
                    } else if flag & FLAG_Y_SAME == 0 {
                        2
                    } else {
                        0
                    };
                    coord_bytes =
                        coord_bytes.saturating_add(repeat.saturating_mul(xb.saturating_add(yb)));
                    coords = coords.saturating_add(repeat);
                    if coords >= num_coordinates {
                        break;
                    }
                }
                if coords != num_coordinates {
                    return &[];
                }
                // bytes.length + coord_bytes - (glyph_end - glyph)
                let len = b
                    .len()
                    .saturating_add(coord_bytes)
                    .saturating_sub(b.len().saturating_sub(p));
                b.get(..len).unwrap_or(b)
            }
        }
    }

    /// `drop_hints_bytes`: the bytes to copy without the instructions, as
    /// two pieces.
    pub fn drop_hints_bytes(&self) -> (&'a [u8], &'a [u8]) {
        let b = self.bytes;
        match self.kind {
            Kind::Composite => {
                let ins = self.composite_ins_len();
                (b.get(..b.len().saturating_sub(ins)).unwrap_or(&[]), &[])
            }
            Kind::Simple => self.simple_drop(),
            Kind::Empty => {
                if b.len() >= HEADER_SIZE && self.ins_len_at().saturating_add(2) <= b.len() {
                    self.simple_drop()
                } else {
                    (b, &[])
                }
            }
        }
    }

    fn simple_drop(&self) -> (&'a [u8], &'a [u8]) {
        let b = self.bytes;
        let ins = self.simple_ins_len();
        let glyph_len = self.ins_len_at().saturating_add(2).saturating_add(ins);
        let start = b.get(..glyph_len.saturating_sub(ins)).unwrap_or(&[]);
        let end = b.get(glyph_len..).unwrap_or(&[]);
        (start, end)
    }

    /// `CompositeGlyph::instructions_length`.
    fn composite_ins_len(&self) -> usize {
        let Some(last) = self.components().last() else {
            return 0;
        };
        let end = self.bytes.len();
        let start = if last.flags & WE_HAVE_INSTRUCTIONS != 0 {
            last.at.saturating_add(last.size)
        } else {
            end
        };
        end.saturating_sub(start)
    }
}

/// The component record at `at`, if it fits.
fn component_at(b: &[u8], at: usize, extended: bool) -> Option<Component> {
    if at.saturating_add(4) > b.len() {
        return None;
    }
    let flags = u16_at(b, at)?;
    let mut size = 4usize;
    let wide_gid = extended && flags & GID_IS_24BIT != 0;
    if wide_gid {
        size = size.saturating_add(1);
    }
    size = size.saturating_add(if flags & ARG_1_AND_2_ARE_WORDS != 0 {
        4
    } else {
        2
    });
    if flags & WE_HAVE_A_SCALE != 0 {
        size = size.saturating_add(2);
    } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
        size = size.saturating_add(4);
    } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
        size = size.saturating_add(8);
    }
    if at.saturating_add(size) > b.len() {
        return None;
    }
    let gid = if wide_gid {
        u24_at(b, at.saturating_add(2))?
    } else {
        u32::from(u16_at(b, at.saturating_add(2))?)
    };
    Some(Component {
        at,
        size,
        flags,
        gid,
    })
}

/// The components of `b`, walked as `composite_iter_t` (stops at the
/// first record without MORE_COMPONENTS or that does not fit).
fn components_of(b: &[u8], extended: bool) -> Vec<Component> {
    let mut out = Vec::new();
    let mut at = HEADER_SIZE;
    while let Some(c) = component_at(b, at, extended) {
        out.push(c);
        if c.flags & MORE_COMPONENTS == 0 {
            break;
        }
        at = at.saturating_add(c.size);
    }
    out
}

/// One glyph's output bytes (`SubsetGlyph::serialize`, without padding).
fn subset_glyph(dest: &mut Vec<u8>, extended: bool, plan: &Plan) {
    if dest.is_empty() {
        return;
    }
    let is_composite = Glyph::new(dest, extended).kind == Kind::Composite;
    if is_composite {
        for c in components_of(dest, extended) {
            if let Some(n) = plan.new_glyph(c.gid) {
                if extended && c.flags & GID_IS_24BIT != 0 {
                    let b = n.to_be_bytes();
                    if let Some(s) = dest.get_mut(c.at.saturating_add(2)..c.at.saturating_add(5)) {
                        s.copy_from_slice(b.get(1..).unwrap_or(&[0; 3]));
                    }
                } else if let Some(s) = dest.get_mut(c.at.saturating_add(2)..c.at.saturating_add(4))
                {
                    s.copy_from_slice(&((n & 0xFFFF) as u16).to_be_bytes());
                }
            }
        }
        if extended {
            lower_gids(dest);
        }
    }
    let g = Glyph::new(dest, extended);
    let kind = g.kind;
    if plan.flags.has(Flags::NO_HINTING) {
        match kind {
            Kind::Composite => {
                for c in components_of(dest, extended) {
                    let f = c.flags & !WE_HAVE_INSTRUCTIONS;
                    if let Some(s) = dest.get_mut(c.at..c.at.saturating_add(2)) {
                        s.copy_from_slice(&f.to_be_bytes());
                    }
                }
            }
            Kind::Simple | Kind::Empty => {
                if kind == Kind::Simple || dest.len() >= HEADER_SIZE {
                    let at = g.ins_len_at();
                    if at.saturating_add(2) <= dest.len()
                        && let Some(s) = dest.get_mut(at..at.saturating_add(2))
                    {
                        s.fill(0);
                    }
                }
            }
        }
    }
    if plan.flags.has(Flags::SET_OVERLAPS_FLAG) {
        let g = Glyph::new(dest, extended);
        match g.kind {
            Kind::Composite => {
                // `check_range (&glyph_chain, min_size)`: 4 bytes.
                if dest.len() >= HEADER_SIZE + 4 {
                    let f = u16_at(dest, HEADER_SIZE).unwrap_or(0) | OVERLAP_COMPOUND;
                    if let Some(s) = dest.get_mut(HEADER_SIZE..HEADER_SIZE + 2) {
                        s.copy_from_slice(&f.to_be_bytes());
                    }
                }
            }
            Kind::Simple => {
                let at = g
                    .ins_len_at()
                    .saturating_add(2)
                    .saturating_add(g.simple_ins_len());
                if let Some(f) = dest.get_mut(at) {
                    *f |= FLAG_OVERLAP_SIMPLE;
                }
            }
            Kind::Empty => {}
        }
    }
}

/// `lower_gid_24_to_16` of every component, moving the records down and
/// filling the freed bytes at the end with ROFF (0x7A).
fn lower_gids(dest: &mut Vec<u8>) {
    let comps = components_of(dest, true);
    if comps.is_empty() {
        return;
    }
    let end = dest.len();
    let first = comps.first().map_or(HEADER_SIZE, |c| c.at);
    let mut out: Vec<u8> = Vec::with_capacity(end);
    out.extend_from_slice(dest.get(..first).unwrap_or(&[]));
    let mut q = first;
    for c in &comps {
        let rec = dest.get(c.at..c.at.saturating_add(c.size)).unwrap_or(&[]);
        q = q.saturating_add(c.size);
        if c.flags & GID_IS_24BIT != 0 && c.gid <= 0xFFFF {
            out.extend_from_slice(&(c.flags & !GID_IS_24BIT).to_be_bytes());
            out.extend_from_slice(&((c.gid & 0xFFFF) as u16).to_be_bytes());
            out.extend_from_slice(rec.get(5..).unwrap_or(&[]));
        } else {
            out.extend_from_slice(rec);
        }
    }
    out.extend_from_slice(dest.get(q..end).unwrap_or(&[]));
    out.resize(end, 0x7A);
    *dest = out;
}

/// Subsets glyf (or GLYF) and adds loca and head (`glyf::subset`).
pub(crate) fn subset(
    plan: &Plan,
    font: &Font<'_>,
    tag: Tag,
    s: &mut Serializer,
    tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let accel = GlyfAccel::new(font);
    let extended = font.table(GLYF_WIDE).is_some_and(|g| !g.is_empty())
        && font.table(LOCA_WIDE).is_some_and(|l| !l.is_empty());
    if extended != (tag == GLYF_WIDE) {
        return Ok(false);
    }
    let (loc, data_format) = head_formats(font);
    if !(loc <= 1 && data_format <= 1) {
        return Ok(false);
    }
    let notdef_outline = plan.flags.has(Flags::NOTDEF_OUTLINE);
    let no_hinting = plan.flags.has(Flags::NO_HINTING);
    let glyphs: Vec<(u32, Vec<u8>)> = plan
        .glyphs
        .new_to_old
        .iter()
        .map(|&(new, old)| {
            let source = if old == 0 && new == 0 && !notdef_outline {
                Glyph::new(&[], extended)
            } else {
                accel.glyph(old, true)
            };
            let mut bytes = Vec::new();
            if no_hinting {
                let (a, b) = source.drop_hints_bytes();
                bytes.extend_from_slice(a);
                bytes.extend_from_slice(b);
            } else {
                bytes.extend_from_slice(source.bytes);
            }
            (new, bytes)
        })
        .collect();
    let total: usize = glyphs
        .iter()
        .map(|(_, g)| g.len().saturating_add(g.len() % 2))
        .sum();
    let short = !plan.force_long_loca && total < 0x1FFFF;
    let init = s.len();
    for (_, g) in &glyphs {
        let mut dest = g.clone();
        let pad = if short { g.len() % 2 } else { 0 };
        subset_glyph(&mut dest, extended, plan);
        s.extend(&dest);
        s.zeros(pad);
    }
    if s.len() == init {
        s.u8(0);
    }
    let sizes = glyphs.iter().map(|(_, g)| {
        if short {
            g.len().saturating_add(g.len() % 2)
        } else {
            g.len()
        }
    });
    let loca = write_loca(plan, sizes, short);
    tables.insert(if extended { LOCA_WIDE } else { LOCA }, loca);
    let mut head = font
        .table(HEAD)
        .ok_or(SubsetError::Failed { tag: HEAD })?
        .to_vec();
    if let Some(f) = head.get_mut(50..52) {
        f.copy_from_slice(&(if short { 0u16 } else { 1u16 }).to_be_bytes());
    } else {
        return Err(SubsetError::Malformed { tag: HEAD });
    }
    tables.insert(HEAD, head);
    Ok(true)
}

/// `_write_loca`.
fn write_loca(plan: &Plan, sizes: impl Iterator<Item = usize>, short: bool) -> Vec<u8> {
    let num_offsets = plan.num_output_glyphs as usize + 1;
    let mut out = Vec::with_capacity(num_offsets.saturating_mul(if short { 2 } else { 4 }));
    let push = |out: &mut Vec<u8>, v: usize| {
        if short {
            out.extend_from_slice(&u16::try_from((v >> 1) & 0xFFFF).unwrap_or(0).to_be_bytes());
        } else {
            out.extend_from_slice(&u32::try_from(v & 0xFFFF_FFFF).unwrap_or(0).to_be_bytes());
        }
    };
    let mut offset = 0usize;
    push(&mut out, 0);
    let mut last = 0u32;
    for (&(gid, _), size) in plan.glyphs.new_to_old.iter().zip(sizes) {
        while last < gid {
            push(&mut out, offset);
            last = last.saturating_add(1);
        }
        offset = offset.saturating_add(size);
        push(&mut out, offset);
        last = last.saturating_add(1);
    }
    let num_glyphs = u32::try_from(num_offsets & 0xFFFF_FFFF)
        .unwrap_or(0)
        .wrapping_sub(1);
    while last < num_glyphs {
        push(&mut out, offset);
        last = last.saturating_add(1);
    }
    out
}
