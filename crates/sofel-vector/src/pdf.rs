//! A single PDF page containing positioned, solid-filled outlines.
use alloc::{format, string::String, vec::Vec};
use core::fmt::Write as _;
use sofel_geom::{Color, CompositeMode, Pen, Point, PointF, Transform};

/// A PDF page in y-up user space. Text is drawn as paths.
pub struct PdfPage {
    bounds: [i64; 4],
    stream: String,
    images: Vec<PdfImage>,
    states: Vec<(&'static str, u8)>,
}

struct PdfImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn blend(mode: CompositeMode) -> &'static str {
    use CompositeMode::*;
    match mode {
        Multiply => "Multiply",
        Screen | Plus => "Screen",
        Overlay => "Overlay",
        Darken => "Darken",
        Lighten => "Lighten",
        ColorDodge => "ColorDodge",
        ColorBurn => "ColorBurn",
        HardLight => "HardLight",
        SoftLight => "SoftLight",
        Difference | Xor => "Difference",
        Exclusion => "Exclusion",
        HslHue => "Hue",
        HslSaturation => "Saturation",
        HslColor => "Color",
        HslLuminosity => "Luminosity",
        _ => "Normal",
    }
}

impl PdfPage {
    /// Starts a page with the given lower-left corner, dimensions and background.
    pub fn new(x: i32, y: i32, width: u32, height: u32, background: Option<Color>) -> Self {
        let bounds = [
            i64::from(x),
            i64::from(y),
            i64::from(x).saturating_add(i64::from(width)),
            i64::from(y).saturating_add(i64::from(height)),
        ];
        let mut page = Self {
            bounds,
            stream: String::new(),
            states: Vec::new(),
            images: Vec::new(),
        };
        if let Some(color) = background {
            page.begin_fill(color, CompositeMode::SrcOver);
            let _ = writeln!(page.stream, "{x} {y} {width} {height} re f\nQ");
        }
        page
    }
    fn begin_fill(&mut self, color: Color, mode: CompositeMode) {
        self.stream.push_str("q\n");
        let state = (blend(mode), color.a);
        if state != ("Normal", 255) {
            let idx = self
                .states
                .iter()
                .position(|v| v == &state)
                .unwrap_or_else(|| {
                    let i = self.states.len();
                    self.states.push(state);
                    i
                });
            let _ = writeln!(self.stream, "/GS{idx} gs");
        }
        let [r, g, b] = [color.r, color.g, color.b].map(|v| number(f32::from(v) / 255.0, 4));
        let _ = writeln!(self.stream, "{r} {g} {b} rg");
    }
    /// Fills a path in the given color, transform and composite mode.
    ///
    /// # Errors
    /// Propagates drawing errors without retaining a partial path.
    pub fn fill_path<E>(
        &mut self,
        transform: Transform,
        color: Color,
        mode: CompositeMode,
        draw: impl FnOnce(&mut PdfPen<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        let start = self.stream.len();
        let state_count = self.states.len();
        self.begin_fill(color, mode);
        let path_start = self.stream.len();
        let result = draw(&mut PdfPen {
            stream: &mut self.stream,
            transform,
            current: PointF::default(),
        });
        if result.is_err() || path_start == self.stream.len() {
            self.stream.truncate(start);
            self.states.truncate(state_count);
            return result;
        }
        self.stream.push_str("f\nQ\n");
        Ok(())
    }
    /// Positions unassociated top-down RGBA samples using the glyph extents.
    /// Returns false when their length does not match the pixel dimensions.
    #[allow(clippy::cast_precision_loss)]
    pub fn draw_rgba(
        &mut self,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        e: sofel_geom::GlyphExtents,
        transform: Transform,
    ) -> bool {
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4));
        if width == 0 || height == 0 || len != Some(rgba.len()) {
            return false;
        }
        let matrix = transform.multiply(&Transform {
            xx: e.width as f32,
            yy: -(e.height as f32),
            dx: e.x_bearing as f32,
            dy: e.y_bearing as f32 + e.height as f32,
            ..Transform::IDENTITY
        });
        let id = self.images.len();
        let [a, b, c, d, x, y] = [
            matrix.xx, matrix.yx, matrix.xy, matrix.yy, matrix.dx, matrix.dy,
        ]
        .map(|v| number(v, 2));
        let _ = writeln!(self.stream, "q\n{a} {b} {c} {d} {x} {y} cm\n/Im{id} Do\nQ");
        self.images.push(PdfImage {
            width,
            height,
            rgba,
        });
        true
    }

    /// Serializes the page and its graphics states with a cross-reference table.
    pub fn finish(self) -> Vec<u8> {
        let mut output = String::from("%PDF-1.4\n");
        let [x0, y0, x1, y1] = self.bounds;
        let mut states = String::new();
        for (i, (mode, alpha)) in self.states.iter().enumerate() {
            let alpha = f32::from(*alpha) / 255.0;
            let _ = write!(
                states,
                "/GS{i} << /Type /ExtGState /BM /{mode} /ca {alpha} >> "
            );
        }
        let mut images = String::new();
        for i in 0..self.images.len() {
            let _ = write!(
                images,
                "/Im{i} {} 0 R ",
                i.saturating_mul(2).saturating_add(6)
            );
        }
        let mut offsets = Vec::new();
        let mut object = |content: &str| {
            offsets.push(output.len());
            let _ = writeln!(output, "{} 0 obj\n{content}\nendobj", offsets.len());
        };
        object("<< /Type /Catalog /Pages 2 0 R >>");
        object("<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
        object(&format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [{x0} {y0} {x1} {y1}] /Resources << /ExtGState << {states}>> /XObject << {images}>> >> /Contents 4 0 R >>"
        ));
        object(&format!(
            "<< /Length {} >>\nstream\n{}endstream",
            self.stream.len(),
            self.stream
        ));
        let mut bytes = output.into_bytes();
        for image in self.images {
            let pixels = image.rgba.as_chunks::<4>().0;
            let mask_id = offsets.len().saturating_add(1);
            for mask in [true, false] {
                offsets.push(bytes.len());
                let space = if mask { "DeviceGray" } else { "DeviceRGB" };
                let mask_ref = if mask {
                    String::new()
                } else {
                    format!("/SMask {mask_id} 0 R ")
                };
                let size = pixels.len().saturating_mul(if mask { 1 } else { 3 });
                bytes.extend_from_slice(format!("{} 0 obj\n<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /{space} /BitsPerComponent 8 {mask_ref}/Length {size} >>\nstream\n", offsets.len(), image.width, image.height).as_bytes());
                for &[r, g, b, a] in pixels {
                    if mask {
                        bytes.push(a);
                    } else {
                        bytes.extend_from_slice(&[r, g, b]);
                    }
                }
                bytes.extend_from_slice(b"\nendstream\nendobj\n");
            }
        }
        let xref = bytes.len();
        let count = offsets.len().saturating_add(1);
        let mut trailer = format!("xref\n0 {count}\n0000000000 65535 f \n");
        for pos in offsets {
            let _ = writeln!(trailer, "{pos:010} 00000 n ");
        }
        let _ = writeln!(
            trailer,
            "trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF"
        );
        bytes.extend_from_slice(trailer.as_bytes());
        bytes
    }
}

/// Emits a glyph's paths as PDF operators.
pub struct PdfPen<'a> {
    stream: &'a mut String,
    transform: Transform,
    current: PointF,
}

pub(super) fn number(v: f32, precision: usize) -> String {
    if !v.is_finite() {
        return String::from("0");
    }
    let mut value = format!("{v:.precision$}");
    while value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    if value == "-0" {
        String::from("0")
    } else {
        value
    }
}

impl PdfPen<'_> {
    #[allow(clippy::cast_precision_loss)]
    fn map(&self, p: Point) -> PointF {
        self.transform.apply(PointF::new(p.x as f32, p.y as f32))
    }
    /// Appends a floating-point outline command at the page transform.
    pub fn command(&mut self, cmd: sofel_geom::FloatCommand) {
        use sofel_geom::FloatCommand::*;
        let t = self.transform;
        match cmd {
            MoveTo(p) | LineTo(p) => {
                let p = t.apply(p);
                let op = if matches!(cmd, MoveTo(_)) { 'm' } else { 'l' };
                let _ = writeln!(self.stream, "{} {} {op}", number(p.x, 2), number(p.y, 2));
                self.current = p;
            }
            QuadTo(c, p) => self.quadratic(t.apply(c), t.apply(p)),
            CubicTo(a, b, p) => self.cubic(t.apply(a), t.apply(b), t.apply(p)),
            Close => self.close(),
        }
    }
    fn quadratic(&mut self, c: PointF, p: PointF) {
        #[allow(clippy::cast_possible_truncation)]
        let interpolate =
            |v: f32, c: f32| (f64::from(v) + (f64::from(c) - f64::from(v)) * (2.0 / 3.0)) as f32;
        let control = |v: PointF| PointF::new(interpolate(v.x, c.x), interpolate(v.y, c.y));
        self.cubic(control(self.current), control(p), p);
    }
    fn cubic(&mut self, a: PointF, b: PointF, p: PointF) {
        let _ = writeln!(
            self.stream,
            "{} {} {} {} {} {} c",
            number(a.x, 2),
            number(a.y, 2),
            number(b.x, 2),
            number(b.y, 2),
            number(p.x, 2),
            number(p.y, 2)
        );
        self.current = p;
    }
}

impl Pen for PdfPen<'_> {
    fn move_to(&mut self, p: Point) {
        let p = self.map(p);
        let _ = writeln!(self.stream, "{} {} m", number(p.x, 2), number(p.y, 2));
        self.current = p;
    }
    fn line_to(&mut self, p: Point) {
        let p = self.map(p);
        let _ = writeln!(self.stream, "{} {} l", number(p.x, 2), number(p.y, 2));
        self.current = p;
    }
    fn quad_to(&mut self, c: Point, p: Point) {
        let c = self.map(c);
        let p = self.map(p);
        self.quadratic(c, p);
    }
    fn cubic_to(&mut self, a: Point, b: Point, p: Point) {
        self.cubic(self.map(a), self.map(b), self.map(p));
    }
    fn close(&mut self) {
        self.stream.push_str("h\n");
    }
}
