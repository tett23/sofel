//! Compare positioned SVG pages with the fixed HarfBuzz output (sofel ADR 0074).
use quick_xml::{Reader, XmlVersion, events::Event};
use sofel_font::Font;
use sofel_geom::{Color, GlyphExtents, Image, ImageFormat, Point, Rect, Transform};
use sofel_outline::{Outlines, Scale, Scratch};
use sofel_vector::SvgPage;
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
enum Node {
    Start(String, BTreeMap<String, String>),
    End,
    Content(String),
}

fn nodes(xml: &str) -> Result<Vec<Node>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().expand_empty_elements = true;
    let mut scene = Vec::new();
    let mut depth = 0_usize;
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        match event {
            Event::Start(tag) => {
                let name = tag.name().as_ref().to_owned();
                if depth == 0 && (!scene.is_empty() || name != "svg") {
                    return Err("expected a single svg root".into());
                }
                let mut attrs = BTreeMap::new();
                for attr in tag.attributes() {
                    let attr = attr.map_err(|e| e.to_string())?;
                    let key = attr.key.as_ref();
                    let value = attr
                        .normalized_value(XmlVersion::Implicit1_0)
                        .map_err(|e| e.to_string())?;
                    attrs.insert(key.to_owned(), value.into_owned());
                }
                depth += 1;
                scene.push(Node::Start(name, attrs));
            }
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or("unmatched end tag")?;
                scene.push(Node::End);
            }
            Event::Text(text) if text.as_ref().bytes().all(|b| b.is_ascii_whitespace()) => (),
            Event::Text(text) if depth > 0 => scene.push(Node::Content(
                text.xml_content(XmlVersion::Implicit1_0).into_owned(),
            )),
            Event::Eof if depth == 0 && !scene.is_empty() => return Ok(scene),
            Event::Eof => return Err("incomplete SVG document".into()),
            other => return Err(format!("unexpected XML event {other:?}")),
        }
    }
}

/// Compares either the viewport or every element in paint order.
pub fn compare(expected: &str, actual: &str, observation: &str) -> Result<(), String> {
    let expected = nodes(expected)?;
    let actual = nodes(actual)?;
    match observation {
        "page-size" => {
            let (Some(Node::Start(_, want)), Some(Node::Start(_, got))) =
                (expected.first(), actual.first())
            else {
                return Err("missing svg root".into());
            };
            for attr in ["viewBox", "width", "height"] {
                let want = want
                    .get(attr)
                    .ok_or_else(|| format!("reference missing {attr}"))?;
                if got.get(attr) != Some(want) {
                    return Err(format!("{attr}: expected {want}, got {:?}", got.get(attr)));
                }
            }
        }
        "scene-tree" => {
            for index in 0..expected.len().max(actual.len()) {
                if expected.get(index) != actual.get(index) {
                    return Err(format!(
                        "element {index}: expected {:?}, got {:?}",
                        expected.get(index),
                        actual.get(index)
                    ));
                }
            }
        }
        other => return Err(format!("unknown output-page observation {other}")),
    }
    Ok(())
}

fn render(case: &str) -> Result<(&'static str, String), String> {
    let mut page = SvgPage::new(0, -2048, 2048, 2048, None);
    let expected = match case {
        "empty" => include_str!("../../../conformance/svg-page/empty.svg"),
        "single" | "translucent-overlap" => {
            let bytes = std::fs::read(
                crate::paths::testdata().join("harfbuzz/test/api/fonts/Roboto-Regular.abc.ttf"),
            )
            .map_err(|e| format!("Roboto-Regular.abc.ttf: {e}"))?;
            let font = Font::new(&bytes, 0).map_err(|e| format!("{e:?}"))?;
            let outlines = Outlines::new(font.default_instance());
            let mut scratch = Scratch::default();
            let mut draw = |ch: char, dx, dy, fill| {
                let gid = font
                    .nominal_glyph(u32::from(ch))
                    .ok_or_else(|| format!("missing glyph {ch}"))?;
                outlines
                    .load(gid, Scale::Unscaled, &mut scratch)
                    .map_err(|e| format!("{e:?}"))?;
                page.fill_path(
                    Transform {
                        dx,
                        dy,
                        ..Transform::scale(0.5, 0.5)
                    },
                    fill,
                    |pen| scratch.decompose(Point::default(), 2, pen),
                )
                .map_err(|e| format!("{e:?}"))
            };
            draw('a', 0.0, 0.0, Color::new(255, 0, 0, 51))?;
            if case == "single" {
                include_str!("../../../conformance/svg-page/single.svg")
            } else {
                draw('b', 160.0, 64.0, Color::new(0, 0, 255, 102))?;
                include_str!("../../../conformance/svg-page/translucent-overlap.svg")
            }
        }
        "clipped-image" => {
            let image = Image {
                format: ImageFormat::Png,
                data: include_bytes!("../../../conformance/svg-page/red.png"),
                width: 1,
                height: 1,
                slant: 0.0,
                extents: Some(GlyphExtents {
                    x_bearing: 0,
                    y_bearing: 4,
                    width: 4,
                    height: -4,
                }),
            };
            let clip = Rect {
                x_min: 2.0,
                y_min: 4.0,
                x_max: 12.0,
                y_max: 14.0,
            };
            let transform = Transform {
                dx: 1.0,
                dy: 2.0,
                ..Transform::scale(2.0, 3.0)
            };
            if !page.draw_image(&image, transform, Some(clip)) {
                return Err("PNG image was rejected".into());
            }
            include_str!("../../../conformance/svg-page/clipped-image.svg")
        }
        other => return Err(format!("unknown output-page case {other}")),
    };
    Ok((expected, page.finish()))
}

/// Renders the requested page and checks it against a committed reference.
pub fn observe(case: &str, observation: &str) -> Result<(), String> {
    let (reference, actual) = render(case)?;
    compare(reference, &actual, observation)
}

/// Element and attribute values in paint order, independent of XML indentation.
pub fn measurement(case: &str) -> Result<String, String> {
    let (_, actual) = render(case)?;
    let normalized = nodes(&actual)?;
    Ok(format!(
        "{} XML events; fnv1a64={:016x}",
        normalized.len(),
        crate::svg_input::fingerprint(format!("{normalized:?}").as_bytes())
    ))
}
