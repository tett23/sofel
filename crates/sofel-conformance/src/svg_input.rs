//! OT-SVG input observations for ADR 0074.
use sofel_color::{PaintOptions, Painted, Scratch};
use sofel_font::Font;
use sofel_geom::{Brush, CompositeMode, Image, ImageFormat, Painter, Rect, Transform};
use sofel_read::Tag;

const DOCUMENT: &[u8] = include_bytes!("../../../conformance/svg/two-glyphs.svg");
// Generated independently with gzip -n -c conformance/svg/two-glyphs.svg.
const GZIP: &[u8] = include_bytes!("../../../conformance/svg/two-glyphs.svg.gz");

/// Returns the measurement for a supported OT-SVG observation.
pub fn observe(feature: &str, case: &str, observation: &str) -> Option<Result<(), String>> {
    match (feature, case, observation) {
        ("in-document", "gzip", "decompressed-bytes") => Some(check_document(GZIP, DOCUMENT)),
        _ => None,
    }
}

#[derive(Default)]
struct ImageCapture {
    images: Vec<(ImageFormat, Vec<u8>)>,
}

impl Painter for ImageCapture {
    fn image(&mut self, image: &Image<'_>) -> bool {
        self.images.push((image.format, image.data.to_vec()));
        true
    }
    fn push_transform(&mut self, _: Transform) {}
    fn pop_transform(&mut self) {}
    fn push_clip_glyph(&mut self, _: u32) {}
    fn push_clip_rectangle(&mut self, _: Rect) {}
    fn pop_clip(&mut self) {}
    fn fill(&mut self, _: &Brush<'_>) {}
    fn push_group(&mut self, _: CompositeMode) {}
    fn pop_group(&mut self, _: CompositeMode) {}
}

fn check_document(stored: &[u8], expected: &[u8]) -> Result<(), String> {
    // SVG Document List at offset 10, one record for glyphs 1..=2,
    // document at list-relative offset 14.
    let mut table = vec![0, 0, 0, 0, 0, 10, 0, 0, 0, 0, 0, 1, 0, 1, 0, 2, 0, 0, 0, 14];
    let length = u32::try_from(stored.len()).map_err(|_| "SVG fixture exceeds u32")?;
    table.extend_from_slice(&length.to_be_bytes());
    table.extend_from_slice(stored);
    let mut head = [0u8; 54];
    head[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    head[12..16].copy_from_slice(&sofel_read::head::MAGIC_NUMBER.to_be_bytes());
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    let maxp = [0, 0, 0x50, 0, 0, 3];
    let bytes = sofel_write::build_sfnt(&[
        (Tag::new(b"SVG "), &table),
        (Tag::new(b"head"), &head),
        (Tag::new(b"maxp"), &maxp),
    ])
    .map_err(|e| format!("building SVG fixture: {e:?}"))?;
    let font = Font::new(&bytes, 0).map_err(|e| format!("reading SVG fixture: {e:?}"))?;
    let mut capture = ImageCapture::default();
    let painted = sofel_color::paint(
        &font,
        1,
        &PaintOptions::default(),
        &mut Scratch::default(),
        &mut capture,
    );
    let [(ImageFormat::Svg { gzip: false }, actual)] = capture.images.as_slice() else {
        let formats: Vec<_> = capture.images.iter().map(|(format, _)| format).collect();
        return Err(format!(
            "glyph 1: expected one expanded SVG image, got {painted:?} with {formats:?}"
        ));
    };
    if painted != Painted::Svg {
        return Err(format!("glyph 1: expected SVG paint, got {painted:?}"));
    }
    if actual != expected {
        let offset = actual
            .iter()
            .zip(expected)
            .position(|(a, e)| a != e)
            .unwrap_or(actual.len().min(expected.len()));
        return Err(format!(
            "glyph 1 SVG byte {offset}: expected {:?}, got {:?}",
            expected.get(offset),
            actual.get(offset)
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i2_svg_observations_check_painted_document_bytes() {
        assert_eq!(check_document(DOCUMENT, DOCUMENT), Ok(()));
        assert!(check_document(b"<svg/>", DOCUMENT).is_err());
        assert!(check_document(&[], DOCUMENT).is_err());
    }

    #[test]
    fn i2_svg_observations_do_not_promote_bytes_to_pixels() {
        assert!(observe("in-document", "gzip", "decompressed-bytes").is_some());
        assert!(observe("in-document", "gzip", "rgba").is_none());
    }
}
