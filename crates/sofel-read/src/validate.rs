//! Validation of whole fonts: the table directory, the collection header
//! and every table that has a validator, with the context each needs.

use crate::be::{self, to_u32, to_usize};
use crate::hhea::Hhea;
use crate::loca::Loca;
use crate::maxp::Maxp;
use crate::sfnt::{Collection, Sfnt, TableRecord};
use crate::{CountWidth, Diagnostic, Kind, Tag, cmap, glyf, head, hhea, hmtx, loca, maxp};

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
/// those from `HHEA` and `MAXP` (or `maxp`), and `cmap` glyph ids
/// against `MAXP` or `maxp`.
pub fn sfnt(sfnt: &Sfnt<'_>, report: &mut dyn FnMut(Diagnostic)) {
    directory(sfnt, report);
    if let Some(d) = table(sfnt, head::TAG) {
        head::validate(d, report);
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

    let pairs = [
        (hhea::TAG, hmtx::TAG, CountWidth::U16, glyphs_narrow),
        (
            hhea::TAG_WIDE,
            hmtx::TAG_WIDE,
            CountWidth::U24,
            glyphs_wide.or(glyphs_narrow),
        ),
    ];
    for (hhea_tag, hmtx_tag, width, glyphs) in pairs {
        let Some(hhea_data) = table(sfnt, hhea_tag) else {
            continue;
        };
        hhea::validate(hhea_data, width, report);
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
}
