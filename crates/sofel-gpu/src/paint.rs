//! Clipped layers and groups from HarfBuzz 15.1.0 src/hb-gpu-paint.cc.
//! Copyright © 2026 Behdad Esfahbod (Old MIT). See THIRD_PARTY_NOTICES.md.
use super::gradient::{self, Gradient};
use super::{OutOfRange, Outline, Texel};
use alloc::{vec, vec::Vec};
use sofel_geom::{Brush, CompositeMode, Image, Painter, Rect, Solid, Transform};

#[derive(Clone, Copy)]
enum Clip {
    Glyph(u32, Transform),
    Stored(usize),
    Empty,
}

enum Op {
    Group(Texel),
    Layer([Option<usize>; 3], Ink),
}

enum Ink {
    Solid(Solid),
    Gradient {
        index: usize,
        kind: i16,
        extend: i16,
        count: i16,
    },
}

/// Collects paint commands. The callback returns a glyph's outline under the
/// transform active when its clip was pushed. Each clip is encoded once.
pub struct Paint<F> {
    resolve: F,
    scale: [i32; 2],
    transform: Transform,
    transforms: Vec<Transform>,
    clips: Vec<Clip>,
    rects: [usize; 4],
    groups: usize,
    ops: Vec<Op>,
    blobs: Vec<Vec<Texel>>,
    bounds: Option<Texel>,
    failed: bool,
    remaining: usize,
}

impl<F: FnMut(u32, Transform) -> Result<Outline, OutOfRange>> Paint<F> {
    /// Starts a paint session for a particular scale and outline provider.
    pub fn new(scale: [i32; 2], resolve: F) -> Self {
        Self {
            resolve,
            scale,
            transform: Transform::IDENTITY,
            transforms: Vec::new(),
            clips: Vec::new(),
            rects: [0; 4],
            groups: 0,
            ops: Vec::new(),
            blobs: Vec::new(),
            bounds: None,
            failed: false,
            remaining: 1 << 24,
        }
    }

    fn store(&mut self, outline: &Outline) -> Result<Clip, OutOfRange> {
        let data = outline.encode(self.scale)?;
        if data.is_empty() {
            return Ok(Clip::Empty);
        }
        self.remaining = self.remaining.checked_sub(data.len()).ok_or(OutOfRange)?;
        let index = self.blobs.len();
        self.blobs.push(data);
        Ok(Clip::Stored(index))
    }

    /// Pushes a clip path in output coordinates. An empty path suppresses ink.
    pub fn push_clip_path(&mut self, outline: &Outline) {
        if self.failed || self.clips.len() >= 3 {
            self.failed = true;
            return;
        }
        match self.store(outline) {
            Ok(clip) => self.clips.push(clip),
            Err(_) => self.failed = true,
        }
    }

    fn record(&mut self, op: Op) {
        if self.failed {
            return;
        }
        if self.ops.len() >= 32767 {
            self.failed = true;
        } else {
            self.ops.push(op);
        }
    }

    fn active(&mut self) -> Result<Option<[Option<usize>; 3]>, OutOfRange> {
        if self.clips.is_empty() {
            return Ok(None);
        }
        let mut indices = [None; 3];
        for (depth, slot) in indices.iter_mut().enumerate().take(self.clips.len()) {
            let mut clip = *self.clips.get(depth).ok_or(OutOfRange)?;
            if let Clip::Glyph(gid, t) = clip {
                if self.blobs.len() >= 1024 {
                    return Err(OutOfRange);
                }
                let outline = (self.resolve)(gid, t)?;
                clip = self.store(&outline)?;
                *self.clips.get_mut(depth).ok_or(OutOfRange)? = clip;
            }
            let Clip::Stored(index) = clip else {
                return Ok(None);
            };
            let data = self.blobs.get(index).ok_or(OutOfRange)?;
            self.remaining = self.remaining.checked_sub(data.len()).ok_or(OutOfRange)?;
            let &[x0, y0, x1, y1] = data.first().ok_or(OutOfRange)?;
            let lower = |v: i16| v.div_euclid(4);
            let upper = |v: i16| lower(v).saturating_add(i16::from(v.rem_euclid(4) != 0));
            let bounds = [lower(x0), lower(y0), upper(x1), upper(y1)];
            let [a, b, c, d] = self.bounds.unwrap_or(bounds);
            self.bounds = Some([
                a.min(bounds[0]),
                b.min(bounds[1]),
                c.max(bounds[2]),
                d.max(bounds[3]),
            ]);
            *slot = Some(index);
        }
        Ok(Some(indices))
    }

    /// Finishes the op stream, replacing references with texel offsets.
    pub fn encode(self) -> Result<Vec<Texel>, OutOfRange> {
        if self.failed {
            return Err(OutOfRange);
        }
        let Some(bounds) = self.bounds else {
            return Ok(Vec::new());
        };
        let count = i16::try_from(self.ops.len()).map_err(|_| OutOfRange)?;
        let mut cursor = 3u32;
        for op in &self.ops {
            cursor = cursor
                .checked_add(match op {
                    Op::Group(_) => 1,
                    Op::Layer(..) => 3,
                })
                .ok_or(OutOfRange)?;
        }
        let mut offsets = Vec::new();
        for blob in &self.blobs {
            offsets.push(cursor);
            cursor = cursor
                .checked_add(u32::try_from(blob.len()).map_err(|_| OutOfRange)?)
                .ok_or(OutOfRange)?;
        }
        let mut data = vec![[count, 0, 0, 0], bounds, [3, 0, 0, 0]];
        for op in self.ops {
            match op {
                Op::Group(record) => data.push(record),
                Op::Layer(ids, ink) => {
                    let offset = |id: Option<usize>| -> Result<[i16; 2], OutOfRange> {
                        let v = match id {
                            Some(i) => *offsets.get(i).ok_or(OutOfRange)?,
                            None => 0,
                        };
                        let [a, b, c, d] = v.to_be_bytes();
                        Ok([i16::from_be_bytes([a, b]), i16::from_be_bytes([c, d])])
                    };
                    let [a, b] = offset(ids[0])?;
                    let [c, d] = offset(ids[1])?;
                    let [e, f] = offset(ids[2])?;
                    let (op, flags, payload) = match ink {
                        Ink::Solid(s) => (
                            0,
                            i16::from(s.source.index.is_none()),
                            gradient::rgba(s.color),
                        ),
                        Ink::Gradient {
                            index,
                            kind,
                            extend,
                            count,
                        } => {
                            let [hi, lo] = offset(Some(index))?;
                            (1, kind, [hi, lo, extend, count])
                        }
                    };
                    let flags = flags
                        | (i16::from(ids[1].is_some()) << 8)
                        | (i16::from(ids[2].is_some()) << 9);
                    data.push([op, flags, a, b]);
                    data.push([c, d, e, f]);
                    data.push(payload);
                }
            }
        }
        for blob in self.blobs {
            data.extend(blob);
        }
        Ok(data)
    }
}

impl<F: FnMut(u32, Transform) -> Result<Outline, OutOfRange>> Painter for Paint<F> {
    fn push_transform(&mut self, t: Transform) {
        self.transforms.push(self.transform);
        self.transform = self.transform.multiply(&t);
    }
    fn pop_transform(&mut self) {
        if let Some(t) = self.transforms.pop() {
            self.transform = t;
        }
    }
    fn push_clip_glyph(&mut self, glyph: u32) {
        if self.clips.len() >= 3 {
            self.failed = true;
            return;
        }
        self.clips.push(Clip::Glyph(glyph, self.transform));
    }
    fn push_clip_rectangle(&mut self, _: Rect) {
        if let Some(n) = self.rects.get_mut(self.clips.len()) {
            *n = n.saturating_add(1);
        }
    }
    fn pop_clip(&mut self) {
        if let Some(n) = self.rects.get_mut(self.clips.len())
            && *n > 0
        {
            *n = n.saturating_sub(1);
            return;
        }
        self.clips.pop();
    }
    fn push_group(&mut self, _: CompositeMode) {
        self.groups = self.groups.saturating_add(1);
        if self.groups > 4 {
            self.failed = true;
            return;
        }
        self.record(Op::Group([2, 0, 0, 0]));
    }
    fn pop_group(&mut self, mode: CompositeMode) {
        self.groups = self.groups.saturating_sub(1);
        self.record(Op::Group([3, i16::from(mode as u8), 0, 0]));
    }
    fn fill(&mut self, brush: &Brush<'_>) {
        if self.failed {
            return;
        }
        let line = gradient::line(brush);
        if line.is_some_and(|line| line.stops.is_empty()) {
            return;
        }
        match self.active() {
            Ok(Some(ids)) => {
                let ink = match (brush, line) {
                    (Brush::Solid(s), _) => Ink::Solid(*s),
                    (_, Some(line)) => {
                        let Ok(g) = Gradient::new(brush, line, self.transform) else {
                            self.failed = true;
                            return;
                        };
                        let Some(remaining) = self.remaining.checked_sub(g.data.len()) else {
                            self.failed = true;
                            return;
                        };
                        self.remaining = remaining;
                        let index = self.blobs.len();
                        self.blobs.push(g.data);
                        Ink::Gradient {
                            index,
                            kind: g.kind,
                            extend: g.extend,
                            count: g.count,
                        }
                    }
                    _ => return,
                };
                self.record(Op::Layer(ids, ink));
            }
            Ok(None) => (),
            Err(_) => self.failed = true,
        }
    }
    fn image(&mut self, _: &Image<'_>) -> bool {
        self.failed = true;
        false
    }
}
