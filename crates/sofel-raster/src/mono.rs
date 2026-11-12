//! Packed monochrome rasterization (sofel ADR 0064).

use super::PixelBox;

// ft_glyphslot_preset_bitmap rounds ties toward the outside. If an
// interval collapses, keep the pixel containing the larger part of it.
#[allow(
    clippy::arithmetic_side_effects,
    reason = "shifting by six leaves room for rounding and one extra pixel"
)]
fn mono_axis(start: i32, end: i32) -> (i32, i32) {
    let mut lower = (start >> 6) + (((start & 63) + 31) >> 6);
    let mut upper = (end >> 6) + (((end & 63) + 32) >> 6);
    if lower == upper {
        let remainder = (((start & 63) + 31) & 63) - 31 + (((end & 63) + 32) & 63) - 32;
        if remainder < 0 {
            lower -= 1;
        } else {
            upper += 1;
        }
    }
    (lower, upper)
}

/// Grid-fit the 26.6 control box for FreeType's monochrome rasterizer.
pub fn mono_pixel_box(x_min: i32, y_min: i32, x_max: i32, y_max: i32) -> PixelBox {
    let (left, right) = mono_axis(x_min, x_max);
    let (bottom, top) = mono_axis(y_min, y_max);
    PixelBox {
        left,
        top,
        width: u32::try_from(right.saturating_sub(left)).unwrap_or(0),
        rows: u32::try_from(top.saturating_sub(bottom)).unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::mono_axis;

    #[test]
    fn adr0064_i2_mono_bounds_include_centres_and_preserve_collapsed_axes() {
        for (start, end, expected) in [
            (0, 0, (0, 1)),
            (-1, -1, (-1, 0)),
            (1, 1, (0, 1)),
            (31, 31, (0, 1)),
            (32, 32, (0, 1)),
            (33, 33, (0, 1)),
            (-32, 32, (-1, 1)),
            (32, 96, (0, 2)),
            (33, 95, (1, 2)),
            (0, 448, (0, 7)),
            (0, 512, (0, 8)),
            (0, 576, (0, 9)),
        ] {
            assert_eq!(mono_axis(start, end), expected, "{start}..{end}");
        }
    }
}

// Line_Up and Line_Down sample in traversal order. Inputs have already
// been translated by half a pixel, as in FreeType's SCALED macro.
#[allow(
    clippy::arithmetic_side_effects,
    reason = "coordinates come from scaled i32 outline points and precision is 64 or 4096"
)]
fn line_profile(
    (x1, y1): (i64, i64),
    (x2, y2): (i64, i64),
    (bottom, top): (i64, i64),
    precision: i64,
    mut visit: impl FnMut(i64, i64),
) {
    if y1 == y2 {
        return;
    }
    let ascending = y1 < y2;
    let (start, end, low, high) = if ascending {
        (y1, y2, bottom, top)
    } else {
        (-y1, -y2, -top, -bottom)
    };
    let first = (start.div_euclid(precision) + 1).max(low);
    let last = end.div_euclid(precision).min(high);
    for row in first..=last {
        let offset =
            i128::from(row * precision - start) * i128::from(x2 - x1) / i128::from(end - start);
        // The intersection is between the two original x coordinates.
        let x = (i128::from(x1) + offset) as i64;
        visit(if ascending { row } else { -row }, x);
    }
}

#[cfg(test)]
mod profile_tests {
    use super::line_profile;

    #[test]
    fn adr0064_i2_lines_exclude_the_start_include_the_end_and_clip_rows() {
        let samples = |from, to, clip| {
            let mut values = alloc::vec::Vec::new();
            line_profile(from, to, clip, 64, |row, x| values.push((row, x)));
            values
        };
        assert_eq!(
            samples((0, 0), (100, 192), (0, 3)),
            [(1, 33), (2, 66), (3, 100)]
        );
        assert_eq!(
            samples((100, 192), (0, 0), (0, 3)),
            [(2, 67), (1, 34), (0, 0)]
        );
        assert_eq!(samples((10, -32), (-90, 160), (0, 1)), [(0, -6), (1, -40)]);
        assert!(samples((0, 64), (128, 64), (0, 3)).is_empty());
    }
}

#[cfg(test)]
mod curve_tests {
    use super::{Arc, curve_profile};

    #[test]
    fn adr0064_i2_curves_match_reference_intersections_in_both_directions() {
        let mut samples = alloc::vec::Vec::new();
        curve_profile(
            Arc::Quad([(-23, -65), (115, 13), (19, 130)]),
            (-3, 3),
            64,
            |row, x| samples.push((row, x)),
        );
        assert_eq!(samples, [(-1, -21), (0, 48), (1, 53), (2, 20)]);
        samples.clear();
        curve_profile(
            Arc::Cubic([(-35, 155), (105, 123), (-73, 33), (11, -100)]),
            (-3, 3),
            64,
            |row, x| samples.push((row, x)),
        );
        assert_eq!(samples, [(2, 14), (1, 8), (0, -13), (-1, -6)]);
    }
}

type MonoPoint = (i64, i64);

#[derive(Clone, Copy)]
enum Arc {
    Quad([MonoPoint; 3]),
    Cubic([MonoPoint; 4]),
}

impl Arc {
    fn ends(self) -> (MonoPoint, MonoPoint) {
        match self {
            Self::Quad([a, _, b]) | Self::Cubic([a, _, _, b]) => (a, b),
        }
    }

    // Conic_To and Cubic_To allow the control points to extend to the
    // neighboring grid line before subdividing the arc at its midpoint.
    fn directed(self, precision: i64, mut visit: impl FnMut(Self)) {
        let mut pending = alloc::vec![self];
        while let Some(arc) = pending.pop() {
            let (start, end) = arc.ends();
            let low = start.1.min(end.1) & -precision;
            let high = (start.1.max(end.1) + precision - 1) & -precision;
            let outside = |point: MonoPoint| !(low..=high).contains(&point.1);
            let turns = match arc {
                Self::Quad([_, control, _]) => outside(control),
                Self::Cubic([_, first, second, _]) => outside(first) || outside(second),
            };
            if turns {
                let (first, second) = arc.split();
                pending.push(second);
                pending.push(first);
            } else if start.1 != end.1 {
                visit(arc);
            }
        }
    }

    fn flip_y(self) -> Self {
        let flip = |(x, y): MonoPoint| (x, -y);
        match self {
            Self::Quad(p) => Self::Quad(p.map(flip)),
            Self::Cubic(p) => Self::Cubic(p.map(flip)),
        }
    }

    // Accumulate the Bernstein coefficients before shifting. Rounding
    // intermediate midpoints separately changes the intersection by one.
    fn split(self) -> (Self, Self) {
        let add = |a: MonoPoint, b: MonoPoint| (a.0 + b.0, a.1 + b.1);
        let shift = |p: MonoPoint, n: u32| (p.0 >> n, p.1 >> n);
        match self {
            Self::Quad([a, b, c]) => {
                let (ab, bc) = (add(a, b), add(b, c));
                let mid = shift(add(ab, bc), 2);
                (
                    Self::Quad([a, shift(ab, 1), mid]),
                    Self::Quad([mid, shift(bc, 1), c]),
                )
            }
            Self::Cubic([a, b, c, d]) => {
                let (ab, bc, cd) = (add(a, b), add(b, c), add(c, d));
                let (abc, bcd) = (add(ab, bc), add(bc, cd));
                let mid = shift(add(abc, bcd), 3);
                (
                    Self::Cubic([a, shift(ab, 1), shift(abc, 2), mid]),
                    Self::Cubic([mid, shift(bcd, 2), shift(cd, 1), d]),
                )
            }
        }
    }
}

// FreeType's Bezier_Up/Down, for one directed arc after extrema splitting.
fn curve_profile(
    arc: Arc,
    (bottom, top): (i64, i64),
    precision: i64,
    mut visit: impl FnMut(i64, i64),
) {
    let (start, end) = arc.ends();
    let down = start.1 > end.1;
    let (arc, low, high) = if down {
        (arc.flip_y(), -top, -bottom)
    } else {
        (arc, bottom, top)
    };
    let (start, end) = arc.ends();
    let mut row = (start.1.div_euclid(precision) + 1).max(low);
    let last = end.1.div_euclid(precision).min(high);
    let step = if precision == 64 { 32 } else { 256 };
    let mut pending = alloc::vec![arc];
    while row <= last {
        let Some(segment) = pending.pop() else { break };
        let ((x1, y1), (x2, y2)) = segment.ends();
        let y = row * precision;
        if y2 < y {
            continue;
        }
        let x = if y2 == y {
            x2
        } else {
            let (dx, dy) = (x2 - x1, y2 - y1);
            if dy > step || dx.abs() > step {
                let (first, second) = segment.split();
                pending.push(second);
                pending.push(first);
                continue;
            }
            x2 - (y2 - y) * dx / dy
        };
        visit(if down { -row } else { row }, x);
        row += 1;
    }
}

#[cfg(test)]
mod turning_tests {
    use super::{Arc, curve_profile};

    #[test]
    fn adr0064_i2_curve_returning_to_its_start_height_visits_both_slopes() {
        let arc = Arc::Quad([(0, 0), (128, 256), (256, 0)]);
        let mut crossings = alloc::vec::Vec::new();
        arc.directed(64, |part| {
            curve_profile(part, (0, 3), 64, |row, x| crossings.push((row, x)));
        });
        assert_eq!(crossings, [(1, 38), (2, 128), (1, 219), (0, 256)]);
    }
}

#[cfg(test)]
mod span_tests {
    use super::paint_span;

    #[test]
    fn adr0064_i2_mono_spans_clip_pack_and_preserve_existing_ink() {
        for (width, expected) in [(7usize, &[0xfe][..]), (8, &[0xff]), (9, &[0xff, 0x80])] {
            let mut row = alloc::vec![0; width.div_ceil(8)];
            paint_span(&mut row, width, -64, 640, 64);
            assert_eq!(row, expected);
        }
        let mut row = [0x80, 1, 0];
        paint_span(&mut row, 24, 3 * 4096 + 1, 18 * 4096 - 1, 4096);
        assert_eq!(row, [0x8f, 0xff, 0xc0]);
        paint_span(&mut row, 24, 1, 63, 64);
        assert_eq!(row, [0x8f, 0xff, 0xc0]);
    }
}

// Vertical_Sweep_Span. The interval includes both pixel centres and the
// last partial byte is clipped to the logical width.
fn paint_span(row: &mut [u8], width: usize, left: i64, right: i64, precision: i64) {
    let first = ((left + precision - 1).div_euclid(precision)).max(0);
    let last = right.div_euclid(precision).min(width as i64 - 1);
    if first > last {
        return;
    }
    let first = first as usize;
    let last = last as usize;
    let bytes = &mut row[first / 8..=last / 8];
    let start_mask = u8::MAX >> (first % 8);
    let end_mask = u8::MAX << (7 - last % 8);
    if let [byte] = bytes {
        *byte |= start_mask & end_mask;
    } else if let [start, middle @ .., end] = bytes {
        *start |= start_mask;
        middle.fill(u8::MAX);
        *end |= end_mask;
    }
}

#[cfg(test)]
mod dropout_tests {
    use super::dropout_pixel;

    #[test]
    fn adr0064_i2_smart_dropouts_choose_the_nearest_pixel_and_respect_existing_ink() {
        let pick =
            |left, right, smart, ink| dropout_pixel(left, right, 64, smart, 2, |p| Some(p) == ink);
        assert_eq!(pick(39, 60, false, None), Some(0));
        assert_eq!(pick(39, 60, true, None), Some(1));
        assert_eq!(pick(39, 60, true, Some(0)), None);
        assert_eq!(pick(20, 44, true, None), Some(0));
        assert_eq!(pick(-55, -45, true, None), Some(0));
        assert_eq!(pick(104, 120, true, None), Some(1));
        assert_eq!(pick(-120, -110, true, None), None);
    }
}

// A span without a pixel centre lies strictly between two neighbours.
// FreeType handles these after solid spans so existing ink takes precedence.
fn dropout_pixel(
    left: i64,
    right: i64,
    precision: i64,
    smart: bool,
    extent: usize,
    inked: impl FnOnce(usize) -> bool,
) -> Option<usize> {
    let low = right.div_euclid(precision);
    let high = low + 1;
    let prefer_high =
        smart && ((left + right + precision * 63 / 64) >> 1).div_euclid(precision) == high;
    let (preferred, other) = if prefer_high {
        (high, low)
    } else {
        (low, high)
    };
    let inside = |p| usize::try_from(p).ok().filter(|&p| p < extent);
    let other = inside(other);
    match inside(preferred) {
        None => other,
        Some(_) if other.is_some_and(inked) => None,
        pixel => pixel,
    }
}

#[cfg(test)]
mod contour_tests {
    use super::{ContourProfiles, Edge};

    #[test]
    fn adr0064_i2_profiles_join_at_direction_changes_and_close_once() {
        let mut contour = ContourProfiles::new((0, 3), 64);
        for (a, b) in [
            ((0, 64), (0, 128)),
            ((0, 128), (128, 128)),
            ((128, 128), (128, 0)),
            ((128, 0), (0, 0)),
            ((0, 0), (0, 64)),
        ] {
            contour.edge(Edge::Line(a, b));
        }
        let profiles = contour.finish(64);
        assert_eq!(profiles.len(), 3);
        assert_eq!(profiles[0].samples, [(1, 0), (2, 0)]);
        assert_eq!(profiles[1].samples, [(2, 128), (1, 128), (0, 128)]);
        assert_eq!(profiles[2].samples, [(0, 0)]);
    }
}

enum Edge {
    Line(MonoPoint, MonoPoint),
    Curve(Arc),
}

struct Profile {
    up: bool,
    start_y: i64,
    end_y: i64,
    samples: alloc::vec::Vec<(i64, i64)>,
}

struct ContourProfiles {
    profiles: alloc::vec::Vec<Profile>,
    clip: (i64, i64),
    precision: i64,
}

impl ContourProfiles {
    fn new(clip: (i64, i64), precision: i64) -> Self {
        Self {
            profiles: alloc::vec::Vec::new(),
            clip,
            precision,
        }
    }

    fn edge(&mut self, edge: Edge) {
        match edge {
            Edge::Line(a, b) => self.directed(Edge::Line(a, b)),
            Edge::Curve(arc) => {
                arc.directed(self.precision, |part| self.directed(Edge::Curve(part)))
            }
        }
    }

    fn directed(&mut self, edge: Edge) {
        let (start, end) = match edge {
            Edge::Line(a, b) => (a, b),
            Edge::Curve(arc) => arc.ends(),
        };
        if start.1 == end.1 {
            return;
        }
        let up = start.1 < end.1;
        if self
            .profiles
            .last()
            .is_none_or(|previous| previous.up != up)
        {
            let mut samples = alloc::vec::Vec::new();
            let row = start.1.div_euclid(self.precision);
            if start.1.rem_euclid(self.precision) == 0 && (self.clip.0..=self.clip.1).contains(&row)
            {
                samples.push((row, start.0));
            }
            self.profiles.push(Profile {
                up,
                start_y: start.1,
                end_y: end.1,
                samples,
            });
        }
        let Some(profile) = self.profiles.last_mut() else {
            return;
        };
        profile.end_y = end.1;
        let record = |row, x| profile.samples.push((row, x));
        match edge {
            Edge::Line(a, b) => line_profile(a, b, self.clip, self.precision, record),
            Edge::Curve(arc) => curve_profile(arc, self.clip, self.precision, record),
        }
    }

    fn finish(mut self, closing_y: i64) -> alloc::vec::Vec<Profile> {
        let first_up = self
            .profiles
            .iter()
            .find(|p| !p.samples.is_empty())
            .map(|p| p.up);
        if let Some(last) = self.profiles.last_mut()
            && first_up == Some(last.up)
            && closing_y.rem_euclid(self.precision) == 0
            && (self.clip.0..=self.clip.1).contains(&closing_y.div_euclid(self.precision))
        {
            last.samples.pop();
        }
        self.profiles.retain(|p| !p.samples.is_empty());
        self.profiles
    }
}

#[cfg(test)]
mod command_tests {
    use super::glyph_profiles;
    use sofel_geom::{Command, Point};

    #[test]
    fn adr0064_i2_separate_contours_close_in_both_sweep_directions() {
        let p = |x, y| Point { x, y };
        let commands = [
            Command::MoveTo(p(0, 0)),
            Command::LineTo(p(0, 128)),
            Command::LineTo(p(128, 128)),
            Command::MoveTo(p(192, 192)),
            Command::QuadTo(p(192, 320), p(320, 320)),
            Command::CubicTo(p(320, 256), p(256, 192), p(192, 192)),
        ];
        for flipped in [false, true] {
            let profiles = glyph_profiles(&commands, 64, (0, 0), 6, flipped);
            assert_eq!(profiles.len(), 2);
            for contour in profiles {
                assert!(contour.iter().any(|p| p.up));
                assert!(contour.iter().any(|p| !p.up));
            }
        }
    }
}

fn glyph_profiles(
    commands: &[sofel_geom::Command],
    precision: i64,
    origin: MonoPoint,
    scanlines: usize,
    flipped: bool,
) -> alloc::vec::Vec<alloc::vec::Vec<Profile>> {
    use sofel_geom::{Command, Point};
    let transform = |p: Point| {
        let x = (i64::from(p.x) - origin.0 * 64) * (precision / 64) - precision / 2;
        let y = (i64::from(p.y) - origin.1 * 64) * (precision / 64) - precision / 2;
        if flipped { (y, x) } else { (x, y) }
    };
    let mut contours = alloc::vec::Vec::new();
    for commands in commands.chunk_by(|_, next| !matches!(next, Command::MoveTo(_))) {
        let Some((Command::MoveTo(first), edges)) = commands.split_first() else {
            continue;
        };
        let start = transform(*first);
        let mut last = start;
        let mut contour = ContourProfiles::new((0, scanlines as i64 - 1), precision);
        for &command in edges {
            let end = match command {
                Command::LineTo(p) => {
                    let end = transform(p);
                    contour.edge(Edge::Line(last, end));
                    end
                }
                Command::QuadTo(c, p) => {
                    let end = transform(p);
                    contour.edge(Edge::Curve(Arc::Quad([last, transform(c), end])));
                    end
                }
                Command::CubicTo(c1, c2, p) => {
                    let end = transform(p);
                    contour.edge(Edge::Curve(Arc::Cubic([
                        last,
                        transform(c1),
                        transform(c2),
                        end,
                    ])));
                    end
                }
                Command::Close => break,
                Command::MoveTo(_) => unreachable!(),
            };
            last = end;
        }
        contour.edge(Edge::Line(last, start));
        let profiles = contour.finish(start.1);
        if !profiles.is_empty() {
            contours.push(profiles);
        }
    }
    contours
}

#[cfg(test)]
mod bitmap_tests {
    use super::*;
    use sofel_geom::{Pen, Point};

    #[test]
    fn adr0064_i2_render_square_to_packed_bitmap() {
        for size in [7, 8, 9] {
            let mut raster = super::super::Rasterizer::new();
            raster.move_to(Point { x: 0, y: 0 });
            for (x, y) in [(0, size), (size, size), (size, 0)] {
                raster.line_to(Point {
                    x: x * 64,
                    y: y * 64,
                });
            }
            let b = raster.render_mono(MonoOptions::default(), None).unwrap();
            assert_eq!(
                (b.bounds.left, b.bounds.top, b.bounds.width, b.bounds.rows),
                (0, size, size as u32, size as u32)
            );
            for row in b.pixels.chunks_exact(b.stride) {
                assert_eq!(
                    row,
                    match size {
                        7 => &[0xfe][..],
                        8 => &[0xff],
                        _ => &[0xff, 0x80],
                    }
                );
            }
        }
    }
}

/// Outline flags used by FreeType's monochrome scan converter.
#[derive(Clone, Copy, Debug, Default)]
pub struct MonoOptions {
    /// Use 12-bit scan precision for small images.
    pub high_precision: bool,
    /// Skip the horizontal pass.
    pub single_pass: bool,
    /// Suppress dropout pixels.
    pub ignore_dropouts: bool,
    /// Select the closest pixel for narrow spans.
    pub smart_dropouts: bool,
    /// Include narrow stubs even without an overshoot.
    pub include_stubs: bool,
}

/// One bit per pixel, most significant bit first and top row first.
pub struct MonoBitmap<'a> {
    /// Origin and logical dimensions.
    pub bounds: PixelBox,
    /// Bytes per row without alignment padding.
    pub stride: usize,
    /// Packed scanlines.
    pub pixels: &'a [u8],
}

impl super::Rasterizer {
    /// Draw the recorded outline using FreeType monochrome scan conversion.
    ///
    /// # Errors
    /// Returns an error for bitmap bounds outside the supported range.
    pub fn render_mono(
        &mut self,
        options: MonoOptions,
        ppem: Option<(u16, u16)>,
    ) -> Result<MonoBitmap<'_>, super::RasterError> {
        let (x0, y0, x1, y1) = super::control_box(&self.commands).unwrap_or_default();
        let bounds = mono_pixel_box(x0 as i32, y0 as i32, x1 as i32, y1 as i32);
        if super::too_large(
            bounds.left.into(),
            bounds.top.into(),
            bounds.width.into(),
            bounds.rows.into(),
            ppem,
        ) {
            return Err(super::RasterError::TooLarge);
        }
        let width = bounds.width as usize;
        let rows = bounds.rows as usize;
        let stride = width.div_ceil(8);
        let len = stride
            .checked_mul(rows)
            .ok_or(super::RasterError::TooLarge)?;
        self.pixels.clear();
        self.pixels
            .try_reserve(len)
            .map_err(|_| super::RasterError::TooLarge)?;
        self.pixels.resize(len, 0);
        let origin = (
            i64::from(bounds.left),
            i64::from(bounds.top) - i64::from(bounds.rows),
        );
        let precision = if options.high_precision && width + rows < 258 {
            4096
        } else {
            64
        };
        for flipped in [false, true] {
            if flipped && options.single_pass {
                break;
            }
            let (height, extent) = if flipped {
                (width, rows)
            } else {
                (rows, width)
            };
            let contours = glyph_profiles(&self.commands, precision, origin, height, flipped);
            let mut crossings = alloc::vec![(alloc::vec::Vec::new(),alloc::vec::Vec::new());height];
            for (ci, contour) in contours.iter().enumerate() {
                for (pi, profile) in contour.iter().enumerate() {
                    for &(row, x) in &profile.samples {
                        let (up, down) = &mut crossings[row as usize];
                        (if profile.up { up } else { down }).push((x, ci, pi));
                    }
                }
            }
            for (row, (mut up, mut down)) in crossings.into_iter().enumerate() {
                up.sort_by_key(|v| v.0);
                down.sort_by_key(|v| v.0);
                let address = |pos: usize| {
                    let (x, y) = if flipped { (row, pos) } else { (pos, row) };
                    ((rows - 1 - y) * stride + x / 8, 0x80u8 >> (x % 8))
                };
                let mut drops = alloc::vec::Vec::new();
                for ((a, ci, pi), (b, cj, pj)) in up.into_iter().zip(down) {
                    let (left, right) = (a.min(b), a.max(b));
                    if (left + precision - 1).div_euclid(precision) <= right.div_euclid(precision) {
                        if flipped {
                            for v in [left, right] {
                                if v % precision == 0
                                    && (0..extent as i64).contains(&(v / precision))
                                {
                                    let (index, mask) = address((v / precision) as usize);
                                    self.pixels[index] |= mask;
                                }
                            }
                        } else {
                            let begin = (rows - 1 - row) * stride;
                            paint_span(
                                &mut self.pixels[begin..begin + stride],
                                width,
                                left,
                                right,
                                precision,
                            );
                        }
                    } else if !options.ignore_dropouts {
                        let profiles = &contours[ci];
                        let profile = &profiles[pi];
                        if !options.include_stubs && ci == cj {
                            let half = precision / 2;
                            let wide = right - left >= half;
                            let upper = (pi + 1) % profiles.len() == pj
                                && profile.samples.last().is_some_and(|p| p.0 == row as i64);
                            let lower = (pj + 1) % profiles.len() == pi
                                && profile.samples.first().is_some_and(|p| p.0 == row as i64);
                            if (upper && !(wide && profile.end_y.rem_euclid(precision) >= half))
                                || (lower
                                    && !(wide && (-profile.start_y).rem_euclid(precision) >= half))
                            {
                                continue;
                            }
                        }
                        drops.push((left, right));
                    }
                }
                for (left, right) in drops {
                    if let Some(pos) = dropout_pixel(
                        left,
                        right,
                        precision,
                        options.smart_dropouts,
                        extent,
                        |pos| {
                            let (i, m) = address(pos);
                            self.pixels[i] & m != 0
                        },
                    ) {
                        let (i, m) = address(pos);
                        self.pixels[i] |= m;
                    }
                }
            }
        }
        Ok(MonoBitmap {
            bounds,
            stride,
            pixels: &self.pixels,
        })
    }
}
