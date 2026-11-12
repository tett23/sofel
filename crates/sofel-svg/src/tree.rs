//! Resolve an OpenType glyph ID in its SVG tree, including inherited
//! presentation attributes and local references. No resource loader or
//! script engine is involved.
use super::xml::{Document, Node};
use super::{
    ClipElement, ClipPath, GlyphScene, Gradient, GradientKind, IDENTITY, ParseError, Scene, Shape,
    ShapeKind, Stop, attr, color, gradient, number, transform,
};
use alloc::string::String;
use alloc::{collections::BTreeMap, format, vec::Vec};

type Matrix = [f32; 6];

/// Resolve one glyph inside the font's square viewport. The foreground is
/// straight RGBA and supplies the embedding context's paint.
///
/// # Errors
/// Returns an error for malformed documents or excessive reference expansion.
pub fn parse_glyph(
    data: &[u8],
    glyph: u32,
    upem: u16,
    foreground: [u8; 4],
) -> Result<GlyphScene, ParseError> {
    parse(data, glyph, Some(upem), foreground)
}

// SVG matrix multiplication, applying the local transform before the parent.
fn compose([a, b, c, d, e, f]: Matrix, [g, h, i, j, k, l]: Matrix) -> Matrix {
    [
        a * g + c * h,
        b * g + d * h,
        a * i + c * j,
        b * i + d * j,
        a * k + c * l + e,
        b * k + d * l + f,
    ]
}

fn viewport(node: &Node<'_>, upem: u16) -> Result<Matrix, ParseError> {
    let Some(value) = node.attr("viewBox") else {
        return Ok(IDENTITY);
    };
    let values: Vec<_> = value
        .split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .map(|part| part.parse::<f32>().map_err(|_| ParseError))
        .collect::<Result<_, _>>()?;
    let &[x, y, w, h] = values.as_slice() else {
        return Err(ParseError);
    };
    if !values.iter().all(|v| v.is_finite()) || w <= 0.0 || h <= 0.0 {
        return Err(ParseError);
    }
    let edge = f32::from(upem);
    let (sx, sy) = (edge / w, edge / h);
    let mut aspect = node
        .attr("preserveAspectRatio")
        .unwrap_or("xMidYMid meet")
        .split_ascii_whitespace();
    let align = aspect.next().unwrap_or("xMidYMid");
    if align == "none" {
        return Ok([sx, 0.0, 0.0, sy, -sx * x, -sy * y]);
    }
    let scale = if aspect.next() == Some("slice") {
        sx.max(sy)
    } else {
        sx.min(sy)
    };
    let ax = if align.starts_with("xMin") {
        0.0
    } else if align.starts_with("xMax") {
        1.0
    } else {
        0.5
    };
    let ay = if align.ends_with("YMin") {
        0.0
    } else if align.ends_with("YMax") {
        1.0
    } else {
        0.5
    };
    Ok([
        scale,
        0.0,
        0.0,
        scale,
        (edge - w * scale) * ax - x * scale,
        (edge - h * scale) * ay - y * scale,
    ])
}

#[derive(Clone)]
struct Style<'a> {
    fill: &'a str,
    opacity: f32,
    current_color: [u8; 4],
    matrix: Matrix,
    even_odd: bool,
    clips: Vec<usize>,
}

impl<'a> Style<'a> {
    fn enter(&self, node: &Node<'a>) -> Self {
        let mut child = self.clone();
        child.matrix = compose(self.matrix, transform(node.attr("transform")));
        if let Some(fill) = node.property("fill").filter(|v| *v != "inherit") {
            child.fill = fill;
        }
        if let Some(opacity) = node.property("fill-opacity").filter(|v| *v != "inherit") {
            child.opacity = number(Some(opacity), self.opacity).clamp(0.0, 1.0);
        }
        if let Some([r, g, b]) = node.property("color").and_then(color) {
            child.current_color = [r, g, b, 255];
        }
        if let Some(rule) = node.property("clip-rule").filter(|v| *v != "inherit") {
            child.even_odd = rule == "evenodd";
        }
        child
    }
}

fn local_fragment(value: &str) -> Option<&str> {
    value
        .strip_prefix("url(")?
        .strip_suffix(')')?
        .trim()
        .trim_matches(['\'', '"'])
        .strip_prefix('#')
}

// A solid paint is a constant color at every gradient sample position.
fn solid(rgba: [u8; 4]) -> Gradient {
    Gradient {
        id: String::new(),
        kind: GradientKind::Linear {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 0.0,
        },
        user_space: true,
        transform: IDENTITY,
        stops: alloc::vec![Stop { offset: 0.0, rgba }],
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Alpha is clamped to one byte.
fn alpha(value: u8, opacity: f32) -> u8 {
    (f32::from(value) * opacity.clamp(0.0, 1.0) + 0.5) as u8
}

struct Builder<'a> {
    scene: Scene,
    gradient_ids: BTreeMap<&'a str, usize>,
    solids: BTreeMap<[u8; 4], usize>,
    translucent: BTreeMap<(usize, u8), usize>,
    clip_paths: Vec<ClipPath>,
    remaining: usize,
    foreground: [u8; 4],
}

impl Builder<'_> {
    fn charge(&mut self, n: usize) -> Result<(), ParseError> {
        self.remaining = self.remaining.checked_sub(n).ok_or(ParseError)?;
        Ok(())
    }

    fn paint(&mut self, style: &Style<'_>) -> Result<Option<usize>, ParseError> {
        if let Some(id) = local_fragment(style.fill) {
            let Some(&source) = self.gradient_ids.get(id) else {
                return Ok(None);
            };
            let opacity = alpha(255, style.opacity);
            if opacity == 255 {
                return Ok(Some(source));
            }
            if let Some(&index) = self.translucent.get(&(source, opacity)) {
                return Ok(Some(index));
            }
            let n = self
                .scene
                .gradients
                .get(source)
                .ok_or(ParseError)?
                .stops
                .len();
            self.charge(n)?;
            let mut paint = self.scene.gradients.get(source).ok_or(ParseError)?.clone();
            for stop in &mut paint.stops {
                stop.rgba[3] = alpha(stop.rgba[3], style.opacity);
            }
            let index = self.scene.gradients.len();
            self.scene.gradients.push(paint);
            self.translucent.insert((source, opacity), index);
            return Ok(Some(index));
        }
        let mut rgba = match style.fill {
            "context-fill" => self.foreground,
            "currentColor" => style.current_color,
            "transparent" => [0; 4],
            name => {
                let Some([r, g, b]) = color(name) else {
                    return Ok(None);
                };
                [r, g, b, 255]
            }
        };
        rgba[3] = alpha(rgba[3], style.opacity);
        if let Some(&index) = self.solids.get(&rgba) {
            return Ok(Some(index));
        }
        let index = self.scene.gradients.len();
        self.scene.gradients.push(solid(rgba));
        self.solids.insert(rgba, index);
        Ok(Some(index))
    }
}

fn shape_kind(name: &str, attrs: &[(&str, &str)]) -> Option<ShapeKind> {
    let n = |key| number(attr(attrs, key), 0.0);
    match name {
        "rect" if n("width") > 0.0 && n("height") > 0.0 => Some(ShapeKind::Rect {
            x: n("x"),
            y: n("y"),
            width: n("width"),
            height: n("height"),
        }),
        "circle" if n("r") > 0.0 => Some(ShapeKind::Circle {
            cx: n("cx"),
            cy: n("cy"),
            r: n("r"),
        }),
        _ => None,
    }
}

struct Placed {
    shape: Shape,
    even_odd: bool,
    clips: Vec<usize>,
}

#[allow(clippy::too_many_arguments)] // The same recursion handles referenced shapes and clipping outlines.
fn walk<'a>(
    doc: &Document<'a>,
    i: usize,
    parent: &Style<'a>,
    b: &mut Builder<'a>,
    placed: &mut Vec<Placed>,
    clipping: bool,
    active: &mut Vec<usize>,
    depth: usize,
) -> Result<(), ParseError> {
    if depth >= 64 {
        return Err(ParseError);
    }
    b.charge(1)?;
    let node = doc.nodes.get(i).ok_or(ParseError)?;
    let mut style = parent.enter(node);
    if node.property("display") == Some("none") {
        return Ok(());
    }
    if let Some(value) = node.property("clip-path").filter(|v| *v != "none") {
        if clipping {
            return Err(ParseError);
        }
        let Some(&clip) = local_fragment(value).and_then(|id| doc.ids.get(id)) else {
            return Ok(());
        };
        let clip_node = doc.nodes.get(clip).ok_or(ParseError)?;
        if clip_node.name != "clipPath"
            || clip_node.attr("clipPathUnits") == Some("objectBoundingBox")
        {
            return Err(ParseError);
        }
        let mut outlines = Vec::new();
        let mut clip_style = style.clone();
        clip_style.clips.clear();
        walk(
            doc,
            clip,
            &clip_style,
            b,
            &mut outlines,
            true,
            active,
            depth.saturating_add(1),
        )?;
        style.clips.push(b.clip_paths.len());
        b.clip_paths.push(
            outlines
                .into_iter()
                .map(|p| ClipElement {
                    shape: p.shape,
                    even_odd: p.even_odd,
                })
                .collect(),
        );
    }
    match node.name {
        "svg" | "g" | "clipPath" => {
            if node.name == "clipPath" && !clipping {
                return Ok(());
            }
            for child in doc.children(i) {
                walk(doc, child, &style, b, placed, clipping, active, depth.saturating_add(1))?;
            }
        }
        "use" => {
            let Some(&target) = node
                .attr("href")
                .or_else(|| node.attr("xlink:href"))
                .and_then(|v| v.strip_prefix('#'))
                .and_then(|id| doc.ids.get(id))
            else {
                return Ok(());
            };
            if !active.contains(&target) {
                style.matrix = compose(
                    style.matrix,
                    [
                        1.0,
                        0.0,
                        0.0,
                        1.0,
                        number(node.attr("x"), 0.0),
                        number(node.attr("y"), 0.0),
                    ],
                );
                active.push(target);
                walk(doc, target, &style, b, placed, clipping, active, depth.saturating_add(1))?;
                active.pop();
            }
        }
        name => {
            if let Some(kind) = shape_kind(name, &node.attrs) {
                let paint = if clipping { Some(0) } else { b.paint(&style)? };
                if let Some(gradient) = paint {
                    placed.push(Placed {
                        shape: Shape {
                            kind,
                            gradient,
                            transform: style.matrix,
                        },
                        even_odd: style.even_odd,
                        clips: style.clips,
                    });
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn parse(
    data: &[u8],
    glyph: u32,
    upem: Option<u16>,
    foreground: [u8; 4],
) -> Result<GlyphScene, ParseError> {
    let doc = Document::parse(data)?;
    let Some(&target) = doc.ids.get(format!("glyph{glyph}").as_str()) else {
        return Ok(GlyphScene::default());
    };
    let mut b = Builder {
        scene: Scene::default(),
        gradient_ids: BTreeMap::new(),
        solids: BTreeMap::new(),
        translucent: BTreeMap::new(),
        clip_paths: Vec::new(),
        remaining: 65_536,
        foreground,
    };
    for (i, node) in doc.nodes.iter().enumerate() {
        if !matches!(node.name, "linearGradient" | "radialGradient") {
            continue;
        }
        let Some(id) = node.attr("id") else {
            continue;
        };
        let mut g = gradient(node.name, &node.attrs);
        for child in doc
            .children(i)
            .filter_map(|j| doc.nodes.get(j))
            .filter(|n| n.name == "stop")
        {
            let [r, gc, bc] = child
                .property("stop-color")
                .and_then(color)
                .unwrap_or([0; 3]);
            let a = alpha(255, number(child.property("stop-opacity"), 1.0));
            g.stops.push(Stop {
                offset: number(child.attr("offset"), 0.0),
                rgba: [r, gc, bc, a],
            });
        }
        b.gradient_ids.insert(id, b.scene.gradients.len());
        b.scene.gradients.push(g);
    }
    let root = doc.nodes.first().ok_or(ParseError)?;
    let matrix = upem.map_or(Ok(IDENTITY), |u| viewport(root, u))?;
    let mut style = Style {
        fill: "black",
        opacity: 1.0,
        current_color: foreground,
        matrix,
        even_odd: false,
        clips: Vec::new(),
    };
    let mut ancestors = Vec::new();
    let mut next = doc.nodes.get(target).and_then(|node| node.parent);
    while let Some(i) = next {
        let n = doc.nodes.get(i).ok_or(ParseError)?;
        ancestors.push(n);
        next = n.parent;
    }
    for n in ancestors.into_iter().rev() {
        style = style.enter(n);
    }
    let mut placed = Vec::new();
    walk(
        &doc,
        target,
        &style,
        &mut b,
        &mut placed,
        false,
        &mut Vec::new(),
        0,
    )?;
    let mut clips = Vec::new();
    for p in placed {
        let mut shape_clips = Vec::new();
        for i in p.clips {
            let count = b.clip_paths.get(i).ok_or(ParseError)?.len();
            b.charge(count.saturating_add(1))?;
            shape_clips.push(b.clip_paths.get(i).ok_or(ParseError)?.clone());
        }
        clips.push(shape_clips);
        b.scene.shapes.push(p.shape);
    }
    Ok(GlyphScene {
        scene: b.scene,
        clips,
    })
}
