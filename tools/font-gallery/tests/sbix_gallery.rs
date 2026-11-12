//! ADR 0072 I4/I5: inspect the generated sheet, including Rust's PNG stream.

use std::fs;
use std::path::PathBuf;

use font_gallery::inputs::{self, Status};
use font_gallery::pixels::Image;
use font_gallery::sheet;

fn premul(channel: u8, alpha: u8) -> u8 {
    let n = u32::from(channel) * u32::from(alpha) + 128;
    ((n + (n >> 8)) >> 8) as u8
}

fn has_color(image: &Image) -> bool {
    image
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .any(|p| p[3] > 0 && (p[0] != p[1] || p[2] != p[1]))
}

#[test]
#[ignore = "requires a generated gallery and renderer binaries"]
fn i4_i5_published_sbix_card_matches_both_glyphs() {
    let output = PathBuf::from(std::env::var_os("SOFEL_GALLERY_OUTPUT").expect("output path"));
    let binaries =
        PathBuf::from(std::env::var_os("SOFEL_GALLERY_BINARIES").expect("binaries path"));
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = if output.is_absolute() {
        output
    } else {
        repo.join(output)
    };
    let cases = inputs::cases(
        &repo,
        &repo.join("conformance/fonts.tsv"),
        &repo.join("target/testdata/extra-fonts"),
        true,
    )
    .unwrap();
    let index = cases
        .iter()
        .position(|s| s.id == "color-sbix" && s.face == 0)
        .unwrap();
    let sample = &cases[index];
    assert!(!sample.unsupported_empty);
    let rendered = inputs::render_case(
        sample,
        &binaries.join("sofel-render"),
        &binaries.join("sofel-shape"),
    )
    .unwrap();
    assert_eq!(rendered.status, Status::Drawn);
    assert_eq!(rendered.glyph_ids, [26]);
    let solo = rendered.solo.unwrap();
    let glyph = rendered.glyphs[0].image.as_ref().unwrap();
    assert_eq!(
        (solo.width, solo.height, glyph.width, glyph.height),
        (128, 128, 128, 128)
    );
    assert!(has_color(&solo) && has_color(glyph));
    let png = fs::read(output.join("all-samples.png")).unwrap();
    let decoded = sofel_png::decode(&png, 10_000_000).unwrap();
    let width = decoded.width as usize;
    let x = (index % sheet::COLUMNS) * sheet::WIDTH;
    let y = (index / sheet::COLUMNS) * sheet::HEIGHT;
    for (image, dx, over_white) in [(&solo, sheet::PADDING, false), (glyph, sheet::RUN_X, true)] {
        for row in 0..128 {
            for col in 0..128 {
                let src = &image.pixels[(row * 128 + col) * 4..][..4];
                let [r, g, b, a] = [src[0], src[1], src[2], src[3]];
                let expected = if over_white {
                    let over = |c: u8| {
                        ((u32::from(c) * u32::from(a) + 255 * (255 - u32::from(a)) + 127) / 255)
                            as u8
                    };
                    [over(b), over(g), over(r), 255]
                } else {
                    [premul(b, a), premul(g, a), premul(r, a), a]
                };
                let at = ((y + sheet::SAMPLE_TOP + row) * width + x + dx + col) * 4;
                assert_eq!(
                    &decoded.bgra[at..at + 4],
                    &expected,
                    "glyph at x={dx}, row {row}, col {col}"
                );
            }
        }
    }
    for row in (y + 208)..(y + sheet::HEIGHT) {
        let at = (row * width + x) * 4;
        assert!(
            decoded.bgra[at..at + sheet::WIDTH * 4]
                .iter()
                .all(|b| *b == 255)
        );
    }
    let html = fs::read_to_string(output.join("index.html")).unwrap();
    assert!(html.contains("<tr><td>color-sbix / face 0</td>"));
    assert!(html.contains("<td>描画済み</td></tr>"));
}
