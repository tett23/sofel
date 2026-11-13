//! The temporary point and tag stream of FreeType 2.15.0's `ftstroke.c`.
#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]

use alloc::vec::Vec;
use sofel_geom::{Point, mul_fix};

use super::stroke_trig;

const ON: u8 = 1;
const CUBIC: u8 = 2;
const BEGIN: u8 = 4;
const END: u8 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BorderError {
    TooLarge,
}

#[derive(Default)]
pub(super) struct Border {
    points: Vec<Point>,
    tags: Vec<u8>,
    movable: bool,
    start: Option<usize>,
}

impl Border {
    pub(super) fn movable(&self) -> bool {
        self.movable
    }

    pub(super) fn set_movable(&mut self, value: bool) {
        self.movable = value;
    }

    pub(super) fn last(&self) -> Option<Point> {
        self.points.last().copied()
    }

    fn append(&mut self, p: Point, tag: u8) -> Result<(), BorderError> {
        // `close` replaces the first point with the final point and drops
        // that final point, so one temporary point beyond the output limit
        // can still produce an outline of exactly 65535 points.
        if self.points.len() >= 65536 {
            return Err(BorderError::TooLarge);
        }
        self.points.push(p);
        self.tags.push(tag);
        Ok(())
    }

    pub(super) fn close(&mut self, reverse: bool) {
        let Some(start) = self.start.take() else {
            return;
        };
        let count = self.points.len();
        if count <= start + 1 {
            self.points.truncate(start);
            self.tags.truncate(start);
        } else {
            self.points[start] = self.points[count - 1];
            self.tags[start] = self.tags[count - 1];
            self.points.pop();
            self.tags.pop();
            let last = self.points.len();
            if reverse {
                self.points[start + 1..last].reverse();
                self.tags[start + 1..last].reverse();
            }
            self.tags[start] |= BEGIN;
            self.tags[last - 1] |= END;
        }
        self.movable = false;
    }

    pub(super) fn line_to(&mut self, p: Point, movable: bool) -> Result<(), BorderError> {
        let Some(start) = self.start else {
            return Err(BorderError::TooLarge);
        };
        if self.movable {
            *self.points.last_mut().ok_or(BorderError::TooLarge)? = p;
        } else {
            if self.points.len() > start
                && let Some(last) = self.points.last()
                && (i64::from(last.x) - i64::from(p.x)).abs() < 2
                && (i64::from(last.y) - i64::from(p.y)).abs() < 2
            {
                return Ok(());
            }
            self.append(p, ON)?;
        }
        self.movable = movable;
        Ok(())
    }

    pub(super) fn quad_to(&mut self, control: Point, p: Point) -> Result<(), BorderError> {
        self.append(control, 0)?;
        self.append(p, ON)?;
        self.movable = false;
        Ok(())
    }

    pub(super) fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) -> Result<(), BorderError> {
        self.append(c1, CUBIC)?;
        self.append(c2, CUBIC)?;
        self.append(p, ON)?;
        self.movable = false;
        Ok(())
    }

    pub(super) fn move_to(&mut self, p: Point) -> Result<(), BorderError> {
        if self.start.is_some() {
            self.close(false);
        }
        self.start = Some(self.points.len());
        self.movable = false;
        self.line_to(p, false)
    }

    pub(super) fn arc_to(
        &mut self,
        center: Point,
        radius: i32,
        angle_start: i32,
        angle_diff: i32,
    ) -> Result<(), BorderError> {
        let arcs = (i64::from(angle_diff).abs() + i64::from(stroke_trig::PI2) - 1)
            / i64::from(stroke_trig::PI2);
        let arcs = i32::try_from(arcs.max(1)).map_err(|_| BorderError::TooLarge)?;
        let coef = stroke_trig::tan(angle_diff / (4 * arcs));
        let coef = coef + coef / 3;
        let a0 = stroke_trig::from_polar(radius, angle_start);
        let mut a1 = add(
            a0,
            Point {
                x: mul_fix(-a0.y, coef),
                y: mul_fix(a0.x, coef),
            },
        )?;
        a1 = add(a1, center)?;
        for i in 1..=arcs {
            let a3 = stroke_trig::from_polar(radius, angle_start + i * angle_diff / arcs);
            let a2 = add(
                a3,
                Point {
                    x: mul_fix(a3.y, coef),
                    y: mul_fix(-a3.x, coef),
                },
            )?;
            let a3 = add(a3, center)?;
            let a2 = add(a2, center)?;
            self.cubic_to(a1, a2, a3)?;
            if i < arcs {
                let reflect = |end: i32, control: i32| {
                    i32::try_from(2 * i64::from(end) - i64::from(control))
                        .map_err(|_| BorderError::TooLarge)
                };
                a1 = Point {
                    x: reflect(a3.x, a2.x)?,
                    y: reflect(a3.y, a2.y)?,
                };
            }
        }
        Ok(())
    }

    pub(super) fn export(
        &self,
        points: &mut Vec<Point>,
        tags: &mut Vec<u8>,
        ends: &mut Vec<usize>,
    ) -> Result<(), BorderError> {
        if points.len() + self.points.len() > 65535 {
            return Err(BorderError::TooLarge);
        }
        let contours = self.tags.iter().filter(|tag| **tag & END != 0).count();
        if ends.len() + contours > 65535 {
            return Err(BorderError::TooLarge);
        }
        let start = points.len();
        points.extend_from_slice(&self.points);
        tags.extend(self.tags.iter().map(|tag| tag & (ON | CUBIC)));
        for (index, tag) in self.tags.iter().enumerate() {
            if tag & END != 0 {
                ends.push(start + index);
            }
        }
        Ok(())
    }
}

fn add(a: Point, b: Point) -> Result<Point, BorderError> {
    Ok(Point {
        x: a.x.checked_add(b.x).ok_or(BorderError::TooLarge)?,
        y: a.y.checked_add(b.y).ok_or(BorderError::TooLarge)?,
    })
}
