//! Unhinted Type 1 charstring outlines (sofel ADR 0065).
//! Operator and contour behavior follows FreeType 2.15.0
//! `src/psaux/t1decode.c`, `src/psaux/psintrp.c`, and `src/psaux/psobjs.c` (FTL).

use alloc::vec::Vec;

use sofel_font::Type1Face;
use sofel_geom::{FnPen, Pen, Point, mul_fix};

use crate::{CBox, HintTarget, OutlineError, OutlineInfo, Scale, ps_native};

/// Draw one PFA or PFB Type 1 glyph by glyph index.
/// Unsupported Type 1 operators return [`OutlineError::InvalidCharString`].
pub fn draw_type1(
    face: &Type1Face<'_>,
    glyph: usize,
    scale: Scale,
    pen: &mut impl Pen,
) -> Result<OutlineInfo, OutlineError> {
    let program = face
        .glyph_program(glyph)
        .ok_or(OutlineError::InvalidGlyphIndex)?;
    let mut state = State::new(scale, [65536, 0, 0, 65536, 0, 0], false, pen);
    let mut stack = Vec::new();
    decode_program(face, &program, &mut state, &mut stack, 0, false)?;
    Ok(state.info())
}

/// Draw a stemless Type 1 glyph at Adobe's native path scale and return its
/// grid-fitted horizontal advance. The caller supplies FreeType's 16.16 size
/// scale; glyphs with stems remain unsupported.
pub fn draw_type1_native(
    face: &Type1Face<'_>,
    glyph: usize,
    scale: Scale,
    _target: HintTarget,
    pen: &mut impl Pen,
) -> Result<(OutlineInfo, i32), OutlineError> {
    let Scale::Scaled { x, .. } = scale else {
        return Err(OutlineError::InvalidOutline);
    };
    let program = face
        .glyph_program(glyph)
        .ok_or(OutlineError::InvalidGlyphIndex)?;
    let mut null_pen = FnPen(|_| {});
    let mut scan = State::new(
        Scale::Unscaled,
        [65536, 0, 0, 65536, 0, 0],
        false,
        &mut null_pen,
    );
    decode_program(face, &program, &mut scan, &mut Vec::new(), 0, false)?;
    if scan.has_stems {
        return Err(OutlineError::UnsupportedFormat);
    }
    let mut state = State::new(scale, [65536, 0, 0, 65536, 0, 0], true, pen);
    decode_program(face, &program, &mut state, &mut Vec::new(), 0, false)?;
    let metrics = face
        .glyph_metrics(glyph)
        .ok_or(OutlineError::InvalidCharString)?;
    Ok((state.info(), ps_native::advance(metrics.advance_x, x)))
}

fn decode_program<P: Pen>(
    face: &Type1Face<'_>,
    program: &[u8],
    state: &mut State<'_, P>,
    stack: &mut Vec<i64>,
    depth: usize,
    subroutine: bool,
) -> Result<(), OutlineError> {
    if depth >= 32 {
        return Err(OutlineError::InvalidCharString);
    }
    let mut at = 0usize;
    while let Some(&op) = program.get(at) {
        at = at.checked_add(1).ok_or(OutlineError::InvalidCharString)?;
        match op {
            32..=246 => stack.push(i64::from(i32::from(op).saturating_sub(139)) << 16),
            247..=250 => {
                let next = i32::from(*program.get(at).ok_or(OutlineError::InvalidCharString)?);
                at = at.checked_add(1).ok_or(OutlineError::InvalidCharString)?;
                stack.push(
                    i64::from(
                        i32::from(op)
                            .saturating_sub(247)
                            .saturating_mul(256)
                            .saturating_add(next)
                            .saturating_add(108),
                    ) << 16,
                );
            }
            251..=254 => {
                let next = i32::from(*program.get(at).ok_or(OutlineError::InvalidCharString)?);
                at = at.checked_add(1).ok_or(OutlineError::InvalidCharString)?;
                stack.push(
                    i64::from(
                        i32::from(op)
                            .saturating_sub(251)
                            .saturating_mul(256)
                            .saturating_add(next)
                            .saturating_add(108)
                            .saturating_neg(),
                    ) << 16,
                );
            }
            255 => {
                let end = at.checked_add(4).ok_or(OutlineError::InvalidCharString)?;
                let bytes: [u8; 4] = program
                    .get(at..end)
                    .ok_or(OutlineError::InvalidCharString)?
                    .try_into()
                    .map_err(|_| OutlineError::InvalidCharString)?;
                stack.push(i64::from(i32::from_be_bytes(bytes)) << 16);
                at = end;
            }
            1 | 3 => {
                state.has_stems = true;
                stack.clear();
            }
            13 => {
                let [side, _advance] = take(stack)?;
                state.x = side;
                state.y = 0;
            }
            21 => {
                let [dx, dy] = take(stack)?;
                state.move_by(dx, dy)?;
            }
            22 => {
                let [dx] = take(stack)?;
                state.move_by(dx, 0)?;
            }
            4 => {
                let [dy] = take(stack)?;
                state.move_by(0, dy)?;
            }
            5 => {
                let [dx, dy] = take(stack)?;
                state.line_by(dx, dy)?;
            }
            6 => {
                let [dx] = take(stack)?;
                state.line_by(dx, 0)?;
            }
            7 => {
                let [dy] = take(stack)?;
                state.line_by(0, dy)?;
            }
            8 => {
                let [dx1, dy1, dx2, dy2, dx3, dy3] = take(stack)?;
                state.curve_by(dx1, dy1, dx2, dy2, dx3, dy3)?;
            }
            30 => {
                let [dy1, dx2, dy2, dx3] = take(stack)?;
                state.curve_by(0, dy1, dx2, dy2, dx3, 0)?;
            }
            31 => {
                let [dx1, dx2, dy2, dy3] = take(stack)?;
                state.curve_by(dx1, 0, dx2, dy2, 0, dy3)?;
            }
            9 => {
                if !stack.is_empty() {
                    return Err(OutlineError::InvalidCharString);
                }
                state.close();
            }
            10 => {
                let index = stack.pop().ok_or(OutlineError::InvalidCharString)?;
                let index =
                    usize::try_from(index >> 16).map_err(|_| OutlineError::InvalidCharString)?;
                let subr = face
                    .subroutine_program(index)
                    .ok_or(OutlineError::InvalidCharString)?;
                decode_program(face, &subr, state, stack, depth.saturating_add(1), true)?;
            }
            11 if subroutine => return Ok(()),
            14 => {
                if subroutine {
                    return Err(OutlineError::InvalidCharString);
                }
                if !stack.is_empty() {
                    return Err(OutlineError::InvalidCharString);
                }
                state.close();
                return Ok(());
            }
            12 => {
                let escape = *program.get(at).ok_or(OutlineError::InvalidCharString)?;
                at = at.checked_add(1).ok_or(OutlineError::InvalidCharString)?;
                match escape {
                    0 => stack.clear(), // dotsection
                    1 | 2 => {
                        state.has_stems = true;
                        stack.clear();
                    }
                    16 => {
                        let subr_no = stack.pop().ok_or(OutlineError::InvalidCharString)? >> 16;
                        let arg_count = usize::try_from(
                            stack.pop().ok_or(OutlineError::InvalidCharString)? >> 16,
                        )
                        .map_err(|_| OutlineError::InvalidCharString)?;
                        if subr_no == 3 {
                            if arg_count != 1 || stack.is_empty() {
                                return Err(OutlineError::InvalidCharString);
                            }
                            state.pending_pops = 1;
                            continue;
                        }
                        let points = match subr_no {
                            14..=17 => usize::try_from(
                                subr_no
                                    .checked_sub(13)
                                    .ok_or(OutlineError::InvalidCharString)?,
                            )
                            .map_err(|_| OutlineError::InvalidCharString)?,
                            18 => 6,
                            _ => return Err(OutlineError::InvalidCharString),
                        };
                        let weights = face
                            .master_weights()
                            .ok_or(OutlineError::InvalidCharString)?;
                        if arg_count != points.saturating_mul(weights.len())
                            || arg_count > stack.len()
                        {
                            return Err(OutlineError::InvalidCharString);
                        }
                        let start = stack
                            .len()
                            .checked_sub(arg_count)
                            .ok_or(OutlineError::InvalidCharString)?;
                        let args = stack.split_off(start);
                        for point in 0..points {
                            let mut value =
                                *args.get(point).ok_or(OutlineError::InvalidCharString)?;
                            for master in 1..weights.len() {
                                let index = weights
                                    .len()
                                    .checked_sub(1)
                                    .and_then(|n| point.checked_mul(n))
                                    .and_then(|n| points.checked_add(n))
                                    .and_then(|n| n.checked_add(master))
                                    .and_then(|n| n.checked_sub(1))
                                    .ok_or(OutlineError::InvalidCharString)?;
                                let delta =
                                    *args.get(index).ok_or(OutlineError::InvalidCharString)?;
                                let weighted = delta
                                    .checked_mul(i64::from(
                                        *weights
                                            .get(master)
                                            .ok_or(OutlineError::InvalidCharString)?,
                                    ))
                                    .and_then(|n| {
                                        n.checked_add(32768i64.checked_sub(i64::from(n < 0))?)
                                    })
                                    .ok_or(OutlineError::InvalidCharString)?
                                    >> 16;
                                value = value
                                    .checked_add(weighted)
                                    .ok_or(OutlineError::InvalidCharString)?;
                            }
                            stack.push(value);
                        }
                        state.pending_pops = points;
                    }
                    17 if state.pending_pops > 0 => {
                        state.pending_pops = state
                            .pending_pops
                            .checked_sub(1)
                            .ok_or(OutlineError::InvalidCharString)?
                    }
                    7 => {
                        let [sx, sy, _wx, _wy] = take(stack)?;
                        state.x = sx;
                        state.y = sy;
                    }
                    12 => {
                        let [a, b] = take(stack)?;
                        if b == 0 {
                            return Err(OutlineError::InvalidCharString);
                        }
                        stack.push(
                            a.checked_mul(65536)
                                .and_then(|n| n.checked_div(b))
                                .ok_or(OutlineError::InvalidCharString)?,
                        );
                    }
                    _ => return Err(OutlineError::InvalidCharString),
                }
            }
            _ => return Err(OutlineError::InvalidCharString),
        }
        if stack.len() > 48 {
            return Err(OutlineError::InvalidCharString);
        }
    }
    Err(OutlineError::InvalidCharString)
}

fn take<const N: usize>(stack: &mut Vec<i64>) -> Result<[i64; N], OutlineError> {
    let args = stack
        .as_slice()
        .try_into()
        .map_err(|_| OutlineError::InvalidCharString)?;
    stack.clear();
    Ok(args)
}

struct State<'a, P: Pen> {
    scale: Scale,
    matrix: [i32; 6],
    pen: &'a mut P,
    x: i64,
    y: i64,
    start: Option<(i64, i64)>,
    emitted: bool,
    box_: Option<CBox>,
    pending_pops: usize,
    has_stems: bool,
    native: bool,
}

impl<'a, P: Pen> State<'a, P> {
    fn new(scale: Scale, matrix: [i32; 6], native: bool, pen: &'a mut P) -> Self {
        Self {
            scale,
            matrix,
            pen,
            x: 0,
            y: 0,
            start: None,
            emitted: false,
            box_: None,
            pending_pops: 0,
            has_stems: false,
            native,
        }
    }

    fn point(&mut self, x: i64, y: i64) -> Point {
        if self.native
            && let Scale::Scaled { x: sx, y: sy } = self.scale
        {
            let p = Point {
                x: ps_native::coordinate_fixed(x, sx),
                y: ps_native::coordinate_fixed(y, sy),
            };
            self.update_box(p);
            return p;
        }
        let x = i32::try_from(x >> 16).unwrap_or(if x < 0 { i32::MIN } else { i32::MAX });
        let y = i32::try_from(y >> 16).unwrap_or(if y < 0 { i32::MIN } else { i32::MAX });
        let [xx, yx, xy, yy, dx, dy] = self.matrix;
        let transformed_x = mul_fix(x, xx)
            .saturating_add(mul_fix(y, xy))
            .saturating_add(dx);
        let transformed_y = mul_fix(x, yx)
            .saturating_add(mul_fix(y, yy))
            .saturating_add(dy);
        let p = match self.scale {
            Scale::Unscaled => Point {
                x: transformed_x,
                y: transformed_y,
            },
            Scale::Scaled { x: sx, y: sy } => Point {
                x: mul_fix(transformed_x, sx.0),
                y: mul_fix(transformed_y, sy.0),
            },
        };
        self.update_box(p);
        p
    }

    fn update_box(&mut self, p: Point) {
        match &mut self.box_ {
            Some(bounds) => {
                bounds.x_min = bounds.x_min.min(p.x);
                bounds.y_min = bounds.y_min.min(p.y);
                bounds.x_max = bounds.x_max.max(p.x);
                bounds.y_max = bounds.y_max.max(p.y);
            }
            None => {
                self.box_ = Some(CBox {
                    x_min: p.x,
                    y_min: p.y,
                    x_max: p.x,
                    y_max: p.y,
                })
            }
        }
    }

    fn move_by(&mut self, dx: i64, dy: i64) -> Result<(), OutlineError> {
        self.close();
        self.x = self
            .x
            .checked_add(dx)
            .ok_or(OutlineError::InvalidCharString)?;
        self.y = self
            .y
            .checked_add(dy)
            .ok_or(OutlineError::InvalidCharString)?;
        self.start = Some((self.x, self.y));
        self.emitted = false;
        Ok(())
    }

    fn ensure_started(&mut self) -> Result<(), OutlineError> {
        let (x, y) = self.start.ok_or(OutlineError::InvalidCharString)?;
        if !self.emitted {
            let p = self.point(x, y);
            self.pen.move_to(p);
            self.emitted = true;
        }
        Ok(())
    }

    fn line_by(&mut self, dx: i64, dy: i64) -> Result<(), OutlineError> {
        self.ensure_started()?;
        self.x = self
            .x
            .checked_add(dx)
            .ok_or(OutlineError::InvalidCharString)?;
        self.y = self
            .y
            .checked_add(dy)
            .ok_or(OutlineError::InvalidCharString)?;
        let p = self.point(self.x, self.y);
        self.pen.line_to(p);
        Ok(())
    }

    fn curve_by(
        &mut self,
        dx1: i64,
        dy1: i64,
        dx2: i64,
        dy2: i64,
        dx3: i64,
        dy3: i64,
    ) -> Result<(), OutlineError> {
        self.ensure_started()?;
        let x1 = self
            .x
            .checked_add(dx1)
            .ok_or(OutlineError::InvalidCharString)?;
        let y1 = self
            .y
            .checked_add(dy1)
            .ok_or(OutlineError::InvalidCharString)?;
        let x2 = x1.checked_add(dx2).ok_or(OutlineError::InvalidCharString)?;
        let y2 = y1.checked_add(dy2).ok_or(OutlineError::InvalidCharString)?;
        self.x = x2.checked_add(dx3).ok_or(OutlineError::InvalidCharString)?;
        self.y = y2.checked_add(dy3).ok_or(OutlineError::InvalidCharString)?;
        let p1 = self.point(x1, y1);
        let p2 = self.point(x2, y2);
        let p3 = self.point(self.x, self.y);
        self.pen.cubic_to(p1, p2, p3);
        Ok(())
    }

    fn close(&mut self) {
        if let Some((x, y)) = self.start.take() {
            if self.emitted {
                if (self.x, self.y) != (x, y) {
                    let p = self.point(x, y);
                    self.pen.line_to(p);
                }
                self.pen.close();
            }
            self.emitted = false;
        }
    }

    fn info(&self) -> OutlineInfo {
        OutlineInfo {
            overlap: false,
            cbox: self.box_.unwrap_or_default(),
        }
    }
}
