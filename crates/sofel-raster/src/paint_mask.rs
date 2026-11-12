//! HarfBuzz raster clip coverage in 24.8 coordinates (hb-raster-draw.cc,
//! HarfBuzz 15.1.0, Old MIT license in THIRD_PARTY_NOTICES.md).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use alloc::{vec, vec::Vec};
use sofel_geom::{Pen, Point, PointF, Transform};

fn middle(a: PointF, b: PointF) -> PointF {
    PointF::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}
fn fixed(v: f32) -> i64 {
    let scaled = v * 256.;
    i64::from(
        (if scaled < 0. {
            scaled - 0.5
        } else {
            scaled + 0.5
        }) as i32,
    )
}
struct Edge {
    x: i64,
    y: i64,
    end: i64,
    slope: i64,
    wind: i64,
}
/// A glyph clip assembled from transformed, unscaled outline points.
pub struct PaintMask {
    matrix: Transform,
    current: PointF,
    start: PointF,
    edges: Vec<Edge>,
    work: usize,
    initial_work: usize,
    exhausted: bool,
}
impl PaintMask {
    /// Start a glyph with an explicit curve subdivision and edge budget.
    pub fn new(matrix: Transform, work: usize) -> Self {
        Self {
            matrix,
            current: PointF::default(),
            start: PointF::default(),
            edges: Vec::new(),
            work,
            initial_work: work,
            exhausted: false,
        }
    }
    /// Number of emitted outline chords charged to the shared glyph budget.
    pub fn work_used(&self) -> usize {
        self.initial_work.saturating_sub(self.work)
    }
    fn point(&self, p: Point) -> PointF {
        self.matrix.apply(PointF::new(p.x as f32, p.y as f32))
    }
    fn edge(&mut self, a: PointF, b: PointF) {
        if self.work == 0 {
            self.exhausted = true;
            return;
        }
        self.work -= 1;
        let (mut x, mut y, mut x1, mut end) = (fixed(a.x), fixed(a.y), fixed(b.x), fixed(b.y));
        if y == end {
            return;
        }
        let wind = if y > end {
            core::mem::swap(&mut y, &mut end);
            core::mem::swap(&mut x, &mut x1);
            -1
        } else {
            1
        };
        self.edges.push(Edge {
            x,
            y,
            end,
            slope: (x1 - x) * 65536 / (end - y),
            wind,
        });
    }
    fn quad(&mut self, a: PointF, b: PointF, c: PointF, depth: usize) {
        if self.exhausted {
            return;
        }
        if depth == 16
            || ((a.x + c.x - 2. * b.x).abs() <= 0.25 && (a.y + c.y - 2. * b.y).abs() <= 0.25)
        {
            self.edge(a, c);
            return;
        }
        let ab = middle(a, b);
        let bc = middle(b, c);
        let mid = middle(ab, bc);
        self.quad(a, ab, mid, depth + 1);
        self.quad(mid, bc, c, depth + 1);
    }
    fn cubic(&mut self, a: PointF, b: PointF, c: PointF, d: PointF, depth: usize) {
        if self.exhausted {
            return;
        }
        let flat = |a: f32, b: f32, c: f32, d: f32| {
            (2. * a - 3. * b + d).abs() <= 0.5 && (a - 3. * c + 2. * d).abs() <= 0.5
        };
        if depth == 16 || (flat(a.x, b.x, c.x, d.x) && flat(a.y, b.y, c.y, d.y)) {
            self.edge(a, d);
            return;
        }
        let ab = middle(a, b);
        let bc = middle(b, c);
        let cd = middle(c, d);
        let abc = middle(ab, bc);
        let bcd = middle(bc, cd);
        let mid = middle(abc, bcd);
        self.cubic(a, ab, abc, mid, depth + 1);
        self.cubic(mid, bcd, cd, d, depth + 1);
    }
    /// Multiply top-down parent coverage by the nonzero-winding glyph coverage.
    pub fn intersect(
        &self,
        left: i32,
        top: i32,
        width: usize,
        parent: &mut [u8],
        budget: &mut usize,
    ) -> Option<()> {
        if self.exhausted || width == 0 || !parent.len().is_multiple_of(width) {
            return None;
        }
        let rows = parent.len() / width;
        // Walk only edges whose vertical extent intersects the mask.
        let mut cells = vec![(0i64, 0i64); parent.len()];
        for e in &self.edges {
            let bottom = i64::from(top) - i64::try_from(rows).ok()?;
            for row in (e.y >> 8).max(bottom)..((e.end + 255) >> 8).min(i64::from(top)) {
                let surface_y = usize::try_from(i64::from(top) - 1 - row).ok()?;
                let line = cells.get_mut(surface_y * width..(surface_y + 1) * width)?;
                let min_y = e.y.max(row * 256);
                let max_y = e.end.min(row * 256 + 256);
                let xpos = |y| e.x + (((i128::from(y - e.y) * i128::from(e.slope)) >> 16) as i64);
                let (x0, x1) = (xpos(min_y), xpos(max_y));
                let (c0, c1) = (x0 >> 8, x1 >> 8);
                let dy = max_y - min_y;
                let start = i64::from(left);
                let stop = start + i64::try_from(width).ok()?;
                let mut add = |col: i64, x: i64, end: i64, delta: i64| {
                    if col < start {
                        if let Some(first) = line.first_mut() {
                            first.0 += delta * e.wind;
                        }
                    } else if let Some(cell) = usize::try_from(col - start)
                        .ok()
                        .and_then(|i| line.get_mut(i))
                    {
                        cell.0 += delta * e.wind;
                        cell.1 += (x + end) * delta * e.wind;
                    }
                };
                *budget = budget.checked_sub(1)?;
                if c0 == c1 {
                    add(c0, x0 & 255, x1 & 255, dy);
                    continue;
                }
                let forward = c1 > c0;
                let step = if forward { 1 } else { -1 };
                let (begin, end) = if forward { (0, 256) } else { (256, 0) };
                let first_boundary = (c0 + i64::from(forward)) * 256;
                let first = (first_boundary - x0) * dy / (x1 - x0);
                let per = (256 * dy / (x1 - x0)) * step;
                add(c0, x0 & 255, end, first);
                let low = c0.min(c1) + 1;
                let high = c0.max(c1);
                if low < start {
                    add(start - 1, begin, end, (high.min(start) - low) * per);
                }
                for col in low.max(start)..high.min(stop) {
                    *budget = budget.checked_sub(1)?;
                    add(col, begin, end, per);
                }
                add(c1, begin, x1 & 255, dy - first - (high - low) * per);
            }
        }
        for (dst, line) in parent
            .chunks_exact_mut(width)
            .zip(cells.chunks_exact(width))
        {
            let mut cover = 0;
            for (d, &(change, area)) in dst.iter_mut().zip(line) {
                cover += change;
                let cov = (((cover * 512 - area).abs().min(131072) * 255 + 65536) >> 17) as u32;
                *d = ((u32::from(*d) * cov + 255) >> 8) as u8;
            }
        }
        Some(())
    }
}
impl Pen for PaintMask {
    fn move_to(&mut self, p: Point) {
        self.current = self.point(p);
        self.start = self.current;
    }
    fn line_to(&mut self, p: Point) {
        let end = self.point(p);
        self.edge(self.current, end);
        self.current = end;
    }
    fn quad_to(&mut self, b: Point, p: Point) {
        let end = self.point(p);
        self.quad(self.current, self.point(b), end, 0);
        self.current = end;
    }
    fn cubic_to(&mut self, b: Point, c: Point, p: Point) {
        let end = self.point(p);
        self.cubic(self.current, self.point(b), self.point(c), end, 0);
        self.current = end;
    }
    fn close(&mut self) {
        self.edge(self.current, self.start);
        self.current = self.start;
    }
}
