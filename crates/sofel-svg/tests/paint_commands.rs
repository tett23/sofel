//! The selected SVG glyph sends outlines, transforms and resolved fills to a Painter (sofel ADR 0066 I3).
use sofel_geom::{Brush, Color, CompositeMode, Image, Painter, Rect, Transform};
use sofel_svg::{ClipPath, OutlinePainter, Shape, parse_glyph};

#[derive(Default)]
struct Trace {
    paints: Vec<Color>,
    path_counts: Vec<usize>,
    depth: i32,
    transforms: usize,
}

impl<'a> OutlinePainter<'a> for Trace {
    fn push_svg_clip(&mut self, _: &'a Shape, paths: &'a [ClipPath]) {
        self.path_counts.push(paths.len());
        self.depth += 1;
    }
}
impl Painter for Trace {
    fn push_transform(&mut self, _: Transform) {
        self.depth += 1;
        self.transforms += 1;
    }
    fn pop_transform(&mut self) {
        self.depth -= 1;
    }
    fn push_clip_glyph(&mut self, _: u32) {
        unreachable!()
    }
    fn push_clip_rectangle(&mut self, _: Rect) {
        unreachable!()
    }
    fn pop_clip(&mut self) {
        self.depth -= 1;
    }
    fn fill(&mut self, b: &Brush<'_>) {
        match b {
            Brush::Solid(s) => self.paints.push(s.color),
            Brush::Linear { line, .. } => {
                self.paints.extend(line.stops.iter().map(|s| s.color.color))
            }
            _ => unreachable!(),
        }
    }
    fn image(&mut self, _: &Image<'_>) -> bool {
        unreachable!()
    }
    fn push_group(&mut self, _: CompositeMode) {
        unreachable!()
    }
    fn pop_group(&mut self, _: CompositeMode) {
        unreachable!()
    }
}
#[test]
fn i3_selected_shapes_emit_balanced_clips_and_resolved_painter_brushes() {
    let xml = br##"<svg viewBox="0 0 4 4"><defs><linearGradient id="g"><stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient><clipPath id="c"><rect width="2" height="2"/></clipPath><path id="p" d="M0 0 L3 0 L0 3 Z" fill="context-fill"/></defs><g id="glyph1" transform="translate(1,0)" clip-path="url(#c)"><use href="#p"/><rect width="3" height="3" fill="url(#g)"/></g><rect id="glyph2" width="3" height="3" fill="white"/></svg>"##;
    let scene = parse_glyph(xml, 1, 1000, [5, 6, 7, 255]).expect("valid SVG");
    let mut trace = Trace::default();
    assert!(scene.paint(&mut trace));
    assert_eq!(
        trace.paints,
        [
            Color::new(5, 6, 7, 255),
            Color::new(255, 0, 0, 255),
            Color::new(0, 0, 255, 255)
        ]
    );
    assert_eq!(trace.path_counts, [1, 1]);
    assert!(trace.transforms > 0);
    assert_eq!(trace.depth, 0);
}
