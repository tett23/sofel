//! Palettes, painting, recording and glyph boxes on fonts built with the
//! ADR 0007 builders (ADR 0023).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic
)]

#[path = "../../sofel-read/tests/support/build.rs"]
#[allow(dead_code)]
mod build;

use build::*;
use sofel_color::{
    ColorFormat, ColorFormats, PaintCommand, PaintOptions, Painted, Palette, RecordedBrush,
    Recording, Scratch, base_paint, clip_box, color_formats, find_palette, glyph_color_format,
    glyph_extents, layer_paint, layer_paint_count, layers, paint, paint_recording,
    palette_color_name_id, palette_colors, palette_count, palette_flags, palette_name_id,
};
use sofel_outline::Outlines;
use sofel_outline::font::Font;
use sofel_outline::font::geom::{
    Brush, Color, CompositeMode, GlyphExtents, Image, Painter, Rect, Solid, Transform,
};

const RED: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [255, 0, 0, 255];
const HALF: [u8; 4] = [10, 20, 30, 128];

fn square(x0: i16, y0: i16, x1: i16, y1: i16) -> Vec<u8> {
    simple_glyph(
        &[&[
            Pt::On(x0, y0),
            Pt::On(x0, y1),
            Pt::On(x1, y1),
            Pt::On(x1, y0),
        ]],
        false,
        &[],
    )
}

/// A paint and its child right after it: `parent` has a 24-bit child
/// offset at byte 1 that is set to the parent's length.
fn with_child(mut parent: Vec<u8>, child: Vec<u8>) -> Vec<u8> {
    let n = parent.len() as u32;
    parent[1..4].copy_from_slice(&n.to_be_bytes()[1..]);
    parent.extend(child);
    parent
}

fn solid(palette: u16, alpha: i16) -> Vec<u8> {
    Buf::new().u8(2).u16(palette).i16(alpha).done()
}

fn paint_glyph(g: u16) -> Vec<u8> {
    Buf::new().u8(10).u24(0).u16(g).done()
}

/// The test font:
/// - glyph 1: a square, 100..500 × 0..400; glyph 2: 0..1000 × 0..1000;
/// - glyph 3: COLRv0 layers (1, palette 0), (2, foreground), (1, palette 5);
/// - glyph 4: PaintGlyph(1) of PaintSolid(1, alpha 0.5);
/// - glyph 5: PaintColrGlyph(5), a cycle;
/// - glyph 6: PaintGlyph(1) of a linear gradient, with a clip box;
/// - glyph 7: a PaintSolid with nothing to clip it;
/// - glyph 8: PaintComposite(screen) of a solid over PaintGlyph(2);
/// - glyph 9: PaintColrLayers of layers 0 (itself) and 1 (glyph 4's paint).
fn font_bytes() -> Vec<u8> {
    let glyphs = vec![
        Vec::new(),
        square(100, 0, 500, 400),
        square(0, 0, 1000, 1000),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ];
    let p0 = with_child(paint_glyph(1), solid(1, 0x2000));
    let p1 = Buf::new().u8(11).u16(5).done();
    let line = Buf::new()
        .u8(1)
        .u16(2)
        .i16(0x4000)
        .u16(0xFFFF)
        .i16(0x4000)
        .i16(0)
        .u16(0)
        .i16(0x4000)
        .done();
    let mut linear = Buf::new()
        .u8(4)
        .u24(16)
        .i16(0)
        .i16(0)
        .i16(100)
        .i16(0)
        .i16(0)
        .i16(100)
        .done();
    linear.extend(line);
    let p2 = with_child(paint_glyph(1), linear);
    let p3 = solid(0, 0x4000);
    let mut p4 = Buf::new().u8(32).u24(8).u8(13).u24(13).done();
    p4.extend(solid(1, 0x4000));
    p4.extend(with_child(paint_glyph(2), solid(0, 0x4000)));
    let p5 = Buf::new().u8(1).u8(2).u32(0).done();
    let colr = colr_v1(
        &[(3, 0, 3)],
        &[(1, 0), (2, 0xFFFF), (1, 5)],
        &[p0, p1, p2, p3, p4, p5],
        &[(4, 0), (5, 1), (6, 2), (7, 3), (8, 4), (9, 5)],
        &[5, 0],
        &[(
            6,
            6,
            Buf::new().u8(1).i16(0).i16(-10).i16(600).i16(500).done(),
        )],
    );
    let cpal = cpal(
        2,
        &[0, 2],
        &[RED, GREEN, BLUE, HALF],
        Some((Some(vec![1, 2]), Some(vec![300, 301]), Some(vec![400, 401]))),
    );
    glyph_font_with(
        &glyphs,
        false,
        false,
        vec![(b"COLR", colr), (b"CPAL", cpal)],
    )
}

fn color(b: [u8; 4]) -> Color {
    Color::new(b[2], b[1], b[0], b[3])
}

fn record<'a>(font: &Font<'a>, glyph: u32, options: &PaintOptions<'a>) -> (Painted, Recording<'a>) {
    let mut rec = Recording::new();
    let p = paint_recording(font, glyph, options, &mut Scratch::default(), &mut rec);
    (p, rec)
}

#[test]
fn palettes() {
    let bytes = font_bytes();
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(palette_count(&font), 2);
    assert_eq!(
        (palette_name_id(&font, 1), palette_name_id(&font, 2)),
        (301, 0xFFFF)
    );
    assert_eq!((palette_flags(&font, 0), palette_flags(&font, 1)), (1, 2));
    assert_eq!(palette_color_name_id(&font, 1), 401);
    assert_eq!(
        palette_colors(&font, 1).collect::<Vec<_>>(),
        [color(BLUE), color(HALF)]
    );
    assert_eq!(find_palette(&font, 1), Some(0));
    assert_eq!(find_palette(&font, 2), Some(1));
    assert_eq!(find_palette(&font, 3), None);
    // Version 0: no flags.
    let v0 = cpal(1, &[0], &[RED], None);
    let bytes = glyph_font_with(&[Vec::new()], false, false, vec![(b"CPAL", v0)]);
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(find_palette(&font, 1), None);
    assert_eq!(palette_flags(&font, 0), 0);
}

#[test]
fn colr_v0_layers_and_colors() {
    let bytes = font_bytes();
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(
        layers(&font, 3).collect::<Vec<_>>(),
        [(1, Some(0)), (2, None), (1, Some(5))]
    );
    let fg = Color::new(9, 8, 7, 200);
    let options = PaintOptions {
        palette: Palette::Font(7),
        foreground: fg,
        ppem: 0,
    };
    let (p, rec) = record(&font, 3, &options);
    assert_eq!(p, Painted::ColrV0);
    let fills: Vec<(u32, Solid)> = rec
        .commands()
        .iter()
        .map(|c| match c {
            PaintCommand::FillGlyph(g, s) => (*g, *s),
            other => panic!("{other:?}"),
        })
        .collect();
    // Palette 7 does not exist: palette 0. Index 5 is past it: transparent black.
    assert_eq!(fills[0].1.color, color(RED));
    assert_eq!(fills[0].1.source.index, Some(0));
    assert_eq!(fills[1].1.color, fg);
    assert_eq!(fills[1].1.source.index, None);
    assert_eq!(fills[2].1.color, Color::default());
    // A custom palette.
    let custom = [Color::new(1, 2, 3, 4)];
    let options = PaintOptions {
        palette: Palette::Custom(&custom),
        ..PaintOptions::default()
    };
    let (_, rec) = record(&font, 3, &options);
    assert_eq!(
        rec.commands()[0],
        PaintCommand::FillGlyph(
            1,
            Solid {
                color: custom[0],
                source: fills[0].1.source,
            }
        )
    );
}

#[test]
fn colr_v1_solid_glyph_alpha_and_palette() {
    let bytes = font_bytes();
    let font = Font::new(&bytes, 0).unwrap();
    let options = PaintOptions {
        palette: Palette::Font(1),
        ..PaintOptions::default()
    };
    let (p, rec) = record(&font, 4, &options);
    assert_eq!(p, Painted::ColrV1);
    match rec.commands() {
        [PaintCommand::FillGlyph(1, s)] => {
            // HALF's alpha 128 × 0.5 = 64.
            assert_eq!(s.color, Color::new(30, 20, 10, 64));
            assert_eq!(s.source.alpha, 0.5);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn colr_v1_cycles_clips_and_bounds() {
    let bytes = font_bytes();
    let font = Font::new(&bytes, 0).unwrap();
    let o = PaintOptions::default();
    // A glyph that paints itself: nothing.
    let (p, rec) = record(&font, 5, &o);
    assert_eq!((p, rec.commands().len()), (Painted::ColrV1, 0));
    // Unbounded: nothing.
    let (p, rec) = record(&font, 7, &o);
    assert_eq!((p, rec.commands().len()), (Painted::ColrV1, 0));
    // The clip box and the gradient.
    let (_, rec) = record(&font, 6, &o);
    let c = rec.commands();
    assert_eq!(
        c[0],
        PaintCommand::PushClipRectangle(Rect {
            x_min: 0.0,
            y_min: -10.0,
            x_max: 600.0,
            y_max: 500.0
        })
    );
    assert_eq!(c[1], PaintCommand::PushClipGlyph(1));
    let PaintCommand::Fill(b @ RecordedBrush::Linear { .. }) = &c[2] else {
        panic!("{:?}", c[2]);
    };
    let Brush::Linear { line, p1, .. } = rec.brush(b) else {
        panic!()
    };
    assert_eq!(p1.x, 100.0);
    assert_eq!(line.stops.len(), 2);
    assert_eq!(line.stops[0].offset, 1.0);
    assert_eq!(line.stops[0].color.source.index, None);
    assert_eq!(&c[3..], [PaintCommand::PopClip, PaintCommand::PopClip]);
    // Layers that reach themselves stop at the cycle.
    let (_, rec) = record(&font, 9, &o);
    assert!(
        rec.commands()
            .iter()
            .any(|c| matches!(c, PaintCommand::FillGlyph(1, _)))
    );
}

#[test]
fn colr_v1_composite() {
    let bytes = font_bytes();
    let font = Font::new(&bytes, 0).unwrap();
    let (_, rec) = record(&font, 8, &PaintOptions::default());
    let kinds: Vec<&str> = rec
        .commands()
        .iter()
        .map(|c| match c {
            PaintCommand::PushGroup(CompositeMode::SrcOver) => "group",
            PaintCommand::PushGroup(CompositeMode::Screen) => "screen",
            PaintCommand::PopGroup(_) => "pop",
            PaintCommand::FillGlyph(2, _) => "fill 2",
            PaintCommand::Fill(_) => "fill",
            _ => "?",
        })
        .collect();
    assert_eq!(kinds, ["group", "fill 2", "screen", "fill", "pop", "pop"]);
}

/// Logs commands; `color_glyph` answers `answer`.
struct Log(Vec<String>, bool);

impl Painter for Log {
    fn push_transform(&mut self, t: Transform) {
        self.0.push(format!("T {t:?}"));
    }
    fn pop_transform(&mut self) {
        self.0.push("/T".into());
    }
    fn push_clip_glyph(&mut self, g: u32) {
        self.0.push(format!("clip {g}"));
    }
    fn push_clip_rectangle(&mut self, r: Rect) {
        self.0.push(format!("rect {r:?}"));
    }
    fn pop_clip(&mut self) {
        self.0.push("/clip".into());
    }
    fn fill(&mut self, b: &Brush<'_>) {
        self.0.push(format!("fill {b:?}"));
    }
    fn fill_glyph(&mut self, g: u32, c: &Solid) {
        self.0.push(format!("fill_glyph {g} {c:?}"));
    }
    fn image(&mut self, i: &Image<'_>) -> bool {
        self.0
            .push(format!("image {:?} {}", i.format, i.data.len()));
        true
    }
    fn push_group(&mut self, m: CompositeMode) {
        self.0.push(format!("group {m:?}"));
    }
    fn pop_group(&mut self, m: CompositeMode) {
        self.0.push(format!("/group {m:?}"));
    }
    fn color_glyph(&mut self, g: u32) -> bool {
        self.0.push(format!("color_glyph {g}"));
        self.1
    }
}

#[test]
fn recording_replays_the_same_commands() {
    let bytes = font_bytes();
    let font = Font::new(&bytes, 0).unwrap();
    for g in 0..10 {
        let mut direct = Log(Vec::new(), false);
        let p = paint(
            &font,
            g,
            &PaintOptions::default(),
            &mut Scratch::default(),
            &mut direct,
        );
        let (q, rec) = record(&font, g, &PaintOptions::default());
        assert_eq!(p, q);
        let mut replayed = Log(Vec::new(), false);
        rec.replay(&mut replayed);
        assert_eq!(direct.0, replayed.0, "glyph {g}");
    }
}

#[test]
fn color_glyph_true_stops_the_walk() {
    // PaintColrGlyph(5) of glyph 5 is a cycle at once; build a glyph 10
    // whose paint is PaintColrGlyph(4).
    let bytes = font_bytes();
    let font = Font::new(&bytes, 0).unwrap();
    let mut log = Log(Vec::new(), true);
    paint(
        &font,
        5,
        &PaintOptions::default(),
        &mut Scratch::default(),
        &mut log,
    );
    assert!(log.0.is_empty());
}

#[test]
fn formats_and_raw_values() {
    let bytes = font_bytes();
    let font = Font::new(&bytes, 0).unwrap();
    let f = color_formats(&font);
    assert!(f.has(ColorFormats::COLR_V0 | ColorFormats::COLR_V1 | ColorFormats::CPAL));
    assert!(!f.has(ColorFormats::SVG));
    assert_eq!(glyph_color_format(&font, 3), Some(ColorFormat::ColrV0));
    assert_eq!(glyph_color_format(&font, 4), Some(ColorFormat::ColrV1));
    assert_eq!(glyph_color_format(&font, 1), None);
    let p = base_paint(&font, 4).unwrap();
    assert_eq!(p.format(), Some(10));
    let sofel_color::Paint::Glyph {
        paint: Some(child),
        glyph: 1,
    } = p.paint().unwrap()
    else {
        panic!()
    };
    assert_eq!(p.at(child).format(), Some(2));
    assert_eq!(clip_box(&font, 6).unwrap().y_min, -10);
    assert_eq!(layer_paint_count(&font), 2);
    assert_eq!(layer_paint(&font, 1).unwrap().offset(), p.offset());
}

#[test]
fn glyph_boxes() {
    let bytes = font_bytes();
    let font = Font::new(&bytes, 0).unwrap();
    let outlines = Outlines::new(&font);
    let mut s = Scratch::default();
    // COLRv0: the union of the layers.
    assert_eq!(
        glyph_extents(&outlines, 3, &mut s),
        Some(GlyphExtents {
            x_bearing: 0,
            y_bearing: 1000,
            width: 1000,
            height: -1000
        })
    );
    // COLRv1 with a clip box: the box.
    assert_eq!(
        glyph_extents(&outlines, 6, &mut s),
        Some(GlyphExtents {
            x_bearing: 0,
            y_bearing: 500,
            width: 600,
            height: -510
        })
    );
    // COLRv1 without: the painted box.
    assert_eq!(
        glyph_extents(&outlines, 4, &mut s),
        Some(GlyphExtents {
            x_bearing: 100,
            y_bearing: 400,
            width: 400,
            height: -400
        })
    );
    // Unbounded and cyclic glyphs: an empty box.
    assert_eq!(
        glyph_extents(&outlines, 7, &mut s),
        Some(GlyphExtents::default())
    );
    assert_eq!(
        glyph_extents(&outlines, 5, &mut s),
        Some(GlyphExtents::default())
    );
    // Not a color glyph: the outline's box.
    assert_eq!(glyph_extents(&outlines, 1, &mut s).unwrap().width, 400);
}

#[test]
fn bitmaps_and_svg() {
    let png = png_header(20, 30);
    let mut data = Buf::new().u16(3).u16(0);
    let image = Buf::new()
        .bytes(&[30, 20, 2, 25, 22])
        .u32(png.len() as u32)
        .bytes(&png)
        .done();
    data = data.bytes(&image);
    let loc = cblc(
        3,
        &[SizeSpec {
            ppem_x: 109,
            ppem_y: 109,
            bit_depth: 32,
            start_glyph: 1,
            end_glyph: 1,
            subtables: vec![(1, 1, index_offsets(1, 17, 4, &[0, image.len() as u32]))],
            ..SizeSpec::default()
        }],
    );
    let svg = svg_table(&[(2, 2, b"<svg/>".to_vec())]);
    let bytes = glyph_font_with(
        &[Vec::new(), Vec::new(), Vec::new()],
        false,
        false,
        vec![(b"CBDT", data.done()), (b"CBLC", loc), (b"SVG ", svg)],
    );
    let font = Font::new(&bytes, 0).unwrap();
    let mut log = Log(Vec::new(), false);
    assert_eq!(
        paint(
            &font,
            1,
            &PaintOptions::default(),
            &mut Scratch::default(),
            &mut log
        ),
        Painted::Cbdt
    );
    assert_eq!(log.0, ["image Png 29"]);
    let (_, rec) = record(&font, 1, &PaintOptions::default());
    let PaintCommand::Image(i) = &rec.commands()[0] else {
        panic!()
    };
    assert_eq!((i.width, i.height), (20, 30));
    // 1000 / 109 per pixel.
    assert_eq!(
        i.extents,
        Some(GlyphExtents {
            x_bearing: 18,
            y_bearing: 229,
            width: 183,
            height: -275
        })
    );
    let mut log = Log(Vec::new(), false);
    assert_eq!(
        paint(
            &font,
            2,
            &PaintOptions::default(),
            &mut Scratch::default(),
            &mut log
        ),
        Painted::Svg
    );
    assert_eq!(log.0, ["image Svg { gzip: false } 6"]);
    assert_eq!(sofel_color::svg_document(&font, 2).unwrap().data, b"<svg/>");
    assert_eq!(glyph_color_format(&font, 1), Some(ColorFormat::Cbdt));
    assert_eq!(glyph_color_format(&font, 2), Some(ColorFormat::Svg));
    assert!(color_formats(&font).has(ColorFormats::CBDT | ColorFormats::SVG));
}

#[test]
fn nesting_stops_at_64() {
    // Glyphs 2..=41: a PaintTranslate of PaintColrGlyph(next); glyph 41
    // paints PaintGlyph(1) of a solid. Each glyph is two levels deep, so
    // the walk stops after 32 translations (HarfBuzz's 64 levels), while
    // each paint alone is shallow.
    let mut paints = Vec::new();
    let mut base = Vec::new();
    for g in 2u16..41 {
        let mut p = Buf::new().u8(14).u24(8).i16(1).i16(0).done();
        p.extend(Buf::new().u8(11).u16(g + 1).done());
        base.push((g, paints.len()));
        paints.push(p);
    }
    base.push((41, paints.len()));
    paints.push(with_child(paint_glyph(1), solid(0, 0x4000)));
    let colr = colr_v1(&[], &[], &paints, &base, &[], &[]);
    let cpal = cpal(1, &[0], &[RED], None);
    let mut glyphs = vec![Vec::new(); 42];
    glyphs[1] = square(0, 0, 10, 10);
    let bytes = glyph_font_with(
        &glyphs,
        false,
        false,
        vec![(b"COLR", colr), (b"CPAL", cpal)],
    );
    let font = Font::new(&bytes, 0).unwrap();
    let (p, rec) = record(&font, 2, &PaintOptions::default());
    assert_eq!(p, Painted::ColrV1);
    let pushes = rec
        .commands()
        .iter()
        .filter(|c| matches!(c, PaintCommand::PushTransform(_)))
        .count();
    assert_eq!(pushes, 32);
    assert!(
        !rec.commands()
            .iter()
            .any(|c| matches!(c, PaintCommand::FillGlyph(..)))
    );
    // From glyph 30, 12 levels deep: the solid is reached.
    let (_, rec) = record(&font, 30, &PaintOptions::default());
    assert!(
        rec.commands()
            .iter()
            .any(|c| matches!(c, PaintCommand::FillGlyph(1, _)))
    );
}
