//! A single PDF page containing positioned, solid-filled outlines.
use alloc::{format, string::String, vec::Vec};
use core::fmt::Write as _;
use sofel_geom::{Color, CompositeMode, Pen, Point, PointF, Transform};

/// A PDF page in y-up user space. Text is drawn as paths.
pub struct PdfPage {
    bounds: [i64; 4],
    stream: String,
    states: Vec<(&'static str, u8)>,
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
        let mut offsets = Vec::new();
        let mut object = |content: &str| {
            offsets.push(output.len());
            let _ = writeln!(output, "{} 0 obj\n{content}\nendobj", offsets.len());
        };
        object("<< /Type /Catalog /Pages 2 0 R >>");
        object("<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
        object(&format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [{x0} {y0} {x1} {y1}] /Resources << /ExtGState << {states}>> >> /Contents 4 0 R >>"
        ));
        object(&format!(
            "<< /Length {} >>\nstream\n{}endstream",
            self.stream.len(),
            self.stream
        ));
        let xref = output.len();
        output.push_str("xref\n0 5\n0000000000 65535 f \n");
        for pos in offsets {
            let _ = writeln!(output, "{pos:010} 00000 n ");
        }
        let _ = writeln!(
            output,
            "trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF"
        );
        output.into_bytes()
    }
}

/// Emits a glyph's paths as PDF operators.
pub struct PdfPen<'a> {
    stream: &'a mut String,
    transform: Transform,
    current: PointF,
}

fn number(v: f32, precision: usize) -> String {
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
        #[allow(clippy::cast_possible_truncation)]
        let interpolate =
            |v: f32, c: f32| (f64::from(v) + (f64::from(c) - f64::from(v)) * (2.0 / 3.0)) as f32;
        let control = |v: PointF| PointF::new(interpolate(v.x, c.x), interpolate(v.y, c.y));
        self.cubic(control(self.current), control(p), p);
    }
    fn cubic_to(&mut self, a: Point, b: Point, p: Point) {
        self.cubic(self.map(a), self.map(b), self.map(p));
    }
    fn close(&mut self) {
        self.stream.push_str("h\n");
    }
}
