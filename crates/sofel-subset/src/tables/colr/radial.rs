//! HarfBuzz's PaintRadialGradient::subset_nonnegative. Move the two
//! circles along their common parameter and apply the inverse to every stop.

use super::{Errors, NO_VARIATION, Sub, Vec, u24};
use crate::instance::{f64_to_f32, f64_to_i32};
use sofel_geom::float::{floor, hb_roundf32};

#[derive(Clone, Copy)]
struct Stop {
    offset: f64,
    alpha: u16,
    palette: u16,
}

impl Sub<'_, '_> {
    fn transparent(&mut self) -> bool {
        self.s.extend(&[2, 255, 255, 0, 0]);
        true
    }

    pub(super) fn negative_radial(&mut self, at: usize) -> Option<bool> {
        let delta = self.delta?;
        let base = self.t.u32(at.saturating_add(16));
        if base == NO_VARIATION {
            return None;
        }
        let mut circles = [0.0; 6];
        for (i, v) in circles.iter_mut().enumerate() {
            let raw = self
                .t
                .u16(at.saturating_add(4).saturating_add(i.saturating_mul(2)));
            let old = if matches!(i, 2 | 5) {
                f64::from(raw)
            } else {
                f64::from(i16::from_be_bytes(raw.to_be_bytes()))
            };
            *v = old
                + hb_roundf32(delta(
                    base.saturating_add(u32::try_from(i).unwrap_or(u32::MAX)),
                ));
        }
        let [x0, y0, r0, x1, y1, r1] = circles;
        if r0 >= 0.0 && r1 >= 0.0 {
            return None;
        }
        let line_at = at.saturating_add(u24(self.t, at.saturating_add(1)) as usize);
        let line = self.t.at(line_at);
        let extend = line.u8(0);
        let count = line.u16(1);
        if r0 == r1 || count == 0 {
            return Some(self.transparent());
        }
        let mut stops = Vec::with_capacity(usize::from(count));
        for i in 0..usize::from(count) {
            let stop = line.at(3usize.saturating_add(i.saturating_mul(10)));
            let index = stop.u32(6);
            let varied = |field, shift| {
                let value = f32::from(i16::from_be_bytes(stop.u16(field).to_be_bytes()));
                if index == NO_VARIATION {
                    value
                } else {
                    value + delta(index.saturating_add(shift))
                }
            };
            stops.push(Stop {
                offset: hb_roundf32(varied(0, 0)) / 16384.0,
                palette: stop.u16(2),
                alpha: u16::try_from(f64_to_i32(hb_roundf32(varied(4, 1).clamp(0.0, 16384.0))))
                    .unwrap_or_default(),
            });
        }
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        let dr = r1 - r0;
        let mut lattice = 16384u32;
        for difference in [x1 - x0, y1 - y0, dr] {
            let mut right =
                u32::try_from(f64_to_i32(difference.abs() % 16384.0)).unwrap_or_default();
            while right != 0 {
                let r = lattice.checked_rem(right).unwrap_or_default();
                lattice = right;
                right = r;
            }
        }
        let ceil = |v: f64| -floor(-v);
        for shift in 0..16 {
            let span = f64::from(1u32 << shift);
            for grid in [1.0, f64::from(lattice)] {
                let start = if dr > 0.0 {
                    ceil(-r0 / dr * grid) / grid
                } else {
                    floor(r0 / -dr * grid) / grid - span
                };
                let mut geometry = [0u16; 6];
                let mut fits = true;
                for (j, (old, diff)) in [(x0, x1 - x0), (y0, y1 - y0), (r0, dr)]
                    .into_iter()
                    .enumerate()
                {
                    for (field, t) in [(j, start), (j.saturating_add(3), start + span)] {
                        let value = old + t * diff;
                        let range = if j == 2 {
                            0.0..=65535.0
                        } else {
                            -32768.0..=32767.0
                        };
                        fits &= range.contains(&value);
                        if let Some(slot) = geometry.get_mut(field) {
                            let raw = f64_to_i32(value).to_be_bytes();
                            *slot = u16::from_be_bytes([raw[2], raw[3]]);
                        }
                    }
                }
                if !fits {
                    continue;
                }
                let first = stops.first()?;
                let last = stops.last()?;
                let min = (first.offset - start) / span;
                let max = (last.offset - start) / span;
                let tip = -f64::from(geometry[2]) / (span * dr);
                let mut selected = stops.as_slice();
                let mut bias = 0.0;
                let constant = extend == 0 && ((dr > 0.0 && max <= tip) || (dr < 0.0 && min > tip));
                if constant {
                    selected = core::slice::from_ref(if dr > 0.0 { last } else { first });
                } else if extend != 0 {
                    let period = (max - min) * if extend == 2 { 2.0 } else { 1.0 };
                    if period <= 0.0 {
                        return Some(self.transparent());
                    }
                    let n = ceil((-2.0 - min) / period);
                    if n > floor((32767.0 / 16384.0 - max) / period) {
                        continue;
                    }
                    bias = n * period;
                }
                let bits = |stop: &Stop| {
                    if constant {
                        0.0
                    } else {
                        ((stop.offset - start) / span + bias) * 16384.0
                    }
                };
                if !selected.iter().all(|s| {
                    let v = bits(s);
                    (-32768.0..=32767.0).contains(&v) && floor(v) == v
                }) {
                    continue;
                }
                self.s.u8(6);
                let link = self.s.u24(0);
                for field in geometry {
                    self.s.u16(field);
                }
                self.s.push();
                self.s.u8(extend);
                self.s
                    .u16(u16::try_from(selected.len()).unwrap_or(u16::MAX));
                for stop in selected {
                    let value = i16::try_from(f64_to_i32(hb_roundf32(f64_to_f32(bits(stop)))))
                        .unwrap_or_default();
                    self.s.extend(&value.to_be_bytes());
                    let field = self.s.u16(0);
                    if !self.palette(field, stop.palette) {
                        self.s.pop_discard();
                        return Some(false);
                    }
                    self.s.u16(stop.alpha);
                }
                let obj = self.s.pop_pack(true);
                self.s.link24(link, obj);
                return Some(true);
            }
        }
        Some(self.s.err(Errors::INT_OVERFLOW))
    }
}
