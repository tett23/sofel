//! Validation of whole fonts: the table directory, the collection header
//! and every table that has a validator, with the context each needs.

use crate::be::{self, to_u32, to_usize};
use crate::hhea::Hhea;
use crate::layout::damage::{DamageEvent, inspect_damage};
use crate::loca::Loca;
use crate::maxp::Maxp;
use crate::sfnt::{Collection, Sfnt, TableRecord};
use crate::{
    CountWidth, Diagnostic, Kind, Tag, avar, base, cbdt, cblc, cff, cmap, colr, cpal,
    feature_variations, fvar, gdef, glyf, gpos, gsub, gvar, head, hhea, hmtx, hvar, loca, math,
    maxp, mvar, name, os2, post, sbix, stat, svg, varc, vhea, vmtx, vorg,
};

/// The pseudo tag of diagnostics about the table directory.
pub const DIRECTORY: Tag = Tag::new(b"sfnt");
/// The pseudo tag of diagnostics about the collection header.
pub const COLLECTION: Tag = Tag::new(b"ttcf");

/// The sfnt table checksum: the sum of big-endian `u32` words, the last
/// one padded with zeros. For `head`, `checksumAdjustment` counts as 0.
pub fn checksum(tag: Tag, data: &[u8]) -> u32 {
    let (words, rest) = data.as_chunks::<4>();
    let mut sum = words
        .iter()
        .fold(0u32, |s, w| s.wrapping_add(u32::from_be_bytes(*w)));
    if !rest.is_empty() {
        let mut last = [0u8; 4];
        for (d, s) in last.iter_mut().zip(rest) {
            *d = *s;
        }
        sum = sum.wrapping_add(u32::from_be_bytes(last));
    }
    if tag == head::TAG
        && let Some(adjustment) = be::u32_at(data, 8)
    {
        sum = sum.wrapping_sub(adjustment);
    }
    sum
}

/// Checks the table records: order, duplicates, bounds and checksums.
/// Offsets are from the start of the directory.
pub fn directory(sfnt: &Sfnt<'_>, report: &mut dyn FnMut(Diagnostic)) {
    let file = sfnt.file();
    let mut previous: Option<Tag> = None;
    for (i, r) in sfnt.records().enumerate() {
        let at = to_u32(i.saturating_mul(16).saturating_add(12));
        let mut emit = |kind: Kind| report(Diagnostic::new(DIRECTORY, at, kind));
        match previous {
            Some(p) if p == r.tag => emit(Kind::DuplicateTable { tag: r.tag }),
            Some(p) if r.tag < p => emit(Kind::RecordsNotSorted { index: to_u32(i) }),
            _ => {}
        }
        previous = Some(r.tag);
        match r.data(file) {
            None => emit(Kind::OffsetOutOfBounds {
                offset: r.offset,
                length: r.length,
                available: to_u32(file.len()),
            }),
            Some(data) => {
                let computed = checksum(r.tag, data);
                if computed != r.checksum {
                    emit(Kind::ChecksumMismatch {
                        stored: r.checksum,
                        computed,
                    });
                }
            }
        }
    }
}

/// Checks the collection header and the bounds of each font's offset.
pub fn collection(c: &Collection<'_>, report: &mut dyn FnMut(Diagnostic)) {
    for i in 0..c.num_fonts() {
        let at = to_u32(to_usize(i).saturating_mul(4).saturating_add(12));
        if let Err(e) = c.font(i) {
            let offset = c.offset(i).unwrap_or(0);
            let kind = match e {
                crate::ReadError::UnknownVersion(found) => Kind::UnknownVersion { found },
                _ => Kind::OffsetOutOfBounds {
                    offset,
                    length: 12,
                    available: to_u32(c.file().len()),
                },
            };
            report(Diagnostic::new(COLLECTION, at, kind));
        }
    }
}

fn table<'a>(sfnt: &Sfnt<'a>, tag: Tag) -> Option<&'a [u8]> {
    sfnt.record(tag)
        .as_ref()
        .and_then(|r: &TableRecord| r.data(sfnt.file()))
}

/// Validates the directory and every table that has a validator.
/// `hmtx` is checked with the counts from `hhea` and `maxp`, `HMTX` with
/// those from `HHEA` and `MAXP` (or `maxp`), `vmtx` and `VMTX` likewise
/// with `vhea` and `VHEA`, and `cmap` and `VORG` glyph ids against
/// `MAXP` or `maxp`, `MATH` glyph ids likewise, and `CFF ` and `CFF2` with the glyph count (and,
/// for `CFF2`, the axis count of `fvar`) (ADR 0021), and `avar` with the
/// axis count of `fvar` (ADR 0022).
pub fn sfnt(sfnt: &Sfnt<'_>, report: &mut dyn FnMut(Diagnostic)) {
    sfnt_inner(sfnt, report, None);
}

/// Validates the font as [`sfnt`] does, and reports extra layout damage
/// events without repeating any original diagnostics. `report` receives
/// every original diagnostic exactly once; `damage` receives only
/// [`DamageEvent::Unchecked`] and [`DamageEvent::WorkLimit`] events for
/// GDEF, GSUB and GPOS. The caller groups those events by face and table.
pub fn sfnt_with_layout_damage(
    sfnt: &Sfnt<'_>,
    report: &mut dyn FnMut(Diagnostic),
    damage: &mut dyn FnMut(DamageEvent),
) {
    sfnt_inner(sfnt, report, Some(damage));
}

fn sfnt_inner(
    sfnt: &Sfnt<'_>,
    report: &mut dyn FnMut(Diagnostic),
    mut damage: Option<&mut dyn FnMut(DamageEvent)>,
) {
    directory(sfnt, report);
    if let Some(d) = table(sfnt, head::TAG) {
        head::validate(d, report);
    }
    if let Some(d) = table(sfnt, head::TAG_BHED) {
        head::validate_tagged(head::TAG_BHED, d, report);
    }
    let maxp_narrow = table(sfnt, maxp::TAG);
    let maxp_wide = table(sfnt, maxp::TAG_WIDE);
    if let Some(d) = maxp_narrow {
        maxp::validate(d, CountWidth::U16, report);
    }
    if let Some(d) = maxp_wide {
        maxp::validate(d, CountWidth::U24, report);
    }
    let glyphs_narrow = maxp_narrow
        .and_then(|d| Maxp::new(d, CountWidth::U16).ok())
        .map(|m| m.num_glyphs());
    let glyphs_wide = maxp_wide
        .and_then(|d| Maxp::new(d, CountWidth::U24).ok())
        .map(|m| m.num_glyphs());

    if let Some(data) = table(sfnt, crate::kerx::TAG)
        && let Err(d) = crate::kerx::Kerx::new(data, glyphs_wide.or(glyphs_narrow).unwrap_or(0))
    {
        report(d);
    }
    let pairs = [
        (hhea::TAG, hmtx::TAG, CountWidth::U16, glyphs_narrow),
        (
            hhea::TAG_WIDE,
            hmtx::TAG_WIDE,
            CountWidth::U24,
            glyphs_wide.or(glyphs_narrow),
        ),
        (vhea::TAG, vmtx::TAG, CountWidth::U16, glyphs_narrow),
        (
            vhea::TAG_WIDE,
            vmtx::TAG_WIDE,
            CountWidth::U24,
            glyphs_wide.or(glyphs_narrow),
        ),
    ];
    for (hhea_tag, hmtx_tag, width, glyphs) in pairs {
        let Some(hhea_data) = table(sfnt, hhea_tag) else {
            continue;
        };
        hhea::validate_tagged(hhea_tag, hhea_data, width, report);
        let metrics = Hhea::new(hhea_data, width)
            .ok()
            .map(|h| h.number_of_h_metrics());
        if let (Some(d), Some(metrics), Some(glyphs)) = (table(sfnt, hmtx_tag), metrics, glyphs) {
            hmtx::validate_tagged(hmtx_tag, d, metrics, glyphs, report);
        }
    }
    if let Some(d) = table(sfnt, cmap::TAG) {
        cmap::validate(d, glyphs_wide.or(glyphs_narrow), report);
    }
    if let Some(d) = table(sfnt, os2::TAG) {
        os2::validate(d, report);
    }
    if let Some(d) = table(sfnt, name::TAG) {
        name::validate(d, report);
    }
    if let Some(d) = table(sfnt, post::TAG) {
        post::validate(d, report);
    }
    if let Some(d) = table(sfnt, vorg::TAG) {
        vorg::validate(d, glyphs_wide.or(glyphs_narrow), report);
    }
    if let Some(d) = table(sfnt, cff::TAG) {
        cff::validate(d, glyphs_wide.or(glyphs_narrow), report);
    }
    if let Some(d) = table(sfnt, cff::TAG2) {
        let axes = table(sfnt, Tag::new(b"fvar")).and_then(|f| be::u16_at(f, 8));
        cff::validate2(d, glyphs_wide.or(glyphs_narrow), axes, report);
    }
    if let Some(d) = table(sfnt, base::TAG) {
        base::validate(d, report);
    }
    if let Some(d) = table(sfnt, math::TAG) {
        math::validate(d, glyphs_wide.or(glyphs_narrow), report);
    }
    let fvar_data = table(sfnt, fvar::TAG);
    let axes = fvar_data
        .and_then(|f| fvar::Fvar::new(f).ok())
        .map(|f| u16::try_from(f.axis_count()).unwrap_or(u16::MAX));
    if let Some(d) = fvar_data {
        fvar::validate(d, report);
    }
    if let Some(d) = table(sfnt, avar::TAG) {
        avar::validate(d, axes, report);
    }
    for tag in [hvar::TAG, hvar::TAG_VVAR] {
        if let Some(d) = table(sfnt, tag) {
            hvar::validate(d, tag, axes, report);
        }
    }
    if let Some(d) = table(sfnt, mvar::TAG) {
        mvar::validate(d, axes, report);
    }
    if let Some(d) = table(sfnt, varc::TAG) {
        varc::validate(d, report);
    }
    if let Some(d) = table(sfnt, stat::TAG) {
        stat::validate(d, report);
    }
    color_tables(sfnt, glyphs_wide.or(glyphs_narrow), report);
    let gdef_data = table(sfnt, gdef::TAG);
    if let Some(d) = gdef_data {
        validate_layout(gdef::TAG, d, None, axes, report, &mut damage);
    }
    // Lookups may only use mark glyph sets that GDEF defines.
    let mark_sets = Some(
        gdef_data
            .and_then(|d| gdef::Gdef::new(d).ok())
            .and_then(|g| g.mark_glyph_sets().ok().flatten())
            .map_or(0, |s| u16::try_from(s.len()).unwrap_or(u16::MAX)),
    );
    if let Some(d) = table(sfnt, gsub::TAG) {
        validate_layout(gsub::TAG, d, mark_sets, axes, report, &mut damage);
    }
    if let Some(d) = table(sfnt, gpos::TAG) {
        validate_layout(gpos::TAG, d, mark_sets, axes, report, &mut damage);
    }
    // GLYF and LOCA replace glyf and loca when GLYF is present (ADR 0011).
    let extended = table(sfnt, glyf::TAG_WIDE).is_some();
    let (glyf_tag, loca_tag) = if extended {
        (glyf::TAG_WIDE, loca::TAG_WIDE)
    } else {
        (glyf::TAG, loca::TAG)
    };
    let format = table(sfnt, head::TAG)
        .and_then(|d| head::Head::new(d).ok())
        .map(|h| h.index_to_loc_format());
    let glyphs = glyphs_wide.or(glyphs_narrow);
    if let (Some(g), Some(l), Some(format), Some(n)) =
        (table(sfnt, glyf_tag), table(sfnt, loca_tag), format, glyphs)
    {
        loca::validate(l, format, n, g.len(), loca_tag, report);
        if let Ok(lv) = Loca::new(l, format) {
            glyf::validate(g, &lv, n, extended, glyf_tag, report);
        }
    }
    let gvar_tag = if extended { gvar::GVAR_TAG } else { gvar::TAG };
    if let Some(d) = table(sfnt, gvar_tag) {
        let glyf_data = table(sfnt, glyf_tag);
        let loca_view = table(sfnt, loca_tag)
            .zip(format)
            .and_then(|(bytes, fmt)| Loca::new(bytes, fmt).ok());
        gvar::validate_with_points(
            d,
            gvar_tag,
            axes,
            glyphs,
            |glyph| {
                let (Some(glyf_data), Some(loca_view)) = (glyf_data, loca_view) else {
                    return None;
                };
                glyph_point_count(glyf_data, &loca_view, glyph, extended)
            },
            report,
        );
    }
}

fn validate_layout(
    tag: Tag,
    data: &[u8],
    mark_sets: Option<u16>,
    axes: Option<u16>,
    report: &mut dyn FnMut(Diagnostic),
    damage: &mut Option<&mut dyn FnMut(DamageEvent)>,
) {
    use crate::layout::sanitize::{Admission, assess};
    if assess(tag, Some(data)) == Admission::Discarded {
        report(Diagnostic::new(
            tag,
            0,
            Kind::LayoutTableDiscarded { table: tag },
        ));
    }
    if let Some(extra) = damage.as_deref_mut() {
        let mut emit = |event| match event {
            DamageEvent::Diagnostic(diagnostic) => report(diagnostic),
            other => extra(other),
        };
        inspect_damage(tag, data, mark_sets, &mut emit);
        if tag != gdef::TAG {
            layout_variations(data, tag, axes, &mut |diagnostic| {
                crate::layout::damage::forward(diagnostic, &mut emit)
            });
        }
    } else if tag == gdef::TAG {
        gdef::validate(data, report);
    } else if tag == gsub::TAG {
        gsub::validate(data, mark_sets, report);
        layout_variations(data, tag, axes, report);
    } else if tag == gpos::TAG {
        gpos::validate(data, mark_sets, report);
        layout_variations(data, tag, axes, report);
    }
}

fn glyph_point_count(
    glyf_data: &[u8],
    loca: &Loca<'_>,
    glyph: u32,
    extended: bool,
) -> Option<usize> {
    let start = to_usize(loca.offset(to_usize(glyph))?);
    let end = to_usize(loca.offset(to_usize(glyph).checked_add(1)?)?);
    let bytes = be::slice_at(glyf_data, start, end.checked_sub(start)?)?;
    if bytes.is_empty() {
        return Some(4);
    }
    let glyph = glyf::Glyph::new(bytes).ok()?;
    if glyph.number_of_contours() >= 0 {
        return to_usize(glyph.simple().ok()?.num_points()).checked_add(4);
    }
    let mut count = 4usize;
    for component in glyph.components(extended) {
        component.ok()?;
        count = count.checked_add(1)?;
    }
    Some(count)
}

fn layout_variations(data: &[u8], tag: Tag, axes: Option<u16>, report: &mut dyn FnMut(Diagnostic)) {
    let Ok(layout) = crate::layout::LayoutTable::new(data) else {
        return;
    };
    let Some(offset) = layout.feature_variations_offset().filter(|o| *o != 0) else {
        return;
    };
    if let Some(bytes) = be::tail_at(data, to_usize(offset)) {
        let lookup_count = layout.lookup_list().ok().flatten().map(|list| list.len());
        let feature_count = layout.feature_list().ok().flatten().map(|list| list.len());
        feature_variations::validate(
            bytes,
            tag,
            offset,
            axes,
            feature_count,
            lookup_count,
            report,
        );
    } else {
        report(Diagnostic::new(
            tag,
            10,
            Kind::OffsetOutOfBounds {
                offset,
                length: 0,
                available: to_u32(data.len()),
            },
        ));
    }
}

/// The color and bitmap tables (ADR 0023): `CPAL`, `COLR` (with the
/// glyph count and the palette size of `CPAL`), the location and data
/// tables of embedded bitmaps in their pairs, `sbix` and `SVG `.
fn color_tables(sfnt: &Sfnt<'_>, glyphs: Option<u32>, report: &mut dyn FnMut(Diagnostic)) {
    let cpal_data = table(sfnt, cpal::TAG);
    if let Some(d) = cpal_data {
        cpal::validate(d, report);
    }
    if let Some(d) = table(sfnt, colr::TAG) {
        let entries = cpal_data
            .and_then(|c| cpal::Cpal::new(c).ok())
            .map(|c| c.num_palette_entries());
        colr::validate(d, glyphs, entries, report);
    }
    let pairs = [
        (cblc::TAG_CBLC, cbdt::TAG_CBDT),
        (cblc::TAG_EBLC, cbdt::TAG_EBDT),
        (cblc::TAG_BLOC, cbdt::TAG_BDAT),
    ];
    for (loc_tag, data_tag) in pairs {
        let loc = table(sfnt, loc_tag);
        if let Some(d) = loc {
            cblc::validate(loc_tag, d, report);
        }
        if let Some(d) = table(sfnt, data_tag) {
            cbdt::validate(data_tag, d, report);
            if let Some(c) = loc.and_then(|l| cblc::Cblc::new(l).ok()) {
                cbdt::validate_images(data_tag, d, &c, report);
            }
        }
    }
    if let Some(d) = table(sfnt, sbix::TAG) {
        sbix::validate(d, glyphs, report);
    }
    if let Some(d) = table(sfnt, svg::TAG) {
        svg::validate(d, report);
    }
}

/// Checks a nullable embedded ItemVariationStore without allocation.
pub(crate) fn variation_store(
    data: &[u8],
    offset: u32,
    field: u32,
    tag: Tag,
    axes: Option<u16>,
    report: &mut dyn FnMut(Diagnostic),
) {
    if offset == 0 {
        return;
    }
    if let Some(child) = be::tail_at(data, to_usize(offset)) {
        crate::item_variation::validate(child, tag, offset, axes, report);
    } else {
        report(Diagnostic::new(
            tag,
            field,
            Kind::OffsetOutOfBounds {
                offset,
                length: 0,
                available: to_u32(data.len()),
            },
        ));
    }
}
