//! Unhinted CID-keyed Type 1 charstring outlines (sofel ADR 0065).

use alloc::vec::Vec;

use sofel_font::CidFace;
use sofel_geom::{Pen, Point, mul_fix};

use crate::{CBox, OutlineError, OutlineInfo, Scale};

/// Draw a CID-keyed Type 1 glyph using its CID as glyph index.
/// Unsupported Type 1 operators return [`OutlineError::InvalidCharString`].
pub fn draw_cid(
    face: &CidFace,
    cid: usize,
    scale: Scale,
    pen: &mut impl Pen,
) -> Result<OutlineInfo, OutlineError> {
    let program = face
        .glyph_program(cid)
        .ok_or(OutlineError::InvalidGlyphIndex)?;
    let matrix = face.matrix(cid).ok_or(OutlineError::InvalidGlyphIndex)?;
    let mut state = State::new(
        scale,
        [
            matrix.xx, matrix.yx, matrix.xy, matrix.yy, matrix.dx, matrix.dy,
        ],
        pen,
    );
    let mut stack = Vec::new();
    decode_program(face, cid, &program, &mut state, &mut stack, 0, false)?;
    Ok(state.info())
}

fn decode_program<P: Pen>(
    face: &CidFace,
    cid: usize,
    program: &[u8],
    state: &mut State<'_, P>,
    stack: &mut Vec<i32>,
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
            32..=246 => stack.push(i32::from(op).saturating_sub(139)),
            247..=250 => {
                let next = i32::from(*program.get(at).ok_or(OutlineError::InvalidCharString)?);
                at = at.checked_add(1).ok_or(OutlineError::InvalidCharString)?;
                stack.push(
                    i32::from(op)
                        .saturating_sub(247)
                        .saturating_mul(256)
                        .saturating_add(next)
                        .saturating_add(108),
                );
            }
            251..=254 => {
                let next = i32::from(*program.get(at).ok_or(OutlineError::InvalidCharString)?);
                at = at.checked_add(1).ok_or(OutlineError::InvalidCharString)?;
                stack.push(
                    i32::from(op)
                        .saturating_sub(251)
                        .saturating_mul(256)
                        .saturating_add(next)
                        .saturating_add(108)
                        .saturating_neg(),
                );
            }
            255 => {
                let end = at.checked_add(4).ok_or(OutlineError::InvalidCharString)?;
                let bytes: [u8; 4] = program
                    .get(at..end)
                    .ok_or(OutlineError::InvalidCharString)?
                    .try_into()
                    .map_err(|_| OutlineError::InvalidCharString)?;
                stack.push(i32::from_be_bytes(bytes));
                at = end;
            }
            1 | 3 => stack.clear(), // hstem and vstem: no hinting
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
                let index = usize::try_from(index).map_err(|_| OutlineError::InvalidCharString)?;
                let subr = face
                    .subroutine_program(cid, index)
                    .ok_or(OutlineError::InvalidCharString)?;
                decode_program(
                    face,
                    cid,
                    &subr,
                    state,
                    stack,
                    depth.saturating_add(1),
                    true,
                )?;
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
                    0..=2 => stack.clear(), // dotsection, vstem3, hstem3
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
                        stack.push(a.checked_div(b).ok_or(OutlineError::InvalidCharString)?);
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

fn take<const N: usize>(stack: &mut Vec<i32>) -> Result<[i32; N], OutlineError> {
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
    x: i32,
    y: i32,
    start: Option<(i32, i32)>,
    emitted: bool,
    box_: Option<CBox>,
}

impl<'a, P: Pen> State<'a, P> {
    fn new(scale: Scale, matrix: [i32; 6], pen: &'a mut P) -> Self {
        Self {
            scale,
            matrix,
            pen,
            x: 0,
            y: 0,
            start: None,
            emitted: false,
            box_: None,
        }
    }

    fn point(&mut self, x: i32, y: i32) -> Point {
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
        p
    }

    fn move_by(&mut self, dx: i32, dy: i32) -> Result<(), OutlineError> {
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

    fn line_by(&mut self, dx: i32, dy: i32) -> Result<(), OutlineError> {
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
        dx1: i32,
        dy1: i32,
        dx2: i32,
        dy2: i32,
        dx3: i32,
        dy3: i32,
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
