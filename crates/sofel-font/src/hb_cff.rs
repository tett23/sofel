//! HarfBuzz's CFF and CFF2 accelerators (ADR 0021): whether HarfBuzz
//! reads a font's `CFF ` and `CFF2` tables at all (`is_valid`), and what
//! its glyph boxes and glyph names need from them.
//!
//! HarfBuzz checks the tables once, when it builds the accelerator, and
//! gives no boxes and no CFF names when any check fails. [`HbCff1::new`]
//! and [`HbCff2::new`] make the same checks, in the same order and with
//! the same operation budget, and fail exactly when HarfBuzz's
//! accelerators are not valid. They accept more than FreeType in some
//! places (CFF2's FDSelect format 4, charsets read without trimming) and
//! less in others (the glyph count must match `maxp`).
//!
//! The values are the caller's (ADR 0003): `Copy`, without allocation.
//! The Private DICTs of CID-keyed and CFF2 fonts are read again for each
//! glyph, as `sofel-outline` interprets its charstring, instead of being
//! kept.
//!
//! Ported from HarfBuzz 15.1.0: `src/hb-ot-cff1-table.hh` and
//! `src/hb-ot-cff2-table.hh` (`accelerator_templ_t`, `get_glyph_name`,
//! `get_glyph_from_name`), `src/hb-face.cc`
//! (`_hb_face_for_data_reference_table`). Copyright © 2018 Adobe Inc.;
//! © 2009 Red Hat, Inc.; © 2012 Google, Inc. Used under HarfBuzz's
//! "Old MIT" license; see THIRD_PARTY_NOTICES.md.

use sofel_read::Tag;
use sofel_read::be::to_usize;
use sofel_read::cff::hb::{
    DictKind, FdSelectCounts, HbCharset, HbFdSelect, HbIndex, Sanitizer, UNDEF_SID, interpret_dict,
    sanitize_cff2_var_store, sanitize_charset, sanitize_encoding, var_store_region_count,
};
use sofel_read::cff::{self, NUM_STANDARD_STRINGS, STANDARD_STRINGS, Version};

use crate::Font;

/// The bytes of a table as HarfBuzz's `hb_face_reference_table` gives
/// them: a table that runs past the end of the file is cut there; `None`
/// for a missing or empty table and one that starts past the end.
pub(crate) fn hb_table<'a>(font: &Font<'a>, tag: Tag) -> Option<&'a [u8]> {
    let sfnt = font.sfnt();
    let file = sfnt.file();
    let r = sfnt.record(tag)?;
    let start = to_usize(r.offset);
    let rest = file.get(start..)?;
    let len = to_usize(r.length).min(rest.len());
    if len == 0 {
        return None;
    }
    rest.get(..len)
}

/// The Private DICT of a Font DICT as HarfBuzz reads it: its local
/// subroutines and (CFF2) its `vsindex`. `None` when HarfBuzz's
/// accelerator fails on it.
fn private_dict<'a>(
    data: &'a [u8],
    font_dict: (i64, &'a [u8]),
    version: Version,
    sc: &mut Sanitizer,
) -> Option<(HbIndex<'a>, u32)> {
    let (at, bytes) = font_dict;
    if !sc.check_range(at, u64::try_from(bytes.len()).ok()?) {
        return None;
    }
    let kind = match version {
        Version::Cff => DictKind::Cff1Font,
        Version::Cff2 => DictKind::Cff2Font,
    };
    let font = interpret_dict(bytes, kind)?;
    top_private(data, &font, version, sc)
}

/// The Private DICT that `dict` (a Top DICT or a Font DICT) points to.
fn top_private<'a>(
    data: &'a [u8],
    dict: &cff::hb::HbDict,
    version: Version,
    sc: &mut Sanitizer,
) -> Option<(HbIndex<'a>, u32)> {
    let size = dict.private_size;
    let at = sc
        .struct_at(0, dict.private_offset)
        .filter(|&p| sc.check_array(p, size, 1));
    let bytes = match at {
        Some(p) => data
            .get(usize::try_from(p).ok()?..)?
            .get(..to_usize(size))?,
        // HarfBuzz's Null byte string: an error unless the size is 0.
        None if size != 0 => return None,
        None => &[],
    };
    let kind = match version {
        Version::Cff => DictKind::Cff1Private,
        Version::Cff2 => DictKind::Cff2Private,
    };
    let private = interpret_dict(bytes, kind)?;
    let subrs = at
        .and_then(|p| sc.struct_at(p, private.subrs))
        .and_then(|p| HbIndex::sanitize(data, p, version, sc))
        .unwrap_or(HbIndex::null());
    Some((subrs, private.ivs))
}

/// HarfBuzz's CFF (version 1) accelerator (`OT::cff1::accelerator_t`)
/// over a font's `CFF ` table. Built once per font with
/// [`HbCff1::new`]; `Copy`, without allocation.
#[derive(Clone, Copy, Debug)]
pub struct HbCff1<'a> {
    data: &'a [u8],
    num_glyphs: u32,
    char_strings: HbIndex<'a>,
    global_subrs: HbIndex<'a>,
    strings: HbIndex<'a>,
    charset: HbCharset<'a>,
    fd_array: HbIndex<'a>,
    fd_select: HbFdSelect<'a>,
    fd_count: u32,
    is_cid: bool,
    /// The local subroutines of the last Font DICT (the only one without
    /// FDArray), as found while building: HarfBuzz's budget may run out
    /// there and leave them out.
    last_subrs: HbIndex<'a>,
}

impl<'a> HbCff1<'a> {
    /// HarfBuzz's accelerator for `font`'s `CFF ` table, or `None`
    /// exactly when HarfBuzz's is not valid (`is_valid` is false): no
    /// table, a header or INDEX its sanitizer rejects, a Top DICT its
    /// interpreter rejects, a bad charset, encoding, FDArray, FDSelect or
    /// Private DICT, or a CharStrings count other than [`Font::num_glyphs`].
    /// Takes time in proportion to the charset ranges and the Font DICTs.
    pub fn new(font: &Font<'a>) -> Option<HbCff1<'a>> {
        let data = hb_table(font, cff::TAG)?;
        // cff1::sanitize in reference_table.
        if data.len() < 4 || data.first() != Some(&1) {
            return None;
        }
        let face_glyphs = font.num_glyphs();
        let mut sc = Sanitizer::new(data.len());
        let header_size = i64::from(*data.get(2)?);
        let names = HbIndex::sanitize(data, header_size, Version::Cff, &mut sc)?;
        let top_index = sc
            .struct_at(names.position(), names.size_offset())
            .and_then(|p| HbIndex::sanitize(data, p, Version::Cff, &mut sc))
            .filter(|t| !t.is_empty())?;
        let (top_at, top_bytes) = top_index.get_at(0)?;
        if !sc.check_range(top_at, u64::try_from(top_bytes.len()).ok()?) {
            return None;
        }
        let top = interpret_dict(top_bytes, DictKind::Cff1Top)?;

        let charset = if top.charset <= 2 {
            HbCharset::Predefined(top.charset)
        } else {
            sc.struct_at(0, top.charset)
                .and_then(|p| sanitize_charset(data, p, face_glyphs, &mut sc))?
        };

        let is_cid = top.registry != UNDEF_SID;
        let (fd_array, fd_select, fd_count) = if is_cid {
            let fd_array = sc
                .struct_at(0, top.fd_array)
                .and_then(|p| HbIndex::sanitize(data, p, Version::Cff, &mut sc));
            let counts = FdSelectCounts {
                fd_count: fd_array.map_or(0, |a| a.len()),
                glyphs: face_glyphs,
                face_glyphs,
            };
            let fd_select = sc
                .struct_at(0, top.fd_select)
                .and_then(|p| HbFdSelect::sanitize(data, p, Version::Cff, counts, &mut sc));
            let (Some(fd_array), Some(fd_select)) = (fd_array, fd_select) else {
                return None;
            };
            (fd_array, fd_select, fd_array.len())
        } else {
            (HbIndex::null(), HbFdSelect::null(), 1)
        };

        if is_cid {
            if matches!(charset, HbCharset::Predefined(_)) {
                return None;
            }
        } else if top.encoding > 1 {
            sc.struct_at(0, top.encoding)
                .filter(|&p| sanitize_encoding(data, p, &mut sc))?;
        }

        let strings = sc
            .struct_at(top_index.position(), top_index.size_offset())
            .and_then(|p| HbIndex::sanitize(data, p, Version::Cff, &mut sc))?;
        let global_subrs = sc
            .struct_at(strings.position(), strings.size_offset())
            .and_then(|p| HbIndex::sanitize(data, p, Version::Cff, &mut sc))
            .unwrap_or(HbIndex::null());
        let char_strings = sc
            .struct_at(0, top.char_strings)
            .and_then(|p| HbIndex::sanitize(data, p, Version::Cff, &mut sc))?;
        let num_glyphs = char_strings.len();
        if num_glyphs != face_glyphs {
            return None;
        }

        let mut last_subrs = HbIndex::null();
        if is_cid {
            for i in 0..fd_count {
                let (subrs, _) = private_dict(data, fd_array.get_at(i)?, Version::Cff, &mut sc)?;
                last_subrs = subrs;
            }
        } else {
            let (subrs, _) = top_private(data, &top, Version::Cff, &mut sc)?;
            last_subrs = subrs;
        }

        Some(HbCff1 {
            data,
            num_glyphs,
            char_strings,
            global_subrs,
            strings,
            charset,
            fd_array,
            fd_select,
            fd_count,
            is_cid,
            last_subrs,
        })
    }

    /// The table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// The glyph count: the CharStrings count, equal to
    /// [`Font::num_glyphs`].
    pub fn num_glyphs(&self) -> u32 {
        self.num_glyphs
    }

    /// Whether the Top DICT has `ROS` (HarfBuzz's `is_CID`).
    pub fn is_cid(&self) -> bool {
        self.is_cid
    }

    /// The charstring of `glyph` as HarfBuzz reads it (`CFFIndex::operator
    /// []`): `None` for HarfBuzz's empty string, as for a glyph id not
    /// below the count.
    pub fn char_string(&self, glyph: u32) -> Option<&'a [u8]> {
        self.char_strings.get(glyph)
    }

    /// The Global Subrs INDEX (empty when HarfBuzz could not read it).
    pub fn global_subrs(&self) -> HbIndex<'a> {
        self.global_subrs
    }

    /// The CharStrings INDEX (`charStrings`).
    pub fn char_strings(&self) -> HbIndex<'a> {
        self.char_strings
    }

    /// The Font DICT of `glyph` (`fdSelect->get_fd`): 0 without FDSelect.
    pub fn fd(&self, glyph: u32) -> u32 {
        self.fd_select.fd(glyph)
    }

    /// The local subroutines of Font DICT `fd`; empty for Font DICTs
    /// HarfBuzz does not have. Reads the Font DICT and its Private DICT
    /// again (except for the last one).
    pub fn local_subrs(&self, fd: u32) -> HbIndex<'a> {
        if fd >= self.fd_count {
            return HbIndex::null();
        }
        if fd.saturating_add(1) == self.fd_count {
            return self.last_subrs;
        }
        let mut sc = Sanitizer::unlimited(self.data.len());
        self.fd_array
            .get_at(fd)
            .and_then(|d| private_dict(self.data, d, Version::Cff, &mut sc))
            .map_or(HbIndex::null(), |(s, _)| s)
    }

    /// `std_code_to_glyph`: the glyph of Standard encoding `code` (for
    /// seac), 0 when there is none.
    pub fn std_code_to_glyph(&self, code: u32) -> u32 {
        self.charset.std_code_to_glyph(code, self.num_glyphs)
    }

    /// `glyph_to_sid`: the SID of `glyph` in the charset.
    pub fn sid(&self, glyph: u32) -> u32 {
        self.charset.sid(glyph, self.num_glyphs)
    }

    /// `sid_to_glyph`: the first glyph with `sid`, 0 when there is none.
    pub fn glyph_of_sid(&self, sid: u32) -> u32 {
        self.charset.glyph(sid, self.num_glyphs)
    }

    /// The string of `sid` as HarfBuzz reads it for glyph names: a
    /// standard string below 391, else the String INDEX element (empty
    /// when it cannot be read).
    pub fn string(&self, sid: u32) -> &'a [u8] {
        match sid.checked_sub(NUM_STANDARD_STRINGS) {
            None => STANDARD_STRINGS
                .get(to_usize(sid))
                .map_or(&[], |s| s.as_bytes()),
            Some(k) => self.strings.get(k).unwrap_or_default(),
        }
    }

    /// HarfBuzz's `get_glyph_name` before it copies the name: the string
    /// of the glyph's SID; `None` for CID-keyed fonts, glyph ids not
    /// below the count and empty strings. HarfBuzz copies the name with
    /// `strncpy`, which stops at a 0 byte.
    pub fn glyph_name(&self, glyph: u32) -> Option<&'a [u8]> {
        if glyph >= self.num_glyphs || self.is_cid {
            return None;
        }
        let s = self.string(self.sid(glyph));
        (!s.is_empty()).then_some(s)
    }
}

/// HarfBuzz's CFF2 accelerator (`OT::cff2::accelerator_t`) over a font's
/// `CFF2` table. Built once per font with [`HbCff2::new`]; `Copy`,
/// without allocation.
#[derive(Clone, Copy, Debug)]
pub struct HbCff2<'a> {
    data: &'a [u8],
    num_glyphs: u32,
    char_strings: HbIndex<'a>,
    global_subrs: HbIndex<'a>,
    fd_array: HbIndex<'a>,
    fd_select: HbFdSelect<'a>,
    fd_count: u32,
    var_store: Option<i64>,
    last_subrs: HbIndex<'a>,
    last_ivs: u32,
}

impl<'a> HbCff2<'a> {
    /// HarfBuzz's accelerator for `font`'s `CFF2` table, or `None`
    /// exactly when HarfBuzz's is not valid: no table, a header, Top
    /// DICT, INDEX, FDSelect (formats 0, 3 and 4) or Private DICT
    /// HarfBuzz rejects, no Global Subrs, CharStrings or FDArray, or a
    /// CharStrings count other than [`Font::num_glyphs`] (fewer is
    /// accepted when the font has a `VARC` table). A broken Variation
    /// Store is left out, not an error.
    pub fn new(font: &Font<'a>) -> Option<HbCff2<'a>> {
        let data = hb_table(font, cff::TAG2)?;
        if data.len() < 5 || data.first() != Some(&2) {
            return None;
        }
        let face_glyphs = font.num_glyphs();
        let mut sc = Sanitizer::new(data.len());
        let top_at = u16::from(*data.get(2)?);
        let top_size = u16::from_be_bytes([*data.get(3)?, *data.get(4)?]);
        if !sc.check_range(i64::from(top_at), u64::from(top_size)) {
            return None;
        }
        let top_bytes = data
            .get(usize::from(top_at)..)?
            .get(..usize::from(top_size))?;
        let top = interpret_dict(top_bytes, DictKind::Cff2Top)?;

        let gsubrs_offset = i32::from(top_at).saturating_add(i32::from(top_size));
        let global_subrs = sc
            .struct_at(0, gsubrs_offset)
            .and_then(|p| HbIndex::sanitize(data, p, Version::Cff2, &mut sc));
        let var_store = sc
            .struct_at(0, top.vstore)
            .filter(|&p| sanitize_cff2_var_store(data, p, &mut sc));
        let char_strings = sc
            .struct_at(0, top.char_strings)
            .and_then(|p| HbIndex::sanitize(data, p, Version::Cff2, &mut sc));
        let fd_array = sc
            .struct_at(0, top.fd_array)
            .and_then(|p| HbIndex::sanitize(data, p, Version::Cff2, &mut sc));
        let (Some(char_strings), Some(global_subrs), Some(fd_array)) =
            (char_strings, global_subrs, fd_array)
        else {
            return None;
        };

        let num_glyphs = char_strings.len();
        if num_glyphs != face_glyphs {
            // VARC can define glyphs past the CharStrings.
            let varc = hb_table(font, Tag::new(b"VARC")).is_some();
            if !(num_glyphs < face_glyphs && varc) {
                return None;
            }
        }

        let counts = FdSelectCounts {
            fd_count: fd_array.len(),
            glyphs: num_glyphs,
            face_glyphs,
        };
        let fd_select = sc
            .struct_at(0, top.fd_select)
            .and_then(|p| HbFdSelect::sanitize(data, p, Version::Cff2, counts, &mut sc));
        if top.fd_select != 0 && fd_select.is_none() {
            return None;
        }
        let fd_select = fd_select.unwrap_or(HbFdSelect::null());

        let fd_count = fd_array.len();
        let mut last = (HbIndex::null(), 0);
        for i in 0..fd_count {
            last = private_dict(data, fd_array.get_at(i)?, Version::Cff2, &mut sc)?;
        }

        Some(HbCff2 {
            data,
            num_glyphs,
            char_strings,
            global_subrs,
            fd_array,
            fd_select,
            fd_count,
            var_store,
            last_subrs: last.0,
            last_ivs: last.1,
        })
    }

    /// The table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// The CharStrings count. HarfBuzz gives no boxes for glyphs past it.
    pub fn num_glyphs(&self) -> u32 {
        self.num_glyphs
    }

    /// The charstring of `glyph` as HarfBuzz reads it; `None` for
    /// HarfBuzz's empty string.
    pub fn char_string(&self, glyph: u32) -> Option<&'a [u8]> {
        self.char_strings.get(glyph)
    }

    /// The Global Subrs INDEX.
    pub fn global_subrs(&self) -> HbIndex<'a> {
        self.global_subrs
    }

    /// The Font DICT of `glyph`: 0 without FDSelect.
    pub fn fd(&self, glyph: u32) -> u32 {
        self.fd_select.fd(glyph)
    }

    /// The local subroutines and the `vsindex` of Font DICT `fd`'s Private
    /// DICT; empty and 0 for Font DICTs HarfBuzz does not have. Reads the
    /// DICTs again (except for the last Font DICT).
    pub fn private(&self, fd: u32) -> (HbIndex<'a>, u32) {
        if fd >= self.fd_count {
            return (HbIndex::null(), 0);
        }
        if fd.saturating_add(1) == self.fd_count {
            return (self.last_subrs, self.last_ivs);
        }
        let mut sc = Sanitizer::unlimited(self.data.len());
        self.fd_array
            .get_at(fd)
            .and_then(|d| private_dict(self.data, d, Version::Cff2, &mut sc))
            .unwrap_or((HbIndex::null(), 0))
    }

    /// Where the Variation Store (its length field) starts in
    /// [`HbCff2::data`]; `None` without a valid one.
    pub fn var_store(&self) -> Option<i64> {
        self.var_store
    }

    /// The FDArray INDEX.
    pub fn fd_array(&self) -> HbIndex<'a> {
        self.fd_array
    }

    /// The FDSelect: HarfBuzz's Null FDSelect (format 0xFF) when the
    /// font has none.
    pub fn fd_select(&self) -> HbFdSelect<'a> {
        self.fd_select
    }

    /// The number of regions of ItemVariationData `ivs`
    /// (`get_region_index_count`): what `blend` takes per value. 0
    /// without a (valid) Variation Store or such a subtable.
    pub fn region_count(&self, ivs: u32) -> u32 {
        self.var_store
            .map_or(0, |p| var_store_region_count(self.data, p, ivs))
    }
}
