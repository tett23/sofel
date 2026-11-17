//! CFF and CFF2 as FreeType's CFF driver opens them (ADR 0021): the load
//! checks, the normalized FontMatrix, the CID queries, the charset glyph
//! names and the PostScript font information.
//!
//! Ported from FreeType 2.15.0: `src/cff/cffload.c` (`cff_font_load`,
//! `cff_subfont_load`, `cff_load_private_dict`, `cff_vstore_load`,
//! `CFF_Load_FD_Select`, `cff_charset_load`, `cff_encoding_load`),
//! `src/cff/cffobjs.c` (`cff_face_init`), `src/cff/cffdrivr.c`
//! (`cff_get_ros`, `cff_get_cid_from_glyph_index`, `cff_get_glyph_name`,
//! `cff_get_name_index`, `cff_ps_get_font_info`, `cff_ps_get_font_extra`),
//! `src/cff/cffgload.c` (`cff_slot_load`) and `src/base/ftcalc.c`
//! (`FT_Matrix_Multiply_Scaled`, `FT_Vector_Transform_Scaled`). Copyright
//! (C) 1996-2026 by David Turner, Robert Wilhelm, and Werner Lemberg. Used
//! under the FreeType License (FTL); see THIRD_PARTY_NOTICES.md.

use core::fmt;

use sofel_read::be::{self, to_usize};
use sofel_read::cff::dict::{
    BlendWeights, DictError, FontDict, FontMatrix, NO_SID, PrivateDict, TopDict,
};
use sofel_read::cff::{
    self, Charset, CharsetIndex, Encoding, FdSelect, FtBlend, Header, Index, Version, ft,
};
use sofel_read::item_variation::ItemVariationStore;
use sofel_read::{GlyphId, Tag};

use crate::Font;

/// Why FreeType's CFF driver cannot open a face (ADR 0021).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CffError {
    /// The font has neither `CFF2` nor `CFF ` (with a non-zero length),
    /// or its sfnt version is not `OTTO` (FreeType opens it with the
    /// TrueType driver or not at all).
    NotCff,
    /// The header is cut or has a wrong version, `hdrSize` or offset size.
    Header,
    /// An INDEX (Name, Top DICT, String, Global Subrs, CharStrings,
    /// FDArray or Local Subrs) cannot be read.
    Index,
    /// An invalid Name INDEX count or face index, or fewer Top DICTs than names.
    Names,
    /// A Top, Font or Private DICT cannot be parsed.
    Dict(DictError),
    /// A DICT points outside the data.
    OutOfBounds,
    /// The Top DICT has no `CharStrings`.
    NoCharStrings,
    /// The Variation Store cannot be read.
    VariationStore,
    /// More than 256 Font DICTs.
    TooManyFontDicts,
    /// The FDSelect cannot be read (FreeType reads formats 0 and 3 only).
    FdSelect,
    /// The charset cannot be read.
    Charset,
    /// The encoding cannot be read.
    Encoding,
}

impl fmt::Display for CffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CffError::NotCff => f.write_str("not an OpenType font with CFF or CFF2"),
            CffError::Header => f.write_str("bad CFF header"),
            CffError::Index => f.write_str("bad CFF INDEX"),
            CffError::Names => f.write_str("bad CFF Name INDEX"),
            CffError::Dict(e) => write!(f, "bad CFF DICT at {}", e.offset),
            CffError::OutOfBounds => f.write_str("CFF data out of bounds"),
            CffError::NoCharStrings => f.write_str("no CharStrings"),
            CffError::VariationStore => f.write_str("bad CFF2 Variation Store"),
            CffError::TooManyFontDicts => f.write_str("more than 256 Font DICTs"),
            CffError::FdSelect => f.write_str("bad FDSelect"),
            CffError::Charset => f.write_str("bad charset"),
            CffError::Encoding => f.write_str("bad encoding"),
        }
    }
}

impl core::error::Error for CffError {}

/// Why a CID query has no answer (FreeType's `Invalid_Argument`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CidError {
    /// The font is not CID-keyed (no `ROS`; always so for CFF2).
    NotCidKeyed,
    /// The glyph id is not below the CharStrings count.
    InvalidGlyphIndex,
}

impl fmt::Display for CidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CidError::NotCidKeyed => "not a CID-keyed font",
            CidError::InvalidGlyphIndex => "glyph id out of range",
        })
    }
}

impl core::error::Error for CidError {}

/// The registry, ordering and supplement of a CID-keyed font
/// (`FT_Get_CID_Registry_Ordering_Supplement`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ros<'a> {
    /// The registry string; `None` where FreeType gives NULL.
    pub registry: Option<&'a [u8]>,
    /// The ordering string; `None` where FreeType gives NULL.
    pub ordering: Option<&'a [u8]>,
    /// The supplement, truncated to an integer.
    pub supplement: i32,
}

/// FreeType's `PS_FontInfoRec` of a CFF font (`FT_Get_PS_Font_Info`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PsFontInfo<'a> {
    /// `version`; `None` where FreeType gives NULL.
    pub version: Option<&'a [u8]>,
    /// `Notice`.
    pub notice: Option<&'a [u8]>,
    /// `FullName`.
    pub full_name: Option<&'a [u8]>,
    /// `FamilyName`.
    pub family_name: Option<&'a [u8]>,
    /// `Weight`.
    pub weight: Option<&'a [u8]>,
    /// `ItalicAngle`, 16.16 (FreeType's `FT_Long`).
    pub italic_angle: i64,
    /// `isFixedPitch`.
    pub is_fixed_pitch: bool,
    /// `UnderlinePosition`.
    pub underline_position: i16,
    /// `UnderlineThickness`.
    pub underline_thickness: u16,
}

/// A FontMatrix after FreeType's normalization in `cff_face_init`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CffMatrix {
    /// `xx`, `yx`, `xy`, `yy`, 16.16.
    pub m: [i64; 4],
    /// The translation in font units.
    pub offset: (i64, i64),
    /// `units_per_em`.
    pub units_per_em: u64,
}

impl CffMatrix {
    /// Whether the matrix (not the translation) is the identity.
    pub fn is_identity(&self) -> bool {
        self.m == [0x1_0000, 0, 0, 0x1_0000]
    }
}

fn normalize(m: &FontMatrix, upm: u64, mut offset: (i64, i64)) -> CffMatrix {
    let mut v = [m.xx, m.yx, m.xy, m.yy];
    let mut upm = upm;
    let temp = if v[3] != 0 {
        v[3].wrapping_abs()
    } else {
        v[1].wrapping_abs()
    };
    if temp != 0x1_0000 {
        upm = ft::div_fix(upm.cast_signed(), temp).cast_unsigned();
        for x in &mut v {
            *x = ft::div_fix(*x, temp);
        }
        offset = (ft::div_fix(offset.0, temp), ft::div_fix(offset.1, temp));
    }
    CffMatrix {
        m: v,
        offset: (offset.0 >> 16, offset.1 >> 16),
        units_per_em: upm,
    }
}

/// What one glyph's Font DICT gives: the matrix, the Private DICT and
/// the local subroutines (`cff_decoder_prepare`, `cff_slot_load`).
#[derive(Clone, Copy, Debug)]
pub struct Subfont<'a> {
    /// The Font DICT index (0 without an FDArray).
    pub fd: u16,
    /// The Private DICT (FreeType's all-zero values when there is none).
    pub private: PrivateDict,
    /// The Local Subrs INDEX (empty when there are none).
    pub local_subrs: Index<'a>,
    /// The matrix of the glyph, concatenated with the Top DICT's.
    pub matrix: CffMatrix,
}

/// A CFF or CFF2 table as FreeType's CFF driver opened it: the caller's
/// value (ADR 0003), built once with [`Cff::new`]. `Copy` and without
/// allocation; Font DICTs and Private DICTs are read per glyph
/// ([`Cff::subfont`]).
#[derive(Clone, Copy, Debug)]
pub struct Cff<'a> {
    version: Version,
    /// From the start of the table to the end of the file: FreeType reads
    /// through the file stream.
    data: &'a [u8],
    top: TopDict,
    top_matrix: CffMatrix,
    strings: Index<'a>,
    strings_sorted: bool,
    global_subrs: Index<'a>,
    global_subrs_sorted: bool,
    char_strings: Index<'a>,
    charset: Option<Charset<'a>>,
    encoding: Option<Encoding<'a>>,
    fd_array: Option<Index<'a>>,
    fd_select: Option<FdSelect<'a>>,
    store: Option<ItemVariationStore<'a>>,
    /// The axis count of `fvar`, when the font has one.
    fvar_axes: Option<u16>,
    head_upem: u16,
    /// `FormatType` of `post` as FreeType loaded it; `None` when the
    /// sfnt loader could not load `post`.
    post_format: Option<u32>,
    /// Bit `fd`: [`Index::ft_offsets_sorted`] of that Font DICT's Local
    /// Subrs INDEX (bit 0 without an FDArray).
    local_sorted: [u64; 4],
}

/// The table FreeType's `tt_face_goto_table` finds: the first record with
/// the tag and a non-zero length, from its offset to the end of the file.
fn ft_table<'a>(font: &Font<'a>, tag: Tag) -> Option<&'a [u8]> {
    let sfnt = font.sfnt();
    let r = sfnt.records().find(|r| r.tag == tag && r.length != 0)?;
    sfnt.file().get(to_usize(r.offset)..)
}

/// `cff_index_init`: the INDEX at `at` with its data present.
fn ft_index(data: &[u8], at: u64, version: Version) -> Result<Index<'_>, CffError> {
    let tail = usize::try_from(at)
        .ok()
        .and_then(|a| data.get(a..))
        .ok_or(CffError::Index)?;
    let i = Index::new(tail, version).map_err(|_| CffError::Index)?;
    if !i.is_empty() {
        let last = i.offset(i.len()).ok_or(CffError::Index)?;
        if last == 0 || i.end().is_none_or(|e| e > tail.len()) {
            return Err(CffError::Index);
        }
    }
    Ok(i)
}

fn index_end(i: &Index<'_>, at: usize) -> usize {
    at.saturating_add(i.end().unwrap_or(0))
}

/// `cff_vstore_load`.
fn ft_vstore(data: &[u8], offset: u64) -> Result<Option<ItemVariationStore<'_>>, CffError> {
    if offset == 0 {
        return Ok(None);
    }
    let at = usize::try_from(offset)
        .ok()
        .and_then(|o| o.checked_add(2))
        .ok_or(CffError::VariationStore)?;
    let d = data.get(at..).ok_or(CffError::VariationStore)?;
    let s = ItemVariationStore::new(d).map_err(|_| CffError::VariationStore)?;
    if (0..s.data_count()).any(|i| s.item_data(i).is_none()) {
        return Err(CffError::VariationStore);
    }
    Ok(Some(s))
}

/// `CFF_Load_FD_Select`: formats 0 and 3 only.
fn ft_fd_select(data: &[u8], offset: u64, num_glyphs: u32) -> Result<FdSelect<'_>, CffError> {
    let at = usize::try_from(offset).map_err(|_| CffError::FdSelect)?;
    match FdSelect::new(data, at, num_glyphs) {
        Ok(s) if s.format() != 4 => Ok(s),
        _ => Err(CffError::FdSelect),
    }
}

fn private_slice(data: &[u8], size: u64, offset: u64) -> Result<&[u8], CffError> {
    let at = usize::try_from(offset).map_err(|_| CffError::OutOfBounds)?;
    let len = usize::try_from(size).map_err(|_| CffError::OutOfBounds)?;
    be::slice_at(data, at, len).ok_or(CffError::OutOfBounds)
}

impl<'a> Cff<'a> {
    /// Open a bare CFF 1 face by its zero-based Name INDEX position.
    ///
    /// Bare CFF has no sfnt `head`; the default units per em are 1000.
    pub fn from_bare(data: &'a [u8], index: u32) -> Result<Cff<'a>, CffError> {
        Self::from_data(data, Version::Cff, 1000, None, Some(index))
    }

    /// Opens the `CFF2` or else the `CFF ` table of `font` as FreeType's
    /// CFF driver does (`cff_face_init`, `cff_font_load`), failing
    /// exactly when FreeType cannot open the face for CFF reasons. The
    /// sfnt version must be `OTTO`. Takes time in proportion to the
    /// charset ranges and the Font DICTs.
    pub fn new(font: &Font<'a>) -> Result<Cff<'a>, CffError> {
        if font.sfnt().version() != u32::from_be_bytes(*b"OTTO") {
            return Err(CffError::NotCff);
        }
        let fvar_axes = font.table(Tag::new(b"fvar")).and_then(|f| be::u16_at(f, 8));
        // tt_face_load_post reads the 32-byte header through the file.
        let post_format = ft_table(font, Tag::new(b"post"))
            .filter(|p| p.len() >= 32)
            .and_then(|p| be::u32_at(p, 0));
        let c = if let Some(t) = ft_table(font, cff::TAG2) {
            Cff::from_table(t, Version::Cff2, font.units_per_em(), fvar_axes)
        } else {
            let t = ft_table(font, cff::TAG).ok_or(CffError::NotCff)?;
            Cff::from_table(t, Version::Cff, font.units_per_em(), fvar_axes)
        };
        c.map(|c| Cff { post_format, ..c })
    }

    /// Opens a table of an sfnt font: `data` runs from the start of the
    /// table to the end of the file, `units_per_em` is `head`'s, and
    /// `fvar_axes` the axis count of `fvar` if any (ADR 0021).
    pub fn from_table(
        data: &'a [u8],
        version: Version,
        units_per_em: u16,
        fvar_axes: Option<u16>,
    ) -> Result<Cff<'a>, CffError> {
        Self::from_data(data, version, units_per_em, fvar_axes, None)
    }

    fn from_data(
        data: &'a [u8],
        version: Version,
        units_per_em: u16,
        fvar_axes: Option<u16>,
        face_index: Option<u32>,
    ) -> Result<Cff<'a>, CffError> {
        let header = Header::new(data).map_err(|_| CffError::Header)?;
        let hdr = usize::from(header.header_size);
        let empty = Index::empty(version);
        let (top_data, strings, global_subrs) = match version {
            Version::Cff2 => {
                if header.major != 2 || header.header_size < 5 {
                    return Err(CffError::Header);
                }
                let len = usize::from(header.top_dict_length);
                let top = be::slice_at(data, hdr, len).ok_or(CffError::Header)?;
                let g = ft_index(data, hdr.saturating_add(len) as u64, version)?;
                (top, empty, g)
            }
            Version::Cff => {
                if header.major != 1 || header.header_size < 4 || header.off_size > 4 {
                    return Err(CffError::Header);
                }
                if hdr > data.len() {
                    return Err(CffError::Header);
                }
                let names = ft_index(data, hdr as u64, version)?;
                let names_size = names.offset(names.len()).unwrap_or(1).saturating_sub(1);
                if names.len() > 1 && names_size < names.len() {
                    return Err(CffError::Names);
                }
                let top_at = index_end(&names, hdr);
                let tops = ft_index(data, top_at as u64, version)?;
                let strings_at = index_end(&tops, top_at);
                let strings = ft_index(data, strings_at as u64, version)?;
                let g_at = index_end(&strings, strings_at);
                let g = ft_index(data, g_at as u64, version)?;
                if names.len() > tops.len()
                    || face_index.is_none() && names.len() > 1
                    || face_index.is_some_and(|index| index >= names.len())
                {
                    return Err(CffError::Names);
                }
                let top = if tops.is_empty() {
                    &[][..]
                } else {
                    tops.ft_element(face_index.unwrap_or(0))
                        .ok_or(CffError::Index)?
                };
                (top, strings, g)
            }
        };
        let top = TopDict::parse(top_data, version).map_err(CffError::Dict)?;
        let char_strings = ft_index(data, top.char_strings, version)?;
        let cid = top.is_cid() || version == Version::Cff2;
        let mut store = None;
        let mut fd_array = None;
        let mut fd_select = None;
        if cid {
            store = ft_vstore(data, top.vstore)?;
            let fda = ft_index(data, top.fd_array, version)?;
            if fda.len() > 256 {
                return Err(CffError::TooManyFontDicts);
            }
            let blend = FtBlend::new(store, &[]);
            for i in 0..fda.len() {
                let d = fda.ft_element(i).ok_or(CffError::Index)?;
                let f = FontDict::parse(d, version).map_err(CffError::Dict)?;
                if f.cid_registry == NO_SID {
                    load_private(data, f.private_size, f.private_offset, version, &blend)?;
                }
            }
            if version == Version::Cff || fda.len() > 1 {
                fd_select = Some(ft_fd_select(data, top.fd_select, char_strings.len())?);
            }
            fd_array = Some(fda);
        } else {
            load_private(
                data,
                top.private_size,
                top.private_offset,
                version,
                &FtBlend::new(None, &[]),
            )?;
        }
        if top.char_strings == 0 {
            return Err(CffError::NoCharStrings);
        }
        let n = char_strings.len();
        let mut charset = None;
        let mut encoding = None;
        if version == Version::Cff && n > 0 {
            let c = Charset::new(data, top.charset, n).map_err(|_| CffError::Charset)?;
            if !top.is_cid() {
                encoding =
                    Some(Encoding::new(data, top.encoding, n).map_err(|_| CffError::Encoding)?);
            }
            charset = Some(c);
        }
        let mut t = top;
        if !t.font_matrix.present {
            t.font_matrix.units_per_em = u64::from(units_per_em);
        }
        let top_matrix = normalize(
            &t.font_matrix,
            t.font_matrix.units_per_em,
            (t.font_matrix.x, t.font_matrix.y),
        );
        Ok(Cff {
            version,
            data,
            top,
            top_matrix,
            strings,
            strings_sorted: strings.ft_offsets_sorted(),
            global_subrs,
            global_subrs_sorted: global_subrs.ft_offsets_sorted(),
            char_strings,
            charset,
            encoding,
            fd_array,
            fd_select,
            store,
            fvar_axes,
            head_upem: units_per_em,
            post_format: None,
            local_sorted: [0; 4],
        }
        .with_local_sorted())
    }

    fn with_local_sorted(mut self) -> Cff<'a> {
        let n = self.fd_array.map_or(1, |i| i.len());
        for fd in 0..n.min(256) {
            let sorted = self
                .subfont_of(fd)
                .is_some_and(|s| s.local_subrs.ft_offsets_sorted());
            if sorted && let Some(w) = self.local_sorted.get_mut(to_usize(fd / 64)) {
                *w |= 1u64 << (fd % 64);
            }
        }
        self
    }

    /// Whether the Local Subrs INDEX of Font DICT `fd` needs none of
    /// FreeType's offset corrections ([`Index::ft_offsets_sorted`]),
    /// found once by [`Cff::new`].
    pub fn local_subrs_sorted(&self, fd: u16) -> bool {
        self.local_sorted
            .get(usize::from(fd / 64))
            .is_some_and(|w| w & (1u64 << (fd % 64)) != 0)
    }

    /// CFF or CFF2.
    pub fn version(&self) -> Version {
        self.version
    }

    /// The table data FreeType reads (to the end of the file).
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// The number of glyphs: the CharStrings count.
    pub fn num_glyphs(&self) -> u32 {
        self.char_strings.len()
    }

    /// The CharStrings INDEX.
    pub fn char_strings(&self) -> Index<'a> {
        self.char_strings
    }

    /// The Global Subrs INDEX and whether its offsets are sorted
    /// ([`Index::ft_offsets_sorted`]).
    pub fn global_subrs(&self) -> (Index<'a>, bool) {
        (self.global_subrs, self.global_subrs_sorted)
    }

    /// The charset (CFF).
    pub fn charset(&self) -> Option<Charset<'a>> {
        self.charset
    }

    /// The encoding (CFF that is not CID-keyed).
    pub fn encoding(&self) -> Option<Encoding<'a>> {
        self.encoding
    }

    /// The Variation Store (CFF2).
    pub fn variation_store(&self) -> Option<ItemVariationStore<'a>> {
        self.store
    }

    /// The axis count of `fvar`, when the font has one.
    pub fn fvar_axes(&self) -> Option<u16> {
        self.fvar_axes
    }

    /// The Top DICT as FreeType parsed it.
    pub fn top_dict(&self) -> &TopDict {
        &self.top
    }

    /// The Top DICT's matrix after FreeType's normalization.
    pub fn top_matrix(&self) -> CffMatrix {
        self.top_matrix
    }

    /// `unitsPerEm` of `head`.
    pub fn head_units_per_em(&self) -> u16 {
        self.head_upem
    }

    /// The number of Font DICTs (0 without an FDArray).
    pub fn num_font_dicts(&self) -> u32 {
        self.fd_array.map_or(0, |i| i.len())
    }

    /// The FDSelect.
    pub fn fd_select(&self) -> Option<FdSelect<'a>> {
        self.fd_select
    }

    /// Font DICT `fd` as FreeType parses it.
    pub fn font_dict(&self, fd: u32) -> Option<FontDict> {
        let d = self.fd_array?.ft_element(fd)?;
        FontDict::parse(d, self.version).ok()
    }

    /// The blend weights at the default instance: FreeType's blend
    /// vector with all coordinates 0 when the font has `fvar`, else with
    /// none.
    pub fn default_blend(&self) -> FtBlend<'a> {
        let coords: &'static [i32] = match self.fvar_axes {
            Some(n) => ZEROS.get(..usize::from(n)).unwrap_or(&[]),
            None => &[],
        };
        FtBlend::new(self.store, coords)
    }

    /// The Private DICT of Font DICT `fd` (of the Top DICT without an
    /// FDArray), at the default instance; all zero when there is none.
    pub fn private_dict(&self, fd: u32) -> Option<PrivateDict> {
        self.private_dict_with_blend(fd, &self.default_blend())
    }

    /// The Private DICT evaluated with the CFF2 weights of one location.
    pub fn private_dict_with_blend(
        &self,
        fd: u32,
        blend: &dyn BlendWeights,
    ) -> Option<PrivateDict> {
        let (size, offset, cid_stop) = match self.fd_array {
            Some(_) => {
                let f = self.font_dict(fd)?;
                (f.private_size, f.private_offset, f.cid_registry != NO_SID)
            }
            None => (self.top.private_size, self.top.private_offset, false),
        };
        if cid_stop || size == 0 || offset == 0 {
            return Some(PrivateDict::default());
        }
        let d = private_slice(self.data, size, offset).ok()?;
        PrivateDict::parse(d, self.version, blend).ok()
    }

    /// The subfont of `glyph` (`cff_decoder_prepare`): `None` for a glyph
    /// id not below the glyph count, an FDSelect index not below the
    /// Font DICT count, or a Font or Private DICT that cannot be read.
    pub fn subfont(&self, glyph: GlyphId) -> Option<Subfont<'a>> {
        self.subfont_with_blend(glyph, &self.default_blend())
    }

    /// The subfont of `glyph` with the Private DICT evaluated at one location.
    pub fn subfont_with_blend(
        &self,
        glyph: GlyphId,
        blend: &dyn BlendWeights,
    ) -> Option<Subfont<'a>> {
        if glyph.0 >= self.num_glyphs() {
            return None;
        }
        let Some(fda) = self.fd_array else {
            return self.subfont_of_with_blend(0, blend);
        };
        let fd = self
            .fd_select
            .map_or(0, |s| u32::from(s.fd(glyph.0) & 0xFF));
        if fd >= fda.len() {
            return None;
        }
        self.subfont_of_with_blend(fd, blend)
    }

    /// The subfont of Font DICT `fd` (of the Top DICT without an
    /// FDArray).
    fn subfont_of(&self, fd: u32) -> Option<Subfont<'a>> {
        self.subfont_of_with_blend(fd, &self.default_blend())
    }

    fn subfont_of_with_blend(&self, fd: u32, blend: &dyn BlendWeights) -> Option<Subfont<'a>> {
        if self.fd_array.is_none() {
            let private = self.private_dict_with_blend(0, blend)?;
            let local_subrs = self.local_subrs(&private, self.top.private_offset)?;
            return Some(Subfont {
                fd: 0,
                private,
                local_subrs,
                matrix: self.top_matrix,
            });
        }
        let f = self.font_dict(fd)?;
        let private = self.private_dict_with_blend(fd, blend)?;
        let local_subrs = if f.cid_registry == NO_SID {
            self.local_subrs(&private, f.private_offset)?
        } else {
            Index::empty(self.version)
        };
        Some(Subfont {
            fd: u16::try_from(fd).unwrap_or(0),
            private,
            local_subrs,
            matrix: self.sub_matrix(&f),
        })
    }

    fn local_subrs(&self, p: &PrivateDict, private_offset: u64) -> Option<Index<'a>> {
        if p.subrs == 0 || private_offset == 0 {
            return Some(Index::empty(self.version));
        }
        ft_index(
            self.data,
            private_offset.wrapping_add(p.subrs),
            self.version,
        )
        .ok()
    }

    /// The matrix of the Font DICT `f`, as `cff_face_init` concatenates
    /// and normalizes it.
    fn sub_matrix(&self, f: &FontDict) -> CffMatrix {
        let top = &self.top_matrix;
        if !f.font_matrix.present {
            // FreeType copies the normalized Top DICT matrix and
            // normalizes it again, shifting the translation once more.
            let fm = FontMatrix {
                present: true,
                xx: top.m[0],
                yx: top.m[1],
                xy: top.m[2],
                yy: top.m[3],
                x: top.offset.0,
                y: top.offset.1,
                units_per_em: top.units_per_em,
            };
            return normalize(&fm, top.units_per_em, top.offset);
        }
        let s = &f.font_matrix;
        let mut m = [s.xx, s.yx, s.xy, s.yy];
        let mut offset = (s.x, s.y);
        let mut upm = s.units_per_em;
        if self.top.font_matrix.present {
            let scaling: i64 = if top.units_per_em > 1 && s.units_per_em > 1 {
                top.units_per_em.min(s.units_per_em).cast_signed()
            } else {
                1
            };
            let val = 0x1_0000_i64.wrapping_mul(scaling);
            let [axx, ayx, axy, ayy] = top.m;
            let [bxx, byx, bxy, byy] = m;
            let md = |a: i64, b: i64| ft::mul_div(a, b, val);
            m = [
                md(axx, bxx).wrapping_add(md(axy, byx)),
                md(ayx, bxx).wrapping_add(md(ayy, byx)),
                md(axx, bxy).wrapping_add(md(axy, byy)),
                md(ayx, bxy).wrapping_add(md(ayy, byy)),
            ];
            offset = (
                md(offset.0, axx).wrapping_add(md(offset.1, axy)),
                md(offset.0, ayx).wrapping_add(md(offset.1, ayy)),
            );
            upm = ft::mul_div(upm.cast_signed(), top.units_per_em.cast_signed(), scaling)
                .cast_unsigned();
        }
        let fm = FontMatrix {
            present: true,
            xx: m[0],
            yx: m[1],
            xy: m[2],
            yy: m[3],
            x: offset.0,
            y: offset.1,
            units_per_em: upm,
        };
        normalize(&fm, upm, offset)
    }

    /// The string of `sid` as FreeType's `cff_index_get_sid_string` gives
    /// it: `None` for 0xFFFF, for CFF2, and outside the String INDEX.
    pub fn sid_string(&self, sid: u32) -> Option<&'a [u8]> {
        if sid == NO_SID || self.version == Version::Cff2 {
            return None;
        }
        self.string(sid)
    }

    /// The string of `sid` without the 0xFFFF check (`cff_get_name_index`
    /// reads the strings so).
    fn string(&self, sid: u32) -> Option<&'a [u8]> {
        match sid.checked_sub(cff::NUM_STANDARD_STRINGS) {
            None => cff::STANDARD_STRINGS
                .get(to_usize(sid))
                .map(|s: &&'static str| s.as_bytes()),
            Some(k) => self.pool_string(k),
        }
    }

    /// String `k` of FreeType's string pool (`cff_index_get_pointers`
    /// with a pool): each non-empty string is copied with a 0 byte after
    /// it, an empty one is not, so an empty string reads as the next
    /// non-empty one (or as empty at the end).
    fn pool_string(&self, k: u32) -> Option<&'a [u8]> {
        let first = self.strings.ft_pointer(k, self.strings_sorted)?;
        if !first.is_empty() {
            return Some(first);
        }
        Some(
            (k.saturating_add(1)..self.strings.len())
                .filter_map(|i| self.strings.ft_pointer(i, self.strings_sorted))
                .find(|s| !s.is_empty())
                .unwrap_or_default(),
        )
    }

    /// Whether the font is CID-keyed (`FT_Get_CID_Is_Internally_CID_Keyed`):
    /// the Top DICT has `ROS`. Never for CFF2.
    pub fn is_cid_keyed(&self) -> bool {
        self.version == Version::Cff && self.top.is_cid()
    }

    /// Translate a standalone FreeType face glyph identifier to a CharStrings index.
    pub fn bare_charstring_index(&self, glyph: GlyphId) -> Option<GlyphId> {
        if self.is_cid_keyed() && glyph.0 != 0 {
            self.charset()?
                .glyph(u16::try_from(glyph.0).ok()?)
                .map(GlyphId)
        } else {
            (glyph.0 < self.num_glyphs()).then_some(glyph)
        }
    }

    /// The CID of `glyph` (`FT_Get_CID_From_Glyph_Index`): the charset
    /// value. Each call reads the charset ranges before the glyph; to
    /// look up many glyphs, build a [`CharsetIndex`] once (see
    /// [`Cff::cid_with`]).
    pub fn cid(&self, glyph: GlyphId) -> Result<u32, CidError> {
        self.cid_by(None, glyph)
    }

    /// [`Cff::cid`] with an index of [`Cff::charset`] (ADR 0025), built by
    /// [`Charset::index_into`] over an array of [`Charset::num_ranges`]
    /// words: building it reads the ranges once, and each lookup is then
    /// a binary search over them. The result equals [`Cff::cid`]'s;
    /// `index` must come from this font's charset.
    pub fn cid_with(&self, index: &CharsetIndex<'_>, glyph: GlyphId) -> Result<u32, CidError> {
        self.cid_by(Some(index), glyph)
    }

    fn cid_by(&self, index: Option<&CharsetIndex<'_>>, glyph: GlyphId) -> Result<u32, CidError> {
        if !self.is_cid_keyed() {
            return Err(CidError::NotCidKeyed);
        }
        if glyph.0 >= self.num_glyphs() {
            return Err(CidError::InvalidGlyphIndex);
        }
        Ok(self.charset_sid(index, glyph.0).map_or(0, u32::from))
    }

    /// The charset value of `glyph`, through `index` when there is one.
    fn charset_sid(&self, index: Option<&CharsetIndex<'_>>, glyph: u32) -> Option<u16> {
        let c = self.charset?;
        match index {
            Some(ix) => ix.sid(glyph),
            None => c.sid(glyph),
        }
    }

    /// The registry, ordering and supplement
    /// (`FT_Get_CID_Registry_Ordering_Supplement`).
    pub fn ros(&self) -> Result<Ros<'a>, CidError> {
        if !self.is_cid_keyed() {
            return Err(CidError::NotCidKeyed);
        }
        let s = self.top.cid_supplement;
        Ok(Ros {
            registry: self.sid_string(self.top.cid_registry),
            ordering: self.sid_string(self.top.cid_ordering),
            supplement: ft::to_i32(s),
        })
    }

    /// Whether FreeType gives the face glyph names
    /// (`FT_FACE_FLAG_GLYPH_NAMES`, `FT_Has_PS_Glyph_Names`): set by the
    /// sfnt loader when it loads a `post` that is not format 3.0, and by
    /// the CFF driver for CFF that is not CID-keyed.
    /// Only with it do `FT_Get_Glyph_Name` and `FT_Get_Name_Index` answer.
    pub fn has_glyph_names(&self) -> bool {
        self.post_format.is_some_and(|f| f != 0x0003_0000)
            || (self.version == Version::Cff && !self.top.is_cid())
    }

    /// The charset name of `glyph`: the SID's string, cut at a 0 byte as
    /// FreeType's C strings are. `None` for CID-keyed fonts, CFF2, glyph
    /// ids not below the glyph count, and SIDs without a string. To look
    /// up many glyphs, use [`Cff::charset_name_with`].
    pub fn charset_name(&self, glyph: GlyphId) -> Option<&'a [u8]> {
        self.charset_name_by(None, glyph)
    }

    /// [`Cff::charset_name`] with an index of the charset; see
    /// [`Cff::cid_with`]. The result equals [`Cff::charset_name`]'s.
    pub fn charset_name_with(&self, index: &CharsetIndex<'_>, glyph: GlyphId) -> Option<&'a [u8]> {
        self.charset_name_by(Some(index), glyph)
    }

    fn charset_name_by(
        &self,
        index: Option<&CharsetIndex<'_>>,
        glyph: GlyphId,
    ) -> Option<&'a [u8]> {
        if !self.has_charset_names() {
            return None;
        }
        self.charset_string(index, glyph)
    }

    /// Whether the charset holds glyph names: CFF that is not CID-keyed.
    fn has_charset_names(&self) -> bool {
        self.version == Version::Cff && !self.top.is_cid()
    }

    fn charset_string(&self, index: Option<&CharsetIndex<'_>>, glyph: GlyphId) -> Option<&'a [u8]> {
        let sid = self.charset_sid(index, glyph.0)?;
        self.sid_name(sid)
    }

    /// The string of a charset value, cut at a 0 byte.
    fn sid_name(&self, sid: u16) -> Option<&'a [u8]> {
        self.sid_string(u32::from(sid)).map(cut)
    }

    /// FreeType's `FT_Get_Glyph_Name` for a CFF (version 1) face: `None`
    /// (an error) without [`Cff::has_glyph_names`] and for glyph ids not
    /// below the glyph count; else the string of the charset value (also
    /// in CID-keyed fonts, where it is a CID), or an empty name when there
    /// is no such string. CFF2 faces use `post` ([`crate::Font::ft_glyph_name`]).
    /// To look up many glyphs, use [`Cff::ft_glyph_name_with`].
    pub fn ft_glyph_name(&self, glyph: GlyphId) -> Option<&'a [u8]> {
        self.ft_glyph_name_by(None, glyph)
    }

    /// [`Cff::ft_glyph_name`] with an index of the charset; see
    /// [`Cff::cid_with`]. The result equals [`Cff::ft_glyph_name`]'s.
    pub fn ft_glyph_name_with(&self, index: &CharsetIndex<'_>, glyph: GlyphId) -> Option<&'a [u8]> {
        self.ft_glyph_name_by(Some(index), glyph)
    }

    fn ft_glyph_name_by(
        &self,
        index: Option<&CharsetIndex<'_>>,
        glyph: GlyphId,
    ) -> Option<&'a [u8]> {
        if !self.has_glyph_names() || glyph.0 >= self.num_glyphs() || self.version != Version::Cff {
            return None;
        }
        Some(self.charset_string(index, glyph).unwrap_or_default())
    }

    /// FreeType's `FT_Get_Name_Index` for a CFF (version 1) face: the
    /// first glyph, in glyph id order, whose charset string is `name`; 0
    /// when there is none or without [`Cff::has_glyph_names`]. One pass
    /// over the charset.
    pub fn ft_glyph_from_name(&self, name: &[u8]) -> GlyphId {
        if !self.has_glyph_names() || self.version != Version::Cff {
            return GlyphId(0);
        }
        let Some(c) = self.charset else {
            return GlyphId(0);
        };
        (0u32..)
            .zip(c.sids())
            .find(|&(_, sid)| self.string(u32::from(sid)).map(cut) == Some(name))
            .map_or(GlyphId(0), |(g, _)| GlyphId(g))
    }

    /// Fills `buf` (at least [`Cff::num_glyphs`] words) with the glyph
    /// ids sorted by charset name, equal names by glyph id, for
    /// [`Cff::glyph_from_name`]. Glyphs without a name sort first. `None`
    /// when `buf` is too short or the font has no glyph names. Reads the
    /// charset once, in glyph id order, and sorts in O(n log n) time.
    pub fn build_name_index<'b>(&self, buf: &'b mut [u32]) -> Option<&'b [u32]> {
        let sids = self.charset.map(|c| c.sids());
        self.build_name_index_from(buf, sids.into_iter().flatten())
    }

    /// [`Cff::build_name_index`] with an index of the charset; see
    /// [`Cff::cid_with`]. The result equals [`Cff::build_name_index`]'s;
    /// since that already reads the charset in one pass, the index gains
    /// little here.
    pub fn build_name_index_with<'b>(
        &self,
        index: &CharsetIndex<'_>,
        buf: &'b mut [u32],
    ) -> Option<&'b [u32]> {
        let sids = (0..self.num_glyphs()).map_while(|g| self.charset_sid(Some(index), g));
        self.build_name_index_from(buf, sids)
    }

    /// Sorts the glyph ids by the names of `sids` (the charset values in
    /// glyph id order). CFF has at most 65535 glyphs, so each word holds
    /// the SID above the glyph id while sorting.
    fn build_name_index_from<'b>(
        &self,
        buf: &'b mut [u32],
        sids: impl Iterator<Item = u16>,
    ) -> Option<&'b [u32]> {
        if !self.has_charset_names() {
            return None;
        }
        let n = self.num_glyphs();
        let buf = buf.get_mut(..to_usize(n))?;
        if n > 0x1_0000 {
            return None;
        }
        // A glyph the charset has no value for has no name (SID 0xFFFF
        // has no string).
        for (g, slot) in (0u32..).zip(buf.iter_mut()) {
            *slot = (NO_SID << 16) | g;
        }
        for (slot, sid) in buf.iter_mut().zip(sids) {
            *slot = (u32::from(sid) << 16) | (*slot & 0xFFFF);
        }
        let name = |w: u32| {
            u16::try_from(w >> 16)
                .ok()
                .and_then(|sid| self.sid_name(sid))
                .unwrap_or_default()
        };
        buf.sort_unstable_by(|&a, &b| name(a).cmp(name(b)).then((a & 0xFFFF).cmp(&(b & 0xFFFF))));
        for w in buf.iter_mut() {
            *w &= 0xFFFF;
        }
        Some(buf)
    }

    /// The glyph named `name` in an index from [`Cff::build_name_index`]:
    /// a binary search, giving the smallest glyph id among equal names.
    /// `None` for an empty name or no match. Each step of the search
    /// reads the charset; [`Cff::glyph_from_name_with`] does not.
    pub fn glyph_from_name(&self, index: &[u32], name: &[u8]) -> Option<GlyphId> {
        self.glyph_from_name_by(None, index, name)
    }

    /// [`Cff::glyph_from_name`] with an index of the charset (`charset`;
    /// see [`Cff::cid_with`]) for comparing the candidates. The result
    /// equals [`Cff::glyph_from_name`]'s.
    pub fn glyph_from_name_with(
        &self,
        charset: &CharsetIndex<'_>,
        index: &[u32],
        name: &[u8],
    ) -> Option<GlyphId> {
        self.glyph_from_name_by(Some(charset), index, name)
    }

    fn glyph_from_name_by(
        &self,
        charset: Option<&CharsetIndex<'_>>,
        index: &[u32],
        name: &[u8],
    ) -> Option<GlyphId> {
        if name.is_empty() {
            return None;
        }
        let key = |g: &u32| {
            self.charset_name_by(charset, GlyphId(*g))
                .unwrap_or_default()
        };
        let i = index.partition_point(|g| key(g) < name);
        let g = *index.get(i)?;
        (key(&g) == name).then_some(GlyphId(g))
    }

    /// `FT_Get_PS_Font_Info`: the nine values of the Top DICT. CFF2 has
    /// none (`Invalid_Argument`).
    pub fn ps_font_info(&self) -> Result<PsFontInfo<'a>, CffError> {
        if self.version == Version::Cff2 {
            return Err(CffError::NotCff);
        }
        let t = &self.top;
        Ok(PsFontInfo {
            version: self.sid_string(t.version).map(cut),
            notice: self.sid_string(t.notice).map(cut),
            full_name: self.sid_string(t.full_name).map(cut),
            family_name: self.sid_string(t.family_name).map(cut),
            weight: self.sid_string(t.weight).map(cut),
            italic_angle: t.italic_angle,
            is_fixed_pitch: t.is_fixed_pitch != 0,
            underline_position: t.underline_position,
            underline_thickness: t.underline_thickness,
        })
    }

    /// `fs_type` of `FT_Get_PS_Font_Extra`: the number in `/FSType <n>
    /// def` of the Top DICT's `PostScript` string; 0 when there is none,
    /// when anything but digits, spaces, CR and LF stands between, or when
    /// it overflows.
    pub fn ps_fs_type(&self) -> u16 {
        let Some(ps) = self.sid_string(self.top.postscript).map(cut) else {
            return 0;
        };
        fs_type(ps)
    }
}

/// The `/FSType` rule of `cff_ps_get_font_extra`.
pub fn fs_type(ps: &[u8]) -> u16 {
    const KEY: &[u8] = b"/FSType";
    let Some(start) = find(ps, KEY) else {
        return 0;
    };
    let rest = ps
        .get(start.saturating_add(KEY.len())..)
        .unwrap_or_default();
    let Some(end) = find(rest, b"def") else {
        return 0;
    };
    let mut v: u16 = 0;
    for &c in rest.get(..end).unwrap_or_default() {
        match c {
            b'0'..=b'9' => {
                if v >= (u16::MAX - 9) / 10 {
                    return 0;
                }
                v = v
                    .saturating_mul(10)
                    .saturating_add(u16::from(c.wrapping_sub(b'0')));
            }
            b' ' | b'\n' | b'\r' => {}
            _ => return 0,
        }
    }
    v
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// A C string: up to the first 0 byte.
fn cut(s: &[u8]) -> &[u8] {
    s.split(|&b| b == 0).next().unwrap_or_default()
}

static ZEROS: [i32; 64] = [0; 64];

/// `cff_load_private_dict` at load: parse the Private DICT and read its
/// local subroutines.
fn load_private(
    data: &[u8],
    size: u64,
    offset: u64,
    version: Version,
    blend: &dyn BlendWeights,
) -> Result<(), CffError> {
    if size == 0 || offset == 0 {
        return Ok(());
    }
    let d = private_slice(data, size, offset)?;
    let p = PrivateDict::parse(d, version, blend).map_err(CffError::Dict)?;
    if p.subrs != 0 {
        ft_index(data, offset.wrapping_add(p.subrs), version)?;
    }
    Ok(())
}

impl<'a> Font<'a> {
    /// Whether the font has a `CFF ` table with a non-zero length, the
    /// length cut at the end of the file as HarfBuzz's
    /// `hb_face_reference_table` cuts it (ADR 0021).
    pub fn is_cff(&self) -> bool {
        self.clamped_len(cff::TAG) > 0
    }

    /// Whether the font has a `CFF2` table with a non-zero length (as
    /// [`Font::is_cff`]).
    pub fn is_cff2(&self) -> bool {
        self.clamped_len(cff::TAG2) > 0
    }

    fn clamped_len(&self, tag: Tag) -> usize {
        let sfnt = self.sfnt();
        let file = sfnt.file().len();
        sfnt.record(tag).map_or(0, |r| {
            let off = to_usize(r.offset);
            if off >= file {
                0
            } else {
                to_usize(r.length).min(file.saturating_sub(off))
            }
        })
    }
}
