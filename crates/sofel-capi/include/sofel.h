/*
 * sofel.h: the C ABI of sofel (ADR 0015).
 *
 * sofel's own ABI, not HarfBuzz's or FreeType's. Until 1.0.0 the ABI may
 * change between versions; pin the version you build against.
 *
 * Conventions
 *
 * - Every name starts with `sofel_` (functions, types) or `SOFEL_`
 *   (macros). Types are opaque structs behind pointers and fixed-width
 *   integers.
 * - A function that can fail returns an `int32_t`: SOFEL_OK (0) on
 *   success, a negative SOFEL_ERROR_* otherwise. Results go to output
 *   arguments, which are written only on success unless said otherwise.
 * - A NULL pointer argument is an error (SOFEL_ERROR_NULL), except the
 *   `user` pointers, which are passed through untouched, and the
 *   `*_destroy` functions, which do nothing for NULL.
 * - Variable-length output goes to an array the caller gives with its
 *   capacity. When the array is too small, the function writes the
 *   needed length, writes nothing to the array, and returns
 *   SOFEL_ERROR_BUFFER_TOO_SMALL, like snprintf. The array may be NULL
 *   when its capacity is 0.
 * - Values are immutable: a sofel_font_t may be used from several threads
 *   at once. Work areas (sofel_outline_scratch_t, sofel_rasterizer_t) are
 *   owned by the caller and must not be shared between threads; make one
 *   per thread and reuse it.
 * - Coordinates: font units, or 26.6 fixed point (64 = one pixel) when
 *   scaled; scales are 16.16 fixed point (0x10000 = 1). The y axis goes
 *   up.
 *
 * Each function names the Rust function it corresponds to ("Rust:") and
 * the HarfBuzz and FreeType functions closest to it.
 * scripts/check-capi.sh checks the "Rust:" lines against the public
 * functions of the library crates.
 */

#ifndef SOFEL_H
#define SOFEL_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------ version */

#define SOFEL_VERSION_MAJOR 0
#define SOFEL_VERSION_MINOR 0
#define SOFEL_VERSION_PATCH 0
/* The version as one number; each part below 256. */
#define SOFEL_VERSION \
  ((SOFEL_VERSION_MAJOR << 16) | (SOFEL_VERSION_MINOR << 8) | SOFEL_VERSION_PATCH)

/*
 * The version of the library linked, in the form of SOFEL_VERSION.
 *
 * Rust: none
 * HarfBuzz: hb_version
 * FreeType: FT_Library_Version
 */
uint32_t sofel_version(void);

/* ------------------------------------------------------------- errors */

#define SOFEL_OK 0
/* A pointer argument is NULL. */
#define SOFEL_ERROR_NULL (-1)
/* The data is not a readable sfnt or collection. */
#define SOFEL_ERROR_INVALID_FONT (-2)
/* The font has no readable head table. */
#define SOFEL_ERROR_NO_HEAD (-3)
/* No font with this index in the data. */
#define SOFEL_ERROR_FONT_INDEX (-4)
/* The font has no such glyph or table. */
#define SOFEL_ERROR_NOT_FOUND (-5)
/* The glyph id is not below the glyph count. */
#define SOFEL_ERROR_INVALID_GLYPH (-6)
/* The font has no glyf and loca (or GLYF and LOCA) tables. */
#define SOFEL_ERROR_NO_GLYPH_DATA (-7)
/* A glyph's data is broken. */
#define SOFEL_ERROR_INVALID_OUTLINE (-8)
/* A composite glyph is broken, recursive or too deep. */
#define SOFEL_ERROR_INVALID_COMPOSITE (-9)
/* The bitmap would be too large. */
#define SOFEL_ERROR_TOO_LARGE (-10)
/* The output array is too small; the needed length was written. */
#define SOFEL_ERROR_BUFFER_TOO_SMALL (-11)
/* An argument has a value outside the ones listed for it. */
#define SOFEL_ERROR_INVALID_ARGUMENT (-12)

/*
 * A short English description of an error number, as a static
 * NUL-terminated string; "unknown error" for numbers not listed above.
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: FT_Error_String
 */
const char *sofel_error_string(int32_t error);

/* ---------------------------------------------------------- geometry */

/*
 * (a * b) / 0x10000, with halves rounded away from zero; saturated to
 * int32_t.
 *
 * Rust: sofel_geom::mul_fix
 * HarfBuzz: none
 * FreeType: FT_MulFix
 */
int32_t sofel_mul_fix(int32_t a, int32_t b);

/*
 * (a * 0x10000) / b, rounded on the magnitudes; 0x7FFFFFFF with the sign
 * of a when b is 0.
 *
 * Rust: sofel_geom::div_fix
 * HarfBuzz: none
 * FreeType: FT_DivFix
 */
int32_t sofel_div_fix(int32_t a, int32_t b);

/*
 * (a * b) / c, rounded on the magnitudes.
 *
 * Rust: sofel_geom::mul_div
 * HarfBuzz: none
 * FreeType: FT_MulDiv
 */
int32_t sofel_mul_div(int32_t a, int32_t b, int32_t c);

/*
 * (a * b) / c, truncated on the magnitudes.
 *
 * Rust: sofel_geom::mul_div_no_round
 * HarfBuzz: none
 * FreeType: FT_MulDiv_No_Round
 */
int32_t sofel_mul_div_no_round(int32_t a, int32_t b, int32_t c);

/*
 * Transforms (*x, *y) in place by the 16.16 matrix (xx xy; yx yy); x and
 * y must point to distinct integers:
 * x' = x*xx + y*xy, y' = x*yx + y*yy, each product rounded by
 * sofel_mul_fix.
 *
 * Rust: sofel_geom::transform
 * HarfBuzz: none
 * FreeType: FT_Vector_Transform
 */
int32_t sofel_transform(int32_t *x, int32_t *y, int32_t xx, int32_t xy, int32_t yx,
                        int32_t yy);

/*
 * The length of (x, y), computed with FreeType's CORDIC.
 *
 * Rust: sofel_geom::vector_length
 * HarfBuzz: none
 * FreeType: FT_Vector_Length
 */
int32_t sofel_vector_length(int32_t x, int32_t y);

/* -------------------------------------------------------------- fonts */

/* A table tag from its four characters, e.g. SOFEL_TAG('c','m','a','p'). */
#define SOFEL_TAG(a, b, c, d) \
  (((uint32_t)(uint8_t)(a) << 24) | ((uint32_t)(uint8_t)(b) << 16) | \
   ((uint32_t)(uint8_t)(c) << 8) | (uint32_t)(uint8_t)(d))

/* A font: immutable, usable from several threads at once. */
typedef struct sofel_font sofel_font_t;

/*
 * Opens font `index` of `len` bytes at `data`: 0 for a single sfnt, any
 * index of a TTC/OTC collection. The data is not copied: keep it alive
 * and unchanged until sofel_font_destroy.
 *
 * Returns the font, or NULL with an error number in *error
 * (SOFEL_ERROR_NULL, SOFEL_ERROR_INVALID_FONT, SOFEL_ERROR_NO_HEAD,
 * SOFEL_ERROR_FONT_INDEX). *error is SOFEL_OK on success. When `error`
 * is NULL, returns NULL. The font keeps HarfBuzz's CFF and CFF2
 * accelerators for its glyph boxes (ADR 0021); a broken CFF only leaves
 * them out.
 *
 * Rust: sofel_font::Font::new, sofel_outline::HbExtents::new
 * HarfBuzz: hb_blob_create (HB_MEMORY_MODE_READONLY), hb_face_create, hb_font_create
 * FreeType: FT_New_Memory_Face
 */
sofel_font_t *sofel_font_create(const uint8_t *data, size_t len, uint32_t index,
                                int32_t *error);

/*
 * Frees a font made by sofel_font_create. Does nothing for NULL.
 *
 * Rust: none
 * HarfBuzz: hb_font_destroy, hb_face_destroy
 * FreeType: FT_Done_Face
 */
void sofel_font_destroy(sofel_font_t *font);

/*
 * unitsPerEm of the head table, as stored.
 *
 * Rust: sofel_font::Font::units_per_em
 * HarfBuzz: hb_face_get_upem
 * FreeType: FT_Face->units_per_EM
 */
int32_t sofel_font_units_per_em(const sofel_font_t *font, uint16_t *upem);

/*
 * numGlyphs of MAXP if present, else of maxp, else 0.
 *
 * Rust: sofel_font::Font::num_glyphs
 * HarfBuzz: hb_face_get_glyph_count
 * FreeType: FT_Face->num_glyphs
 */
int32_t sofel_font_num_glyphs(const sofel_font_t *font, uint32_t *count);

/*
 * The glyph for a character, from the cmap subtable HarfBuzz would
 * choose. SOFEL_ERROR_NOT_FOUND when the character is not mapped.
 *
 * Rust: sofel_font::Font::nominal_glyph
 * HarfBuzz: hb_font_get_nominal_glyph
 * FreeType: FT_Get_Char_Index
 */
int32_t sofel_font_nominal_glyph(const sofel_font_t *font, uint32_t codepoint,
                                 uint32_t *glyph);

/*
 * The glyph for the variation sequence codepoint + selector (cmap format
 * 14). SOFEL_ERROR_NOT_FOUND when the sequence is not mapped.
 *
 * Rust: sofel_font::Font::variation_glyph
 * HarfBuzz: hb_font_get_variation_glyph
 * FreeType: FT_Face_GetCharVariantIndex
 */
int32_t sofel_font_variation_glyph(const sofel_font_t *font, uint32_t codepoint,
                                   uint32_t selector, uint32_t *glyph);

/*
 * The horizontal advance of a glyph in font units, as HarfBuzz gives it
 * (0 for glyphs beyond the metrics).
 *
 * Rust: sofel_font::Font::advance
 * HarfBuzz: hb_font_get_glyph_h_advance (with the scale at unitsPerEm)
 * FreeType: FT_Get_Advance (FT_LOAD_NO_SCALE)
 */
int32_t sofel_font_h_advance(const sofel_font_t *font, uint32_t glyph, uint16_t *advance);

/*
 * The left side bearing of a glyph in font units, as FreeType reads it
 * for outlines (tt_face_get_metrics), which sofel_font_h_advance does not
 * follow (ADR 0018).
 *
 * Rust: sofel_font::Font::ft_left_side_bearing
 * HarfBuzz: none
 * FreeType: FT_Get_Glyph_Metrics (horiBearingX with FT_LOAD_NO_SCALE)
 */
int32_t sofel_font_ft_left_side_bearing(const sofel_font_t *font, uint32_t glyph,
                                        int16_t *bearing);

/*
 * The bytes of a table, without copying: *data points into the font's
 * data and stays valid as long as the data does.
 * SOFEL_ERROR_NOT_FOUND when the font has no such table.
 *
 * Rust: sofel_font::Font::table, sofel_read::Tag::from_u32, sofel_read::Tag::new, sofel_read::Tag::to_u32
 * HarfBuzz: hb_face_reference_table
 * FreeType: FT_Load_Sfnt_Table
 */
int32_t sofel_font_table(const sofel_font_t *font, uint32_t tag, const uint8_t **data,
                         size_t *len);

/* ------------------------------------------------------------ metrics */

/*
 * Font-wide and vertical metrics (ADR 0016). Values are in font units,
 * as HarfBuzz gives them at its default scale (both scales unitsPerEm);
 * sofel_scale_t gives them at other scales. Glyph boxes come from glyf
 * only for now (CFF with ADR 0021, color glyphs with ADR 0023).
 */

/*
 * unitsPerEm as HarfBuzz uses it: 1000 when the stored value is outside
 * 16..=16384.
 *
 * Rust: sofel_font::Font::hb_units_per_em
 * HarfBuzz: hb_face_get_upem
 * FreeType: none
 */
int32_t sofel_font_hb_units_per_em(const sofel_font_t *font, uint16_t *upem);

/* The fields of the OS/2 table for sofel_font_os2_field. */
#define SOFEL_OS2_VERSION 0
#define SOFEL_OS2_X_AVG_CHAR_WIDTH 1
#define SOFEL_OS2_US_WEIGHT_CLASS 2
#define SOFEL_OS2_US_WIDTH_CLASS 3
#define SOFEL_OS2_FS_TYPE 4
#define SOFEL_OS2_Y_SUBSCRIPT_X_SIZE 5
#define SOFEL_OS2_Y_SUBSCRIPT_Y_SIZE 6
#define SOFEL_OS2_Y_SUBSCRIPT_X_OFFSET 7
#define SOFEL_OS2_Y_SUBSCRIPT_Y_OFFSET 8
#define SOFEL_OS2_Y_SUPERSCRIPT_X_SIZE 9
#define SOFEL_OS2_Y_SUPERSCRIPT_Y_SIZE 10
#define SOFEL_OS2_Y_SUPERSCRIPT_X_OFFSET 11
#define SOFEL_OS2_Y_SUPERSCRIPT_Y_OFFSET 12
#define SOFEL_OS2_Y_STRIKEOUT_SIZE 13
#define SOFEL_OS2_Y_STRIKEOUT_POSITION 14
#define SOFEL_OS2_S_FAMILY_CLASS 15
/* ulUnicodeRange1..4: SOFEL_OS2_UL_UNICODE_RANGE1 + 0..3. */
#define SOFEL_OS2_UL_UNICODE_RANGE1 16
/* achVendID as a big-endian 32-bit value. */
#define SOFEL_OS2_ACH_VEND_ID 20
#define SOFEL_OS2_FS_SELECTION 21
#define SOFEL_OS2_US_FIRST_CHAR_INDEX 22
#define SOFEL_OS2_US_LAST_CHAR_INDEX 23
#define SOFEL_OS2_S_TYPO_ASCENDER 24
#define SOFEL_OS2_S_TYPO_DESCENDER 25
#define SOFEL_OS2_S_TYPO_LINE_GAP 26
#define SOFEL_OS2_US_WIN_ASCENT 27
#define SOFEL_OS2_US_WIN_DESCENT 28
/* ulCodePageRange1..2 (version 1 and later). */
#define SOFEL_OS2_UL_CODE_PAGE_RANGE1 29
#define SOFEL_OS2_UL_CODE_PAGE_RANGE2 30
/* Version 2 and later. */
#define SOFEL_OS2_SX_HEIGHT 31
#define SOFEL_OS2_S_CAP_HEIGHT 32
#define SOFEL_OS2_US_DEFAULT_CHAR 33
#define SOFEL_OS2_US_BREAK_CHAR 34
#define SOFEL_OS2_US_MAX_CONTEXT 35
/* Version 5 and later. */
#define SOFEL_OS2_US_LOWER_OPTICAL_POINT_SIZE 36
#define SOFEL_OS2_US_UPPER_OPTICAL_POINT_SIZE 37
/* panose[0..9]: SOFEL_OS2_PANOSE + 0..9. */
#define SOFEL_OS2_PANOSE 38

/*
 * A field of the OS/2 table (SOFEL_OS2_*). SOFEL_ERROR_NOT_FOUND when the
 * font has no readable OS/2 table or the field is past its version;
 * SOFEL_ERROR_INVALID_ARGUMENT for an unknown field.
 *
 * Rust: sofel_font::Font::os2
 * HarfBuzz: none
 * FreeType: FT_Get_Sfnt_Table (FT_SFNT_OS2)
 */
int32_t sofel_font_os2_field(const sofel_font_t *font, int32_t field, int64_t *value);

/*
 * The embedding permissions: fsType of OS/2, 0 without it or when its
 * version is 0xFFFF.
 *
 * Rust: sofel_font::Font::fs_type
 * HarfBuzz: none
 * FreeType: FT_Get_FSType_Flags
 */
int32_t sofel_font_fs_type(const sofel_font_t *font, uint16_t *fs_type);

/*
 * The horizontal ascender, descender (negative) and line gap, with
 * HarfBuzz's fallback, in extents[0..2].
 *
 * Rust: sofel_font::Font::h_extents
 * HarfBuzz: hb_font_get_extents_for_direction (HB_DIRECTION_LTR)
 * FreeType: none
 */
int32_t sofel_font_h_extents(const sofel_font_t *font, int32_t extents[3]);

/*
 * The vertical ascender, descender and line gap, with HarfBuzz's
 * fallback, in extents[0..2].
 *
 * Rust: sofel_font::Font::v_extents
 * HarfBuzz: hb_font_get_extents_for_direction (HB_DIRECTION_TTB)
 * FreeType: none
 */
int32_t sofel_font_v_extents(const sofel_font_t *font, int32_t extents[3]);

/*
 * The vertical advance, negative downward as in HarfBuzz.
 *
 * Rust: sofel_font::Font::v_advance
 * HarfBuzz: hb_font_get_glyph_v_advance
 * FreeType: none (FT_Get_Advance with FT_LOAD_VERTICAL_LAYOUT is positive
 *           and falls back differently)
 */
int32_t sofel_font_v_advance(const sofel_font_t *font, uint32_t glyph, int32_t *advance);

/*
 * The vertical origin (x, y) in origin[0..1]: VORG, else glyf's top
 * phantom point (with vmtx), else centered on the glyph box of
 * sofel_font_glyph_extents.
 *
 * Rust: sofel_font::Font::v_origin
 * HarfBuzz: hb_font_get_glyph_v_origin
 * FreeType: none
 */
int32_t sofel_font_v_origin(const sofel_font_t *font, uint32_t glyph, int32_t origin[2]);

/*
 * The glyph box (x_bearing, y_bearing, width, height) in extents[0..3];
 * height is negative. SOFEL_ERROR_NOT_FOUND when there is no box. As
 * HarfBuzz gives it from glyf, else CFF2, else CFF (ADR 0021); color
 * glyphs come with ADR 0023.
 *
 * Rust: sofel_outline::HbExtents::glyph_extents, sofel_font::Font::glyf_extents
 * HarfBuzz: hb_font_get_glyph_extents
 * FreeType: none (FT_Outline_Get_CBox of the loaded outline)
 */
int32_t sofel_font_glyph_extents(const sofel_font_t *font, uint32_t glyph, int32_t extents[4]);

/*
 * A font-wide metric by its hb_ot_metrics_tag_t tag ('hasc', 'xhgt',
 * ...). SOFEL_ERROR_NOT_FOUND when the font does not have it,
 * SOFEL_ERROR_INVALID_ARGUMENT for an unknown tag.
 *
 * Rust: sofel_font::Font::metrics_position, sofel_font::MetricsTag::from_tag, sofel_font::MetricsTag::tag
 * HarfBuzz: hb_ot_metrics_get_position
 * FreeType: none
 */
int32_t sofel_font_metrics_position(const sofel_font_t *font, uint32_t tag, int32_t *position);

/*
 * Like sofel_font_metrics_position, made up from other metrics when the
 * font does not have it.
 *
 * Rust: sofel_font::Font::metrics_position_with_fallback
 * HarfBuzz: hb_ot_metrics_get_position_with_fallback
 * FreeType: none
 */
int32_t sofel_font_metrics_position_with_fallback(const sofel_font_t *font, uint32_t tag,
                                                  int32_t *position);

/* HarfBuzz's scale of a font (x_scale, y_scale). Immutable. */
typedef struct sofel_scale_t sofel_scale_t;

/*
 * A scale for `font`; NULL on failure, with the reason in *error.
 * Destroy it with sofel_scale_destroy.
 *
 * Rust: sofel_font::HbScale::for_font
 * HarfBuzz: hb_font_set_scale
 * FreeType: none
 */
sofel_scale_t *sofel_scale_create(const sofel_font_t *font, int32_t x_scale, int32_t y_scale,
                                  int32_t *error);

/*
 * Frees a scale. NULL does nothing.
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: none
 */
void sofel_scale_destroy(sofel_scale_t *scale);

/*
 * A value in font units scaled like HarfBuzz's integer values
 * ((v * mult + 32768) >> 16).
 *
 * Rust: sofel_font::HbScale::em_scale_x
 * HarfBuzz: none (hb_font_t::em_scale_x)
 * FreeType: none
 */
int32_t sofel_scale_em_scale_x(const sofel_scale_t *scale, int32_t v, int32_t *out);

/*
 * Rust: sofel_font::HbScale::em_scale_y
 * HarfBuzz: none (hb_font_t::em_scale_y)
 * FreeType: none
 */
int32_t sofel_scale_em_scale_y(const sofel_scale_t *scale, int32_t v, int32_t *out);

/*
 * Scales a glyph box (x_bearing, y_bearing, width, height) in place,
 * rounding outward.
 *
 * Rust: sofel_font::HbScale::scale_glyph_extents
 * HarfBuzz: none (hb_font_t::scale_glyph_extents)
 * FreeType: none
 */
int32_t sofel_scale_glyph_extents(const sofel_scale_t *scale, int32_t extents[4]);

/*
 * sofel_font_h_extents at this scale (the fallback is computed at it).
 *
 * Rust: sofel_font::HbScale::h_extents
 * HarfBuzz: hb_font_get_extents_for_direction (HB_DIRECTION_LTR)
 * FreeType: none
 */
int32_t sofel_scale_h_extents(const sofel_scale_t *scale, const sofel_font_t *font,
                              int32_t extents[3]);

/*
 * Rust: sofel_font::HbScale::v_extents
 * HarfBuzz: hb_font_get_extents_for_direction (HB_DIRECTION_TTB)
 * FreeType: none
 */
int32_t sofel_scale_v_extents(const sofel_scale_t *scale, const sofel_font_t *font,
                              int32_t extents[3]);

/*
 * Rust: sofel_font::HbScale::h_advance
 * HarfBuzz: hb_font_get_glyph_h_advance
 * FreeType: none
 */
int32_t sofel_scale_h_advance(const sofel_scale_t *scale, const sofel_font_t *font,
                              uint32_t glyph, int32_t *advance);

/*
 * Rust: sofel_font::HbScale::v_advance
 * HarfBuzz: hb_font_get_glyph_v_advance
 * FreeType: none
 */
int32_t sofel_scale_v_advance(const sofel_scale_t *scale, const sofel_font_t *font,
                              uint32_t glyph, int32_t *advance);

/*
 * Rust: sofel_font::HbScale::v_origin
 * HarfBuzz: hb_font_get_glyph_v_origin
 * FreeType: none
 */
int32_t sofel_scale_v_origin(const sofel_scale_t *scale, const sofel_font_t *font,
                             uint32_t glyph, int32_t origin[2]);

/*
 * Rust: sofel_outline::HbExtents::scaled_glyph_extents, sofel_font::HbScale::glyf_extents
 * HarfBuzz: hb_font_get_glyph_extents
 * FreeType: none
 */
int32_t sofel_scale_glyph_extents_of(const sofel_scale_t *scale, const sofel_font_t *font,
                                     uint32_t glyph, int32_t extents[4]);

/*
 * Rust: sofel_font::HbScale::metrics_position
 * HarfBuzz: hb_ot_metrics_get_position
 * FreeType: none
 */
int32_t sofel_scale_metrics_position(const sofel_scale_t *scale, const sofel_font_t *font,
                                     uint32_t tag, int32_t *position);

/*
 * Rust: sofel_font::HbScale::metrics_position_with_fallback
 * HarfBuzz: hb_ot_metrics_get_position_with_fallback
 * FreeType: none
 */
int32_t sofel_scale_metrics_position_with_fallback(const sofel_scale_t *scale,
                                                   const sofel_font_t *font, uint32_t tag,
                                                   int32_t *position);

/* -------------------------------------------------------------- names */

/*
 * The number of names HarfBuzz lists: the best record for each name id
 * and language (ADR 0016).
 *
 * Rust: sofel_font::Font::names
 * HarfBuzz: hb_ot_name_list_names
 * FreeType: none
 */
int32_t sofel_font_name_count(const sofel_font_t *font, size_t *count);

/*
 * Name `index` of that list: its name id, and its language as a BCP 47
 * tag (not NUL-terminated) in lang[0..*len]. SOFEL_ERROR_NOT_FOUND past
 * the list.
 *
 * Rust: sofel_font::Font::names, sofel_font::Language::write, sofel_font::Language::len
 * HarfBuzz: hb_ot_name_list_names, hb_language_to_string
 * FreeType: none
 */
int32_t sofel_font_name_entry(const sofel_font_t *font, size_t index, uint16_t *name_id,
                              char *lang, size_t capacity, size_t *len);

/*
 * A name in UTF-8, as hb_ot_name_get_utf8 writes it: as much as fits in
 * text[0..capacity] before a terminating 0; *written is the units written
 * before the 0, *total the units of the whole name (0 when there is no
 * such name). `lang` is a BCP 47 tag of `lang_len` bytes; with a length
 * of 0 (and `lang` NULL or not) English is used, as HarfBuzz does for
 * HB_LANGUAGE_INVALID.
 *
 * Rust: sofel_font::Font::name_utf8, sofel_font::Font::name_entry, sofel_font::Language::new
 * HarfBuzz: hb_ot_name_get_utf8
 * FreeType: none
 */
int32_t sofel_font_name_utf8(const sofel_font_t *font, uint16_t name_id, const char *lang,
                             size_t lang_len, char *text, size_t capacity, size_t *written,
                             size_t *total);

/*
 * Rust: sofel_font::Font::name_utf16
 * HarfBuzz: hb_ot_name_get_utf16
 * FreeType: none
 */
int32_t sofel_font_name_utf16(const sofel_font_t *font, uint16_t name_id, const char *lang,
                              size_t lang_len, uint16_t *text, size_t capacity, size_t *written,
                              size_t *total);

/*
 * Rust: sofel_font::Font::name_utf32
 * HarfBuzz: hb_ot_name_get_utf32
 * FreeType: none
 */
int32_t sofel_font_name_utf32(const sofel_font_t *font, uint16_t name_id, const char *lang,
                              size_t lang_len, uint32_t *text, size_t capacity, size_t *written,
                              size_t *total);

/*
 * The number of name records FreeType keeps.
 *
 * Rust: sofel_font::Font::sfnt_name_count
 * HarfBuzz: none
 * FreeType: FT_Get_Sfnt_Name_Count
 */
int32_t sofel_font_sfnt_name_count(const sofel_font_t *font, size_t *count);

/*
 * Record `index` of those, raw: ids[0..3] are the platform, encoding,
 * language and name ids; *string points into the font's data.
 *
 * Rust: sofel_font::Font::sfnt_name
 * HarfBuzz: none
 * FreeType: FT_Get_Sfnt_Name
 */
int32_t sofel_font_sfnt_name(const sofel_font_t *font, size_t index, uint16_t ids[4],
                             const uint8_t **string, size_t *len);

/*
 * The PostScript name as FreeType finds it (not NUL-terminated).
 * SOFEL_ERROR_NOT_FOUND when there is none.
 *
 * Rust: sofel_font::Font::postscript_name
 * HarfBuzz: none
 * FreeType: FT_Get_Postscript_Name
 */
int32_t sofel_font_postscript_name(const sofel_font_t *font, char *name, size_t capacity,
                                   size_t *len);

/*
 * The name of a glyph as HarfBuzz gives it from post, else from the
 * charset of a CFF font that is not CID-keyed (ADR 0021); *name points
 * into the font's data or static memory, and is not cut at a 0 byte as
 * HarfBuzz's copy is. SOFEL_ERROR_NOT_FOUND without one.
 *
 * Rust: sofel_font::Font::glyph_name, sofel_font::GlyphNames::glyph_name
 * HarfBuzz: hb_font_get_glyph_name
 * FreeType: none
 */
int32_t sofel_font_glyph_name(const sofel_font_t *font, uint32_t glyph, const uint8_t **name,
                              size_t *len);

/*
 * The name of a glyph as FreeType gives it for sfnt fonts.
 *
 * Rust: sofel_font::Font::ft_glyph_name, sofel_font::GlyphNames::ft_glyph_name
 * HarfBuzz: none
 * FreeType: FT_Get_Glyph_Name
 */
int32_t sofel_font_ft_glyph_name(const sofel_font_t *font, uint32_t glyph, const uint8_t **name,
                                 size_t *len);

/* An index of a font's glyph names. Immutable; the font must outlive it. */
typedef struct sofel_glyph_names_t sofel_glyph_names_t;

/*
 * Builds the index; NULL on failure, with the reason in *error.
 *
 * Rust: sofel_font::Font::glyph_names, sofel_font::Font::glyph_names_len, sofel_font::Font::glyph_names_from
 * HarfBuzz: none (built on the first hb_font_get_glyph_from_name)
 * FreeType: none
 */
sofel_glyph_names_t *sofel_glyph_names_create(const sofel_font_t *font, int32_t *error);

/*
 * Frees an index. NULL does nothing.
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: none
 */
void sofel_glyph_names_destroy(sofel_glyph_names_t *names);

/*
 * The glyph with a name, as HarfBuzz finds it from post.
 * SOFEL_ERROR_NOT_FOUND without one.
 *
 * Rust: sofel_font::GlyphNames::glyph_from_name
 * HarfBuzz: hb_font_get_glyph_from_name
 * FreeType: none
 */
int32_t sofel_glyph_names_glyph_from_name(const sofel_glyph_names_t *names, const char *name,
                                          size_t len, uint32_t *glyph);

/*
 * The first glyph with a name, as FreeType finds it.
 * SOFEL_ERROR_NOT_FOUND without one (FreeType returns 0).
 *
 * Rust: sofel_font::GlyphNames::ft_glyph_from_name
 * HarfBuzz: none
 * FreeType: FT_Get_Name_Index
 */
int32_t sofel_glyph_names_ft_glyph_from_name(const sofel_glyph_names_t *names, const char *name,
                                             size_t len, uint32_t *glyph);

/*
 * FreeType's FT_Face values in values[0..10]: ascender, descender,
 * height, max_advance_width, max_advance_height, underline_position,
 * underline_thickness, bbox xMin, yMin, xMax, yMax.
 *
 * Rust: sofel_font::Font::ft_face_metrics
 * HarfBuzz: none
 * FreeType: the FT_Face fields
 */
int32_t sofel_font_ft_face_metrics(const sofel_font_t *font, int32_t values[11]);

/*
 * The vertical advance as FreeType gives it (positive).
 * SOFEL_ERROR_INVALID_GLYPH for glyph ids not below the glyph count.
 *
 * Rust: sofel_font::Font::ft_v_advance
 * HarfBuzz: none
 * FreeType: FT_Get_Advance (FT_LOAD_VERTICAL_LAYOUT | FT_LOAD_NO_SCALE)
 */
int32_t sofel_font_ft_v_advance(const sofel_font_t *font, uint32_t glyph, uint16_t *advance);

/* ----------------------------------------------------------- outlines */

/*
 * The pen: functions that receive an outline as drawing commands, in
 * the order of FreeType's FT_Outline_Decompose. Each contour starts with
 * move_to, returns to its start point with a line or curve, and ends
 * with close. `user` is the pointer given with the functions.
 */

/*
 * Starts a contour at (x, y).
 *
 * Rust: sofel_geom::Pen::move_to
 * HarfBuzz: hb_draw_move_to_func_t
 * FreeType: FT_Outline_MoveToFunc
 */
typedef void (*sofel_move_to_func_t)(void *user, int32_t x, int32_t y);

/*
 * Draws a line from the current point to (x, y).
 *
 * Rust: sofel_geom::Pen::line_to
 * HarfBuzz: hb_draw_line_to_func_t
 * FreeType: FT_Outline_LineToFunc
 */
typedef void (*sofel_line_to_func_t)(void *user, int32_t x, int32_t y);

/*
 * Draws a quadratic Bezier curve through the control point (cx, cy) to
 * (x, y).
 *
 * Rust: sofel_geom::Pen::quad_to
 * HarfBuzz: hb_draw_quadratic_to_func_t
 * FreeType: FT_Outline_ConicToFunc
 */
typedef void (*sofel_quad_to_func_t)(void *user, int32_t cx, int32_t cy, int32_t x,
                                     int32_t y);

/*
 * Draws a cubic Bezier curve through the control points (c1x, c1y) and
 * (c2x, c2y) to (x, y).
 *
 * Rust: sofel_geom::Pen::cubic_to
 * HarfBuzz: hb_draw_cubic_to_func_t
 * FreeType: FT_Outline_CubicToFunc
 */
typedef void (*sofel_cubic_to_func_t)(void *user, int32_t c1x, int32_t c1y, int32_t c2x,
                                      int32_t c2y, int32_t x, int32_t y);

/*
 * Ends the current contour.
 *
 * Rust: sofel_geom::Pen::close
 * HarfBuzz: hb_draw_close_path_func_t
 * FreeType: none
 */
typedef void (*sofel_close_func_t)(void *user);

/* Keep font units (FT_LOAD_NO_SCALE); the x and y scales are ignored. */
#define SOFEL_SCALE_NONE 0
/* Multiply by the 16.16 x and y scales, giving 26.6 coordinates. */
#define SOFEL_SCALE_FIXED 1

/*
 * The work area of the outline functions: the points of the last loaded
 * glyph. Owned by the caller; one per thread, reused across glyphs.
 */
typedef struct sofel_outline_scratch sofel_outline_scratch_t;

/*
 * Makes an empty work area.
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: FT_GlyphSlot (the slot holds the outline)
 */
sofel_outline_scratch_t *sofel_outline_scratch_create(void);

/*
 * Frees a work area. Does nothing for NULL.
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: none
 */
void sofel_outline_scratch_destroy(sofel_outline_scratch_t *scratch);

/*
 * The 16.16 scales for x_ppem and y_ppem pixels per em, as
 * FT_Set_Pixel_Sizes sets them: FT_DivFix(ppem * 64, unitsPerEm), not
 * rounded.
 *
 * Rust: sofel_outline::Scale::pixels
 * HarfBuzz: hb_font_set_scale
 * FreeType: FT_Set_Pixel_Sizes
 */
int32_t sofel_outline_pixel_scale(const sofel_font_t *font, uint16_t x_ppem,
                                  uint16_t y_ppem, int32_t *x_scale, int32_t *y_scale);

/*
 * Loads the points of a glyph into `scratch` without hinting, without
 * sending any command; `mode` is SOFEL_SCALE_NONE or SOFEL_SCALE_FIXED.
 * Writes the control box (x_min, y_min, x_max, y_max) to cbox[0..3] and
 * 1 to *overlap if the outline has FT_OUTLINE_OVERLAP, else 0. A failed
 * load leaves `scratch` empty.
 *
 * Errors: SOFEL_ERROR_INVALID_GLYPH, SOFEL_ERROR_NO_GLYPH_DATA,
 * SOFEL_ERROR_INVALID_OUTLINE, SOFEL_ERROR_INVALID_COMPOSITE,
 * SOFEL_ERROR_INVALID_ARGUMENT (mode).
 *
 * Rust: sofel_outline::load
 * HarfBuzz: none
 * FreeType: FT_Load_Glyph (FT_LOAD_NO_HINTING), FT_Outline_Get_CBox
 */
int32_t sofel_outline_load(const sofel_font_t *font, uint32_t glyph, uint32_t mode,
                           int32_t x_scale, int32_t y_scale,
                           sofel_outline_scratch_t *scratch, int32_t *cbox,
                           uint32_t *overlap);

/*
 * Sends the outline last loaded into `scratch` to the pen, after moving
 * it by (dx, dy) and multiplying every point by `multiplier`, as
 * FT_Outline_Translate followed by FT_Outline_Decompose would.
 *
 * Rust: sofel_outline::Scratch::decompose
 * HarfBuzz: none
 * FreeType: FT_Outline_Translate, FT_Outline_Decompose
 */
int32_t sofel_outline_decompose(const sofel_outline_scratch_t *scratch, int32_t dx,
                                int32_t dy, int32_t multiplier,
                                sofel_move_to_func_t move_to, sofel_line_to_func_t line_to,
                                sofel_quad_to_func_t quad_to,
                                sofel_cubic_to_func_t cubic_to, sofel_close_func_t close,
                                void *user);

/*
 * sofel_outline_load followed by sofel_outline_decompose with no offset:
 * draws a glyph into the pen. Commands sent before an error stay sent.
 *
 * Rust: sofel_outline::draw
 * HarfBuzz: hb_font_draw_glyph
 * FreeType: FT_Load_Glyph (FT_LOAD_NO_HINTING), FT_Outline_Decompose
 */
int32_t sofel_outline_draw(const sofel_font_t *font, uint32_t glyph, uint32_t mode,
                           int32_t x_scale, int32_t y_scale,
                           sofel_outline_scratch_t *scratch, sofel_move_to_func_t move_to,
                           sofel_line_to_func_t line_to, sofel_quad_to_func_t quad_to,
                           sofel_cubic_to_func_t cubic_to, sofel_close_func_t close,
                           void *user, int32_t *cbox, uint32_t *overlap);

/* ------------------------------------------------------- rasterization */

/*
 * The rasterizer: records an outline in 26.6 units and renders 8-bit
 * coverage bitmaps. Owned by the caller; one per thread, reused across
 * glyphs.
 */
typedef struct sofel_rasterizer sofel_rasterizer_t;

/* The non-zero winding rule (glyf's). */
#define SOFEL_FILL_NON_ZERO 0
/* The even-odd rule. */
#define SOFEL_FILL_EVEN_ODD 1

/*
 * Makes an empty rasterizer whose bands hold at most 8192 cells.
 *
 * Rust: sofel_raster::Rasterizer::new
 * HarfBuzz: none
 * FreeType: FT_Outline_Render's pool
 */
sofel_rasterizer_t *sofel_rasterizer_create(void);

/*
 * Makes an empty rasterizer whose bands hold at most `cells` cells (at
 * least one row). The result does not depend on it.
 *
 * Rust: sofel_raster::Rasterizer::with_band_cells
 * HarfBuzz: none
 * FreeType: none
 */
sofel_rasterizer_t *sofel_rasterizer_create_with_band_cells(size_t cells);

/*
 * Frees a rasterizer. Does nothing for NULL.
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: none
 */
void sofel_rasterizer_destroy(sofel_rasterizer_t *rasterizer);

/*
 * Forgets the recorded outline.
 *
 * Rust: sofel_raster::Rasterizer::clear
 * HarfBuzz: none
 * FreeType: none
 */
int32_t sofel_rasterizer_clear(sofel_rasterizer_t *rasterizer);

/*
 * The pen of the rasterizer: records commands. The functions have the
 * types of the pen functions, with the rasterizer as `user`, so they can
 * be passed to sofel_outline_decompose. They do nothing for a NULL
 * rasterizer.
 *
 * Rust: sofel_geom::Pen::move_to
 * HarfBuzz: none
 * FreeType: none
 */
void sofel_rasterizer_move_to(void *rasterizer, int32_t x, int32_t y);

/*
 * Rust: sofel_geom::Pen::line_to
 * HarfBuzz: none
 * FreeType: none
 */
void sofel_rasterizer_line_to(void *rasterizer, int32_t x, int32_t y);

/*
 * Rust: sofel_geom::Pen::quad_to
 * HarfBuzz: none
 * FreeType: none
 */
void sofel_rasterizer_quad_to(void *rasterizer, int32_t cx, int32_t cy, int32_t x,
                              int32_t y);

/*
 * Rust: sofel_geom::Pen::cubic_to
 * HarfBuzz: none
 * FreeType: none
 */
void sofel_rasterizer_cubic_to(void *rasterizer, int32_t c1x, int32_t c1y, int32_t c2x,
                               int32_t c2y, int32_t x, int32_t y);

/*
 * Rust: sofel_geom::Pen::close
 * HarfBuzz: none
 * FreeType: none
 */
void sofel_rasterizer_close(void *rasterizer);

/*
 * The bitmap box for a control box in 26.6 units, rounded outward to
 * whole pixels: box[0..3] = left, top, width, rows (width and rows are
 * not negative), and the offset that moves the bitmap's bottom-left
 * corner to the origin, in 26.6, to shift[0..1].
 *
 * Rust: sofel_raster::pixel_box, sofel_raster::PixelBox::shift
 * HarfBuzz: none
 * FreeType: ft_glyphslot_preset_bitmap
 */
int32_t sofel_pixel_box(int32_t x_min, int32_t y_min, int32_t x_max, int32_t y_max,
                        int32_t *box, int32_t *shift);

/*
 * Renders the recorded outline into an 8-bit coverage bitmap, finding
 * the bitmap box from the recorded points. `fill` is SOFEL_FILL_*;
 * `overlap` nonzero renders with 4x4 supersampling (FT_OUTLINE_OVERLAP);
 * nonzero x_ppem and y_ppem reject bitmaps wider or taller than 16 times
 * them, as FreeType does.
 *
 * The bitmap goes to `pixels` (rows from the top, one byte per pixel):
 * *len is set to width * rows, and box[0..3] to left, top, width and
 * rows (FreeType's bitmap_left, bitmap_top, width, rows), also when
 * `capacity` is too small.
 *
 * Errors: SOFEL_ERROR_TOO_LARGE, SOFEL_ERROR_BUFFER_TOO_SMALL,
 * SOFEL_ERROR_INVALID_ARGUMENT (fill).
 *
 * Rust: sofel_raster::Rasterizer::render
 * HarfBuzz: none
 * FreeType: FT_Outline_Render, FT_Render_Glyph (FT_RENDER_MODE_NORMAL)
 */
int32_t sofel_rasterizer_render(sofel_rasterizer_t *rasterizer, uint32_t fill,
                                uint32_t overlap, uint16_t x_ppem, uint16_t y_ppem,
                                uint8_t *pixels, size_t capacity, size_t *len,
                                int32_t *box);

/*
 * Renders into the given box (left, top, width, rows, as from
 * sofel_pixel_box) an outline recorded already moved by the box's shift
 * (and multiplied by 4 after the move when `overlap` is nonzero). The
 * other arguments are those of sofel_rasterizer_render.
 *
 * Rust: sofel_raster::Rasterizer::render_placed
 * HarfBuzz: none
 * FreeType: ft_smooth_render
 */
int32_t sofel_rasterizer_render_placed(sofel_rasterizer_t *rasterizer, int32_t left,
                                       int32_t top, uint32_t width, uint32_t rows,
                                       uint32_t fill, uint32_t overlap, uint16_t x_ppem,
                                       uint16_t y_ppem, uint8_t *pixels, size_t capacity,
                                       size_t *len, int32_t *box);

/*
 * Renders a glyph at `ppem` pixels per em as FT_Set_Pixel_Sizes,
 * FT_Load_Glyph with FT_LOAD_NO_HINTING and FT_Render_Glyph with
 * FT_RENDER_MODE_NORMAL would. The outline previously recorded in
 * `rasterizer` is cleared. The bitmap is written as by
 * sofel_rasterizer_render.
 *
 * Errors: those of sofel_outline_load, SOFEL_ERROR_TOO_LARGE,
 * SOFEL_ERROR_BUFFER_TOO_SMALL.
 *
 * Rust: sofel_cli::glyph::render
 * HarfBuzz: none
 * FreeType: FT_Set_Pixel_Sizes, FT_Load_Glyph, FT_Render_Glyph
 */
int32_t sofel_render_glyph(const sofel_font_t *font, uint32_t glyph, uint16_t ppem,
                           sofel_outline_scratch_t *scratch, sofel_rasterizer_t *rasterizer,
                           uint8_t *pixels, size_t capacity, size_t *len, int32_t *box);

/* --------------------------------------------------------- validation */

/* One problem found by sofel_validate; valid only during the callback. */
typedef struct sofel_diagnostic sofel_diagnostic_t;

/* The data cannot be read as specified. */
#define SOFEL_SEVERITY_ERROR 0
/* The data violates the specification, but reading is not affected. */
#define SOFEL_SEVERITY_WARNING 1

/* Kinds of the values given by sofel_diagnostic_details. */
#define SOFEL_DETAIL_UINT 0
#define SOFEL_DETAIL_INT 1
#define SOFEL_DETAIL_TAG 2

/*
 * Receives one problem.
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: none
 */
typedef void (*sofel_diagnostic_func_t)(void *user, const sofel_diagnostic_t *diagnostic);

/*
 * Validates the sfnt or collection of `len` bytes at `data`: the table
 * directory and every table that has a validator, then, for a
 * collection, each of its fonts. Calls `report` once per problem; the
 * problems are the lines `sofel-validate` writes.
 *
 * Returns SOFEL_ERROR_INVALID_FONT, reporting nothing, when the data is
 * neither an sfnt nor a collection.
 *
 * Rust: sofel_read::sfnt::FontFile::new, sofel_read::sfnt::Collection::num_fonts, sofel_read::sfnt::Collection::font, sofel_read::validate::sfnt, sofel_read::validate::collection
 * HarfBuzz: hb_sanitize (internal)
 * FreeType: none (FT_Open_Face checks while loading)
 */
int32_t sofel_validate(const uint8_t *data, size_t len, sofel_diagnostic_func_t report,
                       void *user);

/*
 * The index of the font in the collection the problem is in;
 * SOFEL_ERROR_NOT_FOUND for a single sfnt and for the collection header.
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: none
 */
int32_t sofel_diagnostic_font(const sofel_diagnostic_t *diagnostic, uint32_t *index);

/*
 * The table the problem is in, as a tag: SOFEL_TAG('s','f','n','t') for
 * the table directory, SOFEL_TAG('t','t','c','f') for the collection
 * header.
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: none
 */
int32_t sofel_diagnostic_table(const sofel_diagnostic_t *diagnostic, uint32_t *tag);

/*
 * The offset of the problem from the start of the table (or directory,
 * or file).
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: none
 */
int32_t sofel_diagnostic_offset(const sofel_diagnostic_t *diagnostic, uint32_t *offset);

/*
 * SOFEL_SEVERITY_ERROR or SOFEL_SEVERITY_WARNING.
 *
 * Rust: sofel_read::Kind::severity
 * HarfBuzz: none
 * FreeType: none
 */
int32_t sofel_diagnostic_severity(const sofel_diagnostic_t *diagnostic, int32_t *severity);

/*
 * The identifier of the problem, e.g. "segments_not_sorted", as a
 * NUL-terminated string in `buffer`. *len is set to its length without
 * the NUL; `capacity` must be at least *len + 1.
 *
 * Rust: sofel_read::Kind::id
 * HarfBuzz: none
 * FreeType: none
 */
int32_t sofel_diagnostic_kind(const sofel_diagnostic_t *diagnostic, char *buffer,
                              size_t capacity, size_t *len);

/*
 * A description of the problem for people, written as by
 * sofel_diagnostic_kind.
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: none
 */
int32_t sofel_diagnostic_message(const sofel_diagnostic_t *diagnostic, char *buffer,
                                 size_t capacity, size_t *len);

/*
 * Receives one value of a problem: its NUL-terminated name (valid only
 * during the call), SOFEL_DETAIL_*, and the value (a tag as its uint32_t).
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: none
 */
typedef void (*sofel_detail_func_t)(void *user, const char *name, int32_t kind,
                                    int64_t value);

/*
 * Calls `detail` with each value of the problem, e.g. the expected and
 * found values, in the order of `sofel-validate --json`.
 *
 * Rust: sofel_read::Kind::details
 * HarfBuzz: none
 * FreeType: none
 */
int32_t sofel_diagnostic_details(const sofel_diagnostic_t *diagnostic,
                                 sofel_detail_func_t detail, void *user);

/* ------------------------------------------------------------ unicode */

/*
 * Unicode character properties from the Unicode Character Database
 * (ADR 0019), with HarfBuzz's default values beyond U+10FFFF. None of
 * these functions fails or allocates.
 */

/* General_Category: the values of HarfBuzz's hb_unicode_general_category_t. */
#define SOFEL_GC_CONTROL 0
#define SOFEL_GC_FORMAT 1
#define SOFEL_GC_UNASSIGNED 2
#define SOFEL_GC_PRIVATE_USE 3
#define SOFEL_GC_SURROGATE 4
#define SOFEL_GC_LOWERCASE_LETTER 5
#define SOFEL_GC_MODIFIER_LETTER 6
#define SOFEL_GC_OTHER_LETTER 7
#define SOFEL_GC_TITLECASE_LETTER 8
#define SOFEL_GC_UPPERCASE_LETTER 9
#define SOFEL_GC_SPACING_MARK 10
#define SOFEL_GC_ENCLOSING_MARK 11
#define SOFEL_GC_NON_SPACING_MARK 12
#define SOFEL_GC_DECIMAL_NUMBER 13
#define SOFEL_GC_LETTER_NUMBER 14
#define SOFEL_GC_OTHER_NUMBER 15
#define SOFEL_GC_CONNECT_PUNCTUATION 16
#define SOFEL_GC_DASH_PUNCTUATION 17
#define SOFEL_GC_CLOSE_PUNCTUATION 18
#define SOFEL_GC_FINAL_PUNCTUATION 19
#define SOFEL_GC_INITIAL_PUNCTUATION 20
#define SOFEL_GC_OTHER_PUNCTUATION 21
#define SOFEL_GC_OPEN_PUNCTUATION 22
#define SOFEL_GC_CURRENCY_SYMBOL 23
#define SOFEL_GC_MODIFIER_SYMBOL 24
#define SOFEL_GC_MATH_SYMBOL 25
#define SOFEL_GC_OTHER_SYMBOL 26
#define SOFEL_GC_LINE_SEPARATOR 27
#define SOFEL_GC_PARAGRAPH_SEPARATOR 28
#define SOFEL_GC_SPACE_SEPARATOR 29

/* Vertical_Orientation (UAX #50). */
#define SOFEL_VO_U 0
#define SOFEL_VO_R 1
#define SOFEL_VO_TU 2
#define SOFEL_VO_TR 3

/*
 * The Unicode version of the tables, (major << 16) | (minor << 8) |
 * update.
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: none
 */
uint32_t sofel_unicode_version(void);

/*
 * General_Category of a code point, SOFEL_GC_*; SOFEL_GC_UNASSIGNED
 * beyond U+10FFFF.
 *
 * Rust: sofel_unicode::general_category
 * HarfBuzz: hb_unicode_general_category
 * FreeType: none
 */
uint8_t sofel_unicode_general_category(uint32_t codepoint);

/*
 * Canonical_Combining_Class of a code point; 0 beyond U+10FFFF.
 *
 * Rust: sofel_unicode::combining_class
 * HarfBuzz: hb_unicode_combining_class
 * FreeType: none
 */
uint8_t sofel_unicode_combining_class(uint32_t codepoint);

/*
 * Script of a code point, as its ISO 15924 tag with the first letter in
 * the high byte (the values of hb_script_t: 'Latn' is 0x4C61746E). Common,
 * Inherited and Unknown are 'Zyyy', 'Zinh' and 'Zzzz'; 'Zzzz' beyond
 * U+10FFFF.
 *
 * Rust: sofel_unicode::script, sofel_unicode::Script::tag
 * HarfBuzz: hb_unicode_script
 * FreeType: none
 */
uint32_t sofel_unicode_script(uint32_t codepoint);

/*
 * The script of a four-letter ISO 15924 tag of `len` bytes, in the form
 * of sofel_unicode_script: the case is adjusted, HarfBuzz's variants
 * ('Aran', 'Latf', 'Hans', ...) map to their script, and 'Qaai' and
 * 'Qaac' to Inherited and Coptic. SOFEL_ERROR_INVALID_ARGUMENT when the
 * tag is not four letters.
 *
 * Rust: sofel_unicode::Script::from_iso15924
 * HarfBuzz: hb_script_from_string
 * FreeType: none
 */
int32_t sofel_script_from_iso15924(const uint8_t *tag, size_t len, uint32_t *script);

/*
 * Bidi_Mirroring_Glyph of a code point; the code point itself if it has
 * none.
 *
 * Rust: sofel_unicode::mirroring
 * HarfBuzz: hb_unicode_mirroring
 * FreeType: none
 */
uint32_t sofel_unicode_mirroring(uint32_t codepoint);

/*
 * One step of the canonical decomposition of a code point: a and b, or
 * a and 0 for a decomposition into one character. Hangul syllables
 * decompose by arithmetic. SOFEL_ERROR_NOT_FOUND when the code point
 * does not decompose.
 *
 * Rust: sofel_unicode::decompose
 * HarfBuzz: hb_unicode_decompose
 * FreeType: none
 */
int32_t sofel_unicode_decompose(uint32_t codepoint, uint32_t *a, uint32_t *b);

/*
 * The primary composite of a and b. SOFEL_ERROR_NOT_FOUND when either is
 * 0, the pair is not a canonical decomposition, or the composite is a
 * composition exclusion or has a non-zero combining class.
 *
 * Rust: sofel_unicode::compose
 * HarfBuzz: hb_unicode_compose
 * FreeType: none
 */
int32_t sofel_unicode_compose(uint32_t a, uint32_t b, uint32_t *ab);

/*
 * 1 if the code point is Extended_Pictographic, else 0.
 *
 * Rust: sofel_unicode::is_extended_pictographic
 * HarfBuzz: none (internal)
 * FreeType: none
 */
uint8_t sofel_unicode_is_extended_pictographic(uint32_t codepoint);

/*
 * Vertical_Orientation of a code point, SOFEL_VO_*; SOFEL_VO_R beyond
 * U+10FFFF.
 *
 * Rust: sofel_unicode::vertical_orientation
 * HarfBuzz: none
 * FreeType: none
 */
uint8_t sofel_unicode_vertical_orientation(uint32_t codepoint);

#ifdef __cplusplus
}
#endif

#endif /* SOFEL_H */
