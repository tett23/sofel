//! OpenType SVG document data (ADR 0066).
#![no_std]

extern crate alloc;
mod path;
mod transform;
use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
pub use path::Path;
use sofel_geom::{Pen, Point};
use transform::{compose, transform};

/// Reads an SVG document, borrowing plain bytes or expanding a gzip member.
///
/// # Errors
/// Returns a decompression error for invalid gzip data, a failed checksum,
/// or when the expanded data exceeds the supplied limit.
pub fn document(data: &[u8], limit: usize) -> Result<Cow<'_, [u8]>, sofel_inflate::Error> {
    if data.starts_with(&[0x1f, 0x8b]) {
        sofel_inflate::gzip(data, limit).map(Cow::Owned)
    } else {
        Ok(Cow::Borrowed(data))
    }
}

/// One SVG gradient stop, with straight RGBA channels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    /// The position along the gradient, in 0..=1.
    pub offset: f32,
    /// Red, green, blue and alpha.
    pub rgba: [u8; 4],
}

/// Coordinates of a gradient before its transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientKind {
    /// The start and end points.
    Linear { x1: f32, y1: f32, x2: f32, y2: f32 },
    /// The center, radius and focal point.
    Radial {
        cx: f32,
        cy: f32,
        r: f32,
        fx: f32,
        fy: f32,
    },
}

/// A gradient definition referenced by `fill="url(#id)"`.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    /// XML identifier.
    pub id: String,
    /// Gradient geometry.
    pub kind: GradientKind,
    /// True for `gradientUnits="userSpaceOnUse"`.
    pub user_space: bool,
    /// SVG matrix `[a, b, c, d, e, f]`.
    pub transform: [f32; 6],
    /// Stops in document order.
    pub stops: Vec<Stop>,
}

/// Geometries that can be drawn to a pen.
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeKind {
    /// Line and Bézier contours from a path element.
    Path(Path),
    /// A rectangle in SVG coordinates.
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    /// A circle in SVG coordinates.
    Circle { cx: f32, cy: f32, r: f32 },
}

/// A shape and the gradient that paints it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    /// Rectangle or circle.
    pub kind: ShapeKind,
    /// Index into [`Scene::gradients`].
    pub gradient: usize,
    /// Enclosing group transform, in SVG matrix order.
    pub transform: [f32; 6],
}

/// Parsed shapes and gradient definitions for one glyph.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    /// Gradient definitions.
    pub gradients: Vec<Gradient>,
    /// Shapes in document order.
    pub shapes: Vec<Shape>,
}

/// Invalid or unsupported XML in the supported SVG subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseError;

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn number(value: Option<&str>, default: f32) -> f32 {
    value
        .and_then(|v| {
            let v = v.trim();
            v.strip_suffix('%')
                .and_then(|p| p.parse::<f32>().ok().map(|x| x / 100.0))
                .or_else(|| v.parse::<f32>().ok())
        })
        .unwrap_or(default)
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // Each index is checked against the tag length.
fn attributes(tag: &str) -> Vec<(&str, &str)> {
    let bytes = tag.as_bytes();
    let mut out = Vec::new();
    let mut i = tag.find(char::is_whitespace).unwrap_or(tag.len());
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'/'
        {
            i += 1;
        }
        if i == start {
            break;
        }
        let name = &tag[start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let Some(&quote @ (b'\'' | b'"')) = bytes.get(i) else {
            break;
        };
        i += 1;
        let value = i;
        while i < bytes.len() && bytes[i] != quote {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        out.push((name, &tag[value..i]));
        i += 1;
    }
    out
}

fn attr<'a>(attrs: &'a [(&str, &'a str)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
}

#[allow(clippy::cast_possible_truncation)] // Hex pairs are each at most 255.
fn color(value: &str) -> Option<[u8; 3]> {
    match value {
        "gold" => Some([255, 215, 0]),
        "red" => Some([255, 0, 0]),
        "white" => Some([255, 255, 255]),
        "green" => Some([0, 128, 0]),
        "darkblue" => Some([0, 0, 139]),
        "skyblue" => Some([135, 206, 235]),
        "purple" => Some([128, 0, 128]),
        "midnightblue" => Some([25, 25, 112]),
        _ => {
            let digits = value.strip_prefix('#')?;
            if digits.len() != 6 {
                return None;
            }
            let n = u32::from_str_radix(digits, 16).ok()?;
            Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
        }
    }
}

fn gradient(kind: &str, attrs: &[(&str, &str)]) -> Gradient {
    let kind = if kind == "linearGradient" {
        GradientKind::Linear {
            x1: number(attr(attrs, "x1"), 0.0),
            y1: number(attr(attrs, "y1"), 0.0),
            x2: number(attr(attrs, "x2"), 1.0),
            y2: number(attr(attrs, "y2"), 0.0),
        }
    } else {
        let cx = number(attr(attrs, "cx"), 0.5);
        let cy = number(attr(attrs, "cy"), 0.5);
        GradientKind::Radial {
            cx,
            cy,
            r: number(attr(attrs, "r"), 0.5),
            fx: number(attr(attrs, "fx"), cx),
            fy: number(attr(attrs, "fy"), cy),
        }
    };
    Gradient {
        id: attr(attrs, "id").unwrap_or("").to_string(),
        kind,
        user_space: attr(attrs, "gradientUnits") == Some("userSpaceOnUse"),
        transform: transform(attr(attrs, "gradientTransform")),
        stops: Vec::new(),
    }
}

/// Parses rect/circle shapes and referenced gradients for `glyph`.
/// Other SVG elements are left unsupported; no script or external resource is followed.
///
/// # Errors
/// Returns [`ParseError`] if the document is not UTF-8 or a tag is unterminated.
#[allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // Bounded XML indices and clamped alpha.
pub fn parse_shapes(data: &[u8], glyph: u32) -> Result<Scene, ParseError> {
    let xml = core::str::from_utf8(data).map_err(|_| ParseError)?;
    let mut scene = Scene::default();
    let mut at = 0;
    let mut selected = false;
    let mut group_transform = IDENTITY;
    let wanted = alloc::format!("glyph{glyph}");
    let mut current_gradient = None;
    while let Some(start) = xml[at..].find('<').map(|p| at + p) {
        let mut end = start + 1;
        let mut quote = None;
        while end < xml.len() {
            let b = xml.as_bytes()[end];
            if quote == Some(b) {
                quote = None;
            } else if quote.is_none() && (b == b'\'' || b == b'"') {
                quote = Some(b);
            } else if quote.is_none() && b == b'>' {
                break;
            }
            end += 1;
        }
        if end == xml.len() {
            return Err(ParseError);
        }
        let tag = xml[start + 1..end].trim();
        at = end + 1;
        if tag.starts_with("/linearGradient") || tag.starts_with("/radialGradient") {
            current_gradient = None;
            continue;
        }
        if tag.starts_with("/g") {
            selected = false;
            group_transform = IDENTITY;
            continue;
        }
        if tag.starts_with('!') || tag.starts_with('?') || tag.starts_with('/') {
            continue;
        }
        let name = tag
            .split(|c: char| c.is_ascii_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        let attrs = attributes(tag);
        match name {
            "g" => {
                selected = attr(&attrs, "id") == Some(wanted.as_str());
                group_transform = transform(attr(&attrs, "transform"));
            }
            "linearGradient" | "radialGradient" => {
                scene.gradients.push(gradient(name, &attrs));
                current_gradient = Some(scene.gradients.len() - 1);
            }
            "stop" => {
                if let Some(g) = current_gradient.and_then(|i| scene.gradients.get_mut(i)) {
                    let rgb = attr(&attrs, "stop-color")
                        .and_then(color)
                        .unwrap_or([0, 0, 0]);
                    let alpha = (number(attr(&attrs, "stop-opacity"), 1.0).clamp(0.0, 1.0) * 255.0
                        + 0.5) as u8;
                    g.stops.push(Stop {
                        offset: number(attr(&attrs, "offset"), 0.0),
                        rgba: [rgb[0], rgb[1], rgb[2], alpha],
                    });
                }
            }
            "rect" | "circle" | "path" if selected => {
                let Some(id) = attr(&attrs, "fill")
                    .and_then(|v| v.strip_prefix("url(#"))
                    .and_then(|v| v.strip_suffix(')'))
                else {
                    continue;
                };
                let Some(index) = scene.gradients.iter().position(|g| g.id == id) else {
                    continue;
                };
                let Some(kind) = shape_kind(name, &attrs) else {
                    continue;
                };
                scene.shapes.push(Shape {
                    kind,
                    gradient: index,
                    transform: compose(group_transform, transform(attr(&attrs, "transform"))),
                });
            }
            _ => {}
        }
    }
    Ok(scene)
}

pub(crate) fn shape_kind(name: &str, attrs: &[(&str, &str)]) -> Option<ShapeKind> {
    let n = |key| number(attr(attrs, key), 0.0);
    Some(match name {
        "rect" => ShapeKind::Rect {
            x: n("x"),
            y: n("y"),
            width: n("width"),
            height: n("height"),
        },
        "circle" => ShapeKind::Circle {
            cx: n("cx"),
            cy: n("cy"),
            r: n("r"),
        },
        "path" => ShapeKind::Path(Path::parse(attr(attrs, "d")?)?),
        _ => return None,
    })
}

fn map(t: [f32; 6], x: f32, y: f32) -> (f32, f32) {
    (t[0] * x + t[2] * y + t[4], t[1] * x + t[3] * y + t[5])
}

fn inverse(t: [f32; 6], x: f32, y: f32) -> Option<(f32, f32)> {
    let det = t[0] * t[3] - t[1] * t[2];
    if det == 0.0 {
        return None;
    }
    let x = x - t[4];
    let y = y - t[5];
    Some(((t[3] * x - t[2] * y) / det, (-t[1] * x + t[0] * y) / det))
}

fn shape_box(kind: &ShapeKind) -> (f32, f32, f32, f32) {
    match *kind {
        ShapeKind::Path(ref path) => path.bounds,
        ShapeKind::Rect {
            x,
            y,
            width,
            height,
        } => (x, y, x + width, y + height),
        ShapeKind::Circle { cx, cy, r } => (cx - r, cy - r, cx + r, cy + r),
    }
}

fn sqrt(value: f32) -> f32 {
    if value <= 0.0 {
        return 0.0;
    }
    let mut x = if value >= 1.0 { value } else { 1.0 };
    for _ in 0..12 {
        x = 0.5 * (x + value / x);
    }
    x
}

impl Shape {
    /// Sends the transformed SVG geometry to a 26.6 pixel pen.
    /// SVG's downward y axis is converted to the font's upward y axis.
    #[allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation)]
    pub fn draw(&self, scale: f32, pen: &mut impl Pen) {
        let point = |x: f32, y: f32| {
            let (x, y) = map(self.transform, x, y);
            let round = |v: f32| {
                if v < 0.0 {
                    -sofel_geom::float::floorf(-v + 0.5)
                } else {
                    sofel_geom::float::floorf(v + 0.5)
                }
            };
            Point {
                x: round(x * scale * 64.0) as i32,
                y: round(-y * scale * 64.0) as i32,
            }
        };
        match self.kind {
            ShapeKind::Path(ref path) => path.draw(point, pen),
            ShapeKind::Rect {
                x,
                y,
                width,
                height,
            } => {
                let start = point(x, y);
                pen.move_to(start);
                pen.line_to(point(x + width, y));
                pen.line_to(point(x + width, y + height));
                pen.line_to(point(x, y + height));
                pen.line_to(start);
                pen.close();
            }
            ShapeKind::Circle { cx, cy, r } => {
                const K: f32 = 0.552_284_8;
                let start = point(cx + r, cy);
                pen.move_to(start);
                for (a, b, c) in [
                    ((cx + r, cy + K * r), (cx + K * r, cy + r), (cx, cy + r)),
                    ((cx - K * r, cy + r), (cx - r, cy + K * r), (cx - r, cy)),
                    ((cx - r, cy - K * r), (cx - K * r, cy - r), (cx, cy - r)),
                    ((cx + K * r, cy - r), (cx + r, cy - K * r), (cx + r, cy)),
                ] {
                    pen.cubic_to(point(a.0, a.1), point(b.0, b.1), point(c.0, c.1));
                }
                pen.line_to(start);
                pen.close();
            }
        }
    }
}

impl Scene {
    /// Samples the gradient for `shape` at a pixel center relative to the glyph origin.
    /// The returned color has straight RGBA channels.
    #[allow(
        clippy::arithmetic_side_effects,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::indexing_slicing
    )]
    pub fn sample(&self, shape: &Shape, pixel_x: f32, pixel_y: f32, scale: f32) -> [u8; 4] {
        let Some(gradient) = self.gradients.get(shape.gradient) else {
            return [0; 4];
        };
        if scale == 0.0 {
            return [0; 4];
        }
        let Some((x, y)) = inverse(shape.transform, pixel_x / scale, -pixel_y / scale) else {
            return [0; 4];
        };
        let (x0, y0, x1, y1) = shape_box(&shape.kind);
        let (x, y) = if gradient.user_space {
            (x, y)
        } else {
            ((x - x0) / (x1 - x0), (y - y0) / (y1 - y0))
        };
        let Some((x, y)) = inverse(gradient.transform, x, y) else {
            return [0; 4];
        };
        let fraction = match gradient.kind {
            GradientKind::Linear { x1, y1, x2, y2 } => {
                let dx = x2 - x1;
                let dy = y2 - y1;
                let d = dx * dx + dy * dy;
                if d == 0.0 {
                    0.0
                } else {
                    ((x - x1) * dx + (y - y1) * dy) / d
                }
            }
            GradientKind::Radial { cx, cy, r, .. } => {
                if r == 0.0 {
                    0.0
                } else {
                    sqrt((x - cx) * (x - cx) + (y - cy) * (y - cy)) / r
                }
            }
        };
        let Some(first) = gradient.stops.first() else {
            return [0; 4];
        };
        if fraction <= first.offset {
            return first.rgba;
        }
        for pair in gradient.stops.windows(2) {
            let a = pair[0];
            let b = pair[1];
            if fraction <= b.offset {
                let t = ((fraction - a.offset) / (b.offset - a.offset)).clamp(0.0, 1.0);
                return core::array::from_fn(|i| {
                    (f32::from(a.rgba[i]) + (f32::from(b.rgba[i]) - f32::from(a.rgba[i])) * t + 0.5)
                        as u8
                });
            }
        }
        gradient.stops.last().map_or([0; 4], |stop| stop.rgba)
    }
}
