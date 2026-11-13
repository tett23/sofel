//! Fixed-width gallery sheet layout from sofel ADR 0069.

use crate::pixels::{self, Image};

pub const PADDING: usize = 12;
pub const WIDTH: usize = 490;
pub const HEIGHT: usize = 232;
pub const SAMPLE_TOP: usize = 35;
pub const SAMPLE_HEIGHT: usize = 128;
const SAMPLE_GAP: usize = 4;
pub const RUN_X: usize = 152;
pub const COLUMNS: usize = 3;

pub fn card(
    solo: Option<&Image>,
    run: Option<&Image>,
    labels: &[(Image, usize, usize)],
) -> Result<Image, String> {
    let mut card = pixels::blank(WIDTH, HEIGHT)?;
    for (image, x, max_width) in [
        (solo, PADDING, RUN_X - PADDING - SAMPLE_GAP),
        (run, RUN_X, WIDTH - RUN_X - PADDING),
    ] {
        let Some(image) = image else {
            continue;
        };
        if image.width > max_width || image.height > SAMPLE_HEIGHT {
            return Err("sample does not fit at its native ppem".into());
        }
        let y = SAMPLE_TOP + (SAMPLE_HEIGHT - image.height) / 2;
        pixels::copy(&mut card, image, x, y)?;
    }
    for (label, x, y) in labels {
        if x + label.width > WIDTH || y + label.height > HEIGHT {
            return Err("label does not fit".into());
        }
        pixels::blit(&mut card, label, *x as i32, *y as i32, false);
    }
    Ok(card)
}

pub fn layout(cards: &[Image]) -> Result<Image, String> {
    if cards.is_empty() {
        return Err("empty gallery".into());
    }
    let columns = COLUMNS.min(cards.len());
    let mut canvas = pixels::blank(WIDTH * columns, HEIGHT * cards.len().div_ceil(columns))?;
    for (i, card) in cards.iter().enumerate() {
        pixels::copy(
            &mut canvas,
            card,
            WIDTH * (i % columns),
            HEIGHT * (i / columns),
        )?;
    }
    Ok(canvas)
}
