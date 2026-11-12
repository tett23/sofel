//! The basic HarfBuzz SVG page in test/vector (ADR 0068 I1).
#![allow(clippy::unwrap_used, clippy::cast_precision_loss)]
use sofel_font::Font;
use sofel_geom::{Color, Point, Transform};
use sofel_outline::{Outlines, Scale, Scratch};
use sofel_vector::SvgPage;

#[test]
fn i1_vector_page_positioned_abc_matches_harfbuzz() {
    let root = sofel_conformance::paths::testdata().join("harfbuzz/test");
    let bytes = std::fs::read(root.join("api/fonts/Roboto-Regular.abc.ttf")).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let instance = font.default_instance();
    let outlines = Outlines::new(instance);
    let mut scratch = Scratch::default();
    let mut page = SvgPage::new(-16, -1916, 3367, 2432, Some(Color::new(255, 255, 255, 255)));
    let mut x = 0.0;
    for ch in ['a', 'b', 'c'] {
        let gid = font.nominal_glyph(u32::from(ch)).unwrap();
        outlines.load(gid, Scale::Unscaled, &mut scratch).unwrap();
        let transform = Transform {
            dx: x,
            ..Transform::scale(0.5, 0.5)
        };
        page.fill_path(transform, Color::new(0, 0, 0, 255), |pen| {
            scratch.decompose(Point::default(), 2, pen)
        })
        .unwrap();
        x += instance.advance(gid) as f32;
    }
    let expected =
        std::fs::read_to_string(root.join("vector/data/expected/basic/default.svg")).unwrap();
    assert_eq!(page.finish(), expected);
}

#[test]
#[ignore = "requires Poppler; exercised by the Linux conformance job"]
fn i1_vector_page_pdf_quadratics_match_harfbuzz_pixels() {
    use sofel_geom::CompositeMode;
    use sofel_vector::PdfPage;
    let root = sofel_conformance::paths::testdata().join("harfbuzz/test");
    let bytes = std::fs::read(root.join("api/fonts/Roboto-Regular.abc.ttf")).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let instance = font.default_instance();
    let outlines = Outlines::new(instance);
    let mut scratch = Scratch::default();
    let mut page = PdfPage::new(
        -16,
        -16016,
        106752,
        76832,
        Some(Color::new(255, 255, 255, 255)),
    );
    let mut x = 0.0;
    for ch in ['a', 'b', 'c'] {
        let gid = font.nominal_glyph(u32::from(ch)).unwrap();
        outlines.load(gid, Scale::Unscaled, &mut scratch).unwrap();
        let transform = Transform {
            dx: x,
            ..Transform::scale(16.0, 16.0)
        };
        page.fill_path(
            transform,
            Color::new(0, 0, 0, 255),
            CompositeMode::SrcOver,
            |pen| scratch.decompose(Point::default(), 2, pen),
        )
        .unwrap();
        x += instance.advance(gid) as f32 * 32.0;
    }
    let data = page.finish();
    let reference = root.join("vector/data/expected/basic/glyf-quadratics.pdf");
    let tmp = std::env::temp_dir().join(format!("sofel-pdf-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let ours = tmp.join("page.pdf");
    std::fs::write(&ours, data).unwrap();
    for (path, name) in [(&ours, "actual"), (&reference, "reference")] {
        let proc = std::process::Command::new("pdftoppm")
            .args(["-scale-to", "640", "-singlefile"])
            .arg(path)
            .arg(tmp.join(name))
            .output()
            .unwrap();
        assert!(
            proc.status.success(),
            "{}",
            String::from_utf8_lossy(&proc.stderr)
        );
    }
    let actual = std::fs::read(tmp.join("actual.ppm")).unwrap();
    let expected = std::fs::read(tmp.join("reference.ppm")).unwrap();
    assert!(expected.len() > 1000);
    assert_eq!(actual.iter().zip(&expected).position(|(a, b)| a != b), None);
    assert_eq!(actual.len(), expected.len());
    std::fs::remove_dir_all(tmp).unwrap();
}

#[test]
#[ignore = "requires Poppler; exercised by the Linux conformance job"]
fn i1_vector_page_pdf_paints_translucent_paths_in_order() {
    use sofel_geom::{CompositeMode, Pen};
    use sofel_vector::PdfPage;
    let white = Color::new(255, 255, 255, 255);
    let square = |pen: &mut sofel_vector::PdfPen<'_>| {
        pen.move_to(Point { x: 0, y: 0 });
        for (x, y) in [(10, 0), (10, 10), (0, 10)] {
            pen.line_to(Point { x, y });
        }
        pen.close();
        Ok::<(), ()>(())
    };
    let tmp = std::env::temp_dir().join(format!("sofel-pdf-blend-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    for (mode, expected) in [
        (CompositeMode::Plus, [200, 176, 136]),
        (CompositeMode::Xor, [160, 96, 96]),
        (CompositeMode::SrcIn, [128, 160, 128]),
    ] {
        let mut page = PdfPage::new(0, 0, 10, 10, Some(white));
        page.fill_path(
            Transform::IDENTITY,
            Color::new(192, 128, 64, 255),
            CompositeMode::SrcOver,
            square,
        )
        .unwrap();
        page.fill_path(
            Transform::IDENTITY,
            Color::new(64, 192, 192, 128),
            mode,
            square,
        )
        .unwrap();
        let path = tmp.join("overlap.pdf");
        std::fs::write(&path, page.finish()).unwrap();
        assert!(
            std::process::Command::new("pdftoppm")
                .args(["-r", "72", "-singlefile"])
                .arg(path)
                .arg(tmp.join("sample"))
                .status()
                .unwrap()
                .success()
        );
        let bytes = std::fs::read(tmp.join("sample.ppm")).unwrap();
        let raster = bytes.strip_prefix(b"P6\n10 10\n255\n").unwrap();
        let pixel = raster.get(165..168).unwrap();
        assert!(
            pixel.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
            "{mode:?}: {pixel:?}"
        );
    }
    std::fs::remove_dir_all(tmp).unwrap();
}
