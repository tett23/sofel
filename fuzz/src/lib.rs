//! Checks shared by the fuzz targets: no panics, and the validator
//! invariants of ADR 0007 (without an `Error`, views can be built and
//! in-range reads are `Some`).

use sofel_read::cmap::Cmap;
use sofel_read::head::Head;
use sofel_read::hhea::Hhea;
use sofel_read::hmtx::Hmtx;
use sofel_read::maxp::Maxp;
use sofel_read::sfnt::{FontFile, Sfnt};
use sofel_read::{CountWidth, Diagnostic, GlyphId, Severity, cmap, head, hhea, hmtx, maxp, validate};

fn has_error(f: impl FnOnce(&mut dyn FnMut(Diagnostic))) -> bool {
    let mut error = false;
    f(&mut |d| error |= d.severity == Severity::Error);
    error
}

/// The first two bytes give numGlyphs (0 means none); the rest is a cmap table.
pub fn check_cmap(input: &[u8]) {
    let Some((n, data)) = input.split_first_chunk::<2>() else { return };
    let n = u32::from(u16::from_be_bytes(*n));
    let num_glyphs = (n != 0).then_some(n);
    let error = has_error(|r| cmap::validate(data, num_glyphs, r));
    let Ok(c) = Cmap::new(data) else {
        assert!(error, "Cmap::new failed without an Error");
        return;
    };
    for rec in c.records() {
        match rec.subtable() {
            Err(_) => assert!(error, "a subtable failed without an Error"),
            Ok(s) => {
                for (cp, g) in s.mappings().take(2000) {
                    assert_eq!(s.glyph(cp), Some(g));
                }
                for cp in [0u32, 0x20, 0x41, 0xFF, 0x100, 0xFFFF, 0x1_0000, 0x10_FFFF, u32::MAX] {
                    let _ = s.glyph(cp);
                    let _ = s.variant(cp, 0xFE00);
                    let _ = s.variant(cp, 0xE0100);
                }
                let _ = s.language();
            }
        }
    }
}

/// The first byte picks the table; the next 4 give numberOfHMetrics
/// and numGlyphs (16 bits each) for hmtx; the rest is the table.
pub fn check_metrics(input: &[u8]) {
    let Some((&which, rest)) = input.split_first() else { return };
    let Some((counts, data)) = rest.split_first_chunk::<4>() else { return };
    let width = if which & 0x80 != 0 { CountWidth::U24 } else { CountWidth::U16 };
    match which & 3 {
        0 => {
            if !has_error(|r| head::validate(data, r)) {
                assert!(Head::new(data).is_ok());
            }
        }
        1 => {
            if !has_error(|r| maxp::validate(data, width, r)) {
                assert!(Maxp::new(data, width).is_ok());
            }
        }
        2 => {
            if !has_error(|r| hhea::validate(data, width, r)) {
                assert!(Hhea::new(data, width).is_ok());
            }
        }
        _ => {
            let metrics = u32::from(u16::from_be_bytes([counts[0], counts[1]]));
            let glyphs = u32::from(u16::from_be_bytes([counts[2], counts[3]]));
            let error = has_error(|r| hmtx::validate(data, metrics, glyphs, r));
            match Hmtx::new(data, metrics, glyphs) {
                Err(_) => assert!(error, "Hmtx::new failed without an Error"),
                Ok(h) => {
                    for g in 0..glyphs.saturating_add(2) {
                        let a = h.advance(GlyphId(g));
                        let l = h.left_side_bearing(GlyphId(g));
                        if !error && g < glyphs {
                            assert!(a.is_some() && l.is_some(), "in-range read is None");
                        }
                    }
                }
            }
        }
    }
}

/// A whole font file: directory, collection and every table.
pub fn check_font(data: &[u8]) {
    match FontFile::new(data) {
        Ok(FontFile::Sfnt(s)) => check_sfnt(&s),
        Ok(FontFile::Collection(c)) => {
            validate::collection(&c, &mut |_| {});
            for i in 0..c.num_fonts().min(64) {
                if let Ok(s) = c.font(i) {
                    check_sfnt(&s);
                }
            }
        }
        Err(_) => {}
    }
}

fn check_sfnt(s: &Sfnt<'_>) {
    validate::sfnt(s, &mut |_| {});
    for r in s.records() {
        let _ = r.data(s.file());
    }
}

/// A whole font file: every glyph's outline (up to 256 glyphs), unscaled
/// and at 16 pixels (ADR 0011). Must not panic.
pub fn check_outline(data: &[u8]) {
    let Ok(font) = sofel_font::Font::new(data, 0) else { return };
    let mut scratch = sofel_outline::Scratch::default();
    let scales = [sofel_outline::Scale::Unscaled, sofel_outline::Scale::pixels(&font, 16, 16)];
    for g in 0..font.num_glyphs().min(256) {
        for scale in scales {
            let mut n = 0usize;
            let _ = sofel_outline::draw(
                &font,
                GlyphId(g),
                scale,
                &mut scratch,
                &mut sofel_geom::FnPen(|_| n = n.wrapping_add(1)),
            );
        }
    }
}
