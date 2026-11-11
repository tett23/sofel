//! The functions of bench/c/shim.c (ADR 0013). Declared by hand; the
//! shim keeps FreeType's and HarfBuzz's structs out of Rust.

use std::os::raw::{c_int, c_long, c_uchar, c_uint, c_ulong, c_void};

unsafe extern "C" {
    fn bft_set_design(face: *mut c_void, count: c_uint, coords: *mut c_long) -> c_int;
    fn bft_get_normalized(face: *mut c_void, count: c_uint, coords: *mut c_long) -> c_int;
    fn bhb_set_weight(font: *mut c_void, weight: f32);
    fn bhb_normalized(font: *mut c_void, count: *mut c_uint) -> *const c_int;
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
    pub fn bhb_math_constant(font: *mut c_void, index: c_uint) -> c_int;
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

    // Shaping (ADR 0020).
    pub fn bhb_buffer_create() -> *mut c_void;
    pub fn bhb_buffer_destroy(buf: *mut c_void);
    pub fn bhb_shape_line(
        font: *mut c_void,
        buf: *mut c_void,
        text: *const c_uchar,
        text_len: c_uint,
        line_len: c_uint,
        direction: c_int,
        glyphs: *mut c_uint,
        clusters: *mut c_uint,
        flags: *mut c_uint,
        positions: *mut c_int,
        cap: c_uint,
    ) -> c_long;
    pub fn bhb_shape_text(
        font: *mut c_void,
        buf: *mut c_void,
        text: *const c_uchar,
        text_len: c_uint,
        direction: c_int,
    ) -> u64;
    pub fn bhb_first_shape(
        data: *const c_uchar,
        len: c_uint,
        text: *const c_uchar,
        text_len: c_uint,
        line_len: c_uint,
        direction: c_int,
    ) -> u64;

    // Subsetting (ADR 0022).
    pub fn bhb_subset_face(
        face: *mut c_void,
        glyphs: *const c_uint,
        num_glyphs: c_uint,
        unicodes: *const c_uint,
        num_unicodes: c_uint,
        flags: c_uint,
        len: *mut c_uint,
    ) -> u64;
    pub fn bhb_subset_preprocess(font: *mut c_void) -> *mut c_void;
    pub fn bhb_face_glyph_count(face: *mut c_void) -> c_uint;
    pub fn bhb_face_destroy(face: *mut c_void);
    pub fn bhb_subset(
        font: *mut c_void,
        glyphs: *const c_uint,
        num_glyphs: c_uint,
        unicodes: *const c_uint,
        num_unicodes: c_uint,
        flags: c_uint,
        len: *mut c_uint,
    ) -> u64;

    fn hb_set_create() -> *mut c_void;
    fn hb_set_destroy(set: *mut c_void);
    fn hb_face_collect_unicodes(face: *mut c_void, set: *mut c_void);
    fn hb_set_next_many(set: *const c_void, after: u32, out: *mut u32, size: c_uint) -> c_uint;

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
    pub fn set_design(&self, coordinates: &mut [c_long]) -> bool {
        // SAFETY: the face is live; the slice supplies count coordinates.
        unsafe {
            bft_set_design(
                self.0,
                coordinates.len() as c_uint,
                coordinates.as_mut_ptr(),
            ) == 0
        }
    }

    pub fn normalized(&self, coordinates: &mut [c_long]) -> bool {
        // SAFETY: the slice is writable for count entries.
        unsafe {
            bft_get_normalized(
                self.0,
                coordinates.len() as c_uint,
                coordinates.as_mut_ptr(),
            ) == 0
        }
    }

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
    pub fn set_weight(&self, weight: f32) {
        // SAFETY: self holds a live font.
        unsafe {
            bhb_set_weight(self.0, weight);
        }
    }
    pub fn normalized(&self) -> Vec<i32> {
        let mut count = 0;
        // SAFETY: HarfBuzz owns count readable values until the next font mutation.
        unsafe {
            let data = bhb_normalized(self.0, &mut count);
            if count == 0 {
                return Vec::new();
            }
            std::slice::from_raw_parts(data, count as usize).to_vec()
        }
    }

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
    /// `hb_subset_or_fail` of the glyphs or characters with the flags:
    /// the FNV-1a hash of the subset font and its length, `(0, 0)` on
    /// failure.
    pub fn subset(&self, glyphs: &[u32], unicodes: &[u32], flags: u32) -> (u64, usize) {
        let mut len: c_uint = 0;
        // SAFETY: a valid font; the arrays have the lengths given.
        let h = unsafe {
            bhb_subset(
                self.0,
                glyphs.as_ptr(),
                glyphs.len() as c_uint,
                unicodes.as_ptr(),
                unicodes.len() as c_uint,
                flags,
                &mut len,
            )
        };
        (h, len as usize)
    }
    pub fn nominal_glyph(&self, c: u32) -> u32 {
        // SAFETY: a valid font.
        unsafe { bhb_nominal_glyph(self.0, c) }
    }
    pub fn advance(&self, gid: u32) -> i64 {
        // SAFETY: a valid font.
        unsafe { i64::from(bhb_advance(self.0, gid)) }
    }
    pub fn math_constant(&self, index: u32) -> i64 {
        // SAFETY: a valid font; the caller uses the 56 defined constants.
        unsafe { i64::from(bhb_math_constant(self.0, index)) }
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

/// A face from `hb_subset_preprocess`.
pub struct HbPreprocessed(*mut c_void);

impl HbPreprocessed {
    /// `hb_subset_preprocess` of the font's face.
    pub fn new(font: &HbFont) -> HbPreprocessed {
        // SAFETY: a valid font; the result is a new face (never NULL).
        HbPreprocessed(unsafe { bhb_subset_preprocess(font.0) })
    }
    /// The first mapped code points, in the order of benchmark-subset.cc.
    pub fn first_unicodes(&self, limit: u32) -> Vec<u32> {
        let mut points = vec![0; limit as usize];
        // SAFETY: the face is live. The set is owned until destroyed below
        // and points has space for limit hb_codepoint_t values.
        let count = unsafe {
            let set = hb_set_create();
            hb_face_collect_unicodes(self.0, set);
            let count = hb_set_next_many(set, u32::MAX, points.as_mut_ptr(), limit);
            hb_set_destroy(set);
            count
        };
        points.truncate(count as usize);
        points
    }
    pub fn glyph_count(&self) -> u32 {
        // SAFETY: a valid face.
        unsafe { bhb_face_glyph_count(self.0) }
    }
    /// As [`HbFont::subset`], on the preprocessed face.
    pub fn subset(&self, glyphs: &[u32], unicodes: &[u32], flags: u32) -> (u64, usize) {
        let mut len: c_uint = 0;
        // SAFETY: a valid face; the arrays have the lengths given.
        let h = unsafe {
            bhb_subset_face(
                self.0,
                glyphs.as_ptr(),
                glyphs.len() as c_uint,
                unicodes.as_ptr(),
                unicodes.len() as c_uint,
                flags,
                &mut len,
            )
        };
        (h, len as usize)
    }
}

impl Drop for HbPreprocessed {
    fn drop(&mut self) {
        // SAFETY: a valid face, dropped once.
        unsafe { bhb_face_destroy(self.0) };
    }
}

impl Drop for HbFont {
    fn drop(&mut self) {
        // SAFETY: a valid font, dropped once.
        unsafe { bhb_close(self.0) };
    }
}

/// The glyphs of one shaped line: ids, clusters, flags and four
/// positions each (x advance, y advance, x offset, y offset).
#[derive(Clone, Default, PartialEq, Debug)]
pub struct ShapedLine {
    pub glyphs: Vec<u32>,
    pub clusters: Vec<u32>,
    pub flags: Vec<u32>,
    pub positions: Vec<i32>,
}

/// An `hb_buffer_t` for shaping (ADR 0020).
pub struct HbBuffer(*mut c_void);

impl HbBuffer {
    pub fn new() -> HbBuffer {
        // SAFETY: a new buffer, destroyed in Drop.
        HbBuffer(unsafe { bhb_buffer_create() })
    }

    /// Shapes `text[..line_len]` with the rest of `text` as its
    /// post-context, as perf/benchmark-shape.cc shapes a line, with the
    /// direction (an `hb_direction_t`, 0 for the guessed one).
    pub fn shape_line(
        &self,
        font: &HbFont,
        text: &[u8],
        line_len: usize,
        direction: i32,
    ) -> ShapedLine {
        let mut cap = 64usize;
        loop {
            let mut l = ShapedLine {
                glyphs: vec![0; cap],
                clusters: vec![0; cap],
                flags: vec![0; cap],
                positions: vec![0; cap * 4],
            };
            // SAFETY: valid font and buffer; the text is valid for its
            // length; the arrays hold `cap` glyphs.
            let n = unsafe {
                bhb_shape_line(
                    font.0,
                    self.0,
                    text.as_ptr(),
                    text.len() as c_uint,
                    line_len as c_uint,
                    direction,
                    l.glyphs.as_mut_ptr(),
                    l.clusters.as_mut_ptr(),
                    l.flags.as_mut_ptr(),
                    l.positions.as_mut_ptr(),
                    cap as c_uint,
                )
            } as usize;
            if n <= cap {
                l.glyphs.truncate(n);
                l.clusters.truncate(n);
                l.flags.truncate(n);
                l.positions.truncate(n * 4);
                return l;
            }
            cap = n;
        }
    }

    /// perf/benchmark-shape.cc's loop over the lines of `text`: the sum of
    /// the checksums of the lines.
    pub fn shape_text(&self, font: &HbFont, text: &[u8], direction: i32) -> u64 {
        // SAFETY: valid font and buffer; the text is valid for its length.
        unsafe {
            bhb_shape_text(
                font.0,
                self.0,
                text.as_ptr(),
                text.len() as c_uint,
                direction,
            )
        }
    }
}

impl Drop for HbBuffer {
    fn drop(&mut self) {
        // SAFETY: a valid buffer, dropped once.
        unsafe { bhb_buffer_destroy(self.0) };
    }
}

/// From the font's bytes to the first line shaped, cold; the line's
/// checksum.
pub fn hb_first_shape(data: &[u8], text: &[u8], line_len: usize, direction: i32) -> u64 {
    // SAFETY: the data and text are valid for their lengths; everything
    // HarfBuzz makes is destroyed before the call returns.
    unsafe {
        bhb_first_shape(
            data.as_ptr(),
            data.len() as c_uint,
            text.as_ptr(),
            text.len() as c_uint,
            line_len as c_uint,
            direction,
        )
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
