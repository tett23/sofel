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
fn true_type_orientation(points: &[Point], ends: &[usize]) -> Option<bool> {
    let mut area = 0i64;
    let mut first = 0;
    for &last in ends {
        if last >= points.len() || first > last {
            return None;
        }
        let mut previous = points[last];
        for &current in &points[first..=last] {
            area = area.wrapping_add(
                i64::from(current.y - previous.y) * i64::from(current.x + previous.x),
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
#[allow(clippy::arithmetic_side_effects)] // The pinned C implementation uses 32-bit wrapping arithmetic.
pub(crate) fn normalize(point: Point) -> (Point, i32) {
    let sx = if point.x < 0 { -1i32 } else { 1 };
    let sy = if point.y < 0 { -1i32 } else { 1 };
    let mut x = point.x.unsigned_abs();
    let mut y = point.y.unsigned_abs();
    if x == 0 {
        return (
            Point {
                x: 0,
                y: if y > 0 { sy * 0x10000 } else { 0 },
            },
            y.cast_signed(),
        );
    }
    if y == 0 {
        return (
            Point {
                x: sx * 0x10000,
                y: 0,
            },
            x.cast_signed(),
        );
    }
    let mut l = if x > y {
        x.wrapping_add(y >> 1)
    } else {
        y.wrapping_add(x >> 1)
    };
    let mut shift = i32::try_from(l.leading_zeros()).unwrap_or(0);
    let estimate = 0xAAAA_AAAAu32 >> u32::try_from(shift).unwrap_or(0);
    shift -= 15 + i32::from(l >= estimate);
    if shift > 0 {
        x = x.wrapping_shl(shift.cast_unsigned());
        y = y.wrapping_shl(shift.cast_unsigned());
        l = if x > y {
            x.wrapping_add(y >> 1)
        } else {
            y.wrapping_add(x >> 1)
        };
    } else {
        x >>= shift.unsigned_abs();
        y >>= shift.unsigned_abs();
        l >>= shift.unsigned_abs();
    }
    let mut z = 0x10000i32.wrapping_sub(l.cast_signed());
    let mut u = x.wrapping_add(
        ((x.cast_signed().wrapping_mul(z).wrapping_add(0x8000)) >> 16).cast_unsigned(),
    );
    let mut v = y.wrapping_add(
        ((y.cast_signed().wrapping_mul(z).wrapping_add(0x8000)) >> 16).cast_unsigned(),
    );
    loop {
        z = (u
            .wrapping_mul(u)
            .wrapping_add(v.wrapping_mul(v))
            .cast_signed()
            .wrapping_neg()
            .wrapping_add(0x10000))
            >> 17;
        u = u.wrapping_add(
            ((u.cast_signed().wrapping_mul(z).wrapping_add(0x8000)) >> 16).cast_unsigned(),
        );
        v = v.wrapping_add(
            ((v.cast_signed().wrapping_mul(z).wrapping_add(0x8000)) >> 16).cast_unsigned(),
        );
        if z <= 1 {
            break;
        }
    }
    let normalized = Point {
        x: sx.wrapping_mul(u.cast_signed()),
        y: sy.wrapping_mul(v.cast_signed()),
    };
    l = 0x10000u32.wrapping_add(
        u.wrapping_mul(x)
            .wrapping_add(v.wrapping_mul(y))
            .cast_signed()
            .wrapping_div(0x10000)
            .cast_unsigned(),
    );
    if shift > 0 {
        l = l.wrapping_add(1u32.wrapping_shl(shift.saturating_sub(1).cast_unsigned()))
            >> shift.cast_unsigned();
    } else {
        l = l.wrapping_shl(shift.unsigned_abs());
    }
    (normalized, l.cast_signed())
}

/// Port of `FT_Outline_EmboldenXY` on FreeType's loaded point sequence.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)] // Bounds come from the validated outline loader.
fn embolden_outline(
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
