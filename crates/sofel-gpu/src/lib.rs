//! Slug outline records in the RGBA16I layout consumed by HarfBuzz GPU.
//! Ported from HarfBuzz 15.1.0 src/hb-gpu-draw.cc (Old MIT license).
//! Copyright © 2026 Behdad Esfahbod. See THIRD_PARTY_NOTICES.md.
#![no_std]

extern crate alloc;
use alloc::{vec, vec::Vec};
use sofel_geom::float::floor;

mod cubic;

type Point = [f64; 2];
type Texel = [i16; 4];

/// The outline does not fit the GPU coordinate or address space.
#[derive(Debug, PartialEq, Eq)]
pub struct OutOfRange;

#[derive(Clone, Copy)]
struct Curve {
    points: [Point; 3],
    first: bool,
}

/// Collects straight lines and quadratic curves in glyph coordinates.
#[derive(Default)]
pub struct Outline {
    curves: Vec<Curve>,
    current: Point,
    start: Option<Point>,
    overflow: bool,
}

impl Outline {
    /// Starts a separate contour even when it touches the previous contour.
    pub fn move_to(&mut self, point: [f32; 2]) {
        self.current = point.map(f64::from);
        self.start = None;
    }

    /// Adds a straight edge.
    pub fn line_to(&mut self, point: [f32; 2]) {
        self.conic(self.current, point.map(f64::from));
    }

    /// Adds a quadratic Bézier segment.
    pub fn quad_to(&mut self, control: [f32; 2], point: [f32; 2]) {
        self.conic(control.map(f64::from), point.map(f64::from));
    }

    /// Converts a cubic Bézier into quadratic segments at the reference tolerance.
    pub fn cubic_to(&mut self, first: [f32; 2], second: [f32; 2], end: [f32; 2]) {
        cubic::append(
            self,
            [
                self.current,
                first.map(f64::from),
                second.map(f64::from),
                end.map(f64::from),
            ],
        );
    }

    fn conic(&mut self, control: Point, point: Point) {
        if point == self.current {
            return;
        }
        if self.curves.len() >= usize::from(u16::MAX) {
            self.overflow = true;
            return;
        }
        let first = self.start.is_none();
        self.start.get_or_insert(self.current);
        self.curves.push(Curve {
            points: [self.current, control, point],
            first,
        });
        self.current = point;
    }

    /// Returns to the first point of the contour.
    pub fn close(&mut self) {
        if let Some(start) = self.start {
            self.conic(self.current, start);
        }
    }

    /// Encodes the bounds, band headers, sorted curve indices and shared endpoints.
    pub fn encode(&self, scale: [i32; 2]) -> Result<Vec<Texel>, OutOfRange> {
        if self.overflow {
            return Err(OutOfRange);
        }
        if self.curves.is_empty() {
            return Ok(Vec::new());
        }
        let mut bounds = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];
        let mut points = Vec::new();
        for curve in &self.curves {
            let mut packed = [[0i16; 2]; 3];
            for (dst, src) in packed.iter_mut().zip(curve.points) {
                for (axis, coordinate) in src.into_iter().enumerate() {
                    let v = coordinate * 4.;
                    if !(f64::from(i16::MIN)..=f64::from(i16::MAX)).contains(&v) {
                        return Err(OutOfRange);
                    }
                    let lo = bounds.get_mut(axis).ok_or(OutOfRange)?;
                    *lo = (*lo).min(signed(floor(v)));
                    let hi = bounds.get_mut(axis.saturating_add(2)).ok_or(OutOfRange)?;
                    *hi = (*hi).max(signed(-floor(-v)));
                    *dst.get_mut(axis).ok_or(OutOfRange)? = nearest(v);
                }
            }
            points.push(packed);
        }
        let [xmin, ymin, xmax, ymax] = bounds;
        let count = self.curves.len().min(16);
        let band_count = |min, max| if min == max { 1 } else { count };
        let h = band_count(ymin, ymax);
        let v = band_count(xmin, xmax);
        let mut output = vec![
            bounds,
            [small(h)?, small(v)?, clamp(scale[0]), clamp(scale[1])],
        ];
        let mut bands = Vec::new();
        for (axis, n, lo, hi, midpoint) in [
            (1, h, ymin, ymax, (f64::from(xmin) + f64::from(xmax)) / 2.),
            (0, v, xmin, xmax, (f64::from(ymin) + f64::from(ymax)) / 2.),
        ] {
            for b in 0..n {
                bands.push(Band::new(&points, axis, b, n, lo, hi, midpoint)?);
            }
        }
        let mut indices = 2usize.checked_add(bands.len()).ok_or(OutOfRange)?;
        let mut endpoint = indices;
        for band in &bands {
            endpoint = endpoint
                .checked_add(band.desc.len().checked_mul(2).ok_or(OutOfRange)?)
                .ok_or(OutOfRange)?;
        }
        let mut offsets = Vec::new();
        let mut endpoints = Vec::new();
        for (curve, [start, control, end]) in self.curves.iter().zip(points) {
            if curve.first {
                endpoints.push([start[0], start[1], control[0], control[1]]);
            } else if let Some(last) = endpoints.last_mut() {
                last[2] = control[0];
                last[3] = control[1];
            }
            offsets.push(address(
                endpoint
                    .checked_add(endpoints.len())
                    .and_then(|i| i.checked_sub(1))
                    .ok_or(OutOfRange)?,
            )?);
            endpoints.push([end[0], end[1], 0, 0]);
        }
        if endpoint.checked_add(endpoints.len()).ok_or(OutOfRange)? > 65536 {
            return Err(OutOfRange);
        }
        for band in &bands {
            let n = band.desc.len();
            let asc = indices.checked_add(n).ok_or(OutOfRange)?;
            output.push([small(n)?, address(indices)?, address(asc)?, band.split]);
            indices = asc.checked_add(n).ok_or(OutOfRange)?;
        }
        for band in bands {
            for entry in band.desc.iter().chain(&band.asc) {
                output.push([*offsets.get(entry.index).ok_or(OutOfRange)?, 0, 0, 0]);
            }
        }
        output.extend(endpoints);
        Ok(output)
    }
}

#[allow(clippy::cast_possible_truncation)]
fn signed(v: f64) -> i16 {
    v as i16
}
fn nearest(v: f64) -> i16 {
    signed(if v < 0. {
        -floor(-v + 0.5)
    } else {
        floor(v + 0.5)
    })
}
fn small(v: usize) -> Result<i16, OutOfRange> {
    i16::try_from(v).map_err(|_| OutOfRange)
}
fn clamp(v: i32) -> i16 {
    i16::try_from(v).unwrap_or(if v < 0 { i16::MIN } else { i16::MAX })
}
fn address(v: usize) -> Result<i16, OutOfRange> {
    let v = u16::try_from(v).map_err(|_| OutOfRange)?;
    Ok((v ^ 0x8000).cast_signed())
}

#[derive(Clone, Copy, Default)]
struct Entry {
    index: usize,
    min: i16,
    max: i16,
}
struct Band {
    desc: Vec<Entry>,
    asc: Vec<Entry>,
    split: i16,
}

impl Band {
    fn new(
        points: &[[[i16; 2]; 3]],
        axis: usize,
        b: usize,
        n: usize,
        lo: i16,
        hi: i16,
        midpoint: f64,
    ) -> Result<Self, OutOfRange> {
        let other = 1usize.checked_sub(axis).ok_or(OutOfRange)?;
        let mut desc = Vec::new();
        let n = f64::from(small(n)?);
        let b = f64::from(small(b)?);
        let width = f64::from(hi) - f64::from(lo);
        for (index, curve) in points.iter().enumerate() {
            let range = |axis| -> Result<(i16, i16), OutOfRange> {
                let mut lo = i16::MAX;
                let mut hi = i16::MIN;
                for p in curve {
                    let v = *p.get(axis).ok_or(OutOfRange)?;
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                Ok((lo, hi))
            };
            let (a, z) = range(axis)?;
            let (left, right) = range(other)?;
            if a == z {
                continue;
            }
            let band = |q| (f64::from(q) - f64::from(lo)) * n / width;
            if floor(band(a) - 1. / 1024.) <= b && b <= floor(band(z) + 1. / 1024.) {
                desc.push(Entry {
                    index,
                    min: left,
                    max: right,
                });
            }
        }
        let mut asc = desc.clone();
        sofel_geom::sort::harfbuzz_sort(&mut desc, &|a, b| b.max.cmp(&a.max));
        sofel_geom::sort::harfbuzz_sort(&mut asc, &|a, b| a.min.cmp(&b.min));
        let mut split = nearest(midpoint);
        let mut best = desc.len();
        for (i, e) in desc.iter().enumerate() {
            let worst = i
                .saturating_add(1)
                .max(asc.partition_point(|a| a.min <= e.max));
            if worst < best {
                best = worst;
                split = e.max;
            }
        }
        Ok(Self { desc, asc, split })
    }
}

fn font_point(p: sofel_geom::Point) -> [f32; 2] {
    [p.x as f32, p.y as f32]
}

impl sofel_geom::Pen for Outline {
    fn move_to(&mut self, p: sofel_geom::Point) {
        self.move_to(font_point(p));
    }
    fn line_to(&mut self, p: sofel_geom::Point) {
        self.line_to(font_point(p));
    }
    fn quad_to(&mut self, c: sofel_geom::Point, p: sofel_geom::Point) {
        self.quad_to(font_point(c), font_point(p));
    }
    fn cubic_to(&mut self, a: sofel_geom::Point, b: sofel_geom::Point, p: sofel_geom::Point) {
        self.cubic_to(font_point(a), font_point(b), font_point(p));
    }
    fn close(&mut self) {
        Outline::close(self);
    }
}

mod shaders;
pub use shaders::{ShaderLanguage, ShaderStage, shader_source};

mod gradient;
mod paint;
pub use paint::Paint;
