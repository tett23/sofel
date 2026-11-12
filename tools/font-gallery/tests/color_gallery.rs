//! ADR 0072 I4/I5: inspect the generated sheet, including Rust's PNG stream.

use std::fs;
use std::path::PathBuf;

use font_gallery::inputs::{self, Status};
use font_gallery::pixels::Image;
use font_gallery::sheet;

fn rgba_sheet(bytes: &[u8]) -> (usize, Vec<u8>) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let word = |v: &[u8]| u32::from_be_bytes(v.try_into().unwrap()) as usize;
    let mut chunks = &bytes[8..];
    let mut dimensions = None;
    let mut compressed = Vec::new();
    while !chunks.is_empty() {
        let size = word(&chunks[..4]);
        let chunk = &chunks[4..8 + size];
        assert_eq!(
            sofel_inflate::crc32(chunk) as usize,
            word(&chunks[8 + size..12 + size])
        );
        match &chunk[..4] {
            b"IHDR" => {
                assert_eq!(&chunk[12..], &[8, 6, 0, 0, 0]);
                dimensions = Some((word(&chunk[4..8]), word(&chunk[8..12])));
            }
            b"IDAT" => compressed.extend_from_slice(&chunk[4..]),
            _ => {}
        }
        chunks = &chunks[12 + size..];
    }
    let (width, height) = dimensions.unwrap();
    assert!(width * height <= 10_000_000);
    let stride = 1 + width * 4;
    let raw = sofel_inflate::zlib(&compressed, stride * height).unwrap();
    assert_eq!(raw.len(), stride * height);
    let mut pixels = Vec::with_capacity(width * height * 4);
    for row in raw.chunks_exact(stride) {
        assert_eq!(row[0], 0);
        pixels.extend_from_slice(&row[1..]);
    }
    (width, pixels)
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
fn i4_i5_published_color_cards_match_solo_and_shaped_glyphs() {
    let output = PathBuf::from(std::env::var_os("SOFEL_GALLERY_OUTPUT").expect("output path"));
    let binaries =
        PathBuf::from(std::env::var_os("SOFEL_GALLERY_BINARIES").expect("binaries path"));
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = if output.is_absolute() {
        output
    } else {
        repo.join(output)
    };
    let cases = inputs::gallery_cases(&repo).unwrap();
    let (width, pixels) = rgba_sheet(&fs::read(output.join("all-samples.png")).unwrap());
    let html = fs::read_to_string(output.join("index.html")).unwrap();
    for (id, side, ids) in [
        ("color-cff", 47, &[26][..]),
        ("color-svg", 46, &[26][..]),
        ("color-sbix", 128, &[26][..]),
        ("hb-variable-colr", 48, &[92, 93][..]),
    ] {
        let index = cases
            .iter()
            .position(|s| s.id == id && s.face == 0)
            .unwrap();
        let sample = &cases[index];
        assert!(!sample.unsupported_empty, "{id}");
        let rendered = inputs::render_case(
            sample,
            &binaries.join("sofel-render"),
            &binaries.join("sofel-shape"),
        )
        .unwrap();
        assert_eq!(rendered.status, Status::Drawn, "{id}");
        assert_eq!(rendered.glyph_ids, ids);
        let x = (index % sheet::COLUMNS) * sheet::WIDTH;
        let y = (index / sheet::COLUMNS) * sheet::HEIGHT;
        let solo = rendered.solo.as_ref().unwrap();
        let images = std::iter::once((solo, sheet::PADDING, false)).chain(
            rendered.glyphs.iter().enumerate().map(|(i, g)| {
                if ids.len() > 1 {
                    assert_eq!((g.dx.0, g.dy.0, g.ax.0, g.ay.0), (0, 0, 3072, 0));
                }
                (g.image.as_ref().unwrap(), sheet::RUN_X + i * side, true)
            }),
        );
        for (image, dx, over_white) in images {
            assert_eq!((image.width, image.height), (side, side), "{id}");
            assert!(has_color(image), "{id}");
            for (i, p) in image.pixels.as_chunks::<4>().0.iter().enumerate() {
                let mut expected = *p;
                if over_white {
                    let a = u32::from(p[3]);
                    for c in &mut expected[..3] {
                        *c = ((u32::from(*c) * a + 255 * (255 - a) + 127) / 255) as u8;
                    }
                    expected[3] = 255;
                }
                let at = ((y + sheet::SAMPLE_TOP + (sheet::SAMPLE_HEIGHT - side) / 2 + i / side)
                    * width
                    + x
                    + dx
                    + i % side)
                    * 4;
                assert_eq!(&pixels[at..at + 4], &expected, "{id} offset {dx} pixel {i}");
            }
        }
        for row in y + 208..y + sheet::HEIGHT {
            let at = (row * width + x) * 4;
            assert!(
                pixels[at..at + sheet::WIDTH * 4].iter().all(|b| *b == 255),
                "{id}"
            );
        }
        let prefix = format!("<tr><td>{id} / face 0</td>");
        let row = html
            .split_once(&prefix)
            .unwrap()
            .1
            .split_once("</tr>")
            .unwrap()
            .0;
        assert_eq!(
            row,
            format!(
                "<td>glyph {}</td><td>{}</td><td>{}</td><td>描画済み</td>",
                sample.solo.value(),
                sample.text,
                sample.ppem
            ),
            "{id}"
        );
    }
}

#[test]
#[ignore = "requires a generated gallery and renderer binaries"]
fn i1_i2_noto_color_emoji_pixels_and_published_cards() {
    let output = PathBuf::from(std::env::var_os("SOFEL_GALLERY_OUTPUT").expect("output path"));
    let binaries =
        PathBuf::from(std::env::var_os("SOFEL_GALLERY_BINARIES").expect("binaries path"));
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = if output.is_absolute() {
        output
    } else {
        repo.join(output)
    };
    let cases = inputs::gallery_cases(&repo).unwrap();
    let (sheet_width, sheet_pixels) =
        rgba_sheet(&fs::read(output.join("all-samples.png")).unwrap());
    let html = fs::read_to_string(output.join("index.html")).unwrap();

    for (id, run_id, solo_size, run_size, solo_crc, run_crc) in [
        (
            "color-noto-cbdt",
            2269,
            (136, 128),
            (136, 128),
            0xf26c_4dc6,
            0x532b_95b4,
        ),
        (
            "color-noto-colrv1",
            1205,
            (121, 117),
            (124, 123),
            0x88a3_e34c,
            0x5824_0de7,
        ),
    ] {
        let index = cases
            .iter()
            .position(|sample| sample.id == id && sample.face == 0)
            .unwrap_or_else(|| panic!("missing {id} gallery case"));
        let sample = &cases[index];
        assert_eq!(sample.solo.kind(), "char");
        assert_eq!(sample.solo.value(), "U+1F600");
        assert_eq!(sample.text, "👩‍💻");
        assert_eq!(sample.ppem, 109);
        let rendered = inputs::render_case(
            sample,
            &binaries.join("sofel-render"),
            &binaries.join("sofel-shape"),
        )
        .unwrap();
        assert_eq!(rendered.status, Status::Drawn, "{id}");
        assert_eq!(rendered.glyph_ids, [run_id], "{id} ZWJ shaping");
        assert_eq!(rendered.glyphs.len(), 1, "{id} ZWJ shaping");

        let solo = rendered.solo.as_ref().unwrap();
        let run_glyph = rendered.glyphs[0].image.as_ref().unwrap();
        assert_eq!((solo.width, solo.height), solo_size, "{id} U+1F600");
        assert_eq!(
            (run_glyph.width, run_glyph.height),
            run_size,
            "{id} ZWJ glyph"
        );
        assert_eq!(sofel_inflate::crc32(&solo.pixels), solo_crc, "{id} U+1F600");
        assert_eq!(
            sofel_inflate::crc32(&run_glyph.pixels),
            run_crc,
            "{id} ZWJ glyph"
        );
        assert!(has_color(solo), "{id} U+1F600");
        assert!(has_color(run_glyph), "{id} ZWJ glyph");

        let run = font_gallery::pixels::compose(&rendered.glyphs)
            .unwrap()
            .unwrap();
        assert!(
            sheet::card(Some(solo), Some(&run), &[]).is_ok(),
            "{id} card fit"
        );
        let card_x = (index % sheet::COLUMNS) * sheet::WIDTH;
        let card_y = (index / sheet::COLUMNS) * sheet::HEIGHT;
        for (image, x) in [(solo, sheet::PADDING), (&run, sheet::RUN_X)] {
            let top = sheet::SAMPLE_TOP + (sheet::SAMPLE_HEIGHT - image.height) / 2;
            for row in 0..image.height {
                let source = row * image.width * 4;
                let at = ((card_y + top + row) * sheet_width + card_x + x) * 4;
                assert_eq!(
                    &sheet_pixels[at..at + image.width * 4],
                    &image.pixels[source..source + image.width * 4],
                    "{id} card row {row} at x={x}"
                );
            }
        }
        let row = html
            .split_once(&format!("<tr><td>{id} / face 0</td>"))
            .unwrap_or_else(|| panic!("missing {id} HTML row"))
            .1
            .split_once("</tr>")
            .unwrap()
            .0;
        assert_eq!(
            row, "<td>char U+1F600</td><td>👩‍💻</td><td>109</td><td>描画済み</td>",
            "{id} HTML row"
        );
    }
}
