//! Font-independent SVG pages for positioned glyph outlines.
#![no_std]

extern crate alloc;
mod pdf;
use alloc::string::String;
use core::fmt::Write as _;
pub use pdf::{PdfPage, PdfPen};
use sofel_geom::{Color, Image, ImageFormat, Pen, Point, PointF, Rect, Transform};

/// A page whose view box uses SVG coordinates. Glyph outlines use y-up coordinates.
pub struct SvgPage {
    viewport: (i32, i32, u32, u32),
    background: Option<Color>,
    body: String,
    clips: String,
    clip_count: usize,
    has_image: bool,
}

impl SvgPage {
    /// Starts a page with an optional background in the requested viewport.
    pub fn new(x: i32, y: i32, width: u32, height: u32, background: Option<Color>) -> Self {
        Self {
            viewport: (x, y, width, height),
            background,
            body: String::new(),
            clips: String::new(),
            clip_count: 0,
            has_image: false,
        }
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
                "\" fill=\"#{:X}{:X}{:X}\"",
                r & 15,
                g & 15,
                b & 15
            );
        } else {
            let _ = write!(self.body, "\" fill=\"#{r:02X}{g:02X}{b:02X}\"");
        }
        opacity(&mut self.body, a);
        self.body.push_str("/>\n");
        Ok(())
    }

    /// Places an embedded PNG with a transform in font coordinates. An optional
    /// rectangle clips in the parent coordinate system before the transform.
    /// Returns false without changing the page when image data or dimensions are missing.
    #[allow(clippy::cast_precision_loss)] // HarfBuzz applies viewport transforms in f32.
    pub fn draw_image(
        &mut self,
        image: &Image<'_>,
        transform: Transform,
        clip: Option<Rect>,
    ) -> bool {
        let Some(extents) = image.extents else {
            return false;
        };
        if image.format != ImageFormat::Png
            || image.data.is_empty()
            || image.width == 0
            || image.height == 0
        {
            return false;
        }
        self.has_image = true;
        if let Some(rect) = clip {
            let id = self.clip_count;
            self.clip_count = self.clip_count.saturating_add(1);
            let _ = writeln!(
                self.clips,
                r#"<clipPath id="c{id}"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath>"#,
                rect.x_min,
                rect.y_min,
                rect.x_max - rect.x_min,
                rect.y_max - rect.y_min
            );
            let _ = writeln!(self.body, r#"<g clip-path="url(#c{id})">"#);
        }
        if transform != Transform::IDENTITY {
            let Transform {
                xx,
                yx,
                xy,
                yy,
                dx,
                dy,
            } = transform;
            let _ = writeln!(
                self.body,
                r#"<g transform="matrix({xx},{yx},{xy},{yy},{dx},{dy})">"#
            );
        }
        let _ = writeln!(
            self.body,
            r#"<g transform="translate({},{}) scale({},{})">"#,
            extents.x_bearing,
            extents.y_bearing,
            pdf::number(extents.width as f32 / image.width as f32, 2),
            pdf::number(extents.height as f32 / image.height as f32, 2)
        );
        self.body.push_str(r#"<image href="data:image/png;base64,"#);
        write_base64(&mut self.body, image.data);
        let _ = writeln!(
            self.body,
            r#"" width="{}" height="{}"/>"#,
            image.width, image.height
        );
        self.body.push_str("</g>\n");
        if transform != Transform::IDENTITY {
            self.body.push_str("</g>\n");
        }
        if clip.is_some() {
            self.body.push_str("</g>\n");
        }
        true
    }

    /// Closes the page and returns the complete SVG document.
    pub fn finish(self) -> String {
        let (x, y, width, height) = self.viewport;
        let xlink = if self.has_image {
            r#" xmlns:xlink="http://www.w3.org/1999/xlink""#
        } else {
            ""
        };
        let mut svg = alloc::format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"{xlink} viewBox=\"{x} {y} {width} {height}\" width=\"{width}\" height=\"{height}\">\n"
        );
        if !self.clips.is_empty() {
            svg.push_str("<defs>\n");
            svg.push_str(&self.clips);
            svg.push_str("</defs>\n");
        }
        if let Some(c) = self.background.filter(|c| c.a != 0) {
            let _ = write!(
                svg,
                r#"<rect x="{x}" y="{y}" width="{width}" height="{height}" fill="rgb({},{},{})""#,
                c.r, c.g, c.b
            );
            opacity(&mut svg, c.a);
            svg.push_str("/>\n");
        }
        if !self.body.is_empty() {
            svg.push_str("<g transform=\"scale(1,-1)\">\n");
            svg.push_str(&self.body);
            svg.push_str("</g>\n");
        }
        svg.push_str("</svg>\n");
        svg
    }
}

fn write_base64(out: &mut String, bytes: &[u8]) {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for group in bytes.chunks(3) {
        let word = u32::from_be_bytes([
            0,
            group.first().copied().unwrap_or(0),
            group.get(1).copied().unwrap_or(0),
            group.get(2).copied().unwrap_or(0),
        ]);
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if index > group.len() {
                out.push('=');
            } else {
                #[allow(clippy::indexing_slicing)] // The mask is in 0..64.
                out.push(char::from(ALPHABET[((word >> shift) & 63) as usize]));
            }
        }
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
    /// Appends a floating-point outline command at the page transform.
    pub fn command(&mut self, cmd: sofel_geom::FloatCommand) {
        use sofel_geom::FloatCommand::*;
        match cmd {
            MoveTo(p) => self.float_point('M', p),
            LineTo(p) => self.float_point('L', p),
            QuadTo(c, p) => {
                self.float_point('Q', c);
                self.float_point(' ', p);
            }
            CubicTo(a, b, p) => {
                self.float_point('C', a);
                self.float_point(' ', b);
                self.float_point(' ', p);
            }
            Close => self.close(),
        }
    }
    fn float_point(&mut self, prefix: char, point: PointF) {
        let p = self.transform.apply(point);
        let _ = write!(
            self.body,
            "{prefix}{},{}",
            pdf::number(p.x, 2),
            pdf::number(p.y, 2)
        );
    }
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
