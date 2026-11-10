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
    CountWidth, Diagnostic, GlyphId, Severity, base, cmap, head, hhea, hmtx, maxp, name, os2, post, validate, vorg,
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
    check_font_queries(&font);
    draw_all(&font);
}

/// Every glyph's outline through `Outlines` (glyf, CFF and CFF2), and
/// its rendering.
fn draw_all(font: &sofel_font::Font<'_>) {
    let outlines = sofel_outline::Outlines::new(font);
    let mut scratch = sofel_outline::Scratch::default();
    let mut raster = sofel_raster::Rasterizer::with_band_cells(512);
    let scales = [sofel_outline::Scale::Unscaled, sofel_outline::Scale::pixels(font, 16, 16)];
    for g in 0..font.num_glyphs().max(outlines.cff().map_or(0, |c| c.num_glyphs())).min(256) {
        for scale in scales {
            let mut n = 0usize;
            let _ = outlines.draw(
                GlyphId(g),
                scale,
                &mut scratch,
                &mut sofel_geom::FnPen(|_| n = n.wrapping_add(1)),
            );
        }
        for overlap in [false, true] {
            raster.clear();
            if outlines.draw(GlyphId(g), scales[1], &mut scratch, &mut raster).is_ok() {
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

/// A BASE table (ADR 0017): without an `Error`, every offset can be
/// followed.
pub fn check_base(data: &[u8]) {
    use sofel_read::base::{Base, BaseCoord, MinMax};
    use sofel_read::ReadError;
    fn coord(c: Result<Option<BaseCoord<'_>>, ReadError>) -> bool {
        match c {
            Err(_) => false,
            Ok(None) => true,
            Ok(Some(c)) => {
                let _ = (c.coordinate(), c.reference_glyph(), c.coord_point());
                c.device().map(|d| d.map(|d| d.variation_index())).is_ok()
            }
        }
    }
    fn min_max(m: Result<Option<MinMax<'_>>, ReadError>) -> bool {
        match m {
            Err(_) => false,
            Ok(None) => true,
            Ok(Some(m)) => {
                coord(m.min_coord())
                    && coord(m.max_coord())
                    && (0..m.num_features().min(1000))
                        .all(|i| coord(m.feature_min_coord(i)) && coord(m.feature_max_coord(i)))
            }
        }
    }
    let error = has_error(|r| base::validate(data, r));
    let Ok(b) = Base::new(data) else {
        assert!(error, "Base::new failed without an Error");
        return;
    };
    let walks = [b.horiz_axis(), b.vert_axis()].into_iter().all(|a| {
        let Ok(a) = a else { return false };
        let Some(a) = a else { return true };
        let Ok(tags) = a.base_tag_list() else { return false };
        if let Some(t) = tags {
            let _ = t.find(sofel_read::Tag::new(b"romn"));
        }
        let Ok(list) = a.base_script_list() else { return false };
        let Some(list) = list else { return true };
        let _ = list.find(sofel_read::Tag::new(b"latn"));
        (0..list.len().min(1000)).all(|i| match list.script(i) {
            Err(_) => false,
            Ok(None) => true,
            Ok(Some(s)) => {
                let values = match s.base_values() {
                    Err(_) => false,
                    Ok(None) => true,
                    Ok(Some(v)) => (0..v.len().min(1000)).all(|i| coord(v.coord(i))),
                };
                let _ = s.find_lang_sys(sofel_read::Tag::new(b"ENG "));
                values
                    && min_max(s.default_min_max())
                    && (0..s.num_lang_sys().min(1000)).all(|i| min_max(s.lang_sys_min_max(i)))
            }
        })
    });
    assert!(error || walks, "an offset cannot be followed without an Error");
}

/// The first two bytes give numGlyphs (0 means none); the rest is a MATH table.
pub fn check_math(input: &[u8]) {
    use sofel_read::math::{self, Corner, Math, MathValue};
    use sofel_read::ReadError;
    fn ok<T>(r: Result<Option<T>, ReadError>, f: impl FnOnce(T) -> bool) -> bool {
        match r {
            Err(_) => false,
            Ok(None) => true,
            Ok(Some(t)) => f(t),
        }
    }
    fn device(v: Option<MathValue<'_>>) -> bool {
        v.is_some_and(|v| v.device().is_ok())
    }
    let Some((n, data)) = input.split_first_chunk::<2>() else { return };
    let n = u32::from(u16::from_be_bytes(*n));
    let error = has_error(|r| math::validate(data, (n != 0).then_some(n), r));
    let Ok(m) = Math::new(data) else {
        assert!(error, "Math::new failed without an Error");
        return;
    };
    let constants = ok(m.constants(), |c| {
        let _ = (c.script_percent_scale_down(), c.radical_degree_bottom_raise_percent());
        (0..math::NUM_CONSTANT_VALUES).all(|i| device(c.value(i)))
    });
    let info = ok(m.glyph_info(), |gi| {
        let table = |t| {
            ok(t, |t: math::MathValueTable<'_>| {
                let _ = t.get(GlyphId(3));
                t.coverage().is_ok() && (0..t.len().min(1000)).all(|i| device(t.value(i)))
            })
        };
        table(gi.italics_correction())
            && table(gi.top_accent_attachment())
            && gi.extended_shape_coverage().is_ok()
            && ok(gi.kern_info(), |k| {
                k.coverage().is_ok()
                    && (0..k.len().min(1000)).all(|i| {
                        [Corner::TopRight, Corner::TopLeft, Corner::BottomRight, Corner::BottomLeft]
                            .into_iter()
                            .all(|c| {
                                ok(k.kern(i, c), |k| {
                                    (0..k.height_count().min(1000)).all(|j| device(k.correction_height(j)))
                                        && (0..=k.height_count().min(1000)).all(|j| device(k.kern_value(j)))
                                })
                            })
                    })
            })
    });
    let variants = ok(m.variants(), |v| {
        let construction = |c| {
            ok(c, |c: math::MathGlyphConstruction<'_>| {
                (0..c.num_variants().min(1000)).all(|i| c.variant(i).is_some())
                    && ok(c.glyph_assembly(), |a| {
                        a.italics_correction().device().is_ok()
                            && (0..a.num_parts().min(1000)).all(|i| a.part(i).is_some())
                    })
            })
        };
        let _ = v.min_connector_overlap();
        v.vert_coverage().is_ok()
            && v.horiz_coverage().is_ok()
            && (0..v.vert_glyph_count().min(1000)).all(|i| construction(v.vert_construction(i)))
            && (0..v.horiz_glyph_count().min(1000)).all(|i| construction(v.horiz_construction(i)))
    });
    assert!(error || (constants && info && variants), "a read failed without an Error");
}

/// The queries of ADR 0016 on a whole font: must not panic.
fn check_font_queries(font: &sofel_font::Font<'_>) {
    let scales = [sofel_font::HbScale::unscaled(font), sofel_font::HbScale::for_font(font, 768, -1536)];
    let boxes = |g| font.glyf_extents(g);
    for s in scales {
        let _ = (s.h_extents(font), s.v_extents(font));
        for t in sofel_font::MetricsTag::ALL {
            let _ = s.metrics_position(font, t);
            let _ = s.metrics_position_with_fallback(font, t, boxes);
        }
        for g in 0..font.num_glyphs().min(64).saturating_add(2) {
            let g = GlyphId(g);
            let _ = (s.v_advance(font, g), s.v_origin(font, g, boxes), s.glyf_extents(font, g));
        }
        // ADR 0017.
        for d in [sofel_font::Direction::Horizontal, sofel_font::Direction::Vertical] {
            for script in [b"latn", b"DFLT", b"deva", b"dev2", b"hani"] {
                let script = sofel_read::Tag::new(script);
                for t in sofel_font::baseline::ALL {
                    let _ = s.baseline(font, t, d, script);
                    let _ = s.baseline_with_fallback(font, t, d, script, boxes);
                }
                let _ = s.layout_font_extents(font, d, script, sofel_read::Tag::new(b"dflt"));
            }
        }
    }
    let _ = (font.fs_type(), font.ft_face_metrics());
    let mut out = [0u8; 64];
    let _ = font.postscript_name(&mut out);
    for e in font.names().take(64) {
        let mut lang = [0u8; 64];
        let n = e.language.write(&mut lang);
        let _ = font.name_utf8(e.name_id, lang.get(..n), &mut out);
        let _ = font.name_utf16(e.name_id, None, &mut [0u16; 8]);
        let _ = font.name_utf32(e.name_id, Some(b"en-US"), &mut [0u32; 2]);
    }
    for i in 0..font.sfnt_name_count().min(64) {
        let _ = font.sfnt_name(i);
    }
    let mut buf = vec![0u32; font.glyph_names_len().min(1 << 20)];
    let names = font.glyph_names(&mut buf);
    for g in 0..font.num_glyphs().min(64) {
        let g = GlyphId(g);
        let _ = (font.glyph_name(g), font.ft_glyph_name(g), font.ft_v_advance(g));
        if let (Some(n), Some(idx)) = (font.glyph_name(g), names.as_ref()) {
            let _ = idx.glyph_from_name(n);
            let _ = idx.ft_glyph_from_name(n);
        }
    }
}

/// A CFF or CFF2 table (ADR 0021): the first byte chooses `CFF ` or
/// `CFF2` (bit 0) and whether the font has a one-axis `fvar` (bit 1);
/// the rest is the table. `Cff::new`, every glyph through `Outlines`, the
/// CID queries, the names and the font information must not panic, and a
/// table without an `Error` (and without an FDSelect of format 4) opens.
pub fn check_cff(input: &[u8]) {
    let Some((&flags, table)) = input.split_first() else { return };
    let tag: &[u8; 4] = if flags & 1 == 0 { b"CFF " } else { b"CFF2" };
    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"head", head_table()),
        (*b"maxp", vec![0, 0, 0x50, 0, 0, 16]),
        (*tag, table.to_vec()),
    ];
    if flags & 2 != 0 {
        tables.push((*b"fvar", vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 0]));
    }
    tables.sort();
    let file = sfnt_otto(&tables);
    let Ok(font) = sofel_font::Font::new(&file, 0) else { return };
    let num_glyphs = Some(16);
    let mut error = false;
    let mut format4 = false;
    let mut report = |d: Diagnostic| {
        error |= d.severity == Severity::Error;
        format4 |= d.kind.id() == "fd_select_format4";
    };
    if flags & 1 == 0 {
        sofel_read::cff::validate(table, num_glyphs, &mut report);
    } else {
        sofel_read::cff::validate2(table, num_glyphs, (flags & 2 != 0).then_some(1), &mut report);
    }
    let cff = sofel_font::Cff::new(&font);
    if !error && !format4 {
        assert!(cff.is_ok(), "no Error, but Cff::new fails: {cff:?}");
    }
    if let Ok(c) = cff {
        let _ = c.is_cid_keyed();
        let _ = c.ros();
        let _ = c.ps_font_info();
        let _ = c.ps_fs_type();
        let _ = c.ft_glyph_from_name(b"A");
        let mut index = vec![0u32; c.num_glyphs().min(4096) as usize];
        let built = c.build_name_index(&mut index).map(<[u32]>::to_vec);
        for g in 0..c.num_glyphs().min(256) {
            let _ = c.cid(GlyphId(g));
            let _ = c.charset_name(GlyphId(g));
            let _ = c.ft_glyph_name(GlyphId(g));
        }
        if let Some(idx) = built {
            let _ = c.glyph_from_name(&idx, b".notdef");
        }
        // The charset index (ADR 0025) gives the charset's own answers.
        if let Some(charset) = c.charset() {
            let mut starts = vec![0u32; charset.num_ranges()];
            let index = charset
                .index_into(&mut starts)
                .expect("num_ranges words suffice");
            // Charset::sid(g) is the g-th of Charset::sids; walking those
            // keeps the check linear.
            let n = charset.num_glyphs();
            let mut walked = 0u32;
            for (g, sid) in (0u32..).zip(charset.sids()) {
                assert_eq!(index.sid(g), Some(sid));
                walked = g + 1;
            }
            assert_eq!(walked, n);
            assert_eq!(index.sid(n), None);
            for g in [1, n / 2, n.saturating_sub(1)] {
                assert_eq!(index.sid(g), charset.sid(g));
            }
            for g in 0..c.num_glyphs().min(256) {
                let g = GlyphId(g);
                assert_eq!(c.cid_with(&index, g), c.cid(g));
                assert_eq!(c.ft_glyph_name_with(&index, g), c.ft_glyph_name(g));
            }
        }
    }
    draw_all(&font);
}

fn head_table() -> Vec<u8> {
    let mut h = vec![0u8; 54];
    h[0..4].copy_from_slice(&[0, 1, 0, 0]);
    h[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
    h[18..20].copy_from_slice(&1000u16.to_be_bytes());
    h
}

/// An `OTTO` sfnt of `tables` (sorted by tag).
fn sfnt_otto(tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let n = tables.len();
    let mut out = b"OTTO".to_vec();
    out.extend((n as u16).to_be_bytes());
    out.extend([0u8; 6]);
    let mut at = 12 + 16 * n;
    let mut data = Vec::new();
    for (tag, t) in tables {
        out.extend(tag);
        out.extend([0u8; 4]);
        out.extend((at as u32).to_be_bytes());
        out.extend((t.len() as u32).to_be_bytes());
        data.extend(t);
        while data.len() % 4 != 0 {
            data.push(0);
        }
        at = 12 + 16 * n + data.len();
    }
    out.extend(data);
    out
}

// ---- GDEF, GSUB and GPOS (ADR 0020) ----

mod layout {
    use sofel_read::layout::{
        ChainedSequenceContext, ClassDef, Coverage, CoverageArray, LayoutTable, Lookup,
        SequenceContext,
    };
    use sofel_read::{GlyphId, ReadError, Tag, gdef, gpos, gsub};

    const N: usize = 256;

    fn ok<T>(r: Result<Option<T>, ReadError>, f: impl FnOnce(T) -> bool) -> bool {
        match r {
            Err(_) => false,
            Ok(None) => true,
            Ok(Some(t)) => f(t),
        }
    }

    fn query_coverage(c: &Coverage<'_>) {
        for g in [0, 1, 2, 0xFFFF, 0x1_0000, u32::MAX] {
            let _ = c.get(GlyphId(g));
        }
        let _ = c.ranges().take(N).count();
    }

    fn query_class_def(c: &ClassDef<'_>) {
        for g in [0, 1, 2, 0xFFFF, 0x1_0000, u32::MAX] {
            let _ = c.get(GlyphId(g));
        }
    }

    fn coverages(a: &CoverageArray<'_>) -> bool {
        (0..a.len().min(N)).all(|i| {
            ok(a.get(i), |c| {
                query_coverage(&c);
                true
            })
        })
    }

    fn context(c: &SequenceContext<'_>) -> bool {
        match c {
            SequenceContext::Glyphs {
                coverage,
                rule_sets,
            } => {
                query_coverage(coverage);
                (0..rule_sets.len().min(N)).all(|i| {
                    ok(rule_sets.get(i), |s| {
                        (0..s.len().min(N)).all(|j| {
                            ok(s.get(j), |r| {
                                let _ = (r.input.iter().count(), r.lookups.iter().count());
                                true
                            })
                        })
                    })
                })
            }
            SequenceContext::Classes {
                coverage,
                class_def,
                rule_sets,
            } => {
                query_coverage(coverage);
                query_class_def(class_def);
                (0..rule_sets.len().min(N)).all(|i| {
                    ok(rule_sets.get(i), |s| {
                        (0..s.len().min(N)).all(|j| s.get(j).is_ok())
                    })
                })
            }
            SequenceContext::Coverages {
                coverages: a,
                lookups,
            } => {
                let _ = lookups.iter().count();
                coverages(a)
            }
        }
    }

    fn chain(c: &ChainedSequenceContext<'_>) -> bool {
        match c {
            ChainedSequenceContext::Glyphs { rule_sets, .. }
            | ChainedSequenceContext::Classes { rule_sets, .. } => {
                (0..rule_sets.len().min(N)).all(|i| {
                    ok(rule_sets.get(i), |s| {
                        (0..s.len().min(N)).all(|j| {
                            ok(s.get(j), |r| {
                                let _ = (r.backtrack.iter().count(), r.lookahead.iter().count());
                                true
                            })
                        })
                    })
                })
            }
            ChainedSequenceContext::Coverages {
                backtrack,
                input,
                lookahead,
                ..
            } => coverages(backtrack) && coverages(input) && coverages(lookahead),
        }
    }

    fn layout(data: &[u8], extension: u16, sub: impl Fn(&Lookup<'_>, usize) -> bool) -> bool {
        let Ok(t) = LayoutTable::new(data) else {
            return false;
        };
        let _ = t.feature_variations_offset();
        let scripts = ok(t.script_list(), |l| {
            let _ = l.find(Tag::new(b"latn"));
            (0..l.len().min(N)).all(|i| {
                ok(l.script(i), |s| {
                    let _ = s.find_lang_sys(Tag::new(b"JAN "));
                    s.default_lang_sys().is_ok()
                        && (0..s.num_lang_sys().min(N)).all(|j| s.lang_sys(j).is_ok())
                })
            })
        });
        let features = ok(t.feature_list(), |l| {
            (0..l.len().min(N)).all(|i| l.feature(i).is_ok())
        });
        let lookups = ok(t.lookup_list(), |l| {
            (0..l.len().min(N)).all(|i| {
                ok(l.lookup(i), |lk| {
                    let _ = (lk.mark_filtering_set(), lk.effective_type(extension));
                    lk.extension_types_agree(extension)
                        && (0..lk.num_subtables().min(N))
                            .all(|j| lk.subtable_offset(j) == Some(0) || sub(&lk, j))
                })
            })
        });
        scripts && features && lookups
    }

    pub fn gsub_walks(data: &[u8]) -> bool {
        use gsub::SubstSubtable as S;
        layout(data, gsub::EXTENSION, |lk, j| match gsub::subtable(lk, j) {
            Err(_) => false,
            Ok(S::SingleDelta { coverage, .. }) => {
                query_coverage(&coverage);
                true
            }
            Ok(S::SingleList { substitutes, .. }) => substitutes.iter().take(N).count() <= N,
            Ok(
                S::Multiple { sequences: a, .. }
                | S::Alternate {
                    alternate_sets: a, ..
                },
            ) => (0..a.len().min(N)).all(|i| a.get(i).is_ok()),
            Ok(S::Ligature { ligature_sets, .. }) => (0..ligature_sets.len().min(N)).all(|i| {
                ok(ligature_sets.get(i), |s| {
                    (0..s.len().min(N)).all(|k| s.get(k).is_ok())
                })
            }),
            Ok(S::Context(c)) => context(&c),
            Ok(S::ChainContext(c)) => chain(&c),
            Ok(S::ReverseChainSingle {
                backtrack,
                lookahead,
                ..
            }) => coverages(&backtrack) && coverages(&lookahead),
        })
    }

    fn anchor(a: Result<Option<gpos::Anchor<'_>>, ReadError>) -> bool {
        if let Ok(Some(gpos::Anchor::Format3 {
            x_device: Some(d), ..
        })) = a
        {
            let _ = d.delta_pixels(12);
        }
        a.is_ok()
    }

    fn matrix(m: &gpos::AnchorMatrix<'_>) -> bool {
        (0..m.rows().min(N)).all(|r| (0..m.cols().min(64)).all(|c| anchor(m.get(r, c))))
    }

    fn marks(m: &gpos::MarkArray<'_>) -> bool {
        (0..m.len().min(N)).all(|i| m.get(i).is_some_and(|(_, a)| anchor(a)))
    }

    pub fn gpos_walks(data: &[u8]) -> bool {
        use gpos::PosSubtable as P;
        layout(data, gpos::EXTENSION, |lk, j| match gpos::subtable(lk, j) {
            Err(_) => false,
            Ok(P::Single { value, .. }) => {
                if let Some(d) = value.x_advance_device {
                    let _ = d.delta_pixels(9);
                }
                true
            }
            Ok(P::SingleArray { values, .. }) => {
                (0..values.len().min(N)).all(|i| values.get(i).is_some())
            }
            Ok(P::PairGlyphs { pair_sets, .. }) => (0..pair_sets.len().min(N)).all(|i| {
                ok(pair_sets.get(i), |s| {
                    let _ = s.find(GlyphId(1));
                    true
                })
            }),
            Ok(P::PairClasses {
                values, class_def1, ..
            }) => {
                query_class_def(&class_def1);
                let _ = values.get(1, 1);
                true
            }
            Ok(P::Cursive { entry_exits, .. }) => (0..entry_exits.len().min(N)).all(|i| {
                entry_exits
                    .get(i)
                    .is_some_and(|(a, b)| anchor(a) && anchor(b))
            }),
            Ok(
                P::MarkBase {
                    marks: m, bases: b, ..
                }
                | P::MarkMark {
                    marks: m,
                    mark2s: b,
                    ..
                },
            ) => marks(&m) && matrix(&b),
            Ok(P::MarkLig {
                marks: m,
                ligatures,
                ..
            }) => {
                marks(&m)
                    && (0..ligatures.len().min(N)).all(|i| ok(ligatures.get(i), |x| matrix(&x)))
            }
            Ok(P::Context(c)) => context(&c),
            Ok(P::ChainContext(c)) => chain(&c),
        })
    }

    pub fn gdef_walks(data: &[u8]) -> bool {
        let Ok(t) = gdef::Gdef::new(data) else {
            return false;
        };
        let _ = (
            t.item_var_store_offset(),
            gdef::is_blocklisted(data.len(), 0, 0),
        );
        ok(t.glyph_class_def(), |c| {
            query_class_def(&c);
            true
        }) && t.mark_attach_class_def().is_ok()
            && ok(t.attach_list(), |a| {
                let _ = a.points_of(GlyphId(1));
                a.coverage().is_ok() && (0..a.len().min(N)).all(|i| a.points(i).is_ok())
            })
            && ok(t.lig_caret_list(), |l| {
                l.coverage().is_ok()
                    && (0..l.len().min(N)).all(|i| {
                        ok(l.lig_glyph(i), |g| {
                            (0..g.len().min(N)).all(|k| g.caret(k).is_ok())
                        })
                    })
            })
            && ok(t.mark_glyph_sets(), |s| {
                let _ = s.covers(0, GlyphId(1));
                (0..s.len().min(N)).all(|i| s.coverage(i).is_ok())
            })
    }
}

/// A GDEF table: validator invariants and queries.
pub fn check_gdef(data: &[u8]) {
    let error = has_error(|r| sofel_read::gdef::validate(data, r));
    assert!(
        error || layout::gdef_walks(data),
        "a GDEF offset cannot be followed without an Error"
    );
}

/// The first two bytes give the number of GDEF mark glyph sets; the rest
/// is a GSUB table.
pub fn check_gsub(input: &[u8]) {
    let Some((n, data)) = input.split_first_chunk::<2>() else {
        return;
    };
    let sets = Some(u16::from_be_bytes(*n));
    let error = has_error(|r| sofel_read::gsub::validate(data, sets, r));
    assert!(
        error || layout::gsub_walks(data),
        "a GSUB offset cannot be followed without an Error"
    );
}

/// As [`check_gsub`], for GPOS.
pub fn check_gpos(input: &[u8]) {
    let Some((n, data)) = input.split_first_chunk::<2>() else {
        return;
    };
    let sets = Some(u16::from_be_bytes(*n));
    let error = has_error(|r| sofel_read::gpos::validate(data, sets, r));
    assert!(
        error || layout::gpos_walks(data),
        "a GPOS offset cannot be followed without an Error"
    );
}

/// Shaping (ADR 0020). The first two bytes give the length of the font
/// (big-endian); the third byte the direction (bits 0-1), the cluster
/// level (bits 2-3) and the buffer flags BOT, EOT and the two ways of
/// handling default-ignorables (bits 4-7); then the font; then UTF-8
/// text up to a 0 byte, and comma-separated feature strings after it.
/// Builds a plan and shapes: no panics, at most the buffer's length
/// limit of glyphs (`HB_BUFFER_MAX_LEN_FACTOR`, `HB_BUFFER_MAX_LEN_MIN`),
/// and monotone clusters at the monotone cluster levels. The operation
/// limit (`HB_BUFFER_MAX_OPS_*`) keeps every input within the fuzzer's
/// time limit.
pub fn check_shape(input: &[u8]) {
    use sofel_shape::{
        BufferFlags, ClusterLevel, Direction, Feature, MAX_LEN_FACTOR, MAX_LEN_MIN, Scale,
        SegmentProperties, ShapePlan, UnicodeBuffer,
    };
    let Some((&[a, b, settings], rest)) = input.split_first_chunk::<3>() else {
        return;
    };
    let n = usize::from(u16::from_be_bytes([a, b])).min(rest.len());
    let (font_data, rest) = rest.split_at(n);
    let Ok(font) = sofel_font::Font::new(font_data, 0) else { return };
    let (text, features) = match rest.iter().position(|&c| c == 0) {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, &rest[rest.len()..]),
    };
    let direction = [Direction::Ltr, Direction::Rtl, Direction::Ttb, Direction::Btt]
        [usize::from(settings & 3)];
    let level = [
        ClusterLevel::MonotoneGraphemes,
        ClusterLevel::MonotoneCharacters,
        ClusterLevel::Characters,
        ClusterLevel::Graphemes,
    ][usize::from((settings >> 2) & 3)];
    let features: Vec<Feature> = features
        .split(|&c| c == b',')
        .filter_map(Feature::parse)
        .collect();
    let mut buffer = UnicodeBuffer::new();
    buffer.set_cluster_level(level);
    buffer.set_flags(BufferFlags(u32::from(settings >> 4)));
    buffer.push_utf8(text, 0, None);
    let len = buffer.len();
    let props = SegmentProperties {
        direction: Some(direction),
        ..SegmentProperties::default()
    }
    .guess(&buffer, None);
    let plan = ShapePlan::new(&font, props, &features);
    let boxes = sofel_outline::HbExtents::new(&font);
    let glyphs = sofel_shape::shape_with_extents(
        &font,
        &Scale::unscaled(&font),
        &plan,
        buffer,
        &|g| boxes.glyph_extents(g),
    );
    assert!(
        glyphs.len() <= len.saturating_mul(MAX_LEN_FACTOR).max(MAX_LEN_MIN),
        "{} glyphs from {len} characters",
        glyphs.len()
    );
    if matches!(
        level,
        ClusterLevel::MonotoneGraphemes | ClusterLevel::MonotoneCharacters
    ) {
        let clusters: Vec<u32> = glyphs.glyph_infos().iter().map(|g| g.cluster).collect();
        let monotone = if direction.is_backward() {
            clusters.windows(2).all(|w| w[0] >= w[1])
        } else {
            clusters.windows(2).all(|w| w[0] <= w[1])
        };
        assert!(monotone, "clusters not monotone: {clusters:?}");
    }
}

/// Reading serialized buffers (ADR 0020). The first byte picks the
/// format (bit 0: text or JSON) and the content (bit 1: glyphs or
/// characters); the rest is read. No panics, and what was read is
/// written and read back to the same glyphs (with their defined flags)
/// or characters.
pub fn check_deserialize(input: &[u8]) {
    use sofel_shape::{
        EmptyFont, GlyphFlags, SerializeFlags, SerializeFormat, deserialize_glyphs,
        deserialize_unicode, serialize_glyphs, serialize_unicode,
    };
    let Some((&mode, text)) = input.split_first() else {
        return;
    };
    let format = if mode & 1 == 0 {
        SerializeFormat::Text
    } else {
        SerializeFormat::Json
    };
    if mode & 2 == 0 {
        let (mut infos, mut positions) = (Vec::new(), Vec::new());
        let _ = deserialize_glyphs(text, format, &EmptyFont, &mut infos, &mut positions);
        for i in &mut infos {
            i.flags = GlyphFlags(i.flags.0 & GlyphFlags::DEFINED.0);
        }
        let mut out = Vec::new();
        serialize_glyphs(
            &infos,
            Some(&positions),
            &EmptyFont,
            format,
            SerializeFlags::GLYPH_FLAGS,
            &mut out,
        );
        let (mut again, mut again_positions) = (Vec::new(), Vec::new());
        if !infos.is_empty() {
            assert!(
                deserialize_glyphs(&out, format, &EmptyFont, &mut again, &mut again_positions)
                    .is_ok(),
                "cannot read back {:?}",
                String::from_utf8_lossy(&out)
            );
        }
        assert_eq!((infos, positions), (again, again_positions));
    } else {
        let (mut codepoints, mut clusters) = (Vec::new(), Vec::new());
        let _ = deserialize_unicode(text, format, &mut codepoints, &mut clusters);
        let mut out = Vec::new();
        serialize_unicode(&codepoints, &clusters, format, SerializeFlags::default(), &mut out);
        let (mut again, mut again_clusters) = (Vec::new(), Vec::new());
        if !codepoints.is_empty() {
            assert!(
                deserialize_unicode(&out, format, &mut again, &mut again_clusters).is_ok(),
                "cannot read back {:?}",
                String::from_utf8_lossy(&out)
            );
        }
        assert_eq!((codepoints, clusters), (again, again_clusters));
    }
}

/// Subsets a font with flags, a glyph set and whether to pin every axis
/// to its default taken from the first bytes (ADR 0022): no panic, and a successful output opens and has no
/// validator `Error` in the tables that differ from the source's.
pub fn check_subset(data: &[u8]) {
    use sofel_subset::{Flags, Input, SetKind};
    let (Some(head), Some(font_data)) = (data.get(..6), data.get(6..)) else {
        return;
    };
    let Ok(font) = sofel_font::Font::new(font_data, 0) else {
        return;
    };
    let flags = u32::from(u16::from_be_bytes([head[0], head[1]]));
    let first = u32::from(u16::from_be_bytes([head[2], head[3]]));
    let count = u32::from(head[4]);
    let chars = u32::from(head[5]);
    let input = Input::new()
        .with_flags(Flags(flags & 0x7FFF))
        .map_set(SetKind::Glyphs, |s| s.add_range(first, first.saturating_add(count)))
        .map_set(SetKind::Unicodes, |s| s.add_range(0x20, 0x20 + chars));
    // Bit 15 of the flags pins every axis to its default.
    let input = if flags & 0x8000 != 0 {
        input.with_all_axes_pinned_to_default(&font).unwrap_or(input)
    } else {
        input
    };
    let Ok(out) = sofel_subset::subset(&font, &input) else {
        return;
    };
    let s = Sfnt::new(&out).expect("the subset is an sfnt");
    let src = font.sfnt();
    let mut errors = Vec::new();
    validate::sfnt(&s, &mut |d: Diagnostic| {
        if d.severity == Severity::Error && s.table(d.table) != src.table(d.table) {
            errors.push(d);
        }
    });
    assert!(errors.is_empty(), "{errors:?}");
}

/// An object graph for the repacker (ADR 0022, 再配置): a tag (4 bytes),
/// an object count (1 byte), each object's length (2 bytes) and fill
/// byte, then links: parent and child (object indices, 1 byte each, the
/// child must come first), position (2 bytes), width (1 byte) and flags
/// (1 byte: bit 0 signed, bits 1-2 whence, bit 3 virtual). The last
/// object is the root.
///
/// Repacking must not panic. The repacker returns only bytes whose
/// offsets all fit (it fails otherwise), and keeps every object (adding
/// duplicates), so outside GSUB and GPOS (whose subtables it may shrink)
/// the result is at least as long as the objects. When the objects lay
/// out without an overflow in their order, `Packed::repack` gives the
/// bytes of `Packed::resolve`.
pub fn check_repack(data: &[u8]) {
    use sofel_write::{Link, Object, Packed, Whence, serialize_objects};
    let (Some(tag), Some(&n)) = (data.get(..4), data.get(4)) else {
        return;
    };
    let tag = sofel_read::Tag([tag[0], tag[1], tag[2], tag[3]]);
    let n = usize::from(n);
    let mut at = 5;
    let mut objs = Vec::with_capacity(n);
    for _ in 0..n {
        let Some(o) = data.get(at..at + 3) else {
            return;
        };
        let len = usize::from(u16::from_be_bytes([o[0], o[1]]));
        objs.push(Object {
            bytes: vec![o[2]; len],
            ..Object::default()
        });
        at += 3;
    }
    while let Some(l) = data.get(at..at + 6) {
        at += 6;
        let (parent, child) = (usize::from(l[0]), u32::from(l[1]));
        let Some(o) = objs.get_mut(parent) else {
            continue;
        };
        let link = Link {
            width: l[4],
            signed: l[5] & 1 != 0,
            whence: match (l[5] >> 1) & 3 {
                1 => Whence::Tail,
                2 => Whence::Absolute,
                _ => Whence::Head,
            },
            bias: 0,
            position: u32::from(u16::from_be_bytes([l[2], l[3]])),
            objidx: child + 1,
        };
        if l[5] & 8 != 0 {
            o.virtual_links.push(Link {
                objidx: child + 1,
                ..Link::default()
            });
        } else {
            o.real_links.push(link);
        }
    }
    let total: usize = objs.iter().map(|o| o.bytes.len()).sum();
    if let Ok(out) = serialize_objects(tag, &objs)
        && &tag.0 != b"GSUB"
        && &tag.0 != b"GPOS"
    {
        assert!(out.len() >= total, "{} < {total}", out.len());
    }
    // The serializer's packed objects: index 0 is the null object, and a
    // link may only name an object packed before its parent.
    let ordered = objs.iter().enumerate().all(|(i, o)| {
        o.real_links
            .iter()
            .chain(&o.virtual_links)
            .all(|l| (l.objidx as usize) <= i)
            && o.real_links.iter().all(|l| {
                matches!(l.width, 2 | 3 | 4)
                    && (l.position as usize) + usize::from(l.width) <= o.bytes.len()
            })
    });
    if !ordered || objs.iter().any(|o| o.bytes.is_empty()) || objs.is_empty() {
        return;
    }
    let mut packed_objs = vec![Object::default()];
    packed_objs.extend(objs);
    let packed = Packed::Graph(packed_objs);
    let repacked = packed.repack(tag);
    if let Ok(plain) = packed.resolve() {
        assert_eq!(repacked, Ok(plain));
    }
}
