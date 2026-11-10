//! Bitmaps in the text form of `ft-dump --raster` (ADR 0012).

use std::fmt::Write;

use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_outline::Scratch;
use sofel_raster::Rasterizer;

use crate::outline::GlyphLines;

/// The lines sofel produces for one glyph at `ppem`, in the same form
/// as `ft-dump --raster` (error codes dropped).
pub fn sofel_glyph(
    font: &Font<'_>,
    glyph: u32,
    ppem: u16,
    scratch: &mut Scratch,
    raster: &mut Rasterizer,
) -> GlyphLines {
    let Ok(b) = sofel_cli::glyph::render(font, GlyphId(glyph), ppem, scratch, raster) else {
        return vec!["error".to_string()];
    };
    let mut lines = vec![format!(
        "bitmap {} {} {} {}",
        b.left, b.top, b.width, b.rows
    )];
    if b.width > 0 {
        for row in b.pixels.chunks(b.width as usize) {
            let mut s = String::with_capacity(row.len() * 2);
            for v in row {
                let _ = write!(s, "{v:02x}");
            }
            lines.push(s);
        }
    }
    lines
}
