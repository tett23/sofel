//! Closed straight-segment path of FreeType 2.15.0's glyph stroker.
#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]

use super::stroke_border::{Border, BorderError};
use super::stroke_trig;
use alloc::vec::Vec;
use sofel_geom::{Point, div_fix, mul_div, mul_fix, vector_length};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Join {
    Round,
    Bevel,
    MiterVariable,
    MiterFixed,
}

type OutlineParts = (Vec<Point>, Vec<u8>, Vec<usize>);

pub(super) struct Stroker {
    pub(super) borders: [Border; 2],
    radius: i32,
    join: Join,
    miter_limit: i32,
    angle_in: i32,
    angle_out: i32,
    center: Point,
    line_length: i32,
    first_point: bool,
    subpath_start: Point,
    subpath_angle: i32,
    subpath_line_length: i32,
}

impl Stroker {
    pub(super) fn new(radius: i32, join: Join, miter_limit: i32) -> Self {
        Self {
            borders: [Border::default(), Border::default()],
            radius,
            join,
            miter_limit: miter_limit.max(0x10000),
            angle_in: 0,
            angle_out: 0,
            center: Point::default(),
            line_length: 0,
            first_point: true,
            subpath_start: Point::default(),
            subpath_angle: 0,
            subpath_line_length: 0,
        }
    }

    pub(super) fn begin(&mut self, p: Point) {
        self.first_point = true;
        self.center = p;
        self.subpath_start = p;
        self.angle_in = 0;
    }

    fn polar_at(&self, radius: i32, angle: i32) -> Result<Point, BorderError> {
        add(self.center, stroke_trig::from_polar(radius, angle))
    }

    fn arc_to(&mut self, side: usize) -> Result<(), BorderError> {
        let rotate = rotate(side);
        let mut total = stroke_trig::angle_diff(self.angle_in, self.angle_out);
        if total == stroke_trig::PI {
            total = -rotate * 2;
        }
        self.borders[side].arc_to(self.center, self.radius, self.angle_in + rotate, total)
    }

    fn inside(&mut self, side: usize, line_length: i32) -> Result<(), BorderError> {
        let theta = stroke_trig::angle_diff(self.angle_in, self.angle_out) / 2;
        let rotate = rotate(side);
        let mut sigma = Point::default();
        let intersect = if !self.borders[side].movable()
            || line_length == 0
            || !(-0x59C000..=0x59C000).contains(&theta)
        {
            false
        } else {
            sigma = stroke_trig::unit(theta);
            let min_length = mul_div(self.radius, sigma.y, sigma.x).saturating_abs();
            min_length != 0 && self.line_length >= min_length && line_length >= min_length
        };
        let delta = if intersect {
            let phi = self.angle_in + theta + rotate;
            self.polar_at(div_fix(self.radius, sigma.x), phi)?
        } else {
            self.polar_at(self.radius, self.angle_out + rotate)?
        };
        if !intersect {
            self.borders[side].set_movable(false);
        }
        self.borders[side].line_to(delta, false)
    }

    fn outside(&mut self, side: usize, line_length: i32) -> Result<(), BorderError> {
        if self.join == Join::Round {
            return self.arc_to(side);
        }
        let rotate = rotate(side);
        let fixed_bevel = self.join != Join::MiterVariable;
        let mut bevel = self.join == Join::Bevel;
        let mut phi = 0;
        let mut sigma = Point::default();
        if !bevel {
            let mut theta = stroke_trig::angle_diff(self.angle_in, self.angle_out) / 2;
            if theta == stroke_trig::PI2 {
                theta = -rotate;
            }
            phi = self.angle_in + theta + rotate;
            sigma = stroke_trig::from_polar(self.miter_limit, theta);
            if sigma.x < 0x10000 && (fixed_bevel || theta.saturating_abs() > 57) {
                bevel = true;
            }
        }
        if bevel {
            if fixed_bevel {
                let delta = self.polar_at(self.radius, self.angle_out + rotate)?;
                self.borders[side].set_movable(false);
                return self.borders[side].line_to(delta, false);
            }
            let middle = self.polar_at(mul_fix(self.radius, self.miter_limit), phi)?;
            let coef = div_fix(0x10000 - sigma.x, sigma.y);
            let delta = add(
                middle,
                Point {
                    x: mul_fix(middle.y - self.center.y, coef),
                    y: mul_fix(self.center.x - middle.x, coef),
                },
            )?;
            self.borders[side].line_to(delta, false)?;
            let delta = reflect(middle, delta)?;
            self.borders[side].line_to(delta, false)?;
            if line_length == 0 {
                let delta = self.polar_at(self.radius, self.angle_out + rotate)?;
                self.borders[side].line_to(delta, false)?;
            }
        } else {
            let length = mul_div(self.radius, self.miter_limit, sigma.x);
            let delta = self.polar_at(length, phi)?;
            self.borders[side].line_to(delta, false)?;
            if line_length == 0 {
                let delta = self.polar_at(self.radius, self.angle_out + rotate)?;
                self.borders[side].line_to(delta, false)?;
            }
        }
        Ok(())
    }

    fn corner(&mut self, line_length: i32) -> Result<(), BorderError> {
        let turn = stroke_trig::angle_diff(self.angle_in, self.angle_out);
        if turn == 0 {
            return Ok(());
        }
        let inside_side = usize::from(turn < 0);
        self.inside(inside_side, line_length)?;
        self.outside(1 - inside_side, line_length)
    }

    fn subpath_start(&mut self, angle: i32, line_length: i32) -> Result<(), BorderError> {
        let delta = stroke_trig::from_polar(self.radius, angle + stroke_trig::PI2);
        self.borders[0].move_to(add(self.center, delta)?)?;
        self.borders[1].move_to(sub(self.center, delta)?)?;
        self.subpath_angle = angle;
        self.first_point = false;
        self.subpath_line_length = line_length;
        Ok(())
    }

    pub(super) fn line_to(&mut self, to: Point) -> Result<(), BorderError> {
        let segment = sub(to, self.center)?;
        if segment.x == 0 && segment.y == 0 {
            return Ok(());
        }
        let line_length = vector_length(segment.x, segment.y);
        let angle = stroke_trig::atan2(segment.x, segment.y);
        let mut delta = stroke_trig::from_polar(self.radius, angle + stroke_trig::PI2);
        if self.first_point {
            self.subpath_start(angle, line_length)?;
        } else {
            self.angle_out = angle;
            self.corner(line_length)?;
        }
        for side in 0..=1 {
            self.borders[side].line_to(add(to, delta)?, true)?;
            delta = Point {
                x: -delta.x,
                y: -delta.y,
            };
        }
        self.angle_in = angle;
        self.center = to;
        self.line_length = line_length;
        Ok(())
    }

    pub(super) fn conic_to(&mut self, control: Point, to: Point) -> Result<(), BorderError> {
        if small(sub(self.center, control)?) && small(sub(control, to)?) {
            self.center = to;
            return Ok(());
        }
        let mut stack = [Point::default(); 34];
        stack[0] = to;
        stack[1] = control;
        stack[2] = self.center;
        let mut cursor = 0usize;
        let mut first_arc = true;
        loop {
            let mut angle_in = self.angle_in;
            let mut angle_out = self.angle_in;
            if cursor < 30
                && !conic_small(&stack[cursor..cursor + 3], &mut angle_in, &mut angle_out)?
            {
                if self.first_point {
                    self.angle_in = angle_in;
                }
                conic_split(&mut stack[cursor..cursor + 5])?;
                cursor += 2;
                continue;
            }
            if first_arc {
                first_arc = false;
                if self.first_point {
                    self.subpath_start(angle_in, 0)?;
                } else {
                    self.angle_out = angle_in;
                    self.corner(0)?;
                }
            } else if stroke_trig::angle_diff(self.angle_in, angle_in).saturating_abs()
                > stroke_trig::PI / 24
            {
                self.center = stack[cursor + 2];
                self.angle_out = angle_in;
                let saved = self.join;
                self.join = Join::Round;
                let result = self.corner(0);
                self.join = saved;
                result?;
            }
            let theta = stroke_trig::angle_diff(angle_in, angle_out) / 2;
            let phi = angle_in + theta;
            let length = div_fix(self.radius, stroke_trig::unit(theta).x);
            let alpha0 = if self.join != Join::Round {
                let delta = sub(stack[cursor], stack[cursor + 2])?;
                stroke_trig::atan2(delta.x, delta.y)
            } else {
                0
            };
            for side in 0..=1 {
                let rotate = rotate(side);
                let ctrl = add(
                    stack[cursor + 1],
                    stroke_trig::from_polar(length, phi + rotate),
                )?;
                let end = add(
                    stack[cursor],
                    stroke_trig::from_polar(self.radius, angle_out + rotate),
                )?;
                if self.join != Join::Round {
                    let start = self.borders[side].last().ok_or(BorderError::TooLarge)?;
                    let edge = sub(end, start)?;
                    let alpha1 = stroke_trig::atan2(edge.x, edge.y);
                    if stroke_trig::angle_diff(alpha0, alpha1).saturating_abs() > stroke_trig::PI2 {
                        let beta_vector = sub(stack[cursor + 2], start)?;
                        let gamma_vector = sub(stack[cursor], end)?;
                        let beta = stroke_trig::atan2(beta_vector.x, beta_vector.y);
                        let gamma = stroke_trig::atan2(gamma_vector.x, gamma_vector.y);
                        let blen = vector_length(edge.x, edge.y);
                        let sin_a = stroke_trig::unit(alpha1 - gamma).y.saturating_abs();
                        let sin_b = stroke_trig::unit(beta - gamma).y.saturating_abs();
                        let alen = mul_div(blen, sin_a, sin_b);
                        let delta = add(start, stroke_trig::from_polar(alen, beta))?;
                        self.borders[side].set_movable(false);
                        self.borders[side].line_to(delta, false)?;
                        self.borders[side].line_to(end, false)?;
                        self.borders[side].quad_to(ctrl, start)?;
                        self.borders[side].line_to(end, false)?;
                        continue;
                    }
                }
                self.borders[side].quad_to(ctrl, end)?;
            }
            self.angle_in = angle_out;
            if cursor == 0 {
                break;
            }
            cursor -= 2;
        }
        self.center = to;
        self.line_length = 0;
        Ok(())
    }

    pub(super) fn cubic_to(&mut self, c1: Point, c2: Point, to: Point) -> Result<(), BorderError> {
        if small(sub(self.center, c1)?) && small(sub(c1, c2)?) && small(sub(c2, to)?) {
            self.center = to;
            return Ok(());
        }
        let mut stack = [Point::default(); 37];
        stack[0] = to;
        stack[1] = c2;
        stack[2] = c1;
        stack[3] = self.center;
        let mut cursor = 0usize;
        let mut first_arc = true;
        loop {
            let mut angle_in = self.angle_in;
            let mut angle_mid = self.angle_in;
            let mut angle_out = self.angle_in;
            if cursor < 32
                && !cubic_small(
                    &stack[cursor..cursor + 4],
                    &mut angle_in,
                    &mut angle_mid,
                    &mut angle_out,
                )?
            {
                if self.first_point {
                    self.angle_in = angle_in;
                }
                cubic_split(&mut stack[cursor..cursor + 7])?;
                cursor += 3;
                continue;
            }
            if first_arc {
                first_arc = false;
                if self.first_point {
                    self.subpath_start(angle_in, 0)?;
                } else {
                    self.angle_out = angle_in;
                    self.corner(0)?;
                }
            } else if stroke_trig::angle_diff(self.angle_in, angle_in).saturating_abs()
                > stroke_trig::PI / 32
            {
                self.center = stack[cursor + 3];
                self.angle_out = angle_in;
                let saved = self.join;
                self.join = Join::Round;
                let result = self.corner(0);
                self.join = saved;
                result?;
            }
            let theta1 = stroke_trig::angle_diff(angle_in, angle_mid) / 2;
            let theta2 = stroke_trig::angle_diff(angle_mid, angle_out) / 2;
            let phi1 = angle_mean(angle_in, angle_mid);
            let phi2 = angle_mean(angle_mid, angle_out);
            let length1 = div_fix(self.radius, stroke_trig::unit(theta1).x);
            let length2 = div_fix(self.radius, stroke_trig::unit(theta2).x);
            let alpha0 = if self.join != Join::Round {
                let delta = sub(stack[cursor], stack[cursor + 3])?;
                stroke_trig::atan2(delta.x, delta.y)
            } else {
                0
            };
            for side in 0..=1 {
                let rotate = rotate(side);
                let ctrl1 = add(
                    stack[cursor + 2],
                    stroke_trig::from_polar(length1, phi1 + rotate),
                )?;
                let ctrl2 = add(
                    stack[cursor + 1],
                    stroke_trig::from_polar(length2, phi2 + rotate),
                )?;
                let end = add(
                    stack[cursor],
                    stroke_trig::from_polar(self.radius, angle_out + rotate),
                )?;
                if self.join != Join::Round {
                    let start = self.borders[side].last().ok_or(BorderError::TooLarge)?;
                    let edge = sub(end, start)?;
                    let alpha1 = stroke_trig::atan2(edge.x, edge.y);
                    if stroke_trig::angle_diff(alpha0, alpha1).saturating_abs() > stroke_trig::PI2 {
                        let beta_vector = sub(stack[cursor + 3], start)?;
                        let gamma_vector = sub(stack[cursor], end)?;
                        let beta = stroke_trig::atan2(beta_vector.x, beta_vector.y);
                        let gamma = stroke_trig::atan2(gamma_vector.x, gamma_vector.y);
                        let blen = vector_length(edge.x, edge.y);
                        let sin_a = stroke_trig::unit(alpha1 - gamma).y.saturating_abs();
                        let sin_b = stroke_trig::unit(beta - gamma).y.saturating_abs();
                        let alen = mul_div(blen, sin_a, sin_b);
                        let delta = add(start, stroke_trig::from_polar(alen, beta))?;
                        self.borders[side].set_movable(false);
                        self.borders[side].line_to(delta, false)?;
                        self.borders[side].line_to(end, false)?;
                        self.borders[side].cubic_to(ctrl2, ctrl1, start)?;
                        self.borders[side].line_to(end, false)?;
                        continue;
                    }
                }
                self.borders[side].cubic_to(ctrl1, ctrl2, end)?;
            }
            self.angle_in = angle_out;
            if cursor == 0 {
                break;
            }
            cursor -= 3;
        }
        self.center = to;
        self.line_length = 0;
        Ok(())
    }

    pub(super) fn end(&mut self) -> Result<(), BorderError> {
        if self.first_point {
            return Ok(());
        }
        if (i64::from(self.center.x) - i64::from(self.subpath_start.x)).abs() >= 2
            || (i64::from(self.center.y) - i64::from(self.subpath_start.y)).abs() >= 2
        {
            self.line_to(self.subpath_start)?;
        }
        self.angle_out = self.subpath_angle;
        self.corner(self.subpath_line_length)?;
        self.borders[0].close(false);
        self.borders[1].close(true);
        Ok(())
    }

    pub(super) fn export(&self, side: Option<usize>) -> Result<OutlineParts, BorderError> {
        let mut points = Vec::new();
        let mut tags = Vec::new();
        let mut ends = Vec::new();
        for i in side.map_or_else(|| alloc::vec![0, 1], |v| alloc::vec![v]) {
            self.borders[i].export(&mut points, &mut tags, &mut ends)?;
        }
        Ok((points, tags, ends))
    }
}

fn rotate(side: usize) -> i32 {
    if side == 0 {
        stroke_trig::PI2
    } else {
        -stroke_trig::PI2
    }
}
fn add(a: Point, b: Point) -> Result<Point, BorderError> {
    Ok(Point {
        x: a.x.checked_add(b.x).ok_or(BorderError::TooLarge)?,
        y: a.y.checked_add(b.y).ok_or(BorderError::TooLarge)?,
    })
}
fn sub(a: Point, b: Point) -> Result<Point, BorderError> {
    Ok(Point {
        x: a.x.checked_sub(b.x).ok_or(BorderError::TooLarge)?,
        y: a.y.checked_sub(b.y).ok_or(BorderError::TooLarge)?,
    })
}
fn reflect(middle: Point, point: Point) -> Result<Point, BorderError> {
    let x = i64::from(middle.x) * 2 - i64::from(point.x);
    let y = i64::from(middle.y) * 2 - i64::from(point.y);
    Ok(Point {
        x: i32::try_from(x).map_err(|_| BorderError::TooLarge)?,
        y: i32::try_from(y).map_err(|_| BorderError::TooLarge)?,
    })
}

fn small(v: Point) -> bool {
    v.x > -2 && v.x < 2 && v.y > -2 && v.y < 2
}

fn conic_small(
    base: &[Point],
    angle_in: &mut i32,
    angle_out: &mut i32,
) -> Result<bool, BorderError> {
    let d1 = sub(base[1], base[2])?;
    let d2 = sub(base[0], base[1])?;
    if small(d1) {
        if !small(d2) {
            *angle_in = stroke_trig::atan2(d2.x, d2.y);
            *angle_out = *angle_in;
        }
    } else if small(d2) {
        *angle_in = stroke_trig::atan2(d1.x, d1.y);
        *angle_out = *angle_in;
    } else {
        *angle_in = stroke_trig::atan2(d1.x, d1.y);
        *angle_out = stroke_trig::atan2(d2.x, d2.y);
    }
    Ok(stroke_trig::angle_diff(*angle_in, *angle_out).saturating_abs() < stroke_trig::PI / 6)
}

fn conic_split(base: &mut [Point]) -> Result<(), BorderError> {
    let first = base[0];
    let middle = base[1];
    let last = base[2];
    base[4] = last;
    for coord in 0..=1 {
        let a = i64::from(if coord == 0 { first.x } else { first.y })
            + i64::from(if coord == 0 { middle.x } else { middle.y });
        let b = i64::from(if coord == 0 { middle.x } else { middle.y })
            + i64::from(if coord == 0 { last.x } else { last.y });
        let values = [a >> 1, (a + b) >> 2, b >> 1];
        for (index, value) in [1, 2, 3].into_iter().zip(values) {
            let value = i32::try_from(value).map_err(|_| BorderError::TooLarge)?;
            if coord == 0 {
                base[index].x = value;
            } else {
                base[index].y = value;
            }
        }
    }
    Ok(())
}

fn angle_mean(a: i32, b: i32) -> i32 {
    a + stroke_trig::angle_diff(a, b) / 2
}

fn cubic_small(
    base: &[Point],
    angle_in: &mut i32,
    angle_mid: &mut i32,
    angle_out: &mut i32,
) -> Result<bool, BorderError> {
    let d1 = sub(base[2], base[3])?;
    let d2 = sub(base[1], base[2])?;
    let d3 = sub(base[0], base[1])?;
    let close1 = small(d1);
    let close2 = small(d2);
    let close3 = small(d3);
    if close1 {
        if close2 {
            if !close3 {
                *angle_in = stroke_trig::atan2(d3.x, d3.y);
                *angle_mid = *angle_in;
                *angle_out = *angle_in;
            }
        } else if close3 {
            *angle_in = stroke_trig::atan2(d2.x, d2.y);
            *angle_mid = *angle_in;
            *angle_out = *angle_in;
        } else {
            *angle_in = stroke_trig::atan2(d2.x, d2.y);
            *angle_mid = *angle_in;
            *angle_out = stroke_trig::atan2(d3.x, d3.y);
        }
    } else if close2 {
        if close3 {
            *angle_in = stroke_trig::atan2(d1.x, d1.y);
            *angle_mid = *angle_in;
            *angle_out = *angle_in;
        } else {
            *angle_in = stroke_trig::atan2(d1.x, d1.y);
            *angle_out = stroke_trig::atan2(d3.x, d3.y);
            *angle_mid = angle_mean(*angle_in, *angle_out);
        }
    } else if close3 {
        *angle_in = stroke_trig::atan2(d1.x, d1.y);
        *angle_mid = stroke_trig::atan2(d2.x, d2.y);
        *angle_out = *angle_mid;
    } else {
        *angle_in = stroke_trig::atan2(d1.x, d1.y);
        *angle_mid = stroke_trig::atan2(d2.x, d2.y);
        *angle_out = stroke_trig::atan2(d3.x, d3.y);
    }
    let theta1 = stroke_trig::angle_diff(*angle_in, *angle_mid).saturating_abs();
    let theta2 = stroke_trig::angle_diff(*angle_mid, *angle_out).saturating_abs();
    Ok(theta1 < stroke_trig::PI / 8 && theta2 < stroke_trig::PI / 8)
}

fn cubic_split(base: &mut [Point]) -> Result<(), BorderError> {
    let original = [base[0], base[1], base[2], base[3]];
    base[6] = original[3];
    for coord in 0..=1 {
        let v = original.map(|p| i64::from(if coord == 0 { p.x } else { p.y }));
        let mut a = v[0] + v[1];
        let b = v[1] + v[2];
        let mut c = v[2] + v[3];
        let value5 = c >> 1;
        c += b;
        let value4 = c >> 2;
        let value1 = a >> 1;
        a += b;
        let value2 = a >> 2;
        let value3 = (a + c) >> 3;
        for (index, value) in [
            (1, value1),
            (2, value2),
            (3, value3),
            (4, value4),
            (5, value5),
        ] {
            let value = i32::try_from(value).map_err(|_| BorderError::TooLarge)?;
            if coord == 0 {
                base[index].x = value;
            } else {
                base[index].y = value;
            }
        }
    }
    Ok(())
}
