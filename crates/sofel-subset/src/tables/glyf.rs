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
    let instanced = plan.var.glyf.as_ref().filter(|_| plan.var.instancing());
    let glyphs: Vec<(u32, Vec<u8>)> = plan
        .glyphs
        .new_to_old
        .iter()
        .enumerate()
        .map(|(i, &(new, old))| {
            if let Some(compiled) = instanced {
                return (new, compiled.get(i).cloned().unwrap_or_default());
            }
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
    if plan.var.instancing() {
        // `_add_head_and_set_loca_version`: the bounds and flags bit 1
        // (x of the left side bearing point at the minimum x).
        let m = &plan.var.metrics.head_maxp;
        for (at, v) in [
            (36usize, m.x_min),
            (38, m.y_min),
            (40, m.x_max),
            (42, m.y_max),
        ] {
            if let Some(f) = head.get_mut(at..at.saturating_add(2)) {
                f.copy_from_slice(&crate::instance::lo16(v).to_be_bytes());
            }
        }
        let flags = u16_at(&head, 16).unwrap_or(0);
        let flags = if m.all_x_min_is_lsb {
            flags | 2
        } else {
            flags & !2
        };
        if let Some(f) = head.get_mut(16..18) {
            f.copy_from_slice(&flags.to_be_bytes());
        }
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

// ---------------------------------------------------------------------
// Instancing at the default location (`compile_bytes_with_deltas`).

const ARGS_ARE_XY_VALUES: u16 = 0x0002;
const SCALED_COMPONENT_OFFSET: u16 = 0x0800;
const UNSCALED_COMPONENT_OFFSET: u16 = 0x1000;
const FLAG_ON_CURVE: u8 = 0x01;
const FLAG_CUBIC: u8 = 0x80;

/// `HB_MAX_GRAPH_EDGE_COUNT`.
const MAX_GRAPH_EDGE_COUNT: u32 = 16384;
/// `HB_GLYF_MAX_POINTS`.
const MAX_POINTS: usize = 200_000;

/// `contour_point_t`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Pt {
    x: f32,
    y: f32,
    flag: u8,
}

/// `hb_decycler_t` over the composite levels of one glyph: the
/// tortoise of level `l` is level `l / 2`.
#[derive(Debug, Default)]
struct Decycler {
    values: Vec<u32>,
}

impl Decycler {
    fn enter(&mut self) -> usize {
        self.values.push(0);
        self.values.len().saturating_sub(1)
    }

    fn leave(&mut self) {
        self.values.pop();
    }

    fn visit(&mut self, level: usize, value: u32) -> bool {
        if let Some(v) = self.values.get_mut(level) {
            *v = value;
        }
        let tortoise = level / 2;
        tortoise == level || self.values.get(tortoise) != Some(&value)
    }
}

/// `CompositeGlyphRecord::get_transformation`: the matrix and the
/// offset (zero for anchored components).
fn transformation(b: &[u8], c: &Component, extended: bool) -> ([f32; 4], Pt) {
    let mut m = [1.0f32, 0.0, 0.0, 1.0];
    let mut p = c.at.saturating_add(4);
    if extended && c.flags & GID_IS_24BIT != 0 {
        p = p.saturating_add(1);
    }
    let (mut tx, mut ty) = if c.flags & ARG_1_AND_2_ARE_WORDS != 0 {
        let x = sofel_font::read::be::i16_at(b, p).map_or(0, i32::from);
        let y = sofel_font::read::be::i16_at(b, p.saturating_add(2)).map_or(0, i32::from);
        p = p.saturating_add(4);
        (x, y)
    } else {
        let x = sofel_font::read::be::i8_at(b, p).map_or(0, i32::from);
        let y = sofel_font::read::be::i8_at(b, p.saturating_add(1)).map_or(0, i32::from);
        p = p.saturating_add(2);
        (x, y)
    };
    if c.flags & ARGS_ARE_XY_VALUES == 0 {
        tx = 0;
        ty = 0;
    }
    let f = |i: usize| {
        crate::instance::f2dot14_to_f32(
            sofel_font::read::be::i16_at(b, p.saturating_add(i.saturating_mul(2))).unwrap_or(0),
        )
    };
    if c.flags & WE_HAVE_A_SCALE != 0 {
        m[0] = f(0);
        m[3] = m[0];
    } else if c.flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
        m[0] = f(0);
        m[3] = f(1);
    } else if c.flags & WE_HAVE_A_TWO_BY_TWO != 0 {
        m = [f(0), f(1), f(2), f(3)];
    }
    let t = Pt {
        x: sofel_geom::float::i32_to_f32(tx),
        y: sofel_geom::float::i32_to_f32(ty),
        flag: 0,
    };
    (m, t)
}

/// `CompositeGlyphRecord::get_anchor_points`.
fn anchor_points(b: &[u8], c: &Component, extended: bool) -> (usize, usize) {
    let mut p = c.at.saturating_add(4);
    if extended && c.flags & GID_IS_24BIT != 0 {
        p = p.saturating_add(1);
    }
    if c.flags & ARG_1_AND_2_ARE_WORDS != 0 {
        (
            u16_at(b, p).map_or(0, usize::from),
            u16_at(b, p.saturating_add(2)).map_or(0, usize::from),
        )
    } else {
        (
            b.get(p).map_or(0, |&v| usize::from(v)),
            b.get(p.saturating_add(1)).map_or(0, |&v| usize::from(v)),
        )
    }
}

/// `CompositeGlyphRecord::transform_points`.
fn transform_points(points: &mut [Pt], m: [f32; 4], t: Pt, flags: u16) {
    let transform = |points: &mut [Pt]| {
        if m != [1.0, 0.0, 0.0, 1.0] {
            for p in points.iter_mut() {
                let x = p.x * m[0] + p.y * m[2];
                p.y = p.x * m[1] + p.y * m[3];
                p.x = x;
            }
        }
    };
    let scaled =
        flags & (SCALED_COMPONENT_OFFSET | UNSCALED_COMPONENT_OFFSET) == SCALED_COMPONENT_OFFSET;
    if scaled {
        translate(points, t);
        transform(points);
    } else {
        transform(points);
        translate(points, t);
    }
}

/// `CompositeGlyphRecord::translate`.
fn translate(points: &mut [Pt], t: Pt) {
    if t.x != 0.0 && t.y != 0.0 {
        for p in points.iter_mut() {
            p.x += t.x;
            p.y += t.y;
        }
    } else if t.x != 0.0 {
        points.iter_mut().for_each(|p| p.x += t.x);
    } else if t.y != 0.0 {
        points.iter_mut().for_each(|p| p.y += t.y);
    }
}

/// `SimpleGlyph::get_contour_points`: the points of a simple glyph,
/// appended to `out`.
fn contour_points(g: &Glyph<'_>, out: &mut Vec<Pt>) -> bool {
    let b = g.bytes;
    let nc = g.contours();
    let ends = |i: usize| u16_at(b, HEADER_SIZE.saturating_add(i.saturating_mul(2)));
    // One more item for the instruction length.
    if ends(nc).is_none() {
        return false;
    }
    let num_points = ends(nc.saturating_sub(1)).map_or(0, |v| usize::from(v).saturating_add(1));
    if num_points < nc {
        return false;
    }
    let start = out.len();
    out.resize(start.saturating_add(num_points), Pt::default());
    let Some(points) = out.get_mut(start..) else {
        return false;
    };
    let ins = ends(nc).map_or(0, usize::from);
    let mut p = HEADER_SIZE
        .saturating_add(nc.saturating_add(1).saturating_mul(2))
        .saturating_add(ins);
    if p >= b.len() {
        return false;
    }
    // Flags.
    let mut i = 0usize;
    while i < num_points {
        let Some(&raw) = b.get(p) else {
            return false;
        };
        p = p.saturating_add(1);
        let flag = if g.extended { raw } else { raw & !FLAG_CUBIC };
        if let Some(pt) = points.get_mut(i) {
            pt.flag = flag;
        }
        i = i.saturating_add(1);
        if flag & FLAG_REPEAT != 0 {
            let Some(&n) = b.get(p) else {
                return false;
            };
            p = p.saturating_add(1);
            let stop = i.saturating_add(usize::from(n)).min(num_points);
            for pt in points.get_mut(i..stop).unwrap_or(&mut []) {
                pt.flag = flag;
            }
            i = stop.max(i);
        }
    }
    // Coordinates.
    for (short, same, is_x) in [
        (FLAG_X_SHORT, FLAG_X_SAME, true),
        (FLAG_Y_SHORT, FLAG_Y_SAME, false),
    ] {
        let mut v = 0i32;
        for pt in points.iter_mut() {
            if pt.flag & short != 0 {
                let Some(&d) = b.get(p) else {
                    return false;
                };
                p = p.saturating_add(1);
                let d = i32::from(d);
                v = if pt.flag & same != 0 {
                    v.wrapping_add(d)
                } else {
                    v.wrapping_sub(d)
                };
            } else if pt.flag & same == 0 {
                let Some(d) = sofel_font::read::be::i16_at(b, p) else {
                    return false;
                };
                p = p.saturating_add(2);
                v = v.wrapping_add(i32::from(d));
            }
            let f = sofel_geom::float::i32_to_f32(v);
            if is_x {
                pt.x = f;
            } else {
                pt.y = f;
            }
        }
    }
    true
}

/// What `get_points` works with for one top-level glyph.
struct PointsCx<'a, 'f> {
    accel: &'f GlyfAccel<'a>,
    hmtx: &'f crate::tables::hmtx::MetricsAccel<'a>,
    vmtx: &'f crate::tables::hmtx::MetricsAccel<'a>,
    variation: Option<&'f sofel_font::var::Instance<'f, 'a, 'f, 'f>>,
    scratch: &'f mut sofel_font::var::gvar::HbScratch,
    budget: i64,
    decycler: Decycler,
    edge_count: u32,
    /// `head_maxp_info` when the glyph counts toward it.
    head: Option<&'f mut crate::instance::HeadMaxp>,
    composite_contours: Option<u32>,
    /// The component points of the top-level composite.
    points_with_deltas: Vec<Pt>,
}

impl PointsCx<'_, '_> {
    /// The phantom points of `gid` (`get_points`'s "Init phantom
    /// points").
    fn phantoms(&self, gid: u32, g: &Glyph<'_>) -> [Pt; 4] {
        let hdr = |at: usize| {
            if g.bytes.len() >= HEADER_SIZE {
                sofel_font::read::be::i16_at(g.bytes, at).map_or(0, i32::from)
            } else {
                0
            }
        };
        let lsb = self.hmtx.side_bearing(gid);
        let h_delta = hdr(2).wrapping_sub(lsb);
        let tsb = self.vmtx.side_bearing(gid);
        let v_orig = hdr(8).wrapping_add(tsb);
        let h_adv = self.hmtx.advance(gid);
        let v_adv = self.vmtx.advance(gid);
        let f = sofel_geom::float::i32_to_f32;
        let i = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
        [
            Pt {
                x: f(h_delta),
                ..Pt::default()
            },
            Pt {
                x: f(i(h_adv).wrapping_add(h_delta)),
                ..Pt::default()
            },
            Pt {
                y: f(v_orig),
                ..Pt::default()
            },
            Pt {
                y: f(v_orig.wrapping_sub(i(v_adv))),
                ..Pt::default()
            },
        ]
    }

    fn apply_variation(&mut self, gid: u32, g: &Glyph<'_>, points: &mut [Pt]) -> bool {
        let Some(instance) = self.variation else {
            return true;
        };
        let mut ends = (0..if g.kind == Kind::Simple {
            g.contours()
        } else {
            0
        })
            .filter_map(|contour| {
                u16_at(
                    g.bytes,
                    HEADER_SIZE.saturating_add(contour.saturating_mul(2)),
                )
            })
            .peekable();
        let mut varied: Vec<sofel_font::var::gvar::Point<f32>> = points
            .iter()
            .enumerate()
            .map(|(index, point)| {
                let end = ends.peek().is_some_and(|&last| usize::from(last) == index);
                if end {
                    ends.next();
                }
                sofel_font::var::gvar::Point {
                    x: point.x,
                    y: point.y,
                    end,
                }
            })
            .collect();
        if instance
            .apply_gvar_hb(gid, &mut varied, self.scratch, false, &mut self.budget)
            .is_err()
        {
            return false;
        }
        for (point, adjusted) in points.iter_mut().zip(varied) {
            point.x = adjusted.x;
            point.y = adjusted.y;
        }
        true
    }

    /// `Glyph::get_points` without horizontal shift or
    /// USE_MY_METRICS (as `compile_bytes_with_deltas` calls it).
    fn get_points(
        &mut self,
        gid: u32,
        g: &Glyph<'_>,
        all: &mut Vec<Pt>,
        comp: &mut Vec<Pt>,
        depth: u32,
    ) -> bool {
        if depth > crate::limits::MAX_NESTING_LEVEL {
            return false;
        }
        if self.edge_count > MAX_GRAPH_EDGE_COUNT {
            return false;
        }
        self.edge_count = self.edge_count.saturating_add(1);
        if let Some(h) = self.head.as_deref_mut() {
            h.max_component_depth = h.max_component_depth.max(depth);
        }
        let old_length = comp.len();
        let nc = u32::try_from(g.contours()).unwrap_or(0);
        match g.kind {
            Kind::Simple => {
                if depth == 0
                    && let Some(h) = self.head.as_deref_mut()
                {
                    h.max_contours = h.max_contours.max(nc);
                }
                if depth > 0
                    && let Some(cc) = self.composite_contours.as_mut()
                {
                    *cc = cc.saturating_add(nc);
                }
                let before = all.len();
                if !contour_points(g, all) {
                    return false;
                }
                all.extend(self.phantoms(gid, g));
                if !all
                    .get_mut(before..)
                    .is_some_and(|points| self.apply_variation(gid, g, points))
                {
                    return false;
                }
                if depth == 0
                    && let Some(h) = self.head.as_deref_mut()
                {
                    let n = all.len().saturating_sub(before).saturating_sub(4);
                    h.max_points = h.max_points.max(u32::try_from(n).unwrap_or(u32::MAX));
                }
                true
            }
            Kind::Composite => {
                for c in g.components() {
                    comp.push(transformation(g.bytes, &c, g.extended).1);
                }
                comp.extend(self.phantoms(gid, g));
                if !comp
                    .get_mut(old_length..)
                    .is_some_and(|points| self.apply_variation(gid, g, points))
                {
                    return false;
                }
                if depth == 0 {
                    self.points_with_deltas = comp.get(old_length..).unwrap_or(&[]).to_vec();
                }
                let level = self.decycler.enter();
                let mut comp_index = 0usize;
                let mut ok = true;
                for c in g.components() {
                    if !self.decycler.visit(level, c.gid) {
                        comp_index = comp_index.saturating_add(1);
                        continue;
                    }
                    let old_count = all.len();
                    let child = self.accel.glyph(c.gid, false);
                    if !self.get_points(c.gid, &child, all, comp, depth.saturating_add(1)) {
                        ok = false;
                        break;
                    }
                    let (m, _) = transformation(g.bytes, &c, g.extended);
                    let t = comp
                        .get(old_length.saturating_add(comp_index))
                        .copied()
                        .unwrap_or_default();
                    if let Some(cp) = all.get_mut(old_count..) {
                        transform_points(cp, m, t, c.flags);
                    }
                    if c.flags & ARGS_ARE_XY_VALUES == 0 {
                        let (p1, p2) = anchor_points(g.bytes, &c, g.extended);
                        let n = all.len().saturating_sub(old_count);
                        if p1 < all.len() && p2 < n {
                            let a = all.get(p1).copied().unwrap_or_default();
                            let b = all
                                .get(old_count.saturating_add(p2))
                                .copied()
                                .unwrap_or_default();
                            let delta = Pt {
                                x: a.x - b.x,
                                y: a.y - b.y,
                                flag: 0,
                            };
                            if let Some(cp) = all.get_mut(old_count..) {
                                translate(cp, delta);
                            }
                        }
                    }
                    all.truncate(all.len().saturating_sub(4));
                    if all.len() > MAX_POINTS {
                        ok = false;
                        break;
                    }
                    comp_index = comp_index.saturating_add(1);
                }
                self.decycler.leave();
                if !ok {
                    comp.truncate(old_length);
                    return false;
                }
                if depth == 0
                    && let Some(h) = self.head.as_deref_mut()
                {
                    if let Some(cc) = self.composite_contours {
                        h.max_composite_contours = h.max_composite_contours.max(cc);
                    }
                    let n = u32::try_from(all.len()).unwrap_or(u32::MAX);
                    h.max_composite_points = h.max_composite_points.max(n);
                    let e = u32::try_from(comp_index).unwrap_or(u32::MAX);
                    h.max_component_elements = h.max_component_elements.max(e);
                }
                let ph: Vec<Pt> = comp
                    .get(comp.len().saturating_sub(4)..)
                    .unwrap_or(&[])
                    .to_vec();
                all.extend(ph);
                comp.truncate(old_length);
                true
            }
            Kind::Empty => {
                all.extend(self.phantoms(gid, g));
                comp.truncate(old_length);
                true
            }
        }
    }
}

/// `SimpleGlyph::encode_coord`.
fn encode_coord(value: i32, flag: &mut u8, short: u8, same: u8, out: &mut Vec<u8>) {
    if value == 0 {
        *flag |= same;
    } else if (-255..=255).contains(&value) {
        *flag |= short;
        if value > 0 {
            *flag |= same;
        }
        out.push((value.unsigned_abs() & 0xFF) as u8);
    } else {
        out.extend_from_slice(&crate::instance::lo16(value).to_be_bytes());
    }
}

/// `SimpleGlyph::compile_bytes_with_deltas`: the glyph after its header.
fn compile_simple(g: &Glyph<'_>, points: &[Pt], no_hinting: bool) -> Vec<u8> {
    if g.contours() == 0 || points.len() <= 4 {
        return Vec::new();
    }
    let n = points.len().saturating_sub(4);
    let mut flags: Vec<u8> = Vec::with_capacity(n);
    let mut xs = Vec::with_capacity(n.saturating_mul(2));
    let mut ys = Vec::with_capacity(n.saturating_mul(2));
    let (mut last, mut repeat) = (255u32, 0u32);
    let (mut px, mut py) = (0i32, 0i32);
    for p in points.iter().take(n) {
        let mut flag = p.flag & (FLAG_ON_CURVE | FLAG_OVERLAP_SIMPLE | FLAG_CUBIC);
        let x = crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(p.x));
        let y = crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(p.y));
        encode_coord(
            x.wrapping_sub(px),
            &mut flag,
            FLAG_X_SHORT,
            FLAG_X_SAME,
            &mut xs,
        );
        encode_coord(
            y.wrapping_sub(py),
            &mut flag,
            FLAG_Y_SHORT,
            FLAG_Y_SAME,
            &mut ys,
        );
        // `encode_flag`.
        if u32::from(flag) == last && repeat != 255 {
            repeat = repeat.saturating_add(1);
            if repeat == 1 {
                flags.push(flag);
            } else {
                let len = flags.len();
                if let Some(f) = flags.get_mut(len.saturating_sub(2)) {
                    *f = flag | FLAG_REPEAT;
                }
                if let Some(f) = flags.get_mut(len.saturating_sub(1)) {
                    *f = (repeat & 0xFF) as u8;
                }
            }
        } else {
            repeat = 0;
            flags.push(flag);
        }
        px = x;
        py = y;
        last = u32::from(flag);
    }
    let before = 2usize.saturating_mul(g.contours()).saturating_add(2);
    let mut out = Vec::new();
    out.extend_from_slice(
        g.bytes
            .get(HEADER_SIZE..HEADER_SIZE.saturating_add(before))
            .unwrap_or(&[]),
    );
    if !no_hinting {
        let ins = g.simple_ins_len();
        let at = HEADER_SIZE.saturating_add(before);
        out.extend_from_slice(g.bytes.get(at..at.saturating_add(ins)).unwrap_or(&[]));
    }
    out.extend_from_slice(&flags);
    out.extend_from_slice(&xs);
    out.extend_from_slice(&ys);
    out
}

/// `CompositeGlyphRecord::compile_with_point` of the record `rec`.
fn compile_with_point(rec: &[u8], c: &Component, p: Pt, extended: bool, out: &mut Vec<u8>) {
    let mut before = 4usize;
    if extended && c.flags & GID_IS_24BIT != 0 {
        before = before.saturating_add(1);
    }
    let x = crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(p.x));
    let y = crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(p.y));
    let start = out.len();
    if c.flags & ARG_1_AND_2_ARE_WORDS != 0 {
        out.extend_from_slice(rec);
        let w = |v: i32| crate::instance::lo16(v).to_be_bytes();
        if let Some(s) = out
            .get_mut(start.saturating_add(before)..start.saturating_add(before).saturating_add(4))
        {
            s.copy_from_slice(&[w(x), w(y)].concat());
        }
    } else if (-128..=127).contains(&x) && (-128..=127).contains(&y) {
        out.extend_from_slice(rec);
        if let Some(s) = out
            .get_mut(start.saturating_add(before)..start.saturating_add(before).saturating_add(2))
        {
            s.copy_from_slice(&[crate::instance::lo8(x), crate::instance::lo8(y)]);
        }
    } else {
        out.extend_from_slice(rec.get(..before).unwrap_or(&[]));
        if let Some(s) = out.get_mut(start..start.saturating_add(2)) {
            s.copy_from_slice(&(c.flags | ARG_1_AND_2_ARE_WORDS).to_be_bytes());
        }
        out.extend_from_slice(&crate::instance::lo16(x).to_be_bytes());
        out.extend_from_slice(&crate::instance::lo16(y).to_be_bytes());
        out.extend_from_slice(rec.get(before.saturating_add(2)..).unwrap_or(&[]));
    }
}

/// `CompositeGlyph::compile_bytes_with_deltas`: the glyph after its
/// header, from `source` (the bytes to copy, without instructions with
/// `NO_HINTING`). `None` when a component has no point.
fn compile_composite(g: &Glyph<'_>, source: &[u8], points: &[Pt]) -> Option<Vec<u8>> {
    let nc = if g.bytes.len() >= HEADER_SIZE {
        sofel_font::read::be::i16_at(g.bytes, 0).unwrap_or(0)
    } else {
        0
    };
    if source.len() <= HEADER_SIZE || nc != -1 {
        return Some(Vec::new());
    }
    let mut out = Vec::with_capacity(source.len());
    let mut comp_len = 0usize;
    for (i, c) in components_of(source, g.extended).iter().enumerate() {
        if i >= points.len().saturating_sub(4) {
            return None;
        }
        let rec = source.get(c.at..c.at.saturating_add(c.size)).unwrap_or(&[]);
        if c.flags & ARGS_ARE_XY_VALUES == 0 {
            out.extend_from_slice(rec);
        } else {
            compile_with_point(
                rec,
                c,
                points.get(i).copied().unwrap_or_default(),
                g.extended,
                &mut out,
            );
        }
        comp_len = comp_len.saturating_add(c.size);
    }
    let src_len = source.len().saturating_sub(HEADER_SIZE);
    if src_len > comp_len {
        let at = HEADER_SIZE.saturating_add(comp_len);
        out.extend_from_slice(source.get(at..).unwrap_or(&[]));
    }
    Some(out)
}

/// The glyphs of the plan compiled at the default location
/// (`_populate_subset_glyphs` with an instancing font) and the metrics
/// they give; `None` when a glyph cannot be read (HarfBuzz then fails
/// the subset).
pub(crate) fn instance(
    plan_glyphs: &[(u32, u32)],
    flags: Flags,
    num_output_glyphs: u32,
    font: &Font<'_>,
    coords: &[i32],
) -> Option<(Vec<Vec<u8>>, crate::instance::Metrics)> {
    let accel = GlyfAccel::new(font);
    let hmtx = crate::tables::hmtx::MetricsAccel::dual(font, false);
    let vmtx = crate::tables::hmtx::MetricsAccel::dual(font, true);
    let notdef_outline = flags.has(Flags::NOTDEF_OUTLINE);
    let no_hinting = flags.has(Flags::NO_HINTING);
    let location = sofel_font::var::Location::from_normalized_2_14(font, coords);
    let instance = font.instance(&location);
    let mut gvar_scratch = sofel_font::var::gvar::HbScratch::default();
    let mut m = crate::instance::Metrics {
        bounds_width: alloc::vec![u32::MAX; num_output_glyphs as usize],
        bounds_height: alloc::vec![u32::MAX; num_output_glyphs as usize],
        ..crate::instance::Metrics::default()
    };
    let mut glyphs = Vec::with_capacity(plan_glyphs.len());
    for &(new, old) in plan_glyphs {
        let g = accel.glyph(old, true);
        let source: Vec<u8> = if no_hinting {
            let (a, b) = g.drop_hints_bytes();
            [a, b].concat()
        } else {
            g.bytes.to_vec()
        };
        let notdef = old == 0 && !notdef_outline;
        let counts = g.kind != Kind::Empty && !notdef;
        let mut head = m.head_maxp;
        let mut cx = PointsCx {
            accel: &accel,
            hmtx: &hmtx,
            vmtx: &vmtx,
            variation: coords.iter().any(|&c| c != 0).then_some(&instance),
            scratch: &mut gvar_scratch,
            budget: 1 << 24,
            decycler: Decycler::default(),
            edge_count: 0,
            head: counts.then_some(&mut head),
            composite_contours: counts.then_some(0),
            points_with_deltas: Vec::new(),
        };
        let mut all = Vec::new();
        let mut comp = Vec::new();
        if !cx.get_points(old, &g, &mut all, &mut comp, 0) {
            return None;
        }
        let pwd = core::mem::take(&mut cx.points_with_deltas);
        drop(cx);
        m.head_maxp = head;
        let kind = if notdef { Kind::Empty } else { g.kind };
        let mut start: Vec<u8> = Vec::new();
        let end = match kind {
            Kind::Composite => compile_composite(&g, &source, &pwd)?,
            Kind::Simple => compile_simple(&g, &all, no_hinting),
            Kind::Empty => {
                let nc0 = g.bytes.len() >= HEADER_SIZE
                    && g.contours() == 0
                    && sofel_font::read::be::i16_at(g.bytes, 0) == Some(0);
                if !no_hinting && nc0 {
                    let ins = g.simple_ins_len();
                    if ins != 0 {
                        let len = g.ins_len_at().saturating_add(2).saturating_add(ins);
                        start = g.bytes.get(..len).unwrap_or(g.bytes).to_vec();
                    }
                }
                Vec::new()
            }
        };
        // `compile_header_bytes`.
        let (mut x_min, mut x_max, mut y_min, mut y_max) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        if all.len() > 4 {
            let pts = all.get(..all.len().saturating_sub(4)).unwrap_or(&[]);
            if let Some(first) = pts.first() {
                x_min = first.x;
                x_max = first.x;
                y_min = first.y;
                y_max = first.y;
            }
            for p in pts.iter().skip(1) {
                x_min = if x_min <= p.x { x_min } else { p.x };
                x_max = if x_max >= p.x { x_max } else { p.x };
                y_min = if y_min <= p.y { y_min } else { p.y };
                y_max = if y_max >= p.y { y_max } else { p.y };
            }
        }
        let r = |v: f32| {
            let c =
                crate::instance::clamp_f64(sofel_geom::float::hb_roundf32(v), -32768.0, 32767.0);
            crate::instance::f64_to_i32(c)
        };
        let (rx0, rx1, ry0, ry1) = (r(x_min), r(x_max), r(y_min), r(y_max));
        update_mtx(&mut m, new, kind, (rx0, rx1, ry0, ry1), &all);
        if kind != Kind::Empty {
            let h = &mut m.head_maxp;
            h.x_min = h.x_min.min(rx0);
            h.y_min = h.y_min.min(ry0);
            h.x_max = h.x_max.max(rx1);
            h.y_max = h.y_max.max(ry1);
        }
        if kind != Kind::Empty && all.len() >= 4 {
            start = Vec::with_capacity(HEADER_SIZE);
            start.extend_from_slice(g.bytes.get(..2).unwrap_or(&[0, 0]));
            for v in [rx0, ry0, rx1, ry1] {
                start.extend_from_slice(&crate::instance::lo16(v).to_be_bytes());
            }
        }
        start.extend_from_slice(&end);
        glyphs.push(start);
    }
    m.head_maxp.settle();
    Some((glyphs, m))
}

/// `Glyph::update_mtx`.
fn update_mtx(
    m: &mut crate::instance::Metrics,
    new: u32,
    kind: Kind,
    (x_min, x_max, y_min, y_max): (i32, i32, i32, i32),
    all: &[Pt],
) {
    if kind != Kind::Empty {
        if let Some(w) = m.bounds_width.get_mut(new as usize) {
            *w = x_max.wrapping_sub(x_min).cast_unsigned();
        }
        if let Some(h) = m.bounds_height.get_mut(new as usize) {
            *h = y_max.wrapping_sub(y_min).cast_unsigned();
        }
    }
    let len = all.len();
    let at = |i: usize| all.get(len.saturating_sub(i)).copied().unwrap_or_default();
    let (left, right, top, bottom) = (at(4).x, at(3).x, at(2).y, at(1).y);
    let round = |v: f32| crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(v));
    let hori = round(right - left).max(0);
    let lsb = round(sofel_geom::float::i32_to_f32(x_min) - left);
    m.hmtx.insert(new, (hori.unsigned_abs(), lsb));
    if kind != Kind::Empty && lsb != x_min {
        m.head_maxp.all_x_min_is_lsb = false;
    }
    let vert = round(top - bottom).max(0);
    let tsb = round(top - sofel_geom::float::i32_to_f32(y_max));
    m.vmtx.insert(new, (vert.unsigned_abs(), tsb));
}

/// The table (glyf or GLYF) whose subsetting instances the outlines:
/// the one `glyf::subset` writes, when it is subset and its format is
/// known.
pub(crate) fn instanced_tag(plan: &Plan, font: &Font<'_>) -> Option<Tag> {
    let extended = font.table(GLYF_WIDE).is_some_and(|g| !g.is_empty())
        && font.table(LOCA_WIDE).is_some_and(|l| !l.is_empty());
    let (loc, data_format) = head_formats(font);
    if !(loc <= 1 && data_format <= 1) {
        return None;
    }
    [GLYF, GLYF_WIDE]
        .into_iter()
        .find(|&t| plan.subsets_table(font, t) && extended == (t == GLYF_WIDE))
}
