//! Font-independent SVG pages for positioned glyph outlines.
#![no_std]

extern crate alloc;
mod pdf;
use alloc::string::String;
use core::fmt::Write as _;
pub use pdf::{PdfPage, PdfPen};
use sofel_geom::{Color, Pen, Point, PointF, Transform};

/// A page whose view box uses SVG coordinates. Glyph outlines use y-up coordinates.
pub struct SvgPage {
    body: String,
}

impl SvgPage {
    /// Starts a page with an optional background in the requested viewport.
    pub fn new(x: i32, y: i32, width: u32, height: u32, background: Option<Color>) -> Self {
        let mut body = alloc::format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{x} {y} {width} {height}\" width=\"{width}\" height=\"{height}\">\n"
        );
        if let Some(c) = background {
            let _ = write!(
                body,
                "<rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" fill=\"rgb({},{},{})\"",
                c.r, c.g, c.b
            );
            opacity(&mut body, c.a);
            body.push_str("/>\n");
        }
        body.push_str("<g transform=\"scale(1,-1)\">\n");
        Self { body }
    }

    /// Adds a solid filled outline. A failed or empty outline adds no element.
    ///
    /// # Errors
    /// Propagates the error from drawing the glyph.
    pub fn fill_path<E>(
        &mut self,
        transform: Transform,
        fill: Color,
        draw: impl FnOnce(&mut SvgPen<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        let start = self.body.len();
        self.body.push_str("<path d=\"");
        let path_start = self.body.len();
        let result = draw(&mut SvgPen {
            body: &mut self.body,
            transform,
        });
        if result.is_err() || path_start == self.body.len() {
            self.body.truncate(start);
            return result;
        }
        let Color { r, g, b, a } = fill;
        if [r, g, b].iter().all(|v| v >> 4 == v & 15) {
            let _ = write!(
                self.body,
                "\" fill=\"#{:x}{:x}{:x}\"",
                r & 15,
                g & 15,
                b & 15
            );
        } else {
            let _ = write!(self.body, "\" fill=\"#{r:02x}{g:02x}{b:02x}\"");
        }
        opacity(&mut self.body, a);
        self.body.push_str("/>\n");
        Ok(())
    }

    /// Closes the page and returns the complete SVG document.
    pub fn finish(mut self) -> String {
        self.body.push_str("</g>\n</svg>\n");
        self.body
    }
}

fn opacity(out: &mut String, alpha: u8) {
    if alpha != 255 {
        let _ = write!(out, " fill-opacity=\"{}\"", f32::from(alpha) / 255.0);
    }
}

/// Receives outline commands for the current glyph.
pub struct SvgPen<'a> {
    body: &'a mut String,
    transform: Transform,
}

impl SvgPen<'_> {
    #[allow(clippy::cast_precision_loss)] // HarfBuzz vector paths use single-precision coordinates.
    fn point(&mut self, prefix: char, p: Point) {
        let p = self.transform.apply(PointF::new(p.x as f32, p.y as f32));
        let _ = write!(self.body, "{prefix}{},{}", p.x, p.y);
    }
}

impl Pen for SvgPen<'_> {
    fn move_to(&mut self, p: Point) {
        self.point('M', p);
    }
    fn line_to(&mut self, p: Point) {
        self.point('L', p);
    }
    fn quad_to(&mut self, c: Point, p: Point) {
        self.point('Q', c);
        self.point(' ', p);
    }
    fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) {
        self.point('C', c1);
        self.point(' ', c2);
        self.point(' ', p);
    }
    fn close(&mut self) {
        self.body.push('Z');
    }
}
