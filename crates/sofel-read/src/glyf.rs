//! Glyphs of the `glyf` and `GLYF` tables (ADR 0011).
//!
//! In fonts with `GLYF` (ISO OFF 5th edition), flag 0x80 of a simple glyph
//! marks cubic off-curve points and component flag 0x2000 a 24-bit glyph
//! id; callers pass `extended` to say which table the glyph comes from.
//! Decoding follows the checks FreeType 2.15.0 makes
//! (`TT_Load_Simple_Glyph`, `TT_Load_Composite_Glyph`).
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/glyf>

use crate::be::{self, to_u32, to_usize};
use crate::loca::Loca;
use crate::{Diagnostic, GlyphId, Kind, ReadError, Tag};

/// The `glyf` tag.
pub const TAG: Tag = Tag::new(b"glyf");
/// The `GLYF` tag, the variant with cubic outlines and 24-bit
/// component glyph ids.
pub const TAG_WIDE: Tag = Tag::new(b"GLYF");

/// Simple glyph flag: the point is on the curve.
pub const ON_CURVE: u8 = 0x01;
/// Simple glyph flag: the x delta is one byte.
pub const X_SHORT: u8 = 0x02;
/// Simple glyph flag: the y delta is one byte.
pub const Y_SHORT: u8 = 0x04;
/// Simple glyph flag: the next byte is a repeat count for this flag.
pub const REPEAT: u8 = 0x08;
/// Simple glyph flag: with [`X_SHORT`], the x delta is positive;
/// without it, x is the same as the previous point.
pub const X_SAME_OR_POSITIVE: u8 = 0x10;
/// Simple glyph flag: with [`Y_SHORT`], the y delta is positive;
/// without it, y is the same as the previous point.
pub const Y_SAME_OR_POSITIVE: u8 = 0x20;
/// Simple glyph flag: contours may overlap (first flag only).
pub const OVERLAP_SIMPLE: u8 = 0x40;
/// Simple glyph flag in `GLYF`: an off-curve point of a cubic curve.
pub const CUBIC: u8 = 0x80;

/// Component flag: the arguments are 16-bit; otherwise 8-bit.
pub const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
/// Component flag: the arguments are x and y offsets; otherwise point
/// numbers.
pub const ARGS_ARE_XY_VALUES: u16 = 0x0002;
/// Component flag: round the x and y offsets to the grid.
pub const ROUND_XY_TO_GRID: u16 = 0x0004;
/// Component flag: one scale for x and y follows.
pub const WE_HAVE_A_SCALE: u16 = 0x0008;
/// Component flag: another component record follows.
pub const MORE_COMPONENTS: u16 = 0x0020;
/// Component flag: separate x and y scales follow.
pub const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
/// Component flag: a 2x2 transform follows.
pub const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;
/// Component flag: instructions follow the last component.
pub const WE_HAVE_INSTRUCTIONS: u16 = 0x0100;
/// Component flag: use this component's metrics for the composite.
pub const USE_MY_METRICS: u16 = 0x0200;
/// Component flag: the components may overlap.
pub const OVERLAP_COMPOUND: u16 = 0x0400;
/// Component flag: the offset is scaled by the transform.
pub const SCALED_COMPONENT_OFFSET: u16 = 0x0800;
/// Component flag: the offset is not scaled by the transform.
pub const UNSCALED_COMPONENT_OFFSET: u16 = 0x1000;
/// Component flag in `GLYF`: the glyph id is 24-bit.
pub const GID_IS_24_BIT: u16 = 0x2000;

const HEADER: usize = 10;
/// FreeType rejects simple glyphs with this many contours or more.
const MAX_CONTOURS: u16 = 0xFFF;

/// One glyph's data: a header and a simple or composite description.
#[derive(Clone, Copy, Debug)]
pub struct Glyph<'a> {
    data: &'a [u8],
    h: &'a [u8; HEADER],
}

impl<'a> Glyph<'a> {
    /// A glyph over `data`, the bytes between two `loca` entries.
    /// Only the 10-byte header is checked; fails with
    /// [`ReadError::Truncated`] if it does not fit.
    pub fn new(data: &'a [u8]) -> Result<Glyph<'a>, ReadError> {
        let h = data
            .first_chunk::<HEADER>()
            .ok_or(ReadError::truncated(HEADER, data.len()))?;
        Ok(Glyph { data, h })
    }

    /// `numberOfContours`: positive for a simple glyph, negative for a
    /// composite one.
    pub fn number_of_contours(&self) -> i16 {
        i16::from_be_bytes([self.h[0], self.h[1]])
    }
    /// `xMin` of the glyph's bounding box, in font units.
    pub fn x_min(&self) -> i16 {
        i16::from_be_bytes([self.h[2], self.h[3]])
    }
    /// `yMin` of the glyph's bounding box, in font units.
    pub fn y_min(&self) -> i16 {
        i16::from_be_bytes([self.h[4], self.h[5]])
    }
    /// `xMax` of the glyph's bounding box, in font units.
    pub fn x_max(&self) -> i16 {
        i16::from_be_bytes([self.h[6], self.h[7]])
    }
    /// `yMax` of the glyph's bounding box, in font units.
    pub fn y_max(&self) -> i16 {
        i16::from_be_bytes([self.h[8], self.h[9]])
    }

    /// The simple glyph description (for `number_of_contours() > 0`).
    ///
    /// Fails with [`ReadError::Truncated`] if the data ends early, and
    /// with [`ReadError::InvalidValue`] for 0xFFF or more contours,
    /// unsorted end points, or a flag repeat past the last point.
    pub fn simple(&self) -> Result<Simple<'a>, ReadError> {
        parse_simple(self.data).map_err(|e| e.to_read_error())
    }

    /// Reads the instruction stream following the last component. A composite
    /// without WE_HAVE_INSTRUCTIONS has an empty program.
    pub fn composite_instructions(&self, extended: bool) -> Result<&'a [u8], ReadError> {
        let mut records = self.components(extended);
        let mut flags = 0;
        for record in records.by_ref() {
            flags = record?.flags;
        }
        if flags & WE_HAVE_INSTRUCTIONS == 0 {
            return Ok(&[]);
        }
        let offset = records.position();
        let start = offset.saturating_add(2);
        let length =
            be::u16_at(self.data, offset).ok_or(ReadError::truncated(start, self.data.len()))?;
        be::slice_at(self.data, start, usize::from(length)).ok_or(ReadError::truncated(
            start.saturating_add(usize::from(length)),
            self.data.len(),
        ))
    }

    /// The component records (for `number_of_contours() < 0`). The
    /// iterator ends after the record without MORE_COMPONENTS, or after
    /// the first error.
    pub fn components(&self, extended: bool) -> Components<'a> {
        Components {
            data: self.data,
            at: HEADER,
            extended,
            done: false,
        }
    }
}

/// Where and why a simple glyph could not be decoded.
#[derive(Clone, Copy, Debug)]
enum SimpleError {
    Truncated {
        at: usize,
        needed: usize,
        available: usize,
    },
    TooManyContours(u16),
    EndPointsNotSorted {
        at: usize,
        contour: usize,
    },
    FlagRepeatOverrun {
        at: usize,
    },
}

impl SimpleError {
    fn to_read_error(self) -> ReadError {
        match self {
            SimpleError::Truncated {
                needed, available, ..
            } => ReadError::Truncated { needed, available },
            _ => ReadError::InvalidValue,
        }
    }
}

/// A decoded layout of a simple glyph.
#[derive(Clone, Copy, Debug)]
pub struct Simple<'a> {
    end_points: &'a [[u8; 2]],
    instructions: &'a [u8],
    flags: &'a [u8],
    xs: &'a [u8],
    ys: &'a [u8],
    num_points: u32,
}

fn x_size(f: u8) -> usize {
    if f & X_SHORT != 0 {
        1
    } else if f & X_SAME_OR_POSITIVE != 0 {
        0
    } else {
        2
    }
}

fn y_size(f: u8) -> usize {
    if f & Y_SHORT != 0 {
        1
    } else if f & Y_SAME_OR_POSITIVE != 0 {
        0
    } else {
        2
    }
}

fn parse_simple(data: &[u8]) -> Result<Simple<'_>, SimpleError> {
    let len = data.len();
    let truncated = |at: usize, needed: usize| SimpleError::Truncated {
        at,
        needed,
        available: len,
    };
    let n = be::u16_at(data, 0).ok_or(truncated(0, HEADER))?;
    if n >= MAX_CONTOURS {
        return Err(SimpleError::TooManyContours(n));
    }
    let n = usize::from(n);
    let ends_len = n.saturating_mul(2);
    let end_points = be::chunks_at::<2>(data, HEADER, n)
        .ok_or(truncated(HEADER, HEADER.saturating_add(ends_len)))?;
    let mut num_points = 0usize;
    for (i, e) in end_points.iter().enumerate() {
        let e = usize::from(u16::from_be_bytes(*e));
        if e < num_points {
            return Err(SimpleError::EndPointsNotSorted {
                at: HEADER.saturating_add(i.saturating_mul(2)),
                contour: i,
            });
        }
        num_points = e.saturating_add(1);
    }
    let ins_at = HEADER.saturating_add(ends_len);
    let ins_len =
        usize::from(be::u16_at(data, ins_at).ok_or(truncated(ins_at, ins_at.saturating_add(2)))?);
    let ins_start = ins_at.saturating_add(2);
    let instructions = be::slice_at(data, ins_start, ins_len)
        .ok_or(truncated(ins_at, ins_start.saturating_add(ins_len)))?;

    let flags_start = ins_start.saturating_add(ins_len);
    let mut at = flags_start;
    let (mut count, mut x_len, mut y_len) = (0usize, 0usize, 0usize);
    while count < num_points {
        let flag_at = at;
        let f = be::u8_at(data, at).ok_or(truncated(at, at.saturating_add(1)))?;
        at = at.saturating_add(1);
        let mut reps = 1usize;
        if f & REPEAT != 0 {
            let r = usize::from(be::u8_at(data, at).ok_or(truncated(at, at.saturating_add(1)))?);
            at = at.saturating_add(1);
            if count.saturating_add(1).saturating_add(r) > num_points {
                return Err(SimpleError::FlagRepeatOverrun { at: flag_at });
            }
            reps = reps.saturating_add(r);
        }
        x_len = x_len.saturating_add(x_size(f).saturating_mul(reps));
        y_len = y_len.saturating_add(y_size(f).saturating_mul(reps));
        count = count.saturating_add(reps);
    }
    let flags =
        be::slice_at(data, flags_start, at.saturating_sub(flags_start)).ok_or(truncated(at, at))?;
    let xs = be::slice_at(data, at, x_len).ok_or(truncated(len, at.saturating_add(x_len)))?;
    let ys_start = at.saturating_add(x_len);
    let ys = be::slice_at(data, ys_start, y_len)
        .ok_or(truncated(len, ys_start.saturating_add(y_len)))?;
    Ok(Simple {
        end_points,
        instructions,
        flags,
        xs,
        ys,
        num_points: to_u32(num_points),
    })
}

/// One point of a simple glyph, in font units.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GlyphPoint {
    /// The x coordinate.
    pub x: i32,
    /// The y coordinate.
    pub y: i32,
    /// Whether the point is on the curve ([`ON_CURVE`]).
    pub on_curve: bool,
    /// An off-curve point of a cubic curve (GLYF only).
    pub cubic: bool,
}

impl<'a> Simple<'a> {
    /// The number of points: the last end point plus one.
    pub fn num_points(&self) -> u32 {
        self.num_points
    }

    /// The index of the last point of each contour.
    pub fn end_points(&self) -> impl Iterator<Item = u16> + 'a {
        self.end_points.iter().map(|e| u16::from_be_bytes(*e))
    }

    /// The TrueType instructions of the glyph.
    pub fn instructions(&self) -> &'a [u8] {
        self.instructions
    }

    /// Whether the first point has OVERLAP_SIMPLE.
    pub fn overlap(&self) -> bool {
        self.num_points > 0 && self.flags.first().is_some_and(|f| f & OVERLAP_SIMPLE != 0)
    }

    /// The points in order, with coordinates accumulated from the deltas.
    /// `extended` says the glyph comes from `GLYF`, where [`CUBIC`] counts.
    /// The iterator stops early if the data runs out.
    pub fn points(&self, extended: bool) -> Points<'a> {
        Points {
            flags: self.flags,
            xs: self.xs,
            ys: self.ys,
            remaining: self.num_points,
            flag: 0,
            repeat: 0,
            x: 0,
            y: 0,
            extended,
        }
    }
}

/// Iterator over the points of a simple glyph.
#[derive(Clone, Debug)]
pub struct Points<'a> {
    flags: &'a [u8],
    xs: &'a [u8],
    ys: &'a [u8],
    remaining: u32,
    flag: u8,
    repeat: u8,
    x: i32,
    y: i32,
    extended: bool,
}

fn take<'a>(s: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
    let (head, rest) = s.split_at_checked(n)?;
    *s = rest;
    Some(head)
}

fn delta(s: &mut &[u8], f: u8, short: u8, same_or_positive: u8) -> Option<i32> {
    if f & short != 0 {
        let b = i32::from(*take(s, 1)?.first()?);
        Some(if f & same_or_positive != 0 {
            b
        } else {
            b.wrapping_neg()
        })
    } else if f & same_or_positive != 0 {
        Some(0)
    } else {
        let b = take(s, 2)?;
        Some(i32::from(i16::from_be_bytes([*b.first()?, *b.get(1)?])))
    }
}

impl Iterator for Points<'_> {
    type Item = GlyphPoint;

    fn next(&mut self) -> Option<GlyphPoint> {
        self.remaining = self.remaining.checked_sub(1)?;
        if self.repeat > 0 {
            self.repeat = self.repeat.saturating_sub(1);
        } else {
            self.flag = *take(&mut self.flags, 1)?.first()?;
            if self.flag & REPEAT != 0 {
                self.repeat = *take(&mut self.flags, 1)?.first()?;
            }
        }
        let f = self.flag;
        self.x = self
            .x
            .saturating_add(delta(&mut self.xs, f, X_SHORT, X_SAME_OR_POSITIVE)?);
        self.y = self
            .y
            .saturating_add(delta(&mut self.ys, f, Y_SHORT, Y_SAME_OR_POSITIVE)?);
        Some(GlyphPoint {
            x: self.x,
            y: self.y,
            on_curve: f & ON_CURVE != 0,
            cubic: self.extended && f & CUBIC != 0,
        })
    }
}

/// One component record. Arguments are signed offsets with
/// ARGS_ARE_XY_VALUES and unsigned point numbers otherwise. The
/// transform is in F2Dot14 raw values; without one it is the identity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Component {
    /// The component `flags`, as raw bits.
    pub flags: u16,
    /// `glyphIndex` of the component glyph.
    pub glyph: GlyphId,
    /// `argument1`: an x offset or a point number of the composite.
    pub arg1: i32,
    /// `argument2`: a y offset or a point number of the component.
    pub arg2: i32,
    /// `xscale` of the transform (F2Dot14 raw value).
    pub xx: i16,
    /// `scale01` of the transform (F2Dot14 raw value).
    pub yx: i16,
    /// `scale10` of the transform (F2Dot14 raw value).
    pub xy: i16,
    /// `yscale` of the transform (F2Dot14 raw value).
    pub yy: i16,
}

impl Component {
    /// Whether [`ARGS_ARE_XY_VALUES`] is set.
    pub fn args_are_xy(&self) -> bool {
        self.flags & ARGS_ARE_XY_VALUES != 0
    }
    /// Whether any of the scale or 2x2 transform flags is set.
    pub fn has_scale(&self) -> bool {
        self.flags & (WE_HAVE_A_SCALE | WE_HAVE_AN_X_AND_Y_SCALE | WE_HAVE_A_TWO_BY_TWO) != 0
    }
}

/// Iterator over the component records of a composite glyph.
#[derive(Clone, Debug)]
pub struct Components<'a> {
    data: &'a [u8],
    at: usize,
    extended: bool,
    done: bool,
}

impl Components<'_> {
    /// Where the next record starts, for diagnostics.
    pub fn position(&self) -> usize {
        self.at
    }

    fn read(&mut self) -> Result<Component, ReadError> {
        let d = self.data;
        let at = self.at;
        let short = |need: usize| ReadError::truncated(need, d.len());
        if at.saturating_add(4) > d.len() {
            return Err(short(at.saturating_add(4)));
        }
        let flags = be::u16_at(d, at).ok_or(short(at.saturating_add(4)))?;
        let (glyph, args_at) = if self.extended && flags & GID_IS_24_BIT != 0 {
            (
                be::u24_at(d, at.saturating_add(2)).ok_or(short(at.saturating_add(5)))?,
                at.saturating_add(5),
            )
        } else {
            (
                u32::from(be::u16_at(d, at.saturating_add(2)).ok_or(short(at.saturating_add(4)))?),
                at.saturating_add(4),
            )
        };
        let words = flags & ARG_1_AND_2_ARE_WORDS != 0;
        let mut count = if words { 4usize } else { 2 };
        count = count.saturating_add(if flags & WE_HAVE_A_SCALE != 0 {
            2
        } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
            4
        } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
            8
        } else {
            0
        });
        let end = args_at.saturating_add(count);
        let b = be::slice_at(d, args_at, count).ok_or(short(end))?;
        let i16_at = |o: usize| be::i16_at(b, o).unwrap_or(0);
        let u16_at = |o: usize| be::u16_at(b, o).unwrap_or(0);
        let u8_at = |o: usize| be::u8_at(b, o).unwrap_or(0);
        let (arg1, arg2, rest) = match (flags & ARGS_ARE_XY_VALUES != 0, words) {
            (true, true) => (i32::from(i16_at(0)), i32::from(i16_at(2)), 4),
            (true, false) => (
                i32::from(i8::from_be_bytes([u8_at(0)])),
                i32::from(i8::from_be_bytes([u8_at(1)])),
                2,
            ),
            (false, true) => (i32::from(u16_at(0)), i32::from(u16_at(2)), 4),
            (false, false) => (i32::from(u8_at(0)), i32::from(u8_at(1)), 2),
        };
        let one = 0x4000i16;
        let (xx, yx, xy, yy) = if flags & WE_HAVE_A_SCALE != 0 {
            let s = i16_at(rest);
            (s, 0, 0, s)
        } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
            (i16_at(rest), 0, 0, i16_at(rest.saturating_add(2)))
        } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
            (
                i16_at(rest),
                i16_at(rest.saturating_add(2)),
                i16_at(rest.saturating_add(4)),
                i16_at(rest.saturating_add(6)),
            )
        } else {
            (one, 0, 0, one)
        };
        self.at = end;
        Ok(Component {
            flags,
            glyph: GlyphId(glyph),
            arg1,
            arg2,
            xx,
            yx,
            xy,
            yy,
        })
    }
}

impl Iterator for Components<'_> {
    type Item = Result<Component, ReadError>;

    fn next(&mut self) -> Option<Result<Component, ReadError>> {
        if self.done {
            return None;
        }
        let r = self.read();
        self.done = match &r {
            Ok(c) => c.flags & MORE_COMPONENTS == 0,
            Err(_) => true,
        };
        Some(r)
    }
}

/// Validates every glyph of `glyf` (or `GLYF`, as `tag` and `extended`
/// say). Glyph `i` is the data from loca entry `i` to entry `i + 1`;
/// glyphs whose entries are broken are skipped (the `loca` validator
/// reports those).
pub fn validate(
    data: &[u8],
    loca: &Loca<'_>,
    num_glyphs: u32,
    extended: bool,
    tag: Tag,
    report: &mut dyn FnMut(Diagnostic),
) {
    let mut emit = |offset: usize, kind: Kind| report(Diagnostic::new(tag, to_u32(offset), kind));
    let count = to_usize(num_glyphs).min(loca.num_locations().saturating_sub(1));
    for i in 0..count {
        let (Some(start), Some(end)) = (loca.offset(i), loca.offset(i.saturating_add(1))) else {
            break;
        };
        let (start, end) = (to_usize(start), to_usize(end));
        if end <= start || end > data.len() {
            continue;
        }
        let Some(bytes) = be::slice_at(data, start, end.saturating_sub(start)) else {
            continue;
        };
        let glyph = to_u32(i);
        let Ok(g) = Glyph::new(bytes) else {
            emit(
                start,
                Kind::Truncated {
                    needed: to_u32(HEADER),
                    available: to_u32(bytes.len()),
                },
            );
            continue;
        };
        let n = g.number_of_contours();
        if n > 0 {
            if let Err(e) = parse_simple(bytes) {
                let (at, kind) = match e {
                    SimpleError::Truncated {
                        at,
                        needed,
                        available,
                    } => (
                        at,
                        Kind::Truncated {
                            needed: to_u32(needed),
                            available: to_u32(available),
                        },
                    ),
                    SimpleError::TooManyContours(found) => (
                        0,
                        Kind::TooManyContours {
                            glyph,
                            found: u32::from(found),
                        },
                    ),
                    SimpleError::EndPointsNotSorted { at, contour } => (
                        at,
                        Kind::EndPointsNotSorted {
                            glyph,
                            contour: to_u32(contour),
                        },
                    ),
                    SimpleError::FlagRepeatOverrun { at } => {
                        (at, Kind::FlagRepeatOverrun { glyph })
                    }
                };
                emit(start.saturating_add(at), kind);
            }
        } else if n < 0 {
            let mut comps = g.components(extended);
            let mut index = 0u32;
            loop {
                let at = comps.position();
                match comps.next() {
                    None => break,
                    Some(Err(ReadError::Truncated { needed, available })) => {
                        emit(
                            start.saturating_add(at),
                            Kind::Truncated {
                                needed: to_u32(needed),
                                available: to_u32(available),
                            },
                        );
                        break;
                    }
                    Some(Err(_)) => break,
                    Some(Ok(c)) => {
                        if c.glyph.0 >= num_glyphs {
                            emit(
                                start.saturating_add(at),
                                Kind::ComponentGlyphOutOfRange {
                                    glyph,
                                    component: index,
                                    found: c.glyph.0,
                                },
                            );
                        }
                    }
                }
                index = index.saturating_add(1);
            }
        }
    }
}
