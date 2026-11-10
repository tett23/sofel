//! FreeType's face-wide values and vertical advances (ADR 0016).
//!
//! Ported from FreeType 2.15.0 (commit
//! 37515cf56ba3349656049edbed89ffc6f6466f2d): `src/sfnt/sfobjs.c`
//! (`sfnt_load_face`), `src/sfnt/ttmtx.c` (`tt_face_load_hhea`,
//! `tt_face_get_metrics`), `src/sfnt/ttload.c` (`tt_face_load_post`) and
//! `src/truetype/ttgload.c` (`TT_Get_VMetrics`). Copyright (C) 1996-2026
//! by David Turner, Robert Wilhelm, and Werner Lemberg. Used under the
//! FreeType License (FTL); see THIRD_PARTY_NOTICES.md.

use sofel_read::hhea::Hhea;
use sofel_read::os2::Os2;
use sofel_read::sfnt::Sfnt;
use sofel_read::{CountWidth, GlyphId, Tag, be, glyf, hhea, hmtx, os2, post, vhea, vmtx};

use crate::Font;

/// The values of FreeType's `FT_Face` that come from the tables, in font
/// units. All 0 for faces FreeType does not treat as scalable.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FtFaceMetrics {
    /// `ascender`.
    pub ascender: i16,
    /// `descender`.
    pub descender: i16,
    /// `height`: the baseline-to-baseline distance.
    pub height: i16,
    /// `max_advance_width`.
    pub max_advance_width: i16,
    /// `max_advance_height`.
    pub max_advance_height: i16,
    /// `underline_position`: the center of the underline.
    pub underline_position: i16,
    /// `underline_thickness`.
    pub underline_thickness: i16,
    /// `bbox`: `xMin`, `yMin`, `xMax`, `yMax` of `head`.
    pub bbox: [i16; 4],
}

/// `(FT_Short) v`.
fn short(v: i32) -> i16 {
    let b = v.to_le_bytes();
    i16::from_le_bytes([b[0], b[1]])
}

/// The header and metrics table FreeType reads (`tt_face_load_hhea` and
/// `tt_face_load_hmtx`): the wide tables with `GLYF`, otherwise the
/// narrow ones, or the wide ones when the narrow header is missing.
pub(crate) fn ft_hea<'a>(sfnt: &Sfnt<'a>, tags: [Tag; 4]) -> Option<(Hhea<'a>, Option<&'a [u8]>)> {
    let [narrow_hea, narrow_mtx, wide_hea, wide_mtx] = tags;
    let wide = sfnt.table(glyf::TAG_WIDE).is_some() || sfnt.table(narrow_hea).is_none();
    let (hea, mtx, width) = if wide {
        (wide_hea, wide_mtx, CountWidth::U24)
    } else {
        (narrow_hea, narrow_mtx, CountWidth::U16)
    };
    let h = Hhea::new(sfnt.table(hea)?, width).ok()?;
    Some((h, sfnt.table(mtx)))
}

impl<'a> Font<'a> {
    fn ft_os2(&self) -> Option<Os2<'a>> {
        self.os2
    }

    pub(crate) fn ft_vertical(&self) -> Option<(Hhea<'a>, &'a [u8])> {
        let (h, mtx) = ft_hea(
            &self.sfnt,
            [vhea::TAG, vmtx::TAG, vhea::TAG_WIDE, vmtx::TAG_WIDE],
        )?;
        Some((h, mtx?))
    }

    /// FreeType's `has_outline` in `sfnt_load_face`: glyf, `GLYF`, CFF
    /// or CFF2 data, no CBLC or CBDT, and for `true` fonts an `hhea`.
    pub(crate) fn ft_has_outline(&self) -> bool {
        let t = |tag: &[u8; 4]| self.table(Tag::new(tag)).is_some();
        let outline = t(b"GLYF") || t(b"glyf") || t(b"CFF ") || t(b"CFF2");
        let bitmap = t(b"CBLC") || t(b"CBDT");
        let mac = self.sfnt.version() == u32::from_be_bytes(*b"true") && !t(b"hhea") && !t(b"HHEA");
        outline && !bitmap && !mac
    }

    /// FreeType's face values (`sfnt_load_face`): the ascender,
    /// descender and height from `OS/2`'s `sTypo*` with
    /// `USE_TYPO_METRICS`, else from `hhea`, and when those are both 0,
    /// from `sTypo*` if either is non-zero, else from `usWinAscent` and
    /// `usWinDescent`; signs as stored. The maximum vertical advance is
    /// `vhea`'s, or the height without vertical metrics. The underline
    /// position is `post`'s moved to the center of the line. Values wrap
    /// to 16 bits as FreeType's do. All 0 when FreeType does not treat the
    /// face as scalable (no outlines, or CBLC/CBDT bitmaps).
    pub(crate) fn ft_face_metrics_default(&self) -> FtFaceMetrics {
        if !self.ft_has_outline() {
            return FtFaceMetrics::default();
        }
        let hea = ft_hea(
            &self.sfnt,
            [hhea::TAG, hmtx::TAG, hhea::TAG_WIDE, hmtx::TAG_WIDE],
        );
        let (h_asc, h_desc, h_gap, max_aw) = hea.map_or((0, 0, 0, 0), |(h, _)| {
            (
                i32::from(h.ascender()),
                i32::from(h.descender()),
                i32::from(h.line_gap()),
                h.advance_width_max(),
            )
        });
        let os2 = self.ft_os2();
        let typo = |o: &Os2<'_>| {
            let (a, d) = (
                i32::from(o.s_typo_ascender()),
                i32::from(o.s_typo_descender()),
            );
            (
                a,
                d,
                a.wrapping_sub(d)
                    .wrapping_add(i32::from(o.s_typo_line_gap())),
            )
        };
        let (asc, desc, height) = match os2 {
            Some(o) if o.fs_selection() & os2::USE_TYPO_METRICS != 0 => typo(&o),
            _ if h_asc != 0 || h_desc != 0 => (
                h_asc,
                h_desc,
                h_asc.wrapping_sub(h_desc).wrapping_add(h_gap),
            ),
            Some(o) if o.s_typo_ascender() != 0 || o.s_typo_descender() != 0 => typo(&o),
            Some(o) => {
                let a = i32::from(short(i32::from(o.us_win_ascent())));
                let d = 0i32.wrapping_sub(i32::from(short(i32::from(o.us_win_descent()))));
                (a, d, a.wrapping_sub(d))
            }
            None => (
                h_asc,
                h_desc,
                h_asc.wrapping_sub(h_desc).wrapping_add(h_gap),
            ),
        };
        let (asc, desc) = (short(asc), short(desc));
        let height = short(height);
        let max_ah = self
            .ft_vertical()
            .map_or(height, |(v, _)| short(i32::from(v.advance_width_max())));
        let post = self.table(post::TAG).filter(|d| d.len() >= 32);
        let (ul_pos, ul_thick) = post.map_or((0, 0), |d| {
            (
                be::i16_at(d, 8).unwrap_or(0),
                be::i16_at(d, 10).unwrap_or(0),
            )
        });
        let head = self.head;
        FtFaceMetrics {
            ascender: asc,
            descender: desc,
            height,
            max_advance_width: short(i32::from(max_aw)),
            max_advance_height: max_ah,
            underline_position: short(i32::from(ul_pos).wrapping_sub(i32::from(ul_thick) / 2)),
            underline_thickness: ul_thick,
            bbox: [head.x_min(), head.y_min(), head.x_max(), head.y_max()],
        }
    }

    /// The vertical advance as FreeType's `FT_Get_Advance` gives it with
    /// `FT_LOAD_VERTICAL_LAYOUT | FT_LOAD_NO_SCALE` for TrueType fonts
    /// (`TT_Get_VMetrics`): from `vmtx` read as `tt_face_get_metrics`
    /// does, else `|sTypoAscender − sTypoDescender|` of `OS/2`, else
    /// `|ascender − descender|` of `hhea`. Positive downward, unlike
    /// [`crate::var::Instance::v_advance`]. `None` for glyph ids not below the glyph
    /// count. (FreeType's CFF driver loads the glyph instead when there
    /// is no `vmtx`; that comes with ADR 0021.)
    pub(crate) fn ft_v_advance_default(&self, glyph: GlyphId) -> Option<u16> {
        if glyph.0 >= self.num_glyphs {
            return None;
        }
        if let Some((v, mtx)) = self.ft_vertical() {
            return Some(ft_metrics_advance(mtx, v.number_of_h_metrics(), glyph));
        }
        let abs = |a: i16, d: i16| {
            let v = i32::from(a).wrapping_sub(i32::from(d)).unsigned_abs();
            u16::try_from(v & 0xFFFF).unwrap_or(0)
        };
        if let Some(o) = self.ft_os2() {
            return Some(abs(o.s_typo_ascender(), o.s_typo_descender()));
        }
        let h = ft_hea(
            &self.sfnt,
            [hhea::TAG, hmtx::TAG, hhea::TAG_WIDE, hmtx::TAG_WIDE],
        );
        Some(h.map_or(0, |(h, _)| abs(h.ascender(), h.descender())))
    }
}

/// `tt_face_get_metrics` for the advance, without variations.
fn ft_metrics_advance(mtx: &[u8], long_metrics: u32, glyph: GlyphId) -> u16 {
    let read = |pos: u64| {
        let at = usize::try_from(pos).ok()?;
        be::u16_at(mtx, at)
    };
    let size = u64::try_from(mtx.len()).unwrap_or(u64::MAX);
    let (g, k) = (u64::from(glyph.0), u64::from(long_metrics));
    let advance = if k == 0 {
        None
    } else if g < k {
        let pos = g.saturating_mul(4);
        (pos.saturating_add(4) <= size).then(|| read(pos)).flatten()
    } else {
        let pos = k.saturating_sub(1).saturating_mul(4);
        (pos.saturating_add(2) <= size).then(|| read(pos)).flatten()
    };
    advance.unwrap_or(0)
}
