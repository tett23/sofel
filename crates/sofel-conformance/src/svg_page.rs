//! Compare positioned SVG pages with the fixed HarfBuzz output (sofel ADR 0074).
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

/// The elements, attributes and text of an SVG document with one `svg`
/// root. Empty elements become a start and an end; whitespace-only text is
/// dropped. Comments, declarations, processing instructions, CDATA and
/// document types are errors, as are mismatched or missing end tags.
fn nodes(xml: &str) -> Result<Vec<Node>, String> {
    let mut scene = Vec::new();
    let mut open: Vec<String> = Vec::new();
    let mut rest = xml;
    while !rest.is_empty() {
        let Some(after) = rest.strip_prefix('<') else {
            let end = rest.find('<').unwrap_or(rest.len());
            let (raw, tail) = rest.split_at(end);
            rest = tail;
            if raw.bytes().all(|b| b.is_ascii_whitespace()) {
                continue;
            }
            if open.is_empty() {
                return Err("text outside the svg root".into());
            }
            scene.push(Node::Content(decode(&line_ends(raw), false)?));
            continue;
        };
        if let Some(after) = after.strip_prefix('/') {
            let end = after.find('>').ok_or("unterminated end tag")?;
            let (name, tail) = after.split_at(end);
            rest = tail.get(1..).unwrap_or_default();
            if open.pop().as_deref() != Some(name.trim_end()) {
                return Err(format!("unmatched end tag {name}"));
            }
            scene.push(Node::End);
            continue;
        }
        if after.starts_with(['!', '?']) {
            return Err(format!(
                "unexpected XML markup {:?}",
                after.get(..8).unwrap_or(after)
            ));
        }
        let name_end = after
            .find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
            .ok_or("unterminated start tag")?;
        let (name, mut tail) = after.split_at(name_end);
        if name.is_empty() {
            return Err("empty element name".into());
        }
        if open.is_empty() && (!scene.is_empty() || name != "svg") {
            return Err("expected a single svg root".into());
        }
        let mut attrs = BTreeMap::new();
        let empty = loop {
            tail = tail.trim_start_matches(|c: char| c.is_ascii_whitespace());
            if let Some(t) = tail.strip_prefix("/>") {
                tail = t;
                break true;
            }
            if let Some(t) = tail.strip_prefix('>') {
                tail = t;
                break false;
            }
            let eq = tail.find('=').ok_or("attribute without a value")?;
            let (key, t) = tail.split_at(eq);
            let key = key.trim_end();
            if key.is_empty() || key.contains(|c: char| c.is_ascii_whitespace()) {
                return Err(format!("bad attribute name {key:?}"));
            }
            let t = t
                .get(1..)
                .unwrap_or_default()
                .trim_start_matches(|c: char| c.is_ascii_whitespace());
            let quote = t
                .chars()
                .next()
                .filter(|&q| q == '"' || q == '\'')
                .ok_or("unquoted attribute value")?;
            let t = t.get(1..).unwrap_or_default();
            let close = t.find(quote).ok_or("unterminated attribute value")?;
            let (raw, t) = t.split_at(close);
            if raw.contains('<') {
                return Err("< in an attribute value".into());
            }
            let value = decode(&line_ends(raw), true)?;
            if attrs.insert(key.to_owned(), value).is_some() {
                return Err(format!("duplicate attribute {key}"));
            }
            tail = t.get(1..).unwrap_or_default();
        };
        rest = tail;
        scene.push(Node::Start(name.to_owned(), attrs));
        if empty {
            scene.push(Node::End);
        } else {
            open.push(name.to_owned());
        }
    }
    if open.is_empty() && !scene.is_empty() {
        Ok(scene)
    } else {
        Err("incomplete SVG document".into())
    }
}

/// XML's end-of-line handling: CR LF and lone CR become LF.
fn line_ends(raw: &str) -> String {
    raw.replace("\r\n", "\n").replace('\r', "\n")
}

/// Expands the five predefined entities and character references. In
/// attribute values, tab and LF (after [`line_ends`]) become spaces before
/// the references are expanded, as XML's attribute-value normalization does.
fn decode(text: &str, attribute: bool) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        let (plain, tail) = rest.split_at(at);
        push_plain(&mut out, plain, attribute);
        let end = tail.find(';').ok_or("unterminated reference")?;
        let name = tail.get(1..end).unwrap_or_default();
        let c = match name {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = if let Some(hex) = name.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = name.strip_prefix('#') {
                    dec.parse().ok()
                } else {
                    None
                };
                code.and_then(char::from_u32)
                    .ok_or_else(|| format!("unknown reference &{name};"))?
            }
        };
        out.push(c);
        rest = tail.get(end.saturating_add(1)..).unwrap_or_default();
    }
    push_plain(&mut out, rest, attribute);
    Ok(out)
}

fn push_plain(out: &mut String, plain: &str, attribute: bool) {
    if attribute {
        out.extend(
            plain
                .chars()
                .map(|c| if c == '\t' || c == '\n' { ' ' } else { c }),
        );
    } else {
        out.push_str(plain);
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

#[cfg(test)]
mod tests {
    use super::{Node, nodes};
    use std::collections::BTreeMap;

    fn start(name: &str, attrs: &[(&str, &str)]) -> Node {
        Node::Start(
            name.to_owned(),
            attrs
                .iter()
                .map(|&(k, v)| (k.to_owned(), v.to_owned()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    #[test]
    fn empty_elements_expand_and_both_quotes_read() {
        assert_eq!(
            nodes(r#"<svg><a/><b x='1' y="2" ></b><c z = "3" /></svg>"#),
            Ok(vec![
                start("svg", &[]),
                start("a", &[]),
                Node::End,
                start("b", &[("x", "1"), ("y", "2")]),
                Node::End,
                start("c", &[("z", "3")]),
                Node::End,
                Node::End,
            ])
        );
    }

    #[test]
    fn references_are_decoded() {
        assert_eq!(
            nodes(r#"<svg t="a&amp;b&lt;&gt;&quot;&apos;&#65;&#x42;">x &gt; &#x263A;</svg>"#),
            Ok(vec![
                start("svg", &[("t", "a&b<>\"'AB")]),
                Node::Content("x > \u{263a}".to_owned()),
                Node::End,
            ])
        );
    }

    #[test]
    fn attribute_whitespace_and_text_line_ends_are_normalized() {
        assert_eq!(
            nodes("<svg t=\"a\tb\nc\r\nd\re&#10;f\">g\r\nh\ri</svg>"),
            Ok(vec![
                start("svg", &[("t", "a b c d e\nf")]),
                Node::Content("g\nh\ni".to_owned()),
                Node::End,
            ])
        );
    }

    #[test]
    fn whitespace_only_text_is_skipped() {
        assert_eq!(
            nodes("<svg>\n  <a/>\n</svg>\n"),
            Ok(vec![
                start("svg", &[]),
                start("a", &[]),
                Node::End,
                Node::End
            ])
        );
    }

    #[test]
    fn other_markup_and_broken_documents_are_errors() {
        for xml in [
            "<svg><!-- c --></svg>",
            "<?xml version=\"1.0\"?><svg/>",
            "<svg><![CDATA[x]]></svg>",
            "<!DOCTYPE svg><svg/>",
            "<g/>",
            "<svg/><svg/>",
            "<svg><a></b></svg>",
            "<svg>",
            "",
            "x<svg/>",
            "<svg>&nbsp;</svg>",
            "<svg a=\"1\" a=\"2\"/>",
            "<svg a=1/>",
        ] {
            assert!(nodes(xml).is_err(), "{xml:?}");
        }
    }
}
