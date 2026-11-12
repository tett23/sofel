//! Inspect the pixels of the actual published color-svg gallery card.
use font_gallery::{inputs, pixels, sheet};
use std::path::PathBuf;
use std::sync::OnceLock;

struct Gallery {
    image: sofel_png::Image,
    card_x: usize,
    card_y: usize,
}

fn load() -> Result<Gallery, String> {
    if std::env::var_os("SOFEL_SVG_GALLERY_FAILED").is_some() {
        return Err("gallery generation failed".into());
    }
    let output = std::env::var_os("SOFEL_GALLERY_OUTPUT").map_or_else(
        || crate::paths::root().join("target/font-gallery"),
        PathBuf::from,
    );
    let html = std::fs::read_to_string(output.join("index.html"))
        .map_err(|e| format!("gallery index.html: {e}"))?;
    let rows: Vec<_> = html.split("<tr><td>").skip(1).collect();
    let mut matches = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.starts_with("color-svg / face 0</td>"));
    let (index, row) = matches.next().ok_or("missing color-svg gallery row")?;
    if matches.next().is_some() {
        return Err("duplicate color-svg gallery row".into());
    }
    if !row.starts_with("color-svg / face 0</td><td>glyph 19</td><td>simple_linear</td><td>48</td><td>描画済み</td></tr>") {
        return Err("color-svg / face 0: expected glyph 19, simple_linear, 48 ppem, 描画済み".into());
    }
    let bytes = std::fs::read(output.join("all-samples.png"))
        .map_err(|e| format!("gallery all-samples.png: {e}"))?;
    let image = sofel_png::decode(&bytes, 10_000_000).map_err(|e| format!("gallery PNG: {e:?}"))?;
    let columns = sheet::COLUMNS.min(rows.len());
    let size = (
        sheet::WIDTH * columns,
        sheet::HEIGHT * rows.len().div_ceil(columns),
    );
    if (image.width as usize, image.height as usize) != size {
        return Err(format!(
            "gallery: expected {size:?}, got {}x{}",
            image.width, image.height
        ));
    }
    Ok(Gallery {
        image,
        card_x: (index % columns) * sheet::WIDTH,
        card_y: (index / columns) * sheet::HEIGHT,
    })
}

fn check_run() -> Result<(), String> {
    let args = [
        "-O",
        "json",
        "--no-glyph-names",
        "--language=en",
        "--face-index=0",
        "--font-size=3072",
        "--font-ppem=48",
    ]
    .map(std::ffi::OsString::from);
    let mut args = Vec::from(args);
    args.push(
        crate::paths::testdata()
            .join("extra-fonts/fonts/color-svg.ttf")
            .into_os_string(),
    );
    args.push("simple_linear".into());
    let mut output = Vec::new();
    if sofel_cli::shape::run(&args, &mut output, &mut Vec::new()) != 0 {
        return Err("could not shape color-svg gallery run".into());
    }
    let records = inputs::shape_records(&output)?;
    if !matches!(records.as_slice(), [record] if record.g==26) {
        return Err("simple_linear: expected glyph 26 alone".into());
    }
    Ok(())
}

impl Gallery {
    fn pixel(&self, x: usize, y: usize) -> Result<&[u8; 4], String> {
        if x >= sheet::WIDTH || y >= sheet::HEIGHT {
            return Err(format!("outside gallery card: ({x},{y})"));
        }
        let offset = (self.card_y + y) * (self.image.width as usize) + self.card_x + x;
        self.image
            .bgra
            .as_chunks::<4>()
            .0
            .get(offset)
            .ok_or_else(|| format!("missing gallery pixel ({x},{y})"))
    }

    fn compare(&self, glyph: u32, image: &pixels::Image) -> Result<(), String> {
        if image.pgm
            || !image
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|&[r, g, b, a]| a > 0 && (r != g || g != b))
        {
            return Err(format!("glyph {glyph}: expected visible color"));
        }
        let dx = if glyph == 19 {
            sheet::PADDING
        } else {
            sheet::RUN_X
        };
        let y0 = sheet::SAMPLE_TOP
            + sheet::SAMPLE_HEIGHT
                .checked_sub(image.height)
                .ok_or("glyph exceeds gallery height")?
                / 2;
        for (i, &[r, g, b, a]) in image.pixels.as_chunks::<4>().0.iter().enumerate() {
            let x = dx + i % image.width;
            let y = y0 + i / image.width;
            // The single sample is copied with its alpha. The shaped run is
            // first composited onto the gallery's white run canvas.
            let alpha = u32::from(a);
            let channel = |c| {
                ((u32::from(c) * alpha + if glyph == 19 { 0 } else { 255 * (255 - alpha) } + 127)
                    / 255) as u8
            };
            let expected = [
                channel(b),
                channel(g),
                channel(r),
                if glyph == 19 { a } else { 255 },
            ];
            let actual = self.pixel(x, y)?;
            if *actual != expected {
                return Err(format!(
                    "glyph {glyph} gallery ({x},{y}): expected premultiplied BGRA {expected:?}, got {actual:?}"
                ));
            }
        }
        for y in 208..sheet::HEIGHT {
            for x in 0..sheet::WIDTH {
                if *self.pixel(x, y)? != [255; 4] {
                    return Err(format!("color-svg gallery contains a note at ({x},{y})"));
                }
            }
        }
        Ok(())
    }
}

pub(super) fn observe(glyph: u32) -> Result<(), String> {
    static GALLERY: OnceLock<Result<Gallery, String>> = OnceLock::new();
    let gallery = GALLERY.get_or_init(load).as_ref().map_err(Clone::clone)?;
    if glyph == 26 {
        check_run()?;
    }
    let pam = super::real_pam(glyph)?;
    gallery.compare(glyph, &pixels::parse(&pam)?)
}
