//! FreeType 2.15.0 `FT_GlyphSlot_Embolden` and `FT_Outline_EmboldenXY`.

use sofel_geom::{Point, mul_div, mul_fix};

use crate::{CBox, OutlineError, Scratch, control_box};

/// FreeType slot metrics in 26.6 units, including the vector advance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotMetrics {
    /// Horizontal vector advance.
    pub advance_x: i32,
    /// Vertical vector advance.
    pub advance_y: i32,
    /// Glyph metric width.
    pub width: i32,
    /// Glyph metric height.
    pub height: i32,
    /// Horizontal left bearing.
    pub hori_bearing_x: i32,
    /// Horizontal top bearing.
    pub hori_bearing_y: i32,
    /// Horizontal advance metric.
    pub hori_advance: i32,
    /// Vertical left bearing.
    pub vert_bearing_x: i32,
    /// Vertical top bearing.
    pub vert_bearing_y: i32,
    /// Vertical advance metric.
    pub vert_advance: i32,
}

impl Scratch {
    /// Whether FreeType fills the left side of this outline's directed edges.
    pub fn fill_left(&self) -> bool {
        true_type_orientation(&self.points, &self.ends) == Some(false)
    }

    /// Apply `FT_GlyphSlot_Embolden` to the loaded outline and slot metrics.
    /// `x_ppem` and `y_ppem` are the slot's current pixel sizes.
    pub fn embolden_slot(
        &mut self,
        x_ppem: u16,
        y_ppem: u16,
        metrics: &mut SlotMetrics,
    ) -> Result<CBox, OutlineError> {
        let x_strength = i32::from(x_ppem).saturating_mul(0x0AAA) / 1024;
        let y_strength = i32::from(y_ppem).saturating_mul(0x0AAA) / 1024;
        embolden_outline(&mut self.points, &self.ends, x_strength, y_strength)?;
        metrics.advance_x = nonzero_add(metrics.advance_x, x_strength);
        metrics.advance_y = nonzero_add(metrics.advance_y, y_strength);
        metrics.width = metrics.width.wrapping_add(x_strength);
        metrics.height = metrics.height.wrapping_add(y_strength);
        metrics.hori_advance = metrics.hori_advance.wrapping_add(x_strength);
        metrics.vert_advance = metrics.vert_advance.wrapping_add(y_strength);
        metrics.hori_bearing_y = metrics.hori_bearing_y.wrapping_add(y_strength);
        Ok(control_box(&self.points))
    }
}

fn nonzero_add(value: i32, strength: i32) -> i32 {
    if value == 0 {
        0
    } else {
        value.wrapping_add(strength)
    }
}

/// `FT_Outline_Get_Orientation` on the polygon of control points.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)] // Loader-validated contour bounds; 64-bit area.
pub(super) fn true_type_orientation(points: &[Point], ends: &[usize]) -> Option<bool> {
    let mut area = 0i64;
    let mut first = 0;
    for &last in ends {
        if last >= points.len() || first > last {
            return None;
        }
        let mut previous = points[last];
        for &current in &points[first..=last] {
            area = area.wrapping_add(
                (i64::from(current.y) - i64::from(previous.y))
                    .wrapping_mul(i64::from(current.x) + i64::from(previous.x)),
            );
            previous = current;
        }
        first = last + 1;
    }
    if area == 0 { None } else { Some(area < 0) }
}

/// `FT_Vector_NormLen`: normalize to 16.16 and return the original length.
/// Its integer Newton steps matter at sharp joins; dividing by
/// `FT_Vector_Length` changes individual synthesized outline points.
#[allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)] // The pinned C implementation uses 32-bit wrapping arithmetic.
pub(crate) fn normalize(point: Point) -> (Point, i32) {
    let sign_x = if point.x < 0 { -1i32 } else { 1 };
    let sign_y = if point.y < 0 { -1i32 } else { 1 };
    let mut x = point.x.unsigned_abs();
    let mut y = point.y.unsigned_abs();
    if x == 0 {
        return (
            Point {
                x: 0,
                y: sign_y.wrapping_mul(i32::from(y > 0) * 0x10000),
            },
            y as i32,
        );
    }
    if y == 0 {
        return (
            Point {
                x: sign_x.wrapping_mul(0x10000),
                y: 0,
            },
            x as i32,
        );
    }
    let mut length = x.max(y).wrapping_add(x.min(y) >> 1);
    let mut shift = i32::try_from(length.leading_zeros()).unwrap_or(0);
    shift -= 15 + i32::from(length >= (0xAAAA_AAAAu32 >> shift));
    if shift > 0 {
        x = x.wrapping_shl(shift as u32);
        y = y.wrapping_shl(shift as u32);
        length = x.max(y).wrapping_add(x.min(y) >> 1);
    } else {
        x >>= shift.unsigned_abs();
        y >>= shift.unsigned_abs();
        length >>= shift.unsigned_abs();
    }
    let mut adjustment = 0x10000i32.wrapping_sub(length as i32);
    let mut u;
    let mut v;
    loop {
        u = (x as i32).wrapping_add((x as i32).wrapping_mul(adjustment) >> 16) as u32;
        v = (y as i32).wrapping_add((y as i32).wrapping_mul(adjustment) >> 16) as u32;
        let squared = u.wrapping_mul(u).wrapping_add(v.wrapping_mul(v));
        let mut step = (squared as i32).wrapping_neg() / 0x200;
        step = step.wrapping_mul((0x10000i32.wrapping_add(adjustment)) >> 8) / 0x10000;
        adjustment = adjustment.wrapping_add(step);
        if step <= 0 {
            break;
        }
    }
    let normalized = Point {
        x: sign_x.wrapping_mul(u as i32),
        y: sign_y.wrapping_mul(v as i32),
    };
    length = 0x10000u32
        .wrapping_add((u.wrapping_mul(x).wrapping_add(v.wrapping_mul(y)) as i32 / 0x10000) as u32);
    if shift > 0 {
        length = length.wrapping_add(1u32 << (shift - 1)) >> shift;
    } else {
        length <<= shift.unsigned_abs();
    }
    (normalized, length as i32)
}

/// Port of `FT_Outline_EmboldenXY` on FreeType's loaded point sequence.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)] // Bounds come from the validated outline loader.
pub(crate) fn embolden_outline(
    points: &mut [Point],
    ends: &[usize],
    x_strength: i32,
    y_strength: i32,
) -> Result<(), OutlineError> {
    let x_strength = x_strength / 2;
    let y_strength = y_strength / 2;
    if x_strength == 0 && y_strength == 0 {
        return Ok(());
    }
    let orientation = true_type_orientation(points, ends);
    if orientation.is_none() && !ends.is_empty() {
        return Err(OutlineError::InvalidOutline);
    }
    let true_type = orientation.unwrap_or(true);
    let mut first = 0usize;
    for &last in ends {
        let mut i = last;
        let mut j = first;
        let mut anchor_index = None;
        let mut incoming = Point::default();
        let mut incoming_len = 0;
        let mut anchor = Point::default();
        let mut anchor_len = 0;
        while j != i && Some(i) != anchor_index {
            let (outgoing, outgoing_len) = if Some(j) == anchor_index {
                (anchor, anchor_len)
            } else {
                let delta = Point {
                    x: points[j].x.wrapping_sub(points[i].x),
                    y: points[j].y.wrapping_sub(points[i].y),
                };
                normalize(delta)
            };
            if outgoing_len == 0 {
                j = if j < last { j + 1 } else { first };
                continue;
            }
            if incoming_len != 0 {
                if anchor_index.is_none() {
                    anchor_index = Some(i);
                    anchor = incoming;
                    anchor_len = incoming_len;
                }
                let mut d =
                    mul_fix(incoming.x, outgoing.x).wrapping_add(mul_fix(incoming.y, outgoing.y));
                let shift = if d > -0xF000 {
                    d = d.wrapping_add(0x10000);
                    let mut sx = incoming.y.wrapping_add(outgoing.y);
                    let mut sy = incoming.x.wrapping_add(outgoing.x);
                    if true_type {
                        sx = sx.wrapping_neg();
                    } else {
                        sy = sy.wrapping_neg();
                    }
                    let mut q = mul_fix(outgoing.x, incoming.y)
                        .wrapping_sub(mul_fix(outgoing.y, incoming.x));
                    if true_type {
                        q = q.wrapping_neg();
                    }
                    let l = incoming_len.min(outgoing_len);
                    sx = if mul_fix(x_strength, q) <= mul_fix(l, d) {
                        mul_div(sx, x_strength, d)
                    } else {
                        mul_div(sx, l, q)
                    };
                    sy = if mul_fix(y_strength, q) <= mul_fix(l, d) {
                        mul_div(sy, y_strength, d)
                    } else {
                        mul_div(sy, l, q)
                    };
                    Point { x: sx, y: sy }
                } else {
                    Point::default()
                };
                while i != j {
                    points[i].x = points[i].x.wrapping_add(x_strength).wrapping_add(shift.x);
                    points[i].y = points[i].y.wrapping_add(y_strength).wrapping_add(shift.y);
                    i = if i < last { i + 1 } else { first };
                }
            } else {
                i = j;
            }
            incoming = outgoing;
            incoming_len = outgoing_len;
            j = if j < last { j + 1 } else { first };
        }
        first = last.saturating_add(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Point, embolden_outline};

    #[test]
    fn adr0063_i2_rectangle_points_match_freetype() {
        let mut points = [
            Point { x: 0, y: 0 },
            Point { x: 100, y: 0 },
            Point { x: 100, y: 100 },
            Point { x: 0, y: 100 },
        ];
        assert!(embolden_outline(&mut points, &[3], 42, 42).is_ok());
        assert_eq!(
            points,
            [
                Point { x: 0, y: 0 },
                Point { x: 142, y: 0 },
                Point { x: 142, y: 142 },
                Point { x: 0, y: 142 },
            ]
        );
    }
}
