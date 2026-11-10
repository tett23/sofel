//! Checks shared by the fuzz targets: no panics, and the validator
//! invariants of ADR 0007 (without an `Error`, views can be built and
//! in-range reads are `Some`).

use sofel_read::cmap::Cmap;
use sofel_read::head::Head;
use sofel_read::hhea::Hhea;
use sofel_read::hmtx::Hmtx;
use sofel_read::maxp::Maxp;
use sofel_read::sfnt::{FontFile, Sfnt};
use sofel_read::name::Name;
use sofel_read::os2::Os2;
use sofel_read::post::Post;
use sofel_read::vorg::Vorg;
use sofel_read::{
    CountWidth, Diagnostic, GlyphId, Severity, cmap, head, hhea, hmtx, maxp, name, os2, post, validate, vorg,
};

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
/// and at 16 pixels (ADR 0011), and its rendering at 16 pixels with and
/// without 4x4 supersampling (ADR 0012). Must not panic.
pub fn check_outline(data: &[u8]) {
    let Ok(font) = sofel_font::Font::new(data, 0) else { return };
    let mut scratch = sofel_outline::Scratch::default();
    let mut raster = sofel_raster::Rasterizer::with_band_cells(512);
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
        for overlap in [false, true] {
            raster.clear();
            if sofel_outline::draw(&font, GlyphId(g), scales[1], &mut scratch, &mut raster).is_ok() {
                let options = sofel_raster::RenderOptions {
                    fill: sofel_raster::FillRule::NonZero,
                    overlap,
                    ppem: Some((16, 16)),
                };
                let _ = raster.render(options);
            }
        }
    }
}

/// An OS/2 table: every field reader (ADR 0016).
pub fn check_os2(data: &[u8]) {
    let error = has_error(|r| os2::validate(data, r));
    match Os2::new(data) {
        Err(_) => assert!(error, "Os2::new failed without an Error"),
        Ok(o) => {
            let _ = (o.version(), o.fs_type(), o.fs_selection(), o.panose(), o.ul_unicode_range());
            let _ = (o.ach_vend_id(), o.s_typo_ascender(), o.us_win_descent(), o.ul_code_page_range());
            let _ = (o.sx_height(), o.s_cap_height(), o.us_max_context(), o.us_upper_optical_point_size());
        }
    }
}

/// A name table: every record and language tag string (ADR 0016).
pub fn check_name(data: &[u8]) {
    let error = has_error(|r| name::validate(data, r));
    match Name::new(data) {
        Err(_) => assert!(error, "Name::new failed without an Error"),
        Ok(n) => {
            for r in n.records() {
                let s = n.string(&r);
                assert!(error || s.is_some(), "a string is out of range without an Error");
            }
            for i in 0..usize::from(n.lang_tag_count()) {
                if let Some(t) = n.lang_tag(i) {
                    let s = n.lang_tag_string(&t);
                    assert!(error || s.is_some(), "a language tag is out of range without an Error");
                }
            }
        }
    }
}

/// A post table: every glyph's name index and the stored names (ADR 0016).
pub fn check_post(data: &[u8]) {
    let error = has_error(|r| post::validate(data, r));
    match Post::new(data) {
        Err(_) => assert!(error, "Post::new failed without an Error"),
        Ok(p) => {
            let stored = p.names().take(70_000).count();
            for g in 0..u32::from(p.num_glyphs().unwrap_or(0)) {
                if let Some(i) = p.glyph_name_index(GlyphId(g)) {
                    assert!(error || i < 258 || usize::from(i - 258) < stored, "a name index is out of range without an Error");
                }
                let _ = (p.v25_offset(GlyphId(g)), p.v4_code(GlyphId(g)));
            }
        }
    }
}

/// The first two bytes give numGlyphs; the rest is a VORG table (ADR 0016).
pub fn check_vorg(input: &[u8]) {
    let Some((n, data)) = input.split_first_chunk::<2>() else { return };
    let num_glyphs = u32::from(u16::from_be_bytes(*n));
    let error = has_error(|r| vorg::validate(data, Some(num_glyphs), r));
    match Vorg::new(data) {
        Err(_) => assert!(error, "Vorg::new failed without an Error"),
        Ok(v) => {
            for i in 0..v.num_records().min(100_000) {
                if let Some((g, y)) = v.record(i) {
                    assert!(error || v.lookup(g) == Some(y), "a record is not found without an Error");
                }
            }
            for g in 0..num_glyphs.min(1000) {
                let _ = v.y_origin(GlyphId(g));
            }
        }
    }
}
