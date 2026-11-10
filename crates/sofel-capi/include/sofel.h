/*
 * sofel.h: the C ABI of sofel (ADR 0015).
 *
 * sofel's own ABI, not HarfBuzz's or FreeType's. Until 1.0.0 the ABI may
 * change between versions; pin the version you build against.
 *
 * Conventions
 *
 * - Every name starts with `sofel_` (functions, types) or `SOFEL_`
 *   (macros). Object types are opaque structs behind pointers. Value types
 *   are fixed-width integers and explicitly declared value structs.
 * - A function that can fail returns an `int32_t`: SOFEL_OK (0) on
 *   success, a negative SOFEL_ERROR_* otherwise. Results go to output
 *   arguments, which are written only on success unless said otherwise.
 * - A NULL pointer argument is an error (SOFEL_ERROR_NULL), except the
 *   `user` pointers, which are passed through untouched, and the
 *   `*_destroy` functions, which do nothing for NULL. The location
 *   constructors also accept a NULL input array when its count is 0.
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
/* The font has no CFF or CFF2 table FreeType can open (ADR 0021). */
#define SOFEL_ERROR_INVALID_CFF (-13)
/* The buffer holds glyphs where characters are needed, or characters
 * where glyphs are needed (ADR 0020). */
#define SOFEL_ERROR_BUFFER_CONTENT (-14)
/* Subsetting (ADR 0022): the font has no glyphs. */
#define SOFEL_ERROR_NO_GLYPHS (-15)
/* Subsetting: an input HarfBuzz refuses (a glyph mapping that is not
 * one-to-one, maps a glyph other than 0 to 0, or comes with
 * SOFEL_SUBSET_FLAGS_RETAIN_GIDS; an axis the font does not have). */
#define SOFEL_ERROR_INVALID_INPUT (-16)
/* Subsetting: a table cannot be read. */
#define SOFEL_ERROR_MALFORMED_TABLE (-17)
/* Subsetting: a table, flag or axis setting this version does not subset
 * yet. */
#define SOFEL_ERROR_UNSUPPORTED (-18)
/* Writing: an offset does not fit, even after repacking. */
#define SOFEL_ERROR_OFFSET_OVERFLOW (-19)
/* Subsetting: a limit of HarfBuzz's was reached. */
#define SOFEL_ERROR_LIMIT (-20)
/* Subsetting: a table cannot be written, where HarfBuzz fails too. */
#define SOFEL_ERROR_SUBSET_FAILED (-21)

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
 * Rust: sofel_font::Font::new, sofel_outline::HbExtents::new, sofel_outline::Outlines::new, sofel_font::Cff::new
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

/* ---------------------------------------------------------- variations */

/* A location keeps coordinates and variation scalars (ADR 0027). */
typedef struct sofel_location sofel_location_t;

/* A design-space axis setting, like hb_variation_t. */
typedef struct {
  uint32_t tag;
  float value;
} sofel_variation_t;

/*
 * Constructs a location by selecting a zero-based fvar named instance,
 * then applying the variations in order. UINT32_MAX means no named
 * instance. An invalid fvar index leaves HarfBuzz at the default design
 * coordinates and is reported by sofel_location_ft_error for FreeType.
 * `variations` may be NULL only when `count` is 0. Keep the source font
 * bytes alive and unchanged until sofel_location_destroy; the font handle
 * itself may be destroyed first.
 *
 * Rust: sofel_font::var::Location::new
 * HarfBuzz: hb_font_set_var_named_instance, hb_font_set_variations
 * FreeType: FT_Set_Named_Instance, FT_Set_Var_Design_Coordinates
 */
int32_t sofel_location_create(const sofel_font_t *font, uint32_t named_instance,
                              const sofel_variation_t *variations, size_t count,
                              sofel_location_t **out);

/*
 * Constructs a location from design coordinates. Keep source font bytes
 * alive and unchanged until the location is destroyed.
 *
 * Rust: sofel_font::var::Location::from_design
 * HarfBuzz: hb_font_set_var_coords_design
 * FreeType: FT_Set_Var_Design_Coordinates
 */
int32_t sofel_location_create_design(const sofel_font_t *font, const float *coords,
                                     size_t count, sofel_location_t **out);

/*
 * Constructs a location from HarfBuzz 2.14 normalized coordinates.
 * Keep source font bytes alive and unchanged until destroy.
 *
 * Rust: sofel_font::var::Location::from_normalized_2_14
 * HarfBuzz: hb_font_set_var_coords_normalized
 * FreeType: FT_Set_Var_Blend_Coordinates after multiplying by 4
 */
int32_t sofel_location_create_normalized_2_14(const sofel_font_t *font,
                                               const int32_t *coords, size_t count,
                                               sofel_location_t **out);

/*
 * Constructs a location from FreeType 16.16 normalized coordinates.
 * Keep source font bytes alive and unchanged until destroy.
 *
 * Rust: sofel_font::var::Location::from_normalized_16_16
 * HarfBuzz: hb_ft_font_changed
 * FreeType: FT_Set_Var_Blend_Coordinates
 */
int32_t sofel_location_create_normalized_16_16(const sofel_font_t *font,
                                                const int32_t *coords, size_t count,
                                                sofel_location_t **out);

/*
 * Constructs a location from one zero-based fvar named instance.
 * Keep source font bytes alive and unchanged until destroy.
 *
 * Rust: sofel_font::var::Location::named_instance
 * HarfBuzz: hb_font_set_var_named_instance
 * FreeType: FT_Set_Named_Instance
 */
int32_t sofel_location_create_named_instance(const sofel_font_t *font, uint32_t index,
                                              sofel_location_t **out);

/*
 * Frees a location. The source font bytes may be freed afterwards.
 *
 * Rust: none
 * HarfBuzz: hb_font_destroy
 * FreeType: FT_Done_Face
 */
void sofel_location_destroy(sofel_location_t *location);

/*
 * Copies the HarfBuzz 2.14 normalized coordinates into the caller's array.
 * When capacity is too small, writes the required count to *len and returns
 * SOFEL_ERROR_BUFFER_TOO_SMALL without writing the array.
 *
 * Rust: sofel_font::var::Location::coords_normalized
 * HarfBuzz: hb_font_get_var_coords_normalized
 * FreeType: none
 */
int32_t sofel_location_coords_normalized(const sofel_location_t *location,
                                          int32_t *out, size_t capacity, size_t *len);

/*
 * Copies the HarfBuzz design coordinates into the caller's array.
 * Normalized-coordinate constructors return NaN design coordinates.
 *
 * Rust: sofel_font::var::Location::coords_design
 * HarfBuzz: hb_font_get_var_coords_design
 * FreeType: none
 */
int32_t sofel_location_coords_design(const sofel_location_t *location,
                                     float *out, size_t capacity, size_t *len);

/*
 * Copies the FreeType 16.16 normalized coordinates into the caller's array.
 *
 * Rust: sofel_font::var::Location::ft_coords
 * HarfBuzz: none
 * FreeType: FT_Get_Var_Blend_Coordinates
 */
int32_t sofel_location_ft_coords(const sofel_location_t *location,
                                 int32_t *out, size_t capacity, size_t *len);

/*
 * Copies the FreeType 16.16 design coordinates into the caller's array.
 *
 * Rust: sofel_font::var::Location::ft_design_coords
 * HarfBuzz: none
 * FreeType: FT_Get_Var_Design_Coordinates
 */
int32_t sofel_location_ft_design_coords(const sofel_location_t *location,
                                        int32_t *out, size_t capacity, size_t *len);

/*
 * Returns the FreeType coordinate-setting result: SOFEL_OK or
 * SOFEL_ERROR_INVALID_ARGUMENT. This function itself returns SOFEL_OK.
 *
 * Rust: sofel_font::var::Location::ft_error
 * HarfBuzz: none
 * FreeType: FT_Set_Var_Blend_Coordinates, FT_Set_Named_Instance
 */
int32_t sofel_location_ft_error(const sofel_location_t *location, int32_t *error);

/*
 * Returns the zero-based selected fvar index, or UINT32_MAX when none.
 *
 * Rust: sofel_font::var::Location::named_instance_index
 * HarfBuzz: none
 * FreeType: FT_Face->face_index, upper 16 bits
 */
int32_t sofel_location_named_instance_index(const sofel_location_t *location,
                                             uint32_t *index);

/*
 * Returns 1 when HarfBuzz has any nonzero normalized coordinate.
 *
 * Rust: sofel_font::var::Location::hb_nonzero
 * HarfBuzz: hb_font_t::has_nonzero_coords
 * FreeType: none
 */
int32_t sofel_location_hb_nonzero(const sofel_location_t *location, uint8_t *nonzero);

/*
 * Returns 1 when FreeType treats the location as the default instance.
 *
 * Rust: sofel_font::var::Location::ft_default
 * HarfBuzz: none
 * FreeType: IS_DEFAULT_INSTANCE
 */
int32_t sofel_location_ft_default(const sofel_location_t *location, uint8_t *is_default);

/*
 * Parses one hb-variation or CSS variation string. Invalid UTF-8 and
 * invalid variation syntax return SOFEL_ERROR_INVALID_ARGUMENT.
 *
 * Rust: sofel_font::var::Variation::parse
 * HarfBuzz: hb_variation_from_string
 * FreeType: none
 */
int32_t sofel_variation_parse(const char *input, size_t len, sofel_variation_t *out);

/*
 * Formats one variation as tag=value. The output is NUL-terminated.
 * A short buffer receives no bytes, *len gets the required length, and
 * the function returns SOFEL_ERROR_BUFFER_TOO_SMALL.
 *
 * Rust: sofel_font::var::Variation::format
 * HarfBuzz: hb_variation_to_string
 * FreeType: none
 */
int32_t sofel_variation_format(const sofel_variation_t *variation, char *out,
                               size_t capacity, size_t *len);

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
 * The glyphs of sofel_font_nominal_glyph and sofel_font_variation_glyph
 * as shaping looks them up (ADR 0020): every glyph id the cmap gives,
 * ids not below the glyph count included, and nothing from subtables of
 * formats 2 and 8, which HarfBuzz does not look up.
 *
 * Rust: sofel_font::Font::nominal_glyph_raw, sofel_font::Font::variation_glyph_raw
 * HarfBuzz: hb_font_get_nominal_glyph, hb_font_get_variation_glyph (with the ot font functions)
 * FreeType: none
 */
int32_t sofel_font_nominal_glyph_raw(const sofel_font_t *font, uint32_t codepoint,
                                     uint32_t *glyph);
int32_t sofel_font_variation_glyph_raw(const sofel_font_t *font, uint32_t codepoint,
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
 * sofel_scale_t gives them at other scales. Glyph boxes come from sbix,
 * CBDT and COLR (ADR 0023), then from glyf, CFF2 and CFF (ADR 0021).
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
 * Rust: sofel_font::Font::v_origin, sofel_font::HbScale::v_origin_scaled
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
 * Rust: sofel_color::glyph_extents, sofel_outline::HbExtents::glyph_extents, sofel_outline::Outlines::glyph_extents, sofel_font::Font::glyf_extents, sofel_bitmap::sbix_extents, sofel_bitmap::cbdt_extents
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
 * Rust: sofel_font::Font::metrics_position_with_fallback, sofel_font::HbScale::metrics_position_with_fallback_scaled
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
 * Rust: sofel_color::scaled_glyph_extents, sofel_outline::HbExtents::scaled_glyph_extents, sofel_font::HbScale::glyf_extents
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

/* ---------------------------------------------------------- baselines */

/* Text directions for the baseline queries (ADR 0017). */
#define SOFEL_DIRECTION_HORIZONTAL 0
#define SOFEL_DIRECTION_VERTICAL 1

/* The baseline tags of hb_ot_layout_baseline_tag_t. */
#define SOFEL_BASELINE_ROMAN SOFEL_TAG('r', 'o', 'm', 'n')
#define SOFEL_BASELINE_HANGING SOFEL_TAG('h', 'a', 'n', 'g')
#define SOFEL_BASELINE_IDEO_FACE_BOTTOM_OR_LEFT SOFEL_TAG('i', 'c', 'f', 'b')
#define SOFEL_BASELINE_IDEO_FACE_TOP_OR_RIGHT SOFEL_TAG('i', 'c', 'f', 't')
#define SOFEL_BASELINE_IDEO_FACE_CENTRAL SOFEL_TAG('I', 'c', 'f', 'c')
#define SOFEL_BASELINE_IDEO_EMBOX_BOTTOM_OR_LEFT SOFEL_TAG('i', 'd', 'e', 'o')
#define SOFEL_BASELINE_IDEO_EMBOX_TOP_OR_RIGHT SOFEL_TAG('i', 'd', 't', 'p')
#define SOFEL_BASELINE_IDEO_EMBOX_CENTRAL SOFEL_TAG('I', 'd', 'c', 'e')
#define SOFEL_BASELINE_MATH SOFEL_TAG('m', 'a', 't', 'h')

/*
 * A baseline (a SOFEL_BASELINE_* tag) from BASE, for a direction
 * (SOFEL_DIRECTION_*) and an OpenType script tag (the BaseScript of
 * `script`, else of DFLT). SOFEL_ERROR_NOT_FOUND when the font does not
 * have it; SOFEL_ERROR_INVALID_ARGUMENT for an unknown direction.
 *
 * Rust: sofel_font::Font::baseline
 * HarfBuzz: hb_ot_layout_get_baseline
 * FreeType: none
 */
int32_t sofel_font_baseline(const sofel_font_t *font, uint32_t tag, int32_t direction,
                            uint32_t script, int32_t *coord);

/*
 * Like sofel_font_baseline, synthesized when the font does not have it.
 * The em box center for Han in horizontal text, for example, is
 * sofel_font_baseline_with_fallback (font, SOFEL_BASELINE_IDEO_EMBOX_CENTRAL,
 * SOFEL_DIRECTION_HORIZONTAL, SOFEL_TAG('h', 'a', 'n', 'i'), &coord).
 *
 * Rust: sofel_font::Font::baseline_with_fallback, sofel_font::HbScale::baseline_with_fallback_scaled
 * HarfBuzz: hb_ot_layout_get_baseline_with_fallback
 * FreeType: none
 */
int32_t sofel_font_baseline_with_fallback(const sofel_font_t *font, uint32_t tag,
                                          int32_t direction, uint32_t script, int32_t *coord);

/*
 * The ascender, descender and line gap (0) of a script and language
 * system tag from BASE's MinMax tables in extents[0..2], with *found 1;
 * without them, the font's extents for the direction with *found 0.
 *
 * Rust: sofel_font::Font::layout_font_extents
 * HarfBuzz: hb_ot_layout_get_font_extents
 * FreeType: none
 */
int32_t sofel_font_layout_font_extents(const sofel_font_t *font, int32_t direction,
                                       uint32_t script, uint32_t language, int32_t extents[3],
                                       int32_t *found);

/*
 * Rust: sofel_font::HbScale::baseline
 * HarfBuzz: hb_ot_layout_get_baseline
 * FreeType: none
 */
int32_t sofel_scale_baseline(const sofel_scale_t *scale, const sofel_font_t *font,
                             uint32_t tag, int32_t direction, uint32_t script, int32_t *coord);

/*
 * Rust: sofel_font::HbScale::baseline_with_fallback
 * HarfBuzz: hb_ot_layout_get_baseline_with_fallback
 * FreeType: none
 */
int32_t sofel_scale_baseline_with_fallback(const sofel_scale_t *scale, const sofel_font_t *font,
                                           uint32_t tag, int32_t direction, uint32_t script,
                                           int32_t *coord);

/*
 * Rust: sofel_font::HbScale::layout_font_extents
 * HarfBuzz: hb_ot_layout_get_font_extents
 * FreeType: none
 */
int32_t sofel_scale_layout_font_extents(const sofel_scale_t *scale, const sofel_font_t *font,
                                        int32_t direction, uint32_t script, uint32_t language,
                                        int32_t extents[3], int32_t *found);

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
 * Rust: sofel_font::Font::glyph_name, sofel_font::Font::glyph_name_with, sofel_font::GlyphNames::glyph_name
 * HarfBuzz: hb_font_get_glyph_name
 * FreeType: none
 */
int32_t sofel_font_glyph_name(const sofel_font_t *font, uint32_t glyph, const uint8_t **name,
                              size_t *len);

/*
 * The name of a glyph as FreeType gives it for sfnt fonts.
 *
 * Rust: sofel_font::Font::ft_glyph_name, sofel_font::Cff::ft_glyph_name_with, sofel_font::GlyphNames::ft_glyph_name
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
 * CFF and CFF2 (ADR 0021). The font keeps the CFF opened when it is
 * created; a CFF that FreeType cannot open does not fail the font, but
 * the functions below then return SOFEL_ERROR_INVALID_CFF.
 */

/*
 * 1 when the font has a `CFF ` table of non-zero length (cut at the end
 * of the file), else 0 (also for NULL).
 *
 * Rust: sofel_font::Font::is_cff
 * HarfBuzz: hb_face_reference_table
 * FreeType: none
 */
int32_t sofel_font_is_cff(const sofel_font_t *font);

/*
 * 1 when the font has a `CFF2` table of non-zero length, else 0.
 *
 * Rust: sofel_font::Font::is_cff2
 * HarfBuzz: hb_face_reference_table
 * FreeType: none
 */
int32_t sofel_font_is_cff2(const sofel_font_t *font);

/*
 * 1 in *out when the CFF font is CID-keyed (has ROS), else 0.
 *
 * Rust: sofel_font::Cff::new, sofel_font::Cff::is_cid_keyed
 * HarfBuzz: none
 * FreeType: FT_Get_CID_Is_Internally_CID_Keyed
 */
int32_t sofel_font_is_cid_keyed(const sofel_font_t *font, int32_t *out);

/*
 * The CID of a glyph (the charset value). SOFEL_ERROR_INVALID_ARGUMENT
 * when the font is not CID-keyed or the glyph id is not below the
 * CharStrings count; *cid is 0 then, as in FreeType.
 * The font looks CIDs up in an index of the charset it builds at
 * creation (ADR 0025).
 *
 * Rust: sofel_font::Cff::cid, sofel_font::Cff::cid_with
 * HarfBuzz: none
 * FreeType: FT_Get_CID_From_Glyph_Index
 */
int32_t sofel_font_cid(const sofel_font_t *font, uint32_t glyph, uint32_t *cid);

/*
 * The registry and ordering, written as NUL-terminated strings cut at
 * their first 0 byte, and the supplement. A string FreeType gives as
 * NULL is written empty, with its bit (1 registry, 2 ordering) clear in
 * *present. SOFEL_ERROR_INVALID_ARGUMENT when the font is not CID-keyed;
 * SOFEL_ERROR_BUFFER_TOO_SMALL when a string does not fit.
 *
 * Rust: sofel_font::Cff::ros
 * HarfBuzz: none
 * FreeType: FT_Get_CID_Registry_Ordering_Supplement
 */
int32_t sofel_font_ros(const sofel_font_t *font, char *registry, size_t registry_capacity,
                       char *ordering, size_t ordering_capacity, int32_t *supplement,
                       uint32_t *present);

/*
 * The PostScript font information: values = italic angle (16.16),
 * isFixedPitch, underline position and thickness; the strings version,
 * notice, full name, family name and weight, each NUL-terminated and
 * cut at its first 0 byte, one after the other in `strings`, with their
 * starts in offsets[0..4] (SIZE_MAX for a NULL string). *len is the
 * number of bytes the strings need; SOFEL_ERROR_BUFFER_TOO_SMALL when
 * `capacity` is less. SOFEL_ERROR_INVALID_ARGUMENT for CFF2.
 *
 * Rust: sofel_font::Cff::ps_font_info
 * HarfBuzz: none
 * FreeType: FT_Get_PS_Font_Info
 */
int32_t sofel_font_ps_font_info(const sofel_font_t *font, int64_t values[4], char *strings,
                                size_t capacity, size_t offsets[5], size_t *len);

/*
 * The fs_type of the CFF PostScript string (`/FSType <n> def`), 0 when
 * there is none.
 *
 * Rust: sofel_font::Cff::ps_fs_type
 * HarfBuzz: none
 * FreeType: FT_Get_PS_Font_Extra (fs_type)
 */
int32_t sofel_font_ps_fs_type(const sofel_font_t *font, uint16_t *fs_type);

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
 * Rust: sofel_outline::load, sofel_outline::Outlines::load
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
 * Rust: sofel_outline::draw, sofel_outline::Outlines::draw
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

/* ------------------------------------------------------- color glyphs */

/*
 * Color glyphs (ADR 0023): CPAL palettes, the paint commands of COLR,
 * SVG, CBDT and sbix glyphs as HarfBuzz paints them, in font units
 * without HarfBuzz's root scale, the bitmap strikes as FreeType lists
 * them, and the helpers for mapping the commands to renderers.
 */

/* A color: sRGB, not premultiplied. */
typedef struct sofel_color_t {
  uint8_t r, g, b, a;
} sofel_color_t;

/* A color stop: palette_index is -1 for the foreground; alpha is the
 * font's alpha, clamped to 0..1, that color was resolved with. */
typedef struct sofel_color_stop_t {
  float offset;
  sofel_color_t color;
  int32_t palette_index;
  float alpha;
} sofel_color_stop_t;

/* The kinds of sofel_paint_command_t. */
#define SOFEL_PAINT_PUSH_TRANSFORM 0      /* values: xx yx xy yy dx dy */
#define SOFEL_PAINT_POP_TRANSFORM 1
#define SOFEL_PAINT_PUSH_CLIP_GLYPH 2     /* glyph */
#define SOFEL_PAINT_PUSH_CLIP_RECTANGLE 3 /* values: x_min y_min x_max y_max */
#define SOFEL_PAINT_POP_CLIP 4
#define SOFEL_PAINT_FILL_SOLID 5          /* color, palette_index, alpha */
#define SOFEL_PAINT_FILL_LINEAR 6         /* values: p0 p1 p2; mode: extend; stops */
#define SOFEL_PAINT_FILL_RADIAL 7         /* values: c0 r0 c1 r1; mode: extend; stops */
#define SOFEL_PAINT_FILL_SWEEP 8          /* values: center start end; mode: extend; stops */
#define SOFEL_PAINT_FILL_GLYPH 9          /* glyph, color, palette_index, alpha */
#define SOFEL_PAINT_IMAGE 10              /* mode: 0 PNG, 1 SVG, 2 gzip SVG;
                                             data, len, width, height,
                                             extents when has_extents */
#define SOFEL_PAINT_PUSH_GROUP 11         /* mode: SOFEL_COMPOSITE_* (0..27, HarfBuzz's numbers) */
#define SOFEL_PAINT_POP_GROUP 12          /* mode */
#define SOFEL_PAINT_COLOR_GLYPH 13        /* glyph (HarfBuzz's color_glyph; recorded as failed) */

/* The extend modes of gradients. */
#define SOFEL_EXTEND_PAD 0
#define SOFEL_EXTEND_REPEAT 1
#define SOFEL_EXTEND_REFLECT 2

/* What sofel_paint_glyph painted from (its non-negative results). */
#define SOFEL_PAINTED_NONE 0
#define SOFEL_PAINTED_COLRV1 1
#define SOFEL_PAINTED_COLRV0 2
#define SOFEL_PAINTED_SVG 3
#define SOFEL_PAINTED_CBDT 4
#define SOFEL_PAINTED_SBIX 5

/* One paint command; the fields a kind does not use are 0 (palette_index
 * -1). Image data points into the font's data; stops into the list. */
typedef struct sofel_paint_command_t {
  uint32_t kind;
  uint32_t glyph;
  uint32_t mode;
  int32_t palette_index;
  sofel_color_t color;
  float alpha;
  float values[9];
  int32_t has_extents;
  int32_t extents[4];
  uint32_t width;
  uint32_t height;
  const sofel_color_stop_t *stops;
  size_t n_stops;
  const uint8_t *data;
  size_t len;
} sofel_paint_command_t;

/* The commands of a glyph; free with sofel_paint_commands_free. */
typedef struct sofel_paint_commands_t {
  const sofel_paint_command_t *commands;
  size_t count;
  void *internal;
} sofel_paint_commands_t;

/*
 * Paints a glyph into an array of commands: from COLR, else SVG, else
 * CBDT, else sbix, as HarfBuzz's hb_ot_paint_glyph_or_fail. With
 * custom_colors (n_custom of them), they replace the palette (entries
 * past them are transparent black); otherwise palette `palette` of the
 * font (palette 0 past the palettes). ppem picks bitmap strikes, 0 for
 * the largest. Returns SOFEL_PAINTED_* (SOFEL_PAINTED_NONE with no
 * commands for a glyph that is not a color glyph) or an error.
 *
 * Rust: sofel_color::paint, sofel_color::paint_recording, sofel_color::Recording::new, sofel_color::Recording::commands, sofel_color::Recording::stops, sofel_color::Recording::brush, sofel_color::Recording::clear, sofel_color::Recording::replay
 * HarfBuzz: hb_font_paint_glyph_or_fail
 * FreeType: none
 */
int32_t sofel_paint_glyph(const sofel_font_t *font, uint32_t glyph, uint16_t palette,
                          const sofel_color_t *custom_colors, size_t n_custom,
                          sofel_color_t foreground, uint16_t ppem,
                          sofel_paint_commands_t *out);

/*
 * Frees the commands of sofel_paint_glyph and clears *commands; NULL
 * and cleared commands are ignored.
 *
 * Rust: none
 */
void sofel_paint_commands_free(sofel_paint_commands_t *commands);

/*
 * The number of CPAL palettes, and of colors in each.
 *
 * Rust: sofel_color::palette_count, sofel_color::palette_entry_count
 * HarfBuzz: hb_ot_color_palette_get_count
 * FreeType: FT_Palette_Data_Get
 */
int32_t sofel_palette_count(const sofel_font_t *font, uint16_t *count);
int32_t sofel_palette_entry_count(const sofel_font_t *font, uint16_t *count);

/*
 * A palette's name id (0xFFFF for none), its flags (1: light
 * backgrounds, 2: dark), and a palette entry's name id.
 *
 * Rust: sofel_color::palette_name_id, sofel_color::palette_flags, sofel_color::palette_color_name_id
 * HarfBuzz: hb_ot_color_palette_get_name_id, hb_ot_color_palette_get_flags, hb_ot_color_palette_color_get_name_id
 * FreeType: FT_Palette_Data_Get
 */
int32_t sofel_palette_name_id(const sofel_font_t *font, uint16_t palette, uint16_t *name_id);
int32_t sofel_palette_flags(const sofel_font_t *font, uint16_t palette, uint32_t *flags);
int32_t sofel_palette_color_name_id(const sofel_font_t *font, uint16_t entry,
                                    uint16_t *name_id);

/*
 * The colors of a palette, snprintf-style: *count is set to the number
 * of colors; SOFEL_ERROR_BUFFER_TOO_SMALL when capacity is smaller.
 *
 * Rust: sofel_color::palette_colors
 * HarfBuzz: hb_ot_color_palette_get_colors
 * FreeType: FT_Palette_Select
 */
int32_t sofel_palette_colors(const sofel_font_t *font, uint16_t palette, sofel_color_t *colors,
                             size_t capacity, size_t *count);

/*
 * The first palette whose flags include all of `flags` (CSS font-palette
 * light: 1, dark: 2). SOFEL_ERROR_NOT_FOUND when none.
 *
 * Rust: sofel_color::find_palette
 * HarfBuzz: none
 * FreeType: none
 */
int32_t sofel_find_palette(const sofel_font_t *font, uint32_t flags, uint16_t *palette);

/* The bits of sofel_color_formats. */
#define SOFEL_COLOR_COLRV0 1
#define SOFEL_COLOR_COLRV1 2
#define SOFEL_COLOR_CPAL 4
#define SOFEL_COLOR_SVG 8
#define SOFEL_COLOR_CBDT 16
#define SOFEL_COLOR_SBIX 32
#define SOFEL_COLOR_EBDT 64

/*
 * The color and bitmap formats of a font, SOFEL_COLOR_* bits.
 *
 * Rust: sofel_color::color_formats, sofel_color::ColorFormats::has, sofel_bitmap::has_hb_cbdt, sofel_bitmap::has_hb_sbix
 * HarfBuzz: hb_ot_color_has_layers, hb_ot_color_has_paint, hb_ot_color_has_palettes, hb_ot_color_has_svg, hb_ot_color_has_png
 * FreeType: FT_HAS_COLOR, FT_HAS_FIXED_SIZES
 */
int32_t sofel_color_formats(const sofel_font_t *font, uint32_t *formats);

/*
 * What sofel_paint_glyph would paint a glyph from, without walking the
 * paint: SOFEL_PAINTED_COLRV1 ... SOFEL_PAINTED_SBIX.
 * SOFEL_ERROR_NOT_FOUND for a glyph that is not a color glyph.
 *
 * Rust: sofel_color::glyph_color_format
 * HarfBuzz: hb_ot_color_glyph_has_paint
 * FreeType: none
 */
int32_t sofel_glyph_color_format(const sofel_font_t *font, uint32_t glyph, uint32_t *format);

/*
 * The COLR version 0 layers of a glyph, snprintf-style: each layer's
 * glyph and palette index (-1 for the foreground).
 *
 * Rust: sofel_color::layers
 * HarfBuzz: hb_ot_color_glyph_get_layers
 * FreeType: FT_Get_Color_Glyph_Layer
 */
int32_t sofel_color_layers(const sofel_font_t *font, uint32_t glyph, uint32_t *glyphs,
                           int32_t *indices, size_t capacity, size_t *count);

/*
 * The SVG document of a glyph without copying (gzip-compressed when it
 * starts with 0x1F 0x8B), its index and the glyphs it covers.
 * SOFEL_ERROR_NOT_FOUND when none.
 *
 * Rust: sofel_color::svg_document
 * HarfBuzz: hb_ot_color_glyph_reference_svg, hb_ot_color_glyph_get_svg_document_index, hb_ot_color_get_svg_document_glyph_range
 * FreeType: none (svg-hooks)
 */
int32_t sofel_svg_document(const sofel_font_t *font, uint32_t glyph, const uint8_t **data,
                           size_t *len, uint32_t *index, uint32_t range[2]);

/* A strike: table is 0 CBLC, 1 EBLC, 2 bloc, 3 sbix. */
typedef struct sofel_strike_t {
  uint32_t index;
  uint32_t table;
  uint16_t x_ppem;
  uint16_t y_ppem;
  uint8_t bit_depth;
  uint16_t resolution;
  int16_t height;
  int16_t width;
} sofel_strike_t;

/*
 * The strikes as FreeType's available_sizes, snprintf-style.
 *
 * Rust: sofel_bitmap::strikes
 * HarfBuzz: none
 * FreeType: FT_Face available_sizes
 */
int32_t sofel_bitmap_strikes(const sofel_font_t *font, sofel_strike_t *strikes, size_t capacity,
                             size_t *count);

/*
 * The strike HarfBuzz picks for ppem (0: the largest), and the strike
 * FreeType's FT_Match_Size picks for ppems in 26.6 (0: the other one).
 * SOFEL_ERROR_NOT_FOUND when none.
 *
 * Rust: sofel_bitmap::choose_strike, sofel_bitmap::match_strike
 * HarfBuzz: none (internal)
 * FreeType: FT_Match_Size
 */
int32_t sofel_choose_strike(const sofel_font_t *font, uint16_t ppem, sofel_strike_t *strike);
int32_t sofel_match_strike(const sofel_font_t *font, uint32_t x_ppem, uint32_t y_ppem,
                           sofel_strike_t *strike);

/* A glyph's undecoded bitmap: format is the EBDT/CBDT image format (0
 * for sbix), tag the sbix graphic type (0 for the others). */
typedef struct sofel_glyph_bitmap_t {
  uint32_t format;
  uint32_t tag;
  const uint8_t *data;
  size_t len;
  uint16_t width;
  uint16_t height;
  int16_t hori_bearing_x;
  int16_t hori_bearing_y;
  uint16_t hori_advance;
  int16_t vert_bearing_x;
  int16_t vert_bearing_y;
  uint16_t vert_advance;
  uint8_t bit_depth;
  uint8_t flipped;
} sofel_glyph_bitmap_t;

/*
 * The image data and metrics of a glyph in the strike with index
 * strike_index (sofel_strike_t.index), as FreeType reads them.
 * SOFEL_ERROR_NOT_FOUND when the glyph has none there.
 *
 * Rust: sofel_bitmap::glyph_bitmap
 * HarfBuzz: none
 * FreeType: FT_Load_Glyph (FT_LOAD_BITMAP_METRICS_ONLY)
 */
int32_t sofel_glyph_bitmap(const sofel_font_t *font, uint32_t strike_index, uint32_t glyph,
                           sofel_glyph_bitmap_t *bitmap);

/* A decoded bitmap: pixel_mode is FreeType's (1 mono, 2 gray, 3 gray2,
 * 4 gray4, 7 BGRA); rows top first, pitch bytes each. */
typedef struct sofel_bitmap_layout_t {
  uint16_t width;
  uint16_t rows;
  uint32_t pitch;
  uint32_t pixel_mode;
  int16_t hori_bearing_x;
  int16_t hori_bearing_y;
  uint16_t hori_advance;
  int16_t vert_bearing_x;
  int16_t vert_bearing_y;
  uint16_t vert_advance;
} sofel_bitmap_layout_t;

/*
 * Decodes a glyph's EBDT/CBDT/bdat bitmap in the strike with index
 * strike_index as FreeType's FT_Load_Glyph (FT_LOAD_COLOR): the layout
 * goes to *layout, and the pixels (pitch × rows bytes) to `pixels` unless
 * it is NULL. SOFEL_ERROR_NOT_FOUND without a bitmap,
 * SOFEL_ERROR_NO_GLYPH_DATA for PNG images (not decoded),
 * SOFEL_ERROR_BUFFER_TOO_SMALL when capacity is smaller.
 *
 * Rust: sofel_bitmap::bitmap_layout, sofel_bitmap::draw_bitmap, sofel_bitmap::BitmapLayout::len, sofel_bitmap::BitmapLayout::is_empty
 * HarfBuzz: none
 * FreeType: FT_Load_Glyph
 */
int32_t sofel_draw_bitmap(const sofel_font_t *font, uint32_t strike_index, uint32_t glyph,
                          uint8_t *pixels, size_t capacity, sofel_bitmap_layout_t *layout);

/*
 * Whether the font has outlines as FreeType decides (glyf, GLYF, CFF or
 * CFF2, and no CBLC or CBDT), and whether it has embedded bitmaps (CBLC,
 * EBLC, bloc or sbix): 1 or 0.
 *
 * Rust: sofel_font::Font::has_outlines, sofel_font::Font::has_bitmaps
 * HarfBuzz: none
 * FreeType: FT_IS_SCALABLE, FT_HAS_FIXED_SIZES
 */
int32_t sofel_font_has_outlines(const sofel_font_t *font, uint8_t *has);
int32_t sofel_font_has_bitmaps(const sofel_font_t *font, uint8_t *has);

/*
 * Renders a glyph in color as FT_Load_Glyph with FT_LOAD_COLOR |
 * FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP and FT_Render_Glyph with
 * FT_RENDER_MODE_NORMAL: the COLR version 0 layers (as
 * FT_Get_Color_Glyph_Layer gives them), each rendered as by
 * sofel_render_glyph and blended as by sofel_blend_layer in the color of
 * its palette entry (`foreground` for 0xFFFF). The palette is
 * custom_colors (n_custom of them) or else the font's palette `palette`
 * (palette 0 past the palettes). *is_color is 1 and the pixels are
 * premultiplied BGRA (4 bytes a pixel, 4 × width a row) for a color
 * image; *is_color is 0 and the pixels are 8-bit coverage when the glyph
 * has no layers or a layer cannot be rendered (the glyph's own outline,
 * as FreeType falls back to). *len and box are written as by
 * sofel_rasterizer_render.
 *
 * Errors: those of sofel_render_glyph.
 *
 * Rust: sofel_cli::glyph::render_color, sofel_cli::glyph::color_layers
 * HarfBuzz: none
 * FreeType: FT_Load_Glyph (FT_LOAD_COLOR), FT_Render_Glyph, FT_Get_Color_Glyph_Layer
 */
int32_t sofel_render_color_glyph(const sofel_font_t *font, uint32_t glyph, uint16_t ppem,
                                 uint16_t palette, const sofel_color_t *custom_colors,
                                 size_t n_custom, sofel_color_t foreground,
                                 sofel_outline_scratch_t *scratch,
                                 sofel_rasterizer_t *rasterizer, uint8_t *pixels,
                                 size_t capacity, size_t *len, int32_t *box,
                                 uint32_t *is_color);

/*
 * Blends an 8-bit coverage bitmap (src_*, rows from the top, src_width
 * bytes a row) in `color` over a premultiplied BGRA image, as FreeType's
 * tt_face_colr_blend_layer: box is the image's left, top, width and rows
 * and *len its bytes in `pixels` (0: no image yet, the bitmap's box is
 * taken); the box grows to cover the bitmap. On success box and *len
 * describe the new image. SOFEL_ERROR_BUFFER_TOO_SMALL (with the needed
 * *len, the image unchanged) when it does not fit in capacity.
 *
 * Rust: sofel_raster::blend_layer, sofel_raster::BgraImage::clear
 * HarfBuzz: none
 * FreeType: tt_face_colr_blend_layer
 */
int32_t sofel_blend_layer(uint8_t *pixels, size_t capacity, size_t *len, int32_t *box,
                          int32_t src_left, int32_t src_top, uint32_t src_width,
                          uint32_t src_rows, const uint8_t *src_pixels, sofel_color_t color);

/*
 * The sine, cosine and tangent of x (radians), the same on every
 * target (musl's sincosf and tanf).
 *
 * Rust: sofel_geom::sin_cos, sofel_geom::sin, sofel_geom::cos, sofel_geom::tan
 * HarfBuzz: none
 * FreeType: none
 */
int32_t sofel_sin_cos(float x, float *s, float *c);
float sofel_tan(float x);

/*
 * Sorts the stops by offset (stably) and rescales the offsets to 0..1;
 * the original smallest and largest offsets go to *min and *max.
 *
 * Rust: sofel_geom::normalize_color_line
 * HarfBuzz: hb_paint_normalize_color_line
 * FreeType: none
 */
int32_t sofel_normalize_color_line(sofel_color_stop_t *stops, size_t n, float *min, float *max);

/*
 * The two points (x0 y0 x1 y1) of the axis of COLR's three-point linear
 * gradient (x0 y0 x1 y1 x2 y2).
 *
 * Rust: sofel_geom::reduce_linear_anchors
 * HarfBuzz: hb_paint_reduce_linear_anchors
 * FreeType: none
 */
int32_t sofel_reduce_linear_anchors(const float points[6], float out[4]);

typedef void (*sofel_sweep_tile_func_t)(void *user, float a0, sofel_color_t c0, float a1,
                                        sofel_color_t c1);

/*
 * The segments within 0..2π of a sweep gradient of sorted, normalized
 * stops, from start to end (radians), passed to emit; at most 4096.
 *
 * Rust: sofel_geom::sweep_gradient_tiles
 * HarfBuzz: hb_paint_sweep_gradient_tiles
 * FreeType: none
 */
int32_t sofel_sweep_gradient_tiles(sofel_color_stop_t *stops, size_t n, uint32_t extend,
                                   float start, float end, sofel_sweep_tile_func_t emit,
                                   void *user);

/* ------------------------------------------------------------ shaping */

/*
 * Text shaping as HarfBuzz's OpenType shaper does it (ADR 0020). The
 * calls go: sofel_buffer_create, sofel_buffer_add_utf8 (or another
 * sofel_buffer_add_*), sofel_segment_guess, sofel_shape_plan_create
 * (once per font, segment and features; keep it), sofel_shape, and
 * sofel_buffer_glyphs. sofel_buffer_clear makes the buffer ready for the
 * next text, keeping its memory and settings.
 *
 * A buffer holds either characters (after sofel_buffer_create,
 * sofel_buffer_clear and the sofel_buffer_add_* functions) or glyphs
 * (after sofel_shape and sofel_buffer_deserialize_glyphs). A function
 * given the other kind returns SOFEL_ERROR_BUFFER_CONTENT.
 *
 * Plans are immutable and may be shared between threads; a buffer is
 * used by one thread at a time.
 */

/* Text directions: the values of hb_direction_t. 0 is "not known". */
#define SOFEL_DIRECTION_LTR 4
#define SOFEL_DIRECTION_RTL 5
#define SOFEL_DIRECTION_TTB 6
#define SOFEL_DIRECTION_BTT 7

/*
 * Tests of a text direction, as HarfBuzz's HB_DIRECTION_* macros.
 *
 * Rust: sofel_shape::Direction::is_horizontal, sofel_shape::Direction::is_vertical, sofel_shape::Direction::is_forward, sofel_shape::Direction::is_backward, sofel_shape::Direction::reverse
 * HarfBuzz: HB_DIRECTION_IS_HORIZONTAL, HB_DIRECTION_IS_VERTICAL, HB_DIRECTION_IS_FORWARD, HB_DIRECTION_IS_BACKWARD, HB_DIRECTION_REVERSE
 * FreeType: none
 */
#define SOFEL_DIRECTION_IS_VALID(d) ((((uint32_t)(d)) & ~3u) == 4)
#define SOFEL_DIRECTION_IS_HORIZONTAL(d) ((((uint32_t)(d)) & ~1u) == 4)
#define SOFEL_DIRECTION_IS_VERTICAL(d) ((((uint32_t)(d)) & ~1u) == 6)
#define SOFEL_DIRECTION_IS_FORWARD(d) ((((uint32_t)(d)) & ~2u) == 4)
#define SOFEL_DIRECTION_IS_BACKWARD(d) ((((uint32_t)(d)) & ~2u) == 5)
#define SOFEL_DIRECTION_REVERSE(d) ((int32_t)(((uint32_t)(d)) ^ 1u))

/*
 * The direction a string names by its first letter, `l`, `r`, `t` or
 * `b` in either case; 0 for anything else.
 *
 * Rust: sofel_shape::Direction::from_bytes
 * HarfBuzz: hb_direction_from_string
 * FreeType: none
 */
int32_t sofel_direction_from_string(const char *str, size_t len);

/*
 * "ltr", "rtl", "ttb" or "btt", as a static NUL-terminated string; NULL
 * for other values.
 *
 * Rust: sofel_shape::Direction::as_str
 * HarfBuzz: hb_direction_to_string
 * FreeType: none
 */
const char *sofel_direction_to_string(int32_t direction);

/*
 * The script of a string, in the form of sofel_unicode_script: its
 * first four bytes (up to a NUL byte), padded with spaces, read as
 * sofel_script_from_iso15924 reads a tag. SOFEL_ERROR_INVALID_ARGUMENT
 * when they are not four letters.
 *
 * Rust: sofel_unicode::Script::from_iso15924
 * HarfBuzz: hb_script_from_string
 * FreeType: none
 */
int32_t sofel_script_from_string(const char *str, size_t len, uint32_t *script);

/*
 * The direction a script is written in horizontally: SOFEL_DIRECTION_RTL
 * for the right-to-left scripts, 0 for the scripts written both ways
 * (Old Hungarian, Old Italic, Runic, Tifinagh), SOFEL_DIRECTION_LTR for
 * the others, unknown ones and 0 included.
 *
 * Rust: sofel_shape::script_horizontal_direction
 * HarfBuzz: hb_script_get_horizontal_direction
 * FreeType: none
 */
int32_t sofel_script_horizontal_direction(uint32_t script);

/* A BCP 47 language tag, normalized. Immutable. */
typedef struct sofel_language_t sofel_language_t;

/*
 * The language of `len` bytes: cut to 63 bytes, ASCII letters
 * lowercased, `_` turned into `-`, and cut at the first byte that is
 * not a letter, digit, `-` or `_`. NULL on failure, with the reason in
 * *error: SOFEL_ERROR_INVALID_ARGUMENT for empty text or text that
 * starts with a NUL byte. Destroy it with sofel_language_destroy.
 *
 * Rust: sofel_shape::Language::new
 * HarfBuzz: hb_language_from_string
 * FreeType: none
 */
sofel_language_t *sofel_language_create(const char *str, size_t len, int32_t *error);

/*
 * Frees a language. NULL does nothing.
 *
 * Rust: none
 * HarfBuzz: none
 * FreeType: none
 */
void sofel_language_destroy(sofel_language_t *language);

/*
 * The normalized tag, not NUL-terminated, valid while the language
 * lives.
 *
 * Rust: sofel_shape::Language::as_str
 * HarfBuzz: hb_language_to_string
 * FreeType: none
 */
int32_t sofel_language_string(const sofel_language_t *language, const char **str, size_t *len);

/* The OpenType tags HarfBuzz's shaper asks for. */
#define SOFEL_OT_MAX_TAGS_PER_SCRIPT 3
#define SOFEL_OT_MAX_TAGS_PER_LANGUAGE 3
#define SOFEL_OT_TAG_DEFAULT_SCRIPT SOFEL_TAG('D', 'F', 'L', 'T')
#define SOFEL_OT_TAG_DEFAULT_LANGUAGE SOFEL_TAG('d', 'f', 'l', 't')

/*
 * The OpenType script and language system tags of a script (0 for none)
 * and a language (NULL for none), best first. *script_count and
 * *language_count hold the capacities of the arrays on entry and the
 * numbers written on return; an array may be NULL when its capacity is
 * 0. The shaper asks for SOFEL_OT_MAX_TAGS_PER_SCRIPT and
 * SOFEL_OT_MAX_TAGS_PER_LANGUAGE. SOFEL_ERROR_INVALID_ARGUMENT for a
 * script that is not four letters.
 *
 * Rust: sofel_shape::ot_tags_from_script_and_language, sofel_shape::ot_tags_from_script_and_language_max
 * HarfBuzz: hb_ot_tags_from_script_and_language
 * FreeType: none
 */
int32_t sofel_ot_tags_from_script_and_language(uint32_t script, const sofel_language_t *language,
                                               uint32_t *script_tags, size_t *script_count,
                                               uint32_t *language_tags, size_t *language_count);

/*
 * The script and language of an OpenType script tag and language
 * system tag (0 for `dflt`). *script is 0 for none; *language is a new
 * language to destroy with sofel_language_destroy, or NULL for none.
 *
 * Rust: sofel_shape::ot_tags_to_script_and_language
 * HarfBuzz: hb_ot_tags_to_script_and_language
 * FreeType: none
 */
int32_t sofel_ot_tags_to_script_and_language(uint32_t script_tag, uint32_t language_tag,
                                             uint32_t *script, sofel_language_t **language);

/*
 * The script of an OpenType script tag; 0 for `DFLT`, 'Zzzz' for a tag
 * of no script.
 *
 * Rust: sofel_shape::ot_tag_to_script
 * HarfBuzz: hb_ot_tag_to_script
 * FreeType: none
 */
uint32_t sofel_ot_tag_to_script(uint32_t tag);

/*
 * The language of an OpenType language system tag: a new language to
 * destroy with sofel_language_destroy, or NULL for `dflt`.
 *
 * Rust: sofel_shape::ot_tag_to_language
 * HarfBuzz: hb_ot_tag_to_language
 * FreeType: none
 */
int32_t sofel_ot_tag_to_language(uint32_t tag, sofel_language_t **language);

/* A feature setting for the clusters start..end (hb_feature_t). */
typedef struct sofel_feature_t {
  uint32_t tag;
  uint32_t value; /* 0 disables, 1 enables, higher values pick alternates */
  uint32_t start;
  uint32_t end;
} sofel_feature_t;

/* The range of a feature that applies everywhere. */
#define SOFEL_FEATURE_GLOBAL_START 0u
#define SOFEL_FEATURE_GLOBAL_END UINT32_MAX

/*
 * A feature from its string: an optional `+` or `-`, the tag, an
 * optional range of clusters (`[3:5]`), and an optional value (`=2`,
 * ` on`), as HarfBuzz reads it. SOFEL_ERROR_INVALID_ARGUMENT when it
 * does not parse.
 *
 * Rust: sofel_shape::Feature::parse
 * HarfBuzz: hb_feature_from_string
 * FreeType: none
 */
int32_t sofel_feature_parse(const char *str, size_t len, sofel_feature_t *feature);

/*
 * The string of a feature, as HarfBuzz writes it, NUL-terminated into
 * `buffer` of `size` bytes; *len is its length without the NUL. When
 * `size` is not larger than *len, nothing is written and the result is
 * SOFEL_ERROR_BUFFER_TOO_SMALL.
 *
 * Rust: sofel_shape::Feature::format, sofel_shape::Feature::write
 * HarfBuzz: hb_feature_to_string
 * FreeType: none
 */
int32_t sofel_feature_format(const sofel_feature_t *feature, char *buffer, size_t size,
                             size_t *len);

/*
 * The properties of a run of text: the direction (SOFEL_DIRECTION_*),
 * the script (in the form of sofel_unicode_script) and the language;
 * 0, 0 and NULL when not known.
 */
typedef struct sofel_segment_t {
  int32_t direction;
  uint32_t script;
  const sofel_language_t *language;
} sofel_segment_t;

/*
 * Fills in what `in` does not know from `len` bytes of UTF-8 text:
 * the script from the first character whose script is not Common,
 * Inherited or Unknown; the direction from the script (left to right
 * when the script has none); the language from default_language (NULL
 * for none). The process locale is not read. out->language is
 * in->language or default_language, which must outlive its use.
 *
 * Rust: sofel_shape::SegmentProperties::guess
 * HarfBuzz: hb_buffer_guess_segment_properties
 * FreeType: none
 */
int32_t sofel_segment_guess(const char *utf8, size_t len, const sofel_segment_t *in,
                            const sofel_language_t *default_language, sofel_segment_t *out);

/* The shapers a plan picks (hb_ot_shaper_categorize). */
#define SOFEL_SHAPER_DEFAULT 0
#define SOFEL_SHAPER_ARABIC 1
#define SOFEL_SHAPER_HANGUL 2
#define SOFEL_SHAPER_HEBREW 3
#define SOFEL_SHAPER_INDIC 4
#define SOFEL_SHAPER_KHMER 5
#define SOFEL_SHAPER_MYANMAR 6
#define SOFEL_SHAPER_MYANMAR_ZAWGYI 7
#define SOFEL_SHAPER_THAI 8
#define SOFEL_SHAPER_USE 9

/* What to do for a font, segment and features. Immutable; the font must
 * outlive it. */
typedef struct sofel_shape_plan_t sofel_shape_plan_t;

/*
 * A plan for `font`, the segment (whose direction must be known) and
 * `n` features (`features` may be NULL when `n` is 0). NULL on failure,
 * with the reason in *error. Destroy it with sofel_shape_plan_destroy.
 *
 * Rust: sofel_shape::ShapePlan::new
 * HarfBuzz: hb_shape_plan_create_cached
 * FreeType: none
 */
sofel_shape_plan_t *sofel_shape_plan_create(const sofel_font_t *font,
                                            const sofel_segment_t *segment,
                                            const sofel_feature_t *features, size_t n,
                                            int32_t *error);

/*
 * Frees a plan. NULL does nothing.
 *
 * Rust: none
 * HarfBuzz: hb_shape_plan_destroy
 * FreeType: none
 */
void sofel_shape_plan_destroy(sofel_shape_plan_t *plan);

/*
 * The shaper the plan uses, SOFEL_SHAPER_*.
 *
 * Rust: sofel_shape::ShapePlan::shaper
 * HarfBuzz: hb_shape_plan_get_shaper (always "ot"; the script shaper is internal)
 * FreeType: none
 */
int32_t sofel_shape_plan_shaper(const sofel_shape_plan_t *plan, int32_t *shaper);

/* Buffer flags (hb_buffer_flags_t). */
#define SOFEL_BUFFER_FLAG_DEFAULT 0x0u
#define SOFEL_BUFFER_FLAG_BOT 0x1u
#define SOFEL_BUFFER_FLAG_EOT 0x2u
#define SOFEL_BUFFER_FLAG_PRESERVE_DEFAULT_IGNORABLES 0x4u
#define SOFEL_BUFFER_FLAG_REMOVE_DEFAULT_IGNORABLES 0x8u
#define SOFEL_BUFFER_FLAG_DO_NOT_INSERT_DOTTED_CIRCLE 0x10u
#define SOFEL_BUFFER_FLAG_PRODUCE_UNSAFE_TO_CONCAT 0x40u
#define SOFEL_BUFFER_FLAG_PRODUCE_SAFE_TO_INSERT_TATWEEL 0x80u

/* Cluster levels (hb_buffer_cluster_level_t). */
#define SOFEL_CLUSTER_LEVEL_MONOTONE_GRAPHEMES 0
#define SOFEL_CLUSTER_LEVEL_MONOTONE_CHARACTERS 1
#define SOFEL_CLUSTER_LEVEL_CHARACTERS 2
#define SOFEL_CLUSTER_LEVEL_GRAPHEMES 3

/* Glyph flags (hb_glyph_flags_t). */
#define SOFEL_GLYPH_FLAG_UNSAFE_TO_BREAK 0x1u
#define SOFEL_GLYPH_FLAG_UNSAFE_TO_CONCAT 0x2u
#define SOFEL_GLYPH_FLAG_SAFE_TO_INSERT_TATWEEL 0x4u

/* Text to shape, then its glyphs. Owned by one thread at a time. */
typedef struct sofel_buffer_t sofel_buffer_t;

/*
 * An empty buffer of characters with the default settings; NULL only
 * when memory runs out. Destroy it with sofel_buffer_destroy.
 *
 * Rust: sofel_shape::UnicodeBuffer::new
 * HarfBuzz: hb_buffer_create
 * FreeType: none
 */
sofel_buffer_t *sofel_buffer_create(void);

/*
 * An empty buffer of characters with the settings of `buffer`, which
 * holds characters; NULL on failure, with the reason in *error.
 *
 * Rust: sofel_shape::UnicodeBuffer::similar
 * HarfBuzz: hb_buffer_create_similar
 * FreeType: none
 */
sofel_buffer_t *sofel_buffer_create_similar(const sofel_buffer_t *buffer, int32_t *error);

/*
 * Frees a buffer. NULL does nothing.
 *
 * Rust: none
 * HarfBuzz: hb_buffer_destroy
 * FreeType: none
 */
void sofel_buffer_destroy(sofel_buffer_t *buffer);

/*
 * Empties the buffer and makes it hold characters, keeping the settings
 * and the memory. The random state goes back to 1.
 *
 * Rust: sofel_shape::UnicodeBuffer::clear, sofel_shape::GlyphBuffer::into_unicode
 * HarfBuzz: hb_buffer_clear_contents
 * FreeType: none
 */
int32_t sofel_buffer_clear(sofel_buffer_t *buffer);

/*
 * The number of characters or glyphs.
 *
 * Rust: sofel_shape::UnicodeBuffer::len, sofel_shape::GlyphBuffer::len
 * HarfBuzz: hb_buffer_get_length
 * FreeType: none
 */
int32_t sofel_buffer_length(const sofel_buffer_t *buffer, size_t *len);

/*
 * Adds one character with a cluster value.
 *
 * Rust: sofel_shape::UnicodeBuffer::push
 * HarfBuzz: hb_buffer_add
 * FreeType: none
 */
int32_t sofel_buffer_add(sofel_buffer_t *buffer, uint32_t codepoint, uint32_t cluster);

/*
 * Adds the characters of text[item_offset..item_offset + item_len] (the
 * text has `len` code units; both are clamped to it, so SIZE_MAX takes
 * the rest), with their positions in the text as cluster values. Up to
 * five characters before the item become the pre-context when the
 * buffer is empty, and up to five after it the post-context. Ill-formed
 * text becomes the replacement character: each byte of UTF-8, lone
 * surrogates of UTF-16, and surrogates and values past U+10FFFF of
 * UTF-32. The _codepoints form adds the values as they are, and the
 * _latin1 form one character per byte.
 *
 * Rust: sofel_shape::UnicodeBuffer::push_utf8, sofel_shape::UnicodeBuffer::push_utf16, sofel_shape::UnicodeBuffer::push_utf32, sofel_shape::UnicodeBuffer::push_codepoints, sofel_shape::UnicodeBuffer::push_latin1
 * HarfBuzz: hb_buffer_add_utf8, hb_buffer_add_utf16, hb_buffer_add_utf32, hb_buffer_add_codepoints, hb_buffer_add_latin1
 * FreeType: none
 */
int32_t sofel_buffer_add_utf8(sofel_buffer_t *buffer, const char *text, size_t len,
                              size_t item_offset, size_t item_len);
int32_t sofel_buffer_add_utf16(sofel_buffer_t *buffer, const uint16_t *text, size_t len,
                               size_t item_offset, size_t item_len);
int32_t sofel_buffer_add_utf32(sofel_buffer_t *buffer, const uint32_t *text, size_t len,
                               size_t item_offset, size_t item_len);
int32_t sofel_buffer_add_codepoints(sofel_buffer_t *buffer, const uint32_t *text, size_t len,
                                    size_t item_offset, size_t item_len);
int32_t sofel_buffer_add_latin1(sofel_buffer_t *buffer, const uint8_t *text, size_t len,
                                size_t item_offset, size_t item_len);

/*
 * Adds the characters start..end of `source` (clamped to it) with their
 * cluster values; their context comes from `source` as for
 * sofel_buffer_add_utf8.
 *
 * Rust: sofel_shape::UnicodeBuffer::append
 * HarfBuzz: hb_buffer_append
 * FreeType: none
 */
int32_t sofel_buffer_append(sofel_buffer_t *buffer, const sofel_buffer_t *source, size_t start,
                            size_t end);

/*
 * Sets the cluster value of every character to its index.
 *
 * Rust: sofel_shape::UnicodeBuffer::number_clusters
 * HarfBuzz: none (hb-shape without --utf8-clusters)
 * FreeType: none
 */
int32_t sofel_buffer_number_clusters(sofel_buffer_t *buffer);

/*
 * The characters and their cluster values, written to the arrays of
 * `capacity` elements (either may be NULL); *len is the number of
 * characters. SOFEL_ERROR_BUFFER_TOO_SMALL when `capacity` is smaller.
 *
 * Rust: sofel_shape::UnicodeBuffer::codepoints, sofel_shape::UnicodeBuffer::clusters
 * HarfBuzz: hb_buffer_get_glyph_infos
 * FreeType: none
 */
int32_t sofel_buffer_codepoints(const sofel_buffer_t *buffer, uint32_t *codepoints,
                                uint32_t *clusters, size_t capacity, size_t *len);

/*
 * The pre-context (side 0) or post-context (side 1), nearest character
 * first: up to five characters, their number in *len.
 *
 * Rust: sofel_shape::UnicodeBuffer::pre_context, sofel_shape::UnicodeBuffer::post_context
 * HarfBuzz: none (hb_buffer_t::context)
 * FreeType: none
 */
int32_t sofel_buffer_context(const sofel_buffer_t *buffer, int32_t side, uint32_t context[5],
                             size_t *len);

/*
 * The settings of a buffer of characters, and their setters: the flags
 * (SOFEL_BUFFER_FLAG_*), the cluster level (SOFEL_CLUSTER_LEVEL_*), the
 * replacement character for ill-formed text (U+FFFD), the glyph for
 * default-ignorable characters (0, the font's space glyph), the glyph
 * for characters the font does not have (0), the glyph for variation
 * selectors the font has no variant for (UINT32_MAX for none: they are
 * then default-ignorable), and the state of the random numbers of the
 * `rand` feature (1).
 *
 * Rust: sofel_shape::UnicodeBuffer::flags, sofel_shape::UnicodeBuffer::set_flags, sofel_shape::UnicodeBuffer::cluster_level, sofel_shape::UnicodeBuffer::set_cluster_level, sofel_shape::UnicodeBuffer::replacement_codepoint, sofel_shape::UnicodeBuffer::set_replacement_codepoint, sofel_shape::UnicodeBuffer::invisible_glyph, sofel_shape::UnicodeBuffer::set_invisible_glyph, sofel_shape::UnicodeBuffer::not_found_glyph, sofel_shape::UnicodeBuffer::set_not_found_glyph, sofel_shape::UnicodeBuffer::not_found_variation_selector_glyph, sofel_shape::UnicodeBuffer::set_not_found_variation_selector_glyph, sofel_shape::UnicodeBuffer::random_state, sofel_shape::UnicodeBuffer::set_random_state
 * HarfBuzz: hb_buffer_get_flags, hb_buffer_set_flags, and the others of the same names
 * FreeType: none
 */
int32_t sofel_buffer_get_flags(const sofel_buffer_t *buffer, uint32_t *flags);
int32_t sofel_buffer_set_flags(sofel_buffer_t *buffer, uint32_t flags);
int32_t sofel_buffer_get_cluster_level(const sofel_buffer_t *buffer, int32_t *level);
int32_t sofel_buffer_set_cluster_level(sofel_buffer_t *buffer, int32_t level);
int32_t sofel_buffer_get_replacement_codepoint(const sofel_buffer_t *buffer, uint32_t *codepoint);
int32_t sofel_buffer_set_replacement_codepoint(sofel_buffer_t *buffer, uint32_t codepoint);
int32_t sofel_buffer_get_invisible_glyph(const sofel_buffer_t *buffer, uint32_t *glyph);
int32_t sofel_buffer_set_invisible_glyph(sofel_buffer_t *buffer, uint32_t glyph);
int32_t sofel_buffer_get_not_found_glyph(const sofel_buffer_t *buffer, uint32_t *glyph);
int32_t sofel_buffer_set_not_found_glyph(sofel_buffer_t *buffer, uint32_t glyph);
int32_t sofel_buffer_get_not_found_variation_selector_glyph(const sofel_buffer_t *buffer,
                                                            uint32_t *glyph);
int32_t sofel_buffer_set_not_found_variation_selector_glyph(sofel_buffer_t *buffer,
                                                            uint32_t glyph);
int32_t sofel_buffer_get_random_state(const sofel_buffer_t *buffer, uint32_t *state);
int32_t sofel_buffer_set_random_state(sofel_buffer_t *buffer, uint32_t state);

/*
 * Shapes the characters of `buffer` with `font` (the font of the plan)
 * at `scale` (NULL for the font's unscaled default, positions in font
 * units; no pixel size, so Device tables are not applied) as `plan`
 * says; the buffer then holds the glyphs, in visual order. The plan's
 * direction and script are used. Glyph boxes for fallback mark
 * positioning and vertical origins come from the best source the font
 * has, as HarfBuzz's ot functions take them (sbix, CBDT, COLR, glyf,
 * CFF, CFF2).
 *
 * Rust: sofel_shape::shape, sofel_shape::shape_with_extents, sofel_shape::Scale::unscaled
 * HarfBuzz: hb_shape_plan_execute
 * FreeType: none
 */
int32_t sofel_shape(const sofel_font_t *font, const sofel_scale_t *scale,
                    const sofel_shape_plan_t *plan, sofel_buffer_t *buffer);

/*
 * Shapes as HarfBuzz's `fallback` shaper: the cmap glyph and advance of
 * every character, without layout tables. The segment's direction must
 * be known.
 *
 * Rust: sofel_shape::shape_fallback
 * HarfBuzz: hb_shape_full with the shaper list {"fallback"}
 * FreeType: none
 */
int32_t sofel_shape_fallback(const sofel_font_t *font, const sofel_scale_t *scale,
                             const sofel_segment_t *segment, sofel_buffer_t *buffer);

/*
 * The glyphs of a shaped buffer: glyph ids, cluster values, glyph flags
 * (SOFEL_GLYPH_FLAG_*) and four positions per glyph (x_advance,
 * y_advance, x_offset, y_offset), written to the arrays of `capacity`
 * glyphs, any of which may be NULL. Returns the number of glyphs; when
 * it is larger than `capacity`, nothing is written and the caller calls
 * again with arrays that large. A negative SOFEL_ERROR_* on failure.
 *
 * Rust: sofel_shape::GlyphBuffer::glyph_infos, sofel_shape::GlyphBuffer::glyph_positions
 * HarfBuzz: hb_buffer_get_glyph_infos, hb_buffer_get_glyph_positions
 * FreeType: none
 */
int64_t sofel_buffer_glyphs(const sofel_buffer_t *buffer, uint32_t *glyphs, uint32_t *clusters,
                            uint32_t *flags, int32_t *positions, size_t capacity);

/*
 * The direction the glyphs were shaped in.
 *
 * Rust: sofel_shape::GlyphBuffer::direction
 * HarfBuzz: hb_buffer_get_direction
 * FreeType: none
 */
int32_t sofel_buffer_direction(const sofel_buffer_t *buffer, int32_t *direction);

/*
 * Reorders the glyphs of each cluster into a canonical order and moves
 * the cluster's advance onto one glyph.
 *
 * Rust: sofel_shape::GlyphBuffer::normalize_glyphs
 * HarfBuzz: hb_buffer_normalize_glyphs
 * FreeType: none
 */
int32_t sofel_buffer_normalize_glyphs(sofel_buffer_t *buffer);

/* How two buffers differ (hb_buffer_diff_flags_t). */
#define SOFEL_DIFF_FLAG_EQUAL 0x0u
#define SOFEL_DIFF_FLAG_CONTENT_TYPE_MISMATCH 0x1u
#define SOFEL_DIFF_FLAG_LENGTH_MISMATCH 0x2u
#define SOFEL_DIFF_FLAG_NOTDEF_PRESENT 0x4u
#define SOFEL_DIFF_FLAG_DOTTED_CIRCLE_PRESENT 0x8u
#define SOFEL_DIFF_FLAG_CODEPOINT_MISMATCH 0x10u
#define SOFEL_DIFF_FLAG_CLUSTER_MISMATCH 0x20u
#define SOFEL_DIFF_FLAG_GLYPH_FLAGS_MISMATCH 0x40u
#define SOFEL_DIFF_FLAG_POSITION_MISMATCH 0x80u

/*
 * Compares `buffer` with `reference` as hb_buffer_diff does: the kinds
 * of content, the lengths, which special glyphs the reference has
 * (dottedcircle_glyph is the glyph of U+25CC; UINT32_MAX for none), and
 * item by item the glyphs or characters, cluster values, flags and
 * positions (differences up to position_fuzz allowed).
 *
 * Rust: sofel_shape::diff, sofel_shape::diff_unicode
 * HarfBuzz: hb_buffer_diff
 * FreeType: none
 */
int32_t sofel_buffer_diff(const sofel_buffer_t *buffer, const sofel_buffer_t *reference,
                          uint32_t dottedcircle_glyph, uint32_t position_fuzz, uint32_t *flags);

/* Serialization formats (hb_buffer_serialize_format_t). */
#define SOFEL_SERIALIZE_FORMAT_TEXT SOFEL_TAG('T', 'E', 'X', 'T')
#define SOFEL_SERIALIZE_FORMAT_JSON SOFEL_TAG('J', 'S', 'O', 'N')

/* What to write (hb_buffer_serialize_flags_t). */
#define SOFEL_SERIALIZE_FLAG_DEFAULT 0x0u
#define SOFEL_SERIALIZE_FLAG_NO_CLUSTERS 0x1u
#define SOFEL_SERIALIZE_FLAG_NO_POSITIONS 0x2u
#define SOFEL_SERIALIZE_FLAG_NO_GLYPH_NAMES 0x4u
#define SOFEL_SERIALIZE_FLAG_GLYPH_EXTENTS 0x8u
#define SOFEL_SERIALIZE_FLAG_GLYPH_FLAGS 0x10u
#define SOFEL_SERIALIZE_FLAG_NO_ADVANCES 0x20u

/*
 * The format a name gives (its first four bytes in any case: "text",
 * "json"); 0 for others.
 *
 * Rust: sofel_shape::SerializeFormat::from_name
 * HarfBuzz: hb_buffer_serialize_format_from_string
 * FreeType: none
 */
uint32_t sofel_serialize_format_from_string(const char *str, size_t len);

/*
 * "text" or "json", as a static NUL-terminated string; NULL for other
 * values.
 *
 * Rust: sofel_shape::SerializeFormat::name
 * HarfBuzz: hb_buffer_serialize_format_to_string
 * FreeType: none
 */
const char *sofel_serialize_format_to_string(uint32_t format);

/*
 * Writes the items start..end of `buffer` as hb_buffer_serialize does:
 * glyphs as `[name=cluster@dx,dy+ax,ay|...]` (or JSON), characters as
 * `<U+0041=0|...>`, and `!!` (or `[]`) for an empty buffer of
 * characters. Items are written NUL-terminated into `out` of `size`
 * bytes while they fit; *consumed is the number of bytes written
 * without the NUL and *count the number of items, so that the caller
 * continues from start + *count. Glyph names come from `font` (NULL for
 * `gid<n>`), glyph boxes from `font` at `scale` (NULL for unscaled).
 * SOFEL_ERROR_INVALID_ARGUMENT for an unknown format.
 *
 * Rust: sofel_shape::serialize_glyphs, sofel_shape::serialize_glyphs_range, sofel_shape::serialize_unicode, sofel_shape::serialize_unicode_range, sofel_shape::serialize_invalid
 * HarfBuzz: hb_buffer_serialize
 * FreeType: none
 */
int32_t sofel_buffer_serialize(const sofel_buffer_t *buffer, size_t start, size_t end, char *out,
                               size_t size, size_t *consumed, size_t *count,
                               const sofel_font_t *font, const sofel_scale_t *scale,
                               uint32_t format, uint32_t flags);

/*
 * Reads glyphs written by sofel_buffer_serialize and appends them to
 * `buffer`, which holds glyphs or is an empty buffer of characters (it
 * then holds glyphs in `direction`, with the default settings). Glyph
 * names are looked up in `names` (NULL for none) and `uni<hex>` in the
 * cmap of `font` (NULL for none); numbers and `gid<n>` are glyph ids.
 * *consumed is the number of bytes read, also on failure; the items
 * read before a failure are kept. SOFEL_ERROR_INVALID_ARGUMENT when the
 * text does not read to its end, is empty, or the format or direction
 * is unknown.
 *
 * Rust: sofel_shape::deserialize_glyphs, sofel_shape::GlyphBuffer::from_parts
 * HarfBuzz: hb_buffer_deserialize_glyphs
 * FreeType: none
 */
int32_t sofel_buffer_deserialize_glyphs(sofel_buffer_t *buffer, const char *text, size_t len,
                                        size_t *consumed, const sofel_font_t *font,
                                        const sofel_glyph_names_t *names, int32_t direction,
                                        uint32_t format);

/*
 * Reads characters written by sofel_buffer_serialize and appends them
 * to a buffer of characters, as sofel_buffer_deserialize_glyphs.
 *
 * Rust: sofel_shape::deserialize_unicode
 * HarfBuzz: hb_buffer_deserialize_unicode
 * FreeType: none
 */
int32_t sofel_buffer_deserialize_unicode(sofel_buffer_t *buffer, const char *text, size_t len,
                                         size_t *consumed, uint32_t format);

/*
 * The string of a glyph as serialization writes it: its name (from
 * `font`, NULL for none) cut to 127 bytes, or `gid<n>`; NUL-terminated
 * into `buffer` of `size` bytes as sofel_feature_format.
 *
 * Rust: sofel_shape::glyph_to_string
 * HarfBuzz: hb_font_glyph_to_string
 * FreeType: none
 */
int32_t sofel_font_glyph_to_string(const sofel_font_t *font, uint32_t glyph, char *buffer,
                                   size_t size, size_t *len);

/*
 * The glyph of a string as deserialization reads it: a glyph name (from
 * `names`, NULL for none), a glyph id in decimal, `gid<n>`, or
 * `uni<hex>` mapped through the cmap of `font` (NULL for none).
 * SOFEL_ERROR_NOT_FOUND when it names no glyph.
 *
 * Rust: sofel_shape::glyph_from_string
 * HarfBuzz: hb_font_glyph_from_string
 * FreeType: none
 */
int32_t sofel_font_glyph_from_string(const sofel_font_t *font, const sofel_glyph_names_t *names,
                                     const char *str, size_t len, uint32_t *glyph);

/* The fallback width classes of space characters
 * (hb_unicode_funcs_t::space_t). */
#define SOFEL_SPACE_NOT_SPACE 0
#define SOFEL_SPACE_EM 1
#define SOFEL_SPACE_EM_2 2
#define SOFEL_SPACE_EM_3 3
#define SOFEL_SPACE_EM_4 4
#define SOFEL_SPACE_EM_5 5
#define SOFEL_SPACE_EM_6 6
#define SOFEL_SPACE_EM_16 16
#define SOFEL_SPACE_4_EM_18 17
#define SOFEL_SPACE_SPACE 18
#define SOFEL_SPACE_FIGURE 19
#define SOFEL_SPACE_PUNCTUATION 20
#define SOFEL_SPACE_NARROW 21

/*
 * HarfBuzz's own character properties for shaping: the combining class
 * marks are reordered by; whether a character is default-ignorable (as
 * HarfBuzz hides them), a variation selector matched with the character
 * before, or a regional indicator; the fallback width class of a space
 * (SOFEL_SPACE_*); and the vertical presentation form used without a
 * `vert` feature (the character itself when there is none).
 *
 * Rust: sofel_shape::unicode::modified_combining_class, sofel_shape::unicode::is_default_ignorable, sofel_shape::unicode::is_variation_selector, sofel_shape::unicode::is_regional_indicator, sofel_shape::unicode::space_fallback_type, sofel_shape::unicode::vertical_char_for
 * HarfBuzz: hb_unicode_funcs_t::modified_combining_class, is_default_ignorable, is_variation_selector, _hb_codepoint_is_regional_indicator, space_fallback_type, vertical_char_for (internal)
 * FreeType: none
 */
uint8_t sofel_unicode_modified_combining_class(uint32_t codepoint);
uint8_t sofel_unicode_is_default_ignorable(uint32_t codepoint);
uint8_t sofel_unicode_is_variation_selector(uint32_t codepoint);
uint8_t sofel_unicode_is_regional_indicator(uint32_t codepoint);
uint8_t sofel_unicode_space_fallback_type(uint32_t codepoint);
uint32_t sofel_unicode_vertical_char_for(uint32_t codepoint);

/* --------------------------------------------------------- subsetting */

/*
 * Subsetting as HarfBuzz 15.1.0's hb-subset does, byte for byte, and the
 * writing of tables from objects (ADR 0022). An input
 * (sofel_subset_input_t) says what to keep; a plan
 * (sofel_subset_plan_t) holds the glyph closure and mapping; executing
 * it writes the font. sofel_subset does both.
 *
 * Golem's subsetting (golem_font_subset) is:
 *   sofel_subset_input_create (&input);
 *   sofel_subset_input_set_add (input, SOFEL_SUBSET_SET_GLYPHS, gid) for
 *     each glyph;
 *   sofel_subset_input_set_flags (input, SOFEL_SUBSET_FLAGS_RETAIN_GIDS);
 *   sofel_subset (font, input, &bytes);
 *   sofel_subset_input_destroy (input); ... sofel_bytes_destroy (&bytes);
 */

/* Bytes that sofel allocated: free them with sofel_bytes_destroy. */
typedef struct sofel_bytes
{
  const uint8_t *data;
  size_t len;
} sofel_bytes_t;

/*
 * Frees bytes from sofel and sets them to {NULL, 0}; nothing for NULL or
 * for bytes already {NULL, 0}.
 *
 * Rust: none
 * HarfBuzz: hb_blob_destroy
 * FreeType: none
 */
void sofel_bytes_destroy(sofel_bytes_t *bytes);

/* A subsetting input; not to be shared between threads while changed. */
typedef struct sofel_subset_input sofel_subset_input_t;

/*
 * The subsetting flags, with HarfBuzz's values (hb_subset_flags_t).
 * IFTB_REQUIREMENTS and RETAIN_NUM_GLYPHS behave as in HarfBuzz built
 * with its experimental API.
 */
#define SOFEL_SUBSET_FLAGS_DEFAULT 0x0u
#define SOFEL_SUBSET_FLAGS_NO_HINTING 0x1u
#define SOFEL_SUBSET_FLAGS_RETAIN_GIDS 0x2u
#define SOFEL_SUBSET_FLAGS_DESUBROUTINIZE 0x4u
#define SOFEL_SUBSET_FLAGS_NAME_LEGACY 0x8u
#define SOFEL_SUBSET_FLAGS_SET_OVERLAPS_FLAG 0x10u
#define SOFEL_SUBSET_FLAGS_PASSTHROUGH_UNRECOGNIZED 0x20u
#define SOFEL_SUBSET_FLAGS_NOTDEF_OUTLINE 0x40u
#define SOFEL_SUBSET_FLAGS_GLYPH_NAMES 0x80u
#define SOFEL_SUBSET_FLAGS_NO_PRUNE_UNICODE_RANGES 0x100u
#define SOFEL_SUBSET_FLAGS_NO_LAYOUT_CLOSURE 0x200u
#define SOFEL_SUBSET_FLAGS_OPTIMIZE_IUP_DELTAS 0x400u
#define SOFEL_SUBSET_FLAGS_NO_BIDI_CLOSURE 0x800u
#define SOFEL_SUBSET_FLAGS_IFTB_REQUIREMENTS 0x1000u
#define SOFEL_SUBSET_FLAGS_RETAIN_NUM_GLYPHS 0x2000u
#define SOFEL_SUBSET_FLAGS_DOWNGRADE_CFF2 0x4000u
#define SOFEL_SUBSET_FLAGS_CFF_IDENTITY_CHARSET 0x8000u

/* The sets of an input, with HarfBuzz's numbers (hb_subset_sets_t). */
typedef uint32_t sofel_subset_set_t;
#define SOFEL_SUBSET_SET_GLYPHS 0
#define SOFEL_SUBSET_SET_UNICODES 1
#define SOFEL_SUBSET_SET_NO_SUBSET_TABLES 2
#define SOFEL_SUBSET_SET_DROP_TABLES 3
#define SOFEL_SUBSET_SET_NAME_IDS 4
#define SOFEL_SUBSET_SET_NAME_LANGUAGES 5
#define SOFEL_SUBSET_SET_LAYOUT_FEATURES 6
#define SOFEL_SUBSET_SET_LAYOUT_SCRIPTS 7
/* Never a member of a set (HB_SET_VALUE_INVALID). */
#define SOFEL_SUBSET_SET_INVALID 0xFFFFFFFFu

/*
 * A new input with HarfBuzz's defaults: no glyphs or characters, name ids
 * 0 to 6 in language 0x409, HarfBuzz's dropped, copied and layout feature
 * tables, every script and no flags.
 *
 * Rust: sofel_subset::Input::new
 * HarfBuzz: hb_subset_input_create_or_fail
 * FreeType: none
 */
int32_t sofel_subset_input_create(sofel_subset_input_t **input);

/*
 * Rust: none
 * HarfBuzz: hb_subset_input_destroy
 * FreeType: none
 */
void sofel_subset_input_destroy(sofel_subset_input_t *input);

/*
 * Keeps everything: every glyph, character, name, language, feature and
 * script; no dropped tables; and the flags NOTDEF_OUTLINE, GLYPH_NAMES,
 * NAME_LEGACY, NO_PRUNE_UNICODE_RANGES and PASSTHROUGH_UNRECOGNIZED.
 *
 * Rust: sofel_subset::Input::keep_everything
 * HarfBuzz: hb_subset_input_keep_everything
 * FreeType: none
 */
int32_t sofel_subset_input_keep_everything(sofel_subset_input_t *input);

/*
 * The sets of an input, with HarfBuzz's set semantics: a set may be
 * inverted ("every value but these"), values are below
 * SOFEL_SUBSET_SET_INVALID, and tags are SOFEL_TAG values.
 * SOFEL_ERROR_INVALID_ARGUMENT for a `set` other than SOFEL_SUBSET_SET_*.
 *
 * add, add_range (nothing when first > last), remove, clear (also undoes
 * an inversion) and invert change the set. contains returns 1 or 0.
 * count writes the number of members (of an inverted set, the values
 * below SOFEL_SUBSET_SET_INVALID not excluded). next writes the smallest
 * member greater than *value (SOFEL_SUBSET_SET_INVALID starts from the
 * beginning) and returns 1, or writes SOFEL_SUBSET_SET_INVALID and
 * returns 0 when there is none.
 *
 * Rust: sofel_subset::Input::set, sofel_subset::Input::map_set, sofel_subset::RangeSet::add, sofel_subset::RangeSet::add_range, sofel_subset::RangeSet::remove, sofel_subset::RangeSet::clear, sofel_subset::RangeSet::invert, sofel_subset::RangeSet::contains, sofel_subset::RangeSet::len, sofel_subset::RangeSet::next
 * HarfBuzz: hb_subset_input_set, hb_subset_input_glyph_set, hb_subset_input_unicode_set with hb_set_add, hb_set_add_range, hb_set_del, hb_set_clear, hb_set_invert, hb_set_has, hb_set_get_population, hb_set_next
 * FreeType: none
 */
int32_t sofel_subset_input_set_add(sofel_subset_input_t *input, sofel_subset_set_t set,
                                   uint32_t value);
int32_t sofel_subset_input_set_add_range(sofel_subset_input_t *input, sofel_subset_set_t set,
                                         uint32_t first, uint32_t last);
int32_t sofel_subset_input_set_remove(sofel_subset_input_t *input, sofel_subset_set_t set,
                                      uint32_t value);
int32_t sofel_subset_input_set_clear(sofel_subset_input_t *input, sofel_subset_set_t set);
int32_t sofel_subset_input_set_invert(sofel_subset_input_t *input, sofel_subset_set_t set);
int32_t sofel_subset_input_set_contains(const sofel_subset_input_t *input,
                                        sofel_subset_set_t set, uint32_t value);
int32_t sofel_subset_input_set_count(const sofel_subset_input_t *input, sofel_subset_set_t set,
                                     uint64_t *count);
int32_t sofel_subset_input_set_next(const sofel_subset_input_t *input, sofel_subset_set_t set,
                                    uint32_t *value);

/*
 * The flags (SOFEL_SUBSET_FLAGS_*). set_flags replaces them;
 * SOFEL_ERROR_INVALID_ARGUMENT for bits not listed above.
 *
 * Rust: sofel_subset::Input::flags, sofel_subset::Input::with_flags
 * HarfBuzz: hb_subset_input_get_flags, hb_subset_input_set_flags
 * FreeType: none
 */
int32_t sofel_subset_input_get_flags(const sofel_subset_input_t *input, uint32_t *flags);
int32_t sofel_subset_input_set_flags(sofel_subset_input_t *input, uint32_t flags);

/*
 * The requested glyph mapping: set_glyph_mapping maps old glyph `old` to
 * new glyph `new_glyph` (replacing an earlier mapping of `old`);
 * clear_glyph_mapping removes every pair. The glyphs mapped must also be
 * in the glyph set to be kept, as in HarfBuzz. Plans fail with
 * SOFEL_ERROR_INVALID_INPUT for a mapping HarfBuzz refuses.
 *
 * Rust: sofel_subset::Input::with_glyph_mapping
 * HarfBuzz: hb_subset_input_old_to_new_glyph_mapping with hb_map_set, hb_map_clear
 * FreeType: none
 */
int32_t sofel_subset_input_set_glyph_mapping(sofel_subset_input_t *input, uint32_t old,
                                             uint32_t new_glyph);
int32_t sofel_subset_input_clear_glyph_mapping(sofel_subset_input_t *input);

/*
 * The range of axis `tag` set in the input (the user coordinates min,
 * default and max). SOFEL_ERROR_NOT_FOUND when the input has none.
 *
 * Rust: sofel_subset::Input::axis_range
 * HarfBuzz: hb_subset_input_get_axis_range
 * FreeType: none
 */
int32_t sofel_subset_input_get_axis_range(const sofel_subset_input_t *input, uint32_t tag,
                                          float *min, float *def, float *max);

/*
 * Pins every axis of `font` (its fvar) to its default value.
 * SOFEL_ERROR_NOT_FOUND, with the input unchanged, when the font has no
 * axes. Subsetting runs only when every axis is pinned to its default or
 * no axis is set; other axis settings make plans fail with
 * SOFEL_ERROR_UNSUPPORTED.
 *
 * Rust: sofel_subset::Input::with_all_axes_pinned_to_default
 * HarfBuzz: hb_subset_input_pin_all_axes_to_default
 * FreeType: none
 */
int32_t sofel_subset_input_pin_all_axes_to_default(sofel_subset_input_t *input,
                                                   const sofel_font_t *font);

/*
 * Pins axis `tag` of `font` to its default value (pin_axis_to_default)
 * or to `value` clamped to the axis range (pin_axis_location).
 * SOFEL_ERROR_NOT_FOUND, with the input unchanged, when the font has no
 * such axis.
 *
 * Rust: sofel_subset::Input::with_axis_pinned_to_default, sofel_subset::Input::with_axis_pinned
 * HarfBuzz: hb_subset_input_pin_axis_to_default, hb_subset_input_pin_axis_location
 * FreeType: none
 */
int32_t sofel_subset_input_pin_axis_to_default(sofel_subset_input_t *input,
                                               const sofel_font_t *font, uint32_t tag);
int32_t sofel_subset_input_pin_axis_location(sofel_subset_input_t *input,
                                             const sofel_font_t *font, uint32_t tag, float value);

/*
 * Restricts axis `tag` of `font` to `min`..`max` with default `def`; NaN
 * takes the axis's own value. The minimum and maximum are clamped to
 * the axis range and the default to the new range.
 * SOFEL_ERROR_NOT_FOUND when the font has no such axis,
 * SOFEL_ERROR_INVALID_ARGUMENT when `min` is above `max`; the input is
 * then unchanged.
 *
 * Rust: sofel_subset::Input::with_axis_range
 * HarfBuzz: hb_subset_input_set_axis_range
 * FreeType: none
 */
int32_t sofel_subset_input_set_axis_range(sofel_subset_input_t *input, const sofel_font_t *font,
                                          uint32_t tag, float min, float max, float def);

/*
 * Overrides the name record (platform_id, encoding_id, language_id,
 * name_id) with `text` (UTF-8; `len` bytes, not NUL-terminated), or
 * removes the record when `len` is 0 (`text` may then be NULL).
 * SOFEL_ERROR_INVALID_ARGUMENT, with the input unchanged, for platform 1
 * with a byte that is not ASCII, as HarfBuzz refuses.
 *
 * Rust: sofel_subset::Input::with_name_override
 * HarfBuzz: hb_subset_input_override_name_table
 * FreeType: none
 */
int32_t sofel_subset_input_override_name_table(sofel_subset_input_t *input, uint16_t name_id,
                                               uint16_t platform_id, uint16_t encoding_id,
                                               uint16_t language_id, const char *text,
                                               size_t len);

/*
 * The input as hb-subset options, separated by spaces (not
 * NUL-terminated): the flags, then each set that differs from the
 * defaults, the glyph mapping and the axes.
 *
 * Rust: sofel_subset::Input::to_args
 * HarfBuzz: hb_subset_input_to_string_or_fail
 * FreeType: none
 */
int32_t sofel_subset_input_to_args(const sofel_subset_input_t *input, char *args,
                                   size_t capacity, size_t *len);

/*
 * Parses an axis range as hb-subset's --variations does: `drop`, a value,
 * `min:max` or `min:default:max`, where an empty part means the font's
 * value, written as NaN (drop gives three NaNs; `min:max` a NaN
 * default). SOFEL_ERROR_INVALID_ARGUMENT when it does not parse.
 *
 * Rust: sofel_subset::axis_range_from_string
 * HarfBuzz: hb_subset_axis_range_from_string
 * FreeType: none
 */
int32_t sofel_subset_axis_range_from_string(const char *s, size_t len, float *min, float *def,
                                            float *max);

/*
 * The range of axis `tag` of the input as `min:default:max` (each as C's
 * "%g"; not NUL-terminated). SOFEL_ERROR_NOT_FOUND when the input has
 * none.
 *
 * Rust: sofel_subset::axis_range_to_string
 * HarfBuzz: hb_subset_axis_range_to_string
 * FreeType: none
 */
int32_t sofel_subset_axis_range_to_string(const sofel_subset_input_t *input, uint32_t tag,
                                          char *s, size_t capacity, size_t *len);

/* The result of preprocessing a font: immutable, usable from several
 * threads at once. */
typedef struct sofel_subset_prepared sofel_subset_prepared_t;

/*
 * Preprocesses `font` for subsetting it many times: plans made with the
 * result (sofel_subset_plan_create) give the same bytes, faster. It
 * fails only for a NULL argument.
 *
 * Rust: sofel_subset::Prepared::new
 * HarfBuzz: hb_subset_preprocess
 * FreeType: none
 */
int32_t sofel_subset_prepared_create(const sofel_font_t *font,
                                     sofel_subset_prepared_t **prepared);

/*
 * Plans made with `prepared` keep their own reference to its data and
 * may outlive it.
 *
 * Rust: none
 * HarfBuzz: hb_face_destroy (of the preprocessed face)
 * FreeType: none
 */
void sofel_subset_prepared_destroy(sofel_subset_prepared_t *prepared);

/* A subsetting plan: immutable, usable from several threads at once. */
typedef struct sofel_subset_plan sofel_subset_plan_t;

/*
 * Computes the glyph closure and mapping of `input` for `font`, with
 * `prepared` (from sofel_subset_prepared_create for the same font) when
 * it is not NULL. The errors are those of subsetting
 * (SOFEL_ERROR_NO_GLYPHS, _INVALID_INPUT, _MALFORMED_TABLE, _UNSUPPORTED,
 * _LIMIT); SOFEL_ERROR_INVALID_INPUT also when `prepared` was made from
 * a font with another glyph count or cmap, DMAP or CFF table length.
 *
 * Rust: sofel_subset::Plan::new, sofel_subset::Plan::with_prepared
 * HarfBuzz: hb_subset_plan_create_or_fail (on a preprocessed face)
 * FreeType: none
 */
int32_t sofel_subset_plan_create(const sofel_font_t *font,
                                 const sofel_subset_prepared_t *prepared,
                                 const sofel_subset_input_t *input, sofel_subset_plan_t **plan);

/*
 * Rust: none
 * HarfBuzz: hb_subset_plan_destroy
 * FreeType: none
 */
void sofel_subset_plan_destroy(sofel_subset_plan_t *plan);

/*
 * The bytes of the CharStrings INDEX of the font's CFF table, without
 * copying: *data points into the font's data. SOFEL_ERROR_NOT_FOUND
 * when HarfBuzz does not read the table.
 *
 * Rust: sofel_subset::cff_charstrings_index
 * HarfBuzz: hb_subset_cff_get_charstrings_index
 * FreeType: none
 */
int32_t sofel_subset_cff_charstrings_index(const sofel_font_t *font, const uint8_t **data,
                                           size_t *len);

/*
 * The charstring of `glyph` in the font's CFF table, without copying.
 * SOFEL_ERROR_NOT_FOUND when HarfBuzz does not read the table, for a
 * glyph not below the count and for an empty charstring.
 *
 * Rust: sofel_subset::cff_charstring_data
 * HarfBuzz: hb_subset_cff_get_charstring_data
 * FreeType: none
 */
int32_t sofel_subset_cff_charstring_data(const sofel_font_t *font, uint32_t glyph,
                                         const uint8_t **data, size_t *len);

/*
 * Writes the subset font of a plan made for `font`; free the bytes with
 * sofel_bytes_destroy. The errors are those of subsetting.
 *
 * Rust: sofel_subset::Plan::execute
 * HarfBuzz: hb_subset_plan_execute_or_fail, hb_face_reference_blob
 * FreeType: none
 */
int32_t sofel_subset_plan_execute(const sofel_subset_plan_t *plan, const sofel_font_t *font,
                                  sofel_bytes_t *out);

/*
 * The number of glyphs of the subset font.
 *
 * Rust: sofel_subset::Plan::num_output_glyphs
 * HarfBuzz: none
 * FreeType: none
 */
int32_t sofel_subset_plan_num_output_glyphs(const sofel_subset_plan_t *plan, uint32_t *count);

/*
 * The new glyph of an old glyph, the old glyph of a new glyph, and the old
 * glyph of a kept character. SOFEL_ERROR_NOT_FOUND when there is none.
 *
 * Rust: sofel_subset::Plan::new_glyph, sofel_subset::Plan::old_glyph
 * HarfBuzz: hb_subset_plan_old_to_new_glyph_mapping, hb_subset_plan_new_to_old_glyph_mapping, hb_subset_plan_unicode_to_old_glyph_mapping with hb_map_get
 * FreeType: none
 */
int32_t sofel_subset_plan_old_to_new_glyph(const sofel_subset_plan_t *plan, uint32_t old,
                                           uint32_t *new_glyph);
int32_t sofel_subset_plan_new_to_old_glyph(const sofel_subset_plan_t *plan, uint32_t new_glyph,
                                           uint32_t *old);
int32_t sofel_subset_plan_unicode_to_old_glyph(const sofel_subset_plan_t *plan,
                                               uint32_t unicode, uint32_t *old);

/*
 * Every pair of a mapping, into the caller's arrays of `capacity` pairs:
 * (old, new) ascending by old glyph, (new, old) ascending by new glyph,
 * and (character, old glyph) ascending by character. *len is the number
 * of pairs; SOFEL_ERROR_BUFFER_TOO_SMALL as for other arrays.
 *
 * Rust: sofel_subset::Plan::old_to_new_glyphs, sofel_subset::Plan::new_to_old_glyphs, sofel_subset::Plan::unicode_to_old_glyphs
 * HarfBuzz: hb_subset_plan_old_to_new_glyph_mapping, hb_subset_plan_new_to_old_glyph_mapping, hb_subset_plan_unicode_to_old_glyph_mapping with hb_map_next
 * FreeType: none
 */
int32_t sofel_subset_plan_old_to_new_glyphs(const sofel_subset_plan_t *plan, uint32_t *old,
                                            uint32_t *new_glyph, size_t capacity, size_t *len);
int32_t sofel_subset_plan_new_to_old_glyphs(const sofel_subset_plan_t *plan,
                                            uint32_t *new_glyph, uint32_t *old,
                                            size_t capacity, size_t *len);
int32_t sofel_subset_plan_unicode_to_old_glyphs(const sofel_subset_plan_t *plan,
                                                uint32_t *unicode, uint32_t *old,
                                                size_t capacity, size_t *len);

/*
 * Subsets `font` as `input` says, into bytes to free with
 * sofel_bytes_destroy. The errors are those of sofel_subset_plan_create
 * and sofel_subset_plan_execute.
 *
 * Rust: sofel_subset::subset
 * HarfBuzz: hb_subset_or_fail, hb_face_reference_blob
 * FreeType: none
 */
int32_t sofel_subset(const sofel_font_t *font, const sofel_subset_input_t *input,
                     sofel_bytes_t *out);

/* What a link's offset is measured from. */
#define SOFEL_SERIALIZE_WHENCE_HEAD 0     /* the start of the parent */
#define SOFEL_SERIALIZE_WHENCE_TAIL 1     /* the end of the parent */
#define SOFEL_SERIALIZE_WHENCE_ABSOLUTE 2 /* the start of the table */

/* A link from an object to another (hb_subset_serialize_link_t, with the
 * offset kinds of HarfBuzz's serializer). `objidx` 1 is objects[0].
 * Virtual links (which only order the objects) use `objidx` alone. */
typedef struct sofel_serialize_link
{
  uint32_t width;     /* the offset's bytes: 2, 3 or 4 */
  uint32_t position;  /* of the offset in the parent */
  uint32_t objidx;
  uint32_t is_signed; /* 0, or 1 for a signed offset (width 2 or 4) */
  uint32_t whence;    /* SOFEL_SERIALIZE_WHENCE_* */
  uint32_t bias;      /* subtracted from the offset */
} sofel_serialize_link_t;

/* An object: its bytes (offsets zero) and links
 * (hb_subset_serialize_object_t). */
typedef struct sofel_serialize_object
{
  const uint8_t *data;
  size_t len;
  const sofel_serialize_link_t *real_links;
  size_t num_real_links;
  const sofel_serialize_link_t *virtual_links;
  size_t num_virtual_links;
} sofel_serialize_object_t;

/*
 * Serializes caller-built objects into one table, the last object being
 * the root, and repacks them to resolve offset overflows (for `GSUB` and
 * `GPOS`, splitting subtables and promoting lookups to extension
 * lookups). Free the bytes with sofel_bytes_destroy.
 * SOFEL_ERROR_INVALID_ARGUMENT for no objects, a width other than 2, 3 or
 * 4, a signed offset of 3 bytes, an unknown whence, a link outside its
 * object or to an object that does not exist, or a graph the repacker
 * refuses; SOFEL_ERROR_OFFSET_OVERFLOW when the overflows cannot be
 * resolved. A pointer of an empty array may be NULL.
 *
 * Rust: sofel_write::serialize_objects
 * HarfBuzz: hb_subset_serialize_or_fail
 * FreeType: none
 */
int32_t sofel_serialize_objects(uint32_t tag, const sofel_serialize_object_t *objects,
                                size_t count, sofel_bytes_t *out);

#ifdef __cplusplus
}
#endif

#endif /* SOFEL_H */
