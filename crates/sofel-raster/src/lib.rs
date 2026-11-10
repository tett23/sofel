//! Anti-aliased rasterization with the arithmetic of FreeType's smooth
//! rasterizer (ADR 0012).
//!
//! Ported from FreeType 2.15.0 (commit 37515cf56ba3349656049edbed89ffc6f6466f2d):
//! `src/smooth/ftgrays.c` (`gray_render_line`, `gray_render_conic`,
//! `gray_render_cubic`, `gray_split_cubic`, `gray_sweep`, `FT_UDIV`,
//! `FT_FILL_RULE`), `src/smooth/ftsmooth.c` (`ft_smooth_render`,
//! `ft_smooth_raster_overlap`) and `src/base/ftobjs.c`
//! (`ft_glyphslot_preset_bitmap`). Copyright (C) 1996-2026 by David
//! Turner, Robert Wilhelm, and Werner Lemberg. Used under the FreeType
//! License (FTL); see THIRD_PARTY_NOTICES.md.
//!
//! The cover and area values put into each cell are FreeType's. Only the
//! storage differs: FreeType keeps cells in per-row linked lists in a
//! fixed pool; here they go into a dense array for a band of rows, with
//! one extra column on the left for cells left of the bitmap. A band's
//! array stays within a cell budget; taller glyphs are drawn band by
//! band, as FreeType does when its pool overflows.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

use sofel_geom::{Command, Pen, Point};

/// How overlapping or self-intersecting contours are filled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

/// What to render, as `FT_Render_Glyph` with `FT_RENDER_MODE_NORMAL`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RenderOptions {
    pub fill: FillRule,
    /// The outline has FreeType's `FT_OUTLINE_OVERLAP`: render with 4x4
    /// supersampling.
    pub overlap: bool,
    /// `(x_ppem, y_ppem)` of the size, if known: like FreeType, refuse
    /// bitmaps wider than 16 x_ppem or taller than 16 y_ppem.
    pub ppem: Option<(u16, u16)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RasterError {
    /// The bitmap would be too large (FreeType's `Raster_Overflow`).
    TooLarge,
}

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bitmap too large")
    }
}

impl core::error::Error for RasterError {}

/// An 8-bit coverage image. Rows go from the top; `left` and `top` place
/// it like FreeType's `bitmap_left` and `bitmap_top`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bitmap<'a> {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub rows: u32,
    pub pixels: &'a [u8],
}

/// The default number of cells in one band (8 bytes each: 64 KiB).
const BAND_CELLS: usize = 8192;

const PIXEL_BITS: u32 = 8;
const ONE_PIXEL: i64 = 1 << PIXEL_BITS;

/// Records an outline in 26.6 units through [`Pen`], then renders it.
/// Keep one and reuse it; [`Rasterizer::render`] does not clear it.
#[derive(Clone, Debug)]
pub struct Rasterizer {
    commands: Vec<Command>,
    cells: Vec<(i32, i32)>,
    pixels: Vec<u8>,
    band_cells: usize,
}

impl Default for Rasterizer {
    fn default() -> Self {
        Rasterizer::new()
    }
}

impl Pen for Rasterizer {
    fn move_to(&mut self, p: Point) {
        self.commands.push(Command::MoveTo(p));
    }
    fn line_to(&mut self, p: Point) {
        self.commands.push(Command::LineTo(p));
    }
    fn quad_to(&mut self, c: Point, p: Point) {
        self.commands.push(Command::QuadTo(c, p));
    }
    fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) {
        self.commands.push(Command::CubicTo(c1, c2, p));
    }
    fn close(&mut self) {}
}

/// Where a bitmap goes and how large it is, like FreeType's
/// `bitmap_left`, `bitmap_top`, width and rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PixelBox {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub rows: u32,
}

impl PixelBox {
    /// The offset that moves the bitmap's bottom-left corner to the
    /// origin, in 26.6 units (as `ft_smooth_render` translates the
    /// outline).
    pub fn shift(&self) -> Point {
        let rows = i32::try_from(self.rows).unwrap_or(i32::MAX);
        Point {
            x: self.left.saturating_mul(-64),
            y: rows.saturating_sub(self.top).saturating_mul(64),
        }
    }
}

/// The bitmap box for a control box in 26.6 units
/// (`ft_glyphslot_preset_bitmap`, normal mode): the box rounded outward
/// to whole pixels.
pub fn pixel_box(x_min: i32, y_min: i32, x_max: i32, y_max: i32) -> PixelBox {
    let (left, top, width, rows) = pixel_box_wide(
        i64::from(x_min),
        i64::from(y_min),
        i64::from(x_max),
        i64::from(y_max),
    );
    PixelBox {
        left: i32::try_from(left).unwrap_or(0),
        top: i32::try_from(top).unwrap_or(0),
        width: u32::try_from(width).unwrap_or(0),
        rows: u32::try_from(rows).unwrap_or(0),
    }
}

fn pixel_box_wide(x0: i64, y0: i64, x1: i64, y1: i64) -> (i64, i64, i64, i64) {
    let floor = |v: i64| (v >> 6).wrapping_add((v & 63) >> 6);
    let ceil = |v: i64| (v >> 6).wrapping_add((v & 63).wrapping_add(63) >> 6);
    let (left, bottom, right, top) = (floor(x0), floor(y0), ceil(x1), ceil(y1));
    (
        left,
        top,
        right.wrapping_sub(left),
        top.wrapping_sub(bottom),
    )
}

/// The control box of recorded commands, all points included.
fn control_box(commands: &[Command]) -> Option<(i64, i64, i64, i64)> {
    let mut points = commands.iter().flat_map(|c| {
        let (a, b, d) = match *c {
            Command::MoveTo(p) | Command::LineTo(p) => (Some(p), None, None),
            Command::QuadTo(c, p) => (Some(c), Some(p), None),
            Command::CubicTo(c1, c2, p) => (Some(c1), Some(c2), Some(p)),
            Command::Close => (None, None, None),
        };
        [a, b, d].into_iter().flatten()
    });
    let first = points.next()?;
    let init = (
        i64::from(first.x),
        i64::from(first.y),
        i64::from(first.x),
        i64::from(first.y),
    );
    Some(points.fold(init, |(a, b, c, d), p| {
        let (x, y) = (i64::from(p.x), i64::from(p.y));
        (a.min(x), b.min(y), c.max(x), d.max(y))
    }))
}

fn too_large(left: i64, top: i64, width: i64, rows: i64, ppem: Option<(u16, u16)>) -> bool {
    let bottom = top.wrapping_sub(rows);
    let right = left.wrapping_add(width);
    width >= 0x10000
        || rows >= 0x10000
        || left < -0x100_0000
        || right >= 0x100_0000
        || bottom < -0x100_0000
        || top >= 0x100_0000
        || ppem.is_some_and(|(x, y)| {
            width > i64::from(x).saturating_mul(16) || rows > i64::from(y).saturating_mul(16)
        })
}

/// `FT_FILL_RULE`: area to an 8-bit coverage.
#[inline]
fn coverage(area: i32, fill: FillRule) -> u8 {
    let c = area >> (PIXEL_BITS * 2 + 1 - 8);
    match fill {
        FillRule::NonZero => {
            let c = if c < 0 { !c } else { c };
            u8::try_from(c).unwrap_or(u8::MAX)
        }
        FillRule::EvenOdd => {
            let c = if c & 0x100 != 0 { !c } else { c };
            c.to_le_bytes()[0]
        }
    }
}

/// The worker for one band: FreeType's `gray_TWorker` with dense cells.
struct Band<'a> {
    cells: &'a mut [(i32, i32)],
    /// Columns: `min_ex - 1 ..= max_ex - 1`.
    columns: usize,
    min_ex: i32,
    max_ex: i32,
    min_ey: i32,
    max_ey: i32,
    /// The current cell, or `None` for FreeType's null cell.
    cell: Option<usize>,
    x: i64,
    y: i64,
}

#[inline]
fn trunc(v: i64) -> i32 {
    i32::try_from(v >> PIXEL_BITS).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX })
}

#[inline]
fn fract(v: i64) -> i32 {
    i32::try_from(v & (ONE_PIXEL - 1)).unwrap_or(0)
}

/// `UPSCALE`: 26.6 to 24.8.
#[inline]
fn upscale(v: i32) -> i64 {
    i64::from(v).wrapping_mul(ONE_PIXEL >> 6)
}

/// `FT_UDIVPREP`: the reciprocal used by [`udiv`].
#[inline]
fn udiv_prep(c: bool, b: i64) -> i64 {
    if c {
        0xFFFF_FFFF_i64.checked_div(b).unwrap_or(0)
    } else {
        0
    }
}

/// `FT_UDIV`: an approximate division by multiplication and shift.
#[inline]
fn udiv(a: i64, r: i64) -> i32 {
    let v = (a.cast_unsigned().wrapping_mul(r.cast_unsigned())) >> 32;
    i32::from_ne_bytes(u32::try_from(v & 0xFFFF_FFFF).unwrap_or(0).to_ne_bytes())
}

impl Band<'_> {
    /// `gray_set_cell`.
    fn set_cell(&mut self, ex: i32, ey: i32) {
        if ey < self.min_ey || ey >= self.max_ey || ex >= self.max_ex {
            self.cell = None;
            return;
        }
        let ex = ex.max(self.min_ex.wrapping_sub(1));
        let col = usize::try_from(ex.wrapping_sub(self.min_ex.wrapping_sub(1))).unwrap_or(0);
        let row = usize::try_from(ey.wrapping_sub(self.min_ey)).unwrap_or(0);
        self.cell = Some(row.wrapping_mul(self.columns).wrapping_add(col));
    }

    /// `FT_INTEGRATE`.
    #[inline]
    fn integrate(&mut self, a: i32, b: i32) {
        if let Some(c) = self.cell.and_then(|i| self.cells.get_mut(i)) {
            c.0 = c.0.wrapping_add(a);
            c.1 = c.1.wrapping_add(a.wrapping_mul(b));
        }
    }

    fn move_to(&mut self, p: Point) {
        let (x, y) = (upscale(p.x), upscale(p.y));
        self.set_cell(trunc(x), trunc(y));
        self.x = x;
        self.y = y;
    }

    /// `gray_render_line` (the 64-bit version).
    fn line(&mut self, to_x: i64, to_y: i64) {
        let mut ey1 = trunc(self.y);
        let ey2 = trunc(to_y);
        let outside =
            (ey1 >= self.max_ey && ey2 >= self.max_ey) || (ey1 < self.min_ey && ey2 < self.min_ey);
        if !outside {
            self.line_inside(to_x, to_y, &mut ey1, ey2);
        }
        self.x = to_x;
        self.y = to_y;
    }

    fn line_inside(&mut self, to_x: i64, to_y: i64, ey1: &mut i32, ey2: i32) {
        let mut ex1 = trunc(self.x);
        let ex2 = trunc(to_x);
        let mut fx1 = fract(self.x);
        let mut fy1 = fract(self.y);
        let dx = to_x.wrapping_sub(self.x);
        let dy = to_y.wrapping_sub(self.y);
        let one = ONE_PIXEL;
        let one_i = 1i32 << PIXEL_BITS;
        if ex1 == ex2 && *ey1 == ey2 {
            // Inside one cell.
        } else if dy == 0 {
            self.set_cell(ex2, ey2);
            return;
        } else if dx == 0 {
            if dy > 0 {
                loop {
                    self.integrate(one_i.wrapping_sub(fy1), fx1.wrapping_mul(2));
                    fy1 = 0;
                    *ey1 = ey1.wrapping_add(1);
                    self.set_cell(ex1, *ey1);
                    if *ey1 == ey2 {
                        break;
                    }
                }
            } else {
                loop {
                    self.integrate(fy1.wrapping_neg(), fx1.wrapping_mul(2));
                    fy1 = one_i;
                    *ey1 = ey1.wrapping_sub(1);
                    self.set_cell(ex1, *ey1);
                    if *ey1 == ey2 {
                        break;
                    }
                }
            }
        } else {
            let mut prod = dx
                .wrapping_mul(i64::from(fy1))
                .wrapping_sub(dy.wrapping_mul(i64::from(fx1)));
            let dx_r = udiv_prep(ex1 != ex2, dx);
            let dy_r = udiv_prep(*ey1 != ey2, dy);
            let dxo = dx.wrapping_mul(one);
            let dyo = dy.wrapping_mul(one);
            loop {
                let (fx2, fy2);
                if prod.wrapping_sub(dxo) > 0 && prod <= 0 {
                    // left
                    fx2 = 0;
                    fy2 = udiv(prod.wrapping_neg(), dx_r.wrapping_neg());
                    prod = prod.wrapping_sub(dyo);
                    self.integrate(fy2.wrapping_sub(fy1), fx1.wrapping_add(fx2));
                    fx1 = one_i;
                    fy1 = fy2;
                    ex1 = ex1.wrapping_sub(1);
                } else if prod.wrapping_sub(dxo).wrapping_add(dyo) > 0
                    && prod.wrapping_sub(dxo) <= 0
                {
                    // up
                    prod = prod.wrapping_sub(dxo);
                    fx2 = udiv(prod.wrapping_neg(), dy_r);
                    fy2 = one_i;
                    self.integrate(fy2.wrapping_sub(fy1), fx1.wrapping_add(fx2));
                    fx1 = fx2;
                    fy1 = 0;
                    *ey1 = ey1.wrapping_add(1);
                } else if prod.wrapping_add(dyo) >= 0
                    && prod.wrapping_sub(dxo).wrapping_add(dyo) <= 0
                {
                    // right
                    prod = prod.wrapping_add(dyo);
                    fx2 = one_i;
                    fy2 = udiv(prod, dx_r);
                    self.integrate(fy2.wrapping_sub(fy1), fx1.wrapping_add(fx2));
                    fx1 = 0;
                    fy1 = fy2;
                    ex1 = ex1.wrapping_add(1);
                } else {
                    // down
                    fx2 = udiv(prod, dy_r.wrapping_neg());
                    fy2 = 0;
                    prod = prod.wrapping_add(dxo);
                    self.integrate(fy2.wrapping_sub(fy1), fx1.wrapping_add(fx2));
                    fx1 = fx2;
                    fy1 = one_i;
                    *ey1 = ey1.wrapping_sub(1);
                }
                self.set_cell(ex1, *ey1);
                if ex1 == ex2 && *ey1 == ey2 {
                    break;
                }
            }
        }
        let fx2 = fract(to_x);
        let fy2 = fract(to_y);
        self.integrate(fy2.wrapping_sub(fy1), fx1.wrapping_add(fx2));
    }

    /// Whether all these y values are on one side outside the band.
    fn outside(&self, ys: &[i64]) -> bool {
        ys.iter().all(|&y| trunc(y) >= self.max_ey) || ys.iter().all(|&y| trunc(y) < self.min_ey)
    }

    /// `gray_render_conic` (the 64-bit version).
    fn conic(&mut self, control: Point, to: Point) {
        let (p0x, p0y) = (self.x, self.y);
        let (p1x, p1y) = (upscale(control.x), upscale(control.y));
        let (p2x, p2y) = (upscale(to.x), upscale(to.y));
        if self.outside(&[p0y, p1y, p2y]) {
            self.x = p2x;
            self.y = p2y;
            return;
        }
        let bx = p1x.wrapping_sub(p0x);
        let by = p1y.wrapping_sub(p0y);
        let ax = p2x.wrapping_sub(p1x).wrapping_sub(bx);
        let ay = p2y.wrapping_sub(p1y).wrapping_sub(by);
        let mut d = ax.wrapping_abs().max(ay.wrapping_abs());
        if d <= ONE_PIXEL / 4 {
            self.line(p2x, p2y);
            return;
        }
        let mut shift: u32 = 16;
        loop {
            d >>= 2;
            shift = shift.wrapping_sub(1);
            if d <= ONE_PIXEL / 4 {
                break;
            }
        }
        let count = 0x10000u32.checked_shr(shift).unwrap_or(0);
        let lshift = |v: i64, s: u32| v.cast_unsigned().wrapping_shl(s).cast_signed();
        let mut rx = lshift(ax, shift.wrapping_mul(2));
        let mut ry = lshift(ay, shift.wrapping_mul(2));
        let mut qx = lshift(bx, shift.wrapping_add(17)).wrapping_add(rx);
        let mut qy = lshift(by, shift.wrapping_add(17)).wrapping_add(ry);
        rx = rx.wrapping_mul(2);
        ry = ry.wrapping_mul(2);
        let mut px = lshift(p0x, 32);
        let mut py = lshift(p0y, 32);
        for _ in 0..count {
            px = px.wrapping_add(qx);
            py = py.wrapping_add(qy);
            qx = qx.wrapping_add(rx);
            qy = qy.wrapping_add(ry);
            self.line(px >> 32, py >> 32);
        }
    }

    /// `gray_render_cubic` with `gray_split_cubic`.
    fn cubic(&mut self, c1: Point, c2: Point, to: Point) {
        let mut stack = [(0i64, 0i64); 16 * 3 + 1];
        let set = |s: &mut [(i64, i64)], i: usize, v: (i64, i64)| {
            if let Some(e) = s.get_mut(i) {
                *e = v;
            }
        };
        set(&mut stack, 0, (upscale(to.x), upscale(to.y)));
        set(&mut stack, 1, (upscale(c2.x), upscale(c2.y)));
        set(&mut stack, 2, (upscale(c1.x), upscale(c1.y)));
        set(&mut stack, 3, (self.x, self.y));
        let get = |s: &[(i64, i64)], i: usize| s.get(i).copied().unwrap_or((0, 0));
        if self.outside(&[
            get(&stack, 0).1,
            get(&stack, 1).1,
            get(&stack, 2).1,
            get(&stack, 3).1,
        ]) {
            let (x, y) = get(&stack, 0);
            self.x = x;
            self.y = y;
            return;
        }
        let half = ONE_PIXEL / 2;
        let mut arc = 0usize;
        loop {
            let a0 = get(&stack, arc);
            let a1 = get(&stack, arc.wrapping_add(1));
            let a2 = get(&stack, arc.wrapping_add(2));
            let a3 = get(&stack, arc.wrapping_add(3));
            let k = |p: i64, q: i64, r: i64| {
                p.wrapping_mul(2)
                    .wrapping_sub(q.wrapping_mul(3))
                    .wrapping_add(r)
                    .wrapping_abs()
            };
            let l = |p: i64, q: i64, r: i64| {
                p.wrapping_sub(q.wrapping_mul(3))
                    .wrapping_add(r.wrapping_mul(2))
                    .wrapping_abs()
            };
            let split = arc.wrapping_add(6) < stack.len()
                && (k(a0.0, a1.0, a3.0) > half
                    || k(a0.1, a1.1, a3.1) > half
                    || l(a0.0, a2.0, a3.0) > half
                    || l(a0.1, a2.1, a3.1) > half);
            if !split {
                self.line(a0.0, a0.1);
                if arc == 0 {
                    return;
                }
                arc = arc.wrapping_sub(3);
                continue;
            }
            // gray_split_cubic(base = arc), per axis.
            let sp = |b0: i64, b1: i64, b2: i64, b3: i64| {
                let mut a = b0.wrapping_add(b1);
                let b = b1.wrapping_add(b2);
                let mut c = b2.wrapping_add(b3);
                let n6 = b3;
                let n5 = c >> 1;
                c = c.wrapping_add(b);
                let n4 = c >> 2;
                let n1 = a >> 1;
                a = a.wrapping_add(b);
                let n2 = a >> 2;
                let n3 = a.wrapping_add(c) >> 3;
                [n1, n2, n3, n4, n5, n6]
            };
            let xs = sp(a0.0, a1.0, a2.0, a3.0);
            let ys = sp(a0.1, a1.1, a2.1, a3.1);
            for (i, (x, y)) in xs.iter().zip(ys.iter()).enumerate() {
                set(&mut stack, arc.wrapping_add(i).wrapping_add(1), (*x, *y));
            }
            arc = arc.wrapping_add(3);
        }
    }
}

impl Rasterizer {
    pub fn new() -> Rasterizer {
        Rasterizer::with_band_cells(BAND_CELLS)
    }

    /// A rasterizer whose bands hold at most `cells` cells (at least one
    /// row). Smaller bands use less memory and redraw the outline more
    /// often; the result is the same.
    pub fn with_band_cells(cells: usize) -> Rasterizer {
        Rasterizer {
            commands: Vec::new(),
            cells: Vec::new(),
            pixels: Vec::new(),
            band_cells: cells.max(1),
        }
    }

    /// Forgets the recorded outline.
    pub fn clear(&mut self) {
        self.commands.clear();
    }

    /// Renders the recorded outline as `FT_Render_Glyph` with
    /// `FT_RENDER_MODE_NORMAL` would, finding the bitmap box from the
    /// recorded points and moving them to it. FreeType decomposes the
    /// outline after that move, so to match it exactly when implied
    /// on-curve points are involved, record the outline already moved
    /// and use [`Rasterizer::render_placed`] instead.
    pub fn render(&mut self, options: RenderOptions) -> Result<Bitmap<'_>, RasterError> {
        let (left, top, width, rows) = control_box(&self.commands)
            .map_or((0, 0, 0, 0), |(a, b, c, d)| pixel_box_wide(a, b, c, d));
        if too_large(left, top, width, rows, options.ppem) {
            return Err(RasterError::TooLarge);
        }
        let to_i32 = |v: i64| i32::try_from(v).unwrap_or(0);
        let to_u32 = |v: i64| u32::try_from(v).unwrap_or(0);
        let b = PixelBox {
            left: to_i32(left),
            top: to_i32(top),
            width: to_u32(width),
            rows: to_u32(rows),
        };
        let shift = b.shift();
        let mul = if options.overlap { 4 } else { 1 };
        self.render_box(b, shift, mul, options)
    }

    /// Renders into `pixel_box` an outline that the caller recorded
    /// already moved by `pixel_box.shift()` (and, with
    /// `options.overlap`, multiplied by 4 after the move), as FreeType's
    /// smooth renderer has it. Get the box from the unmoved outline's
    /// control box with [`pixel_box`].
    pub fn render_placed(
        &mut self,
        pixel_box: PixelBox,
        options: RenderOptions,
    ) -> Result<Bitmap<'_>, RasterError> {
        if too_large(
            i64::from(pixel_box.left),
            i64::from(pixel_box.top),
            i64::from(pixel_box.width),
            i64::from(pixel_box.rows),
            options.ppem,
        ) {
            return Err(RasterError::TooLarge);
        }
        self.render_box(pixel_box, Point::default(), 1, options)
    }

    /// Renders with every recorded point moved by `shift` and multiplied
    /// by `mul`.
    fn render_box(
        &mut self,
        b: PixelBox,
        shift: Point,
        mul: i32,
        options: RenderOptions,
    ) -> Result<Bitmap<'_>, RasterError> {
        let (w, h) = (b.width as usize, b.rows as usize);
        self.pixels.clear();
        self.pixels.resize(w.saturating_mul(h), 0);
        if w > 0 && h > 0 {
            let grid = if options.overlap {
                if u64::from(b.width).wrapping_mul(4) > 0x7FFF {
                    return Err(RasterError::TooLarge);
                }
                4
            } else {
                1
            };
            self.sweep_bands(shift.x, shift.y, mul, grid, w, h, options.fill);
        }
        Ok(Bitmap {
            left: b.left,
            top: b.top,
            width: b.width,
            rows: b.rows,
            pixels: &self.pixels,
        })
    }

    /// Draws all bands of a `w * scale` by `h * scale` raster of the
    /// outline moved by `(dx, dy)` and multiplied by `mul`, and writes
    /// (scale 1) or sums 4x4 samples (scale 4) into the pixels.
    #[allow(clippy::too_many_arguments)]
    fn sweep_bands(
        &mut self,
        dx: i32,
        dy: i32,
        mul: i32,
        scale: i32,
        w: usize,
        h: usize,
        fill: FillRule,
    ) {
        let sw = w.saturating_mul(usize::try_from(scale).unwrap_or(1));
        let sh = h.saturating_mul(usize::try_from(scale).unwrap_or(1));
        let columns = sw.saturating_add(1);
        let band_rows = self.band_cells.checked_div(columns).unwrap_or(1).max(1);
        let max_ex = i32::try_from(sw).unwrap_or(i32::MAX);
        let mut row_cov: Vec<u8> = Vec::with_capacity(sw);
        let mut y0 = 0usize;
        while y0 < sh {
            let y1 = y0.saturating_add(band_rows).min(sh);
            let n = columns.saturating_mul(y1.saturating_sub(y0));
            self.cells.clear();
            self.cells.resize(n, (0, 0));
            let mut band = Band {
                cells: &mut self.cells,
                columns,
                min_ex: 0,
                max_ex,
                min_ey: i32::try_from(y0).unwrap_or(i32::MAX),
                max_ey: i32::try_from(y1).unwrap_or(i32::MAX),
                cell: None,
                x: 0,
                y: 0,
            };
            let tr = |p: Point| Point {
                x: p.x.wrapping_add(dx).wrapping_mul(mul),
                y: p.y.wrapping_add(dy).wrapping_mul(mul),
            };
            for c in &self.commands {
                match *c {
                    Command::MoveTo(p) => band.move_to(tr(p)),
                    Command::LineTo(p) => {
                        let p = tr(p);
                        band.line(upscale(p.x), upscale(p.y));
                    }
                    Command::QuadTo(c, p) => band.conic(tr(c), tr(p)),
                    Command::CubicTo(c1, c2, p) => band.cubic(tr(c1), tr(c2), tr(p)),
                    Command::Close => {}
                }
            }
            // gray_sweep: rows of this band, bottom-up coordinates.
            for (r, cells) in self.cells.chunks(columns).enumerate() {
                let y = y0.saturating_add(r);
                row_cov.clear();
                let mut cover: i32 = 0;
                for (i, &(c, a)) in cells.iter().enumerate() {
                    cover = cover.wrapping_add(c.wrapping_mul(512));
                    if i > 0 {
                        row_cov.push(coverage(cover.wrapping_sub(a), fill));
                    }
                }
                if scale == 1 {
                    let top_row = h.saturating_sub(1).saturating_sub(y);
                    let start = top_row.saturating_mul(w);
                    if let Some(dst) = self.pixels.get_mut(start..start.saturating_add(w)) {
                        dst.copy_from_slice(&row_cov);
                    }
                } else {
                    // ft_smooth_overlap_spans: (coverage + 8) / 16 per sample.
                    let top_row = h.saturating_sub(1).saturating_sub(y / 4);
                    let start = top_row.saturating_mul(w);
                    if let Some(dst) = self.pixels.get_mut(start..start.saturating_add(w)) {
                        for (x, &v) in row_cov.iter().enumerate() {
                            if let Some(d) = dst.get_mut(x / 4) {
                                let sum = u32::from(*d)
                                    .saturating_add((u32::from(v).saturating_add(8)) / 16);
                                *d = sum.saturating_sub(sum >> 8).to_le_bytes()[0];
                            }
                        }
                    }
                }
            }
            y0 = y1;
        }
    }
}
