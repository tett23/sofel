//! The functions of bench/c/shim.c (ADR 0013). Declared by hand; the
//! shim keeps FreeType's and HarfBuzz's structs out of Rust.

use std::os::raw::{c_int, c_long, c_uchar, c_uint, c_ulong, c_void};

unsafe extern "C" {
    pub fn bft_open(data: *const c_uchar, len: c_long, index: c_long) -> *mut c_void;
    pub fn bft_close(face: *mut c_void);
    pub fn bft_num_glyphs(face: *mut c_void) -> c_long;
    pub fn bft_cid(face: *mut c_void, gid: c_uint) -> c_long;
    pub fn bft_set_pixel_sizes(face: *mut c_void, ppem: c_uint) -> c_int;
    pub fn bft_char_index(face: *mut c_void, c: c_ulong) -> c_uint;
    pub fn bft_advance(face: *mut c_void, gid: c_uint) -> c_long;
    pub fn bft_outline(face: *mut c_void, gid: c_uint, unscaled: c_int) -> c_long;
    pub fn bft_render(face: *mut c_void, gid: c_uint) -> c_long;

    pub fn bhb_open(data: *const c_uchar, len: c_uint, index: c_uint) -> *mut c_void;
    pub fn bhb_close(font: *mut c_void);
    pub fn bhb_set_ppem(font: *mut c_void, ppem: c_uint);
    pub fn bhb_nominal_glyph(font: *mut c_void, c: c_uint) -> c_uint;
    pub fn bhb_advance(font: *mut c_void, gid: c_uint) -> c_int;
    pub fn bhb_v_advance(font: *mut c_void, gid: c_uint) -> c_int;
    pub fn bhb_v_origin(font: *mut c_void, gid: c_uint) -> c_long;
    pub fn bhb_draw(font: *mut c_void, gid: c_uint) -> c_long;
    pub fn bhb_glyph_extents(font: *mut c_void, gid: c_uint) -> c_long;
    pub fn bhb_glyph_name(font: *mut c_void, gid: c_uint, buf: *mut c_uchar) -> c_long;
    pub fn bhb_glyph_from_name(font: *mut c_void, name: *const c_uchar, len: c_int) -> c_long;
    pub fn bhb_open_extents(data: *const c_uchar, len: c_uint, gid: c_uint) -> c_long;

    // Color glyphs (ADR 0023).
    pub fn bft_layers(face: *mut c_void, gid: c_uint) -> c_long;
    pub fn bft_colr_walk(face: *mut c_void, gid: c_uint) -> c_long;
    pub fn bft_sbit(face: *mut c_void, gid: c_uint) -> c_long;
    pub fn bft_colr_render(face: *mut c_void, gid: c_uint) -> c_long;
    pub fn bhb_color_format(font: *mut c_void, gid: c_uint) -> c_long;
    pub fn bhb_color_layers(font: *mut c_void, gid: c_uint) -> c_long;
    pub fn bhb_svg_len(font: *mut c_void, gid: c_uint) -> c_long;
    pub fn bhb_paint(font: *mut c_void, gid: c_uint) -> c_long;
    pub fn bhb_set_bitmap_ppem(font: *mut c_void, ppem: c_uint);

    // HarfBuzz's own Unicode functions (hb-unicode.h), called directly.
    pub fn hb_unicode_funcs_get_default() -> *mut c_void;
    pub fn hb_unicode_general_category(ufuncs: *mut c_void, cp: c_uint) -> c_int;
    pub fn hb_unicode_combining_class(ufuncs: *mut c_void, cp: c_uint) -> c_int;
    pub fn hb_unicode_script(ufuncs: *mut c_void, cp: c_uint) -> c_uint;
    pub fn hb_unicode_mirroring(ufuncs: *mut c_void, cp: c_uint) -> c_uint;
}

/// A FreeType face over borrowed font data.
pub struct FtFace(*mut c_void);

impl FtFace {
    pub fn open(data: &[u8]) -> Option<FtFace> {
        // SAFETY: the shim copies nothing; `data` outlives the face (it is
        // kept by the caller for the whole run).
        let f = unsafe { bft_open(data.as_ptr(), data.len() as c_long, 0) };
        (!f.is_null()).then_some(FtFace(f))
    }
    pub fn num_glyphs(&self) -> u32 {
        // SAFETY: a valid face.
        unsafe { bft_num_glyphs(self.0) as u32 }
    }
    pub fn set_ppem(&self, ppem: u32) {
        // SAFETY: a valid face.
        unsafe { bft_set_pixel_sizes(self.0, ppem) };
    }
    pub fn char_index(&self, c: u32) -> u32 {
        // SAFETY: a valid face.
        unsafe { bft_char_index(self.0, c_ulong::from(c)) }
    }
    pub fn advance(&self, gid: u32) -> i64 {
        // SAFETY: a valid face.
        unsafe { bft_advance(self.0, gid) as i64 }
    }
    pub fn outline(&self, gid: u32, unscaled: bool) -> i64 {
        // SAFETY: a valid face.
        unsafe { bft_outline(self.0, gid, c_int::from(unscaled)) as i64 }
    }
    pub fn cid(&self, gid: u32) -> i64 {
        // SAFETY: a valid face.
        unsafe { bft_cid(self.0, gid) as i64 }
    }
    pub fn layers(&self, gid: u32) -> i64 {
        // SAFETY: a valid face.
        unsafe { bft_layers(self.0, gid) as i64 }
    }
    pub fn sbit(&self, gid: u32) -> i64 {
        // SAFETY: a valid face.
        unsafe { bft_sbit(self.0, gid) as i64 }
    }
    pub fn colr_render(&self, gid: u32) -> i64 {
        // SAFETY: a valid face.
        unsafe { bft_colr_render(self.0, gid) as i64 }
    }
    pub fn colr_walk(&self, gid: u32) -> i64 {
        // SAFETY: a valid face.
        unsafe { bft_colr_walk(self.0, gid) as i64 }
    }
    pub fn render(&self, gid: u32) -> i64 {
        // SAFETY: a valid face.
        unsafe { bft_render(self.0, gid) as i64 }
    }
}

impl Drop for FtFace {
    fn drop(&mut self) {
        // SAFETY: a valid face, dropped once.
        unsafe { bft_close(self.0) };
    }
}

/// A HarfBuzz font (default `ot` font functions) over borrowed data.
pub struct HbFont(*mut c_void);

impl HbFont {
    pub fn open(data: &[u8]) -> Option<HbFont> {
        // SAFETY: the blob is read-only over `data`, which outlives the font.
        let f = unsafe { bhb_open(data.as_ptr(), data.len() as c_uint, 0) };
        (!f.is_null()).then_some(HbFont(f))
    }
    /// 0 for unscaled.
    pub fn set_ppem(&self, ppem: u32) {
        // SAFETY: a valid font.
        unsafe { bhb_set_ppem(self.0, ppem) };
    }
    pub fn set_bitmap_ppem(&self, ppem: u32) {
        // SAFETY: a valid font.
        unsafe { bhb_set_bitmap_ppem(self.0, ppem) };
    }
    pub fn color_format(&self, gid: u32) -> i64 {
        // SAFETY: a valid font.
        unsafe { bhb_color_format(self.0, gid) as i64 }
    }
    pub fn color_layers(&self, gid: u32) -> i64 {
        // SAFETY: a valid font.
        unsafe { bhb_color_layers(self.0, gid) as i64 }
    }
    pub fn svg_len(&self, gid: u32) -> i64 {
        // SAFETY: a valid font.
        unsafe { bhb_svg_len(self.0, gid) as i64 }
    }
    /// Paint calls plus image bytes; -1 for glyphs that are not color glyphs.
    pub fn paint(&self, gid: u32) -> i64 {
        // SAFETY: a valid font.
        unsafe { bhb_paint(self.0, gid) as i64 }
    }
    pub fn nominal_glyph(&self, c: u32) -> u32 {
        // SAFETY: a valid font.
        unsafe { bhb_nominal_glyph(self.0, c) }
    }
    pub fn advance(&self, gid: u32) -> i64 {
        // SAFETY: a valid font.
        unsafe { i64::from(bhb_advance(self.0, gid)) }
    }
    pub fn v_advance(&self, gid: u32) -> i64 {
        // SAFETY: a valid font.
        unsafe { i64::from(bhb_v_advance(self.0, gid)) }
    }
    /// The sum of the vertical origin's x and y.
    pub fn v_origin(&self, gid: u32) -> i64 {
        // SAFETY: a valid font.
        unsafe { bhb_v_origin(self.0, gid) as i64 }
    }
    pub fn draw(&self, gid: u32) -> i64 {
        // SAFETY: a valid font.
        unsafe { bhb_draw(self.0, gid) as i64 }
    }
    /// The sum of the glyph box's values; `None` without a box.
    pub fn glyph_extents(&self, gid: u32) -> Option<i64> {
        // SAFETY: a valid font.
        let v = unsafe { bhb_glyph_extents(self.0, gid) };
        (v != c_long::MIN).then_some(v as i64)
    }
    /// The length of the glyph's name (`hb_font_get_glyph_name` with 256
    /// bytes, read back as a C string); `None` without a name.
    pub fn glyph_name_len(&self, gid: u32, buf: &mut [u8; 256]) -> Option<i64> {
        // SAFETY: a valid font; the buffer has the 256 bytes the shim uses.
        let n = unsafe { bhb_glyph_name(self.0, gid, buf.as_mut_ptr()) };
        (n >= 0).then_some(n as i64)
    }
    /// The glyph with `name`, or `None`.
    pub fn glyph_from_name(&self, name: &[u8]) -> Option<u32> {
        // SAFETY: a valid font; `name` is valid for its length.
        let g = unsafe { bhb_glyph_from_name(self.0, name.as_ptr(), name.len() as c_int) };
        (g >= 0).then_some(g as u32)
    }
    /// Opens `data` and gets the box of `gid`: the sum of its values, or
    /// `None`.
    pub fn open_extents(data: &[u8], gid: u32) -> Option<i64> {
        // SAFETY: the blob is read-only over `data` and destroyed before
        // the call returns.
        let v = unsafe { bhb_open_extents(data.as_ptr(), data.len() as c_uint, gid) };
        (v != c_long::MIN).then_some(v as i64)
    }
}

impl Drop for HbFont {
    fn drop(&mut self) {
        // SAFETY: a valid font, dropped once.
        unsafe { bhb_close(self.0) };
    }
}

/// HarfBuzz's default Unicode functions (`hb_unicode_funcs_get_default`).
pub struct HbUnicode(*mut c_void);

impl HbUnicode {
    pub fn get() -> HbUnicode {
        // SAFETY: returns HarfBuzz's static, immutable functions.
        HbUnicode(unsafe { hb_unicode_funcs_get_default() })
    }
    /// The sum of General_Category, Canonical_Combining_Class, the script
    /// tag and Bidi_Mirroring_Glyph of one character, as the benchmark
    /// adds them.
    pub fn sum(&self, cp: u32) -> u64 {
        // SAFETY: valid functions; any code point is allowed.
        unsafe {
            u64::from(hb_unicode_general_category(self.0, cp) as u32)
                + u64::from(hb_unicode_combining_class(self.0, cp) as u32)
                + u64::from(hb_unicode_script(self.0, cp))
                + u64::from(hb_unicode_mirroring(self.0, cp))
        }
    }
}
