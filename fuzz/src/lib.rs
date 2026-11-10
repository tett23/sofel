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
