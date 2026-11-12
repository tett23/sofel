/*
 * Tests of the C ABI (ADR 0015), run by scripts/test-capi.sh natively,
 * with ThreadSanitizer and AddressSanitizer, and on wasm32-wasip1.
 *
 * Every function of sofel.h is called at least once. The expected
 * values come from fixture.h, which crates/sofel-capi/tests/fixture.rs
 * computes with the Rust API.
 */

#include <math.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "sofel.h"
#include "fixture.h"
#include "fixture_shape.h"
#include "fixture_ppem.h"
#include "fixture_subset.h"

#if !defined(__wasi__)
#include <pthread.h>
#define THREADS 4
#endif

/* Large enough for every bitmap of the fixture. */
#define RENDER_CAPACITY 4096

#define COUNT(a) (sizeof(a) / sizeof((a)[0]))

static int failures;

#define CHECK(cond)                                                        \
  do {                                                                     \
    if (!(cond)) {                                                         \
      fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__, __LINE__, #cond); \
      failures++;                                                          \
    }                                                                      \
  } while (0)

/* --------------------------------------------- declarations vs. types */

/*
 * Assigning every function to a variable of its declared type makes a
 * header that disagrees with itself fail to compile (-Werror), and
 * takes the address of every symbol, so a missing one fails to link.
 */
static void check_declarations(void) {
  uint32_t (*version)(void) = sofel_version;
  uint32_t (*u_version)(void) = sofel_unicode_version;
  uint8_t (*u_general_category)(uint32_t) = sofel_unicode_general_category;
  uint8_t (*u_combining_class)(uint32_t) = sofel_unicode_combining_class;
  uint32_t (*u_script)(uint32_t) = sofel_unicode_script;
  int32_t (*u_script_from_iso15924)(const uint8_t *, size_t, uint32_t *) =
      sofel_script_from_iso15924;
  uint32_t (*u_mirroring)(uint32_t) = sofel_unicode_mirroring;
  int32_t (*u_decompose)(uint32_t, uint32_t *, uint32_t *) = sofel_unicode_decompose;
  int32_t (*u_compose)(uint32_t, uint32_t, uint32_t *) = sofel_unicode_compose;
  uint8_t (*u_pictographic)(uint32_t) = sofel_unicode_is_extended_pictographic;
  uint8_t (*u_vertical_orientation)(uint32_t) = sofel_unicode_vertical_orientation;
  const char *(*error_string)(int32_t) = sofel_error_string;
  int32_t (*mul_fix)(int32_t, int32_t) = sofel_mul_fix;
  int32_t (*div_fix)(int32_t, int32_t) = sofel_div_fix;
  int32_t (*mul_div)(int32_t, int32_t, int32_t) = sofel_mul_div;
  int32_t (*mul_div_no_round)(int32_t, int32_t, int32_t) = sofel_mul_div_no_round;
  int32_t (*transform)(int32_t *, int32_t *, int32_t, int32_t, int32_t, int32_t) =
      sofel_transform;
  int32_t (*vector_length)(int32_t, int32_t) = sofel_vector_length;
  sofel_font_t *(*font_create)(const uint8_t *, size_t, uint32_t, int32_t *) =
      sofel_font_create;
  void (*font_destroy)(sofel_font_t *) = sofel_font_destroy;
  int32_t (*units_per_em)(const sofel_font_t *, uint16_t *) = sofel_font_units_per_em;
  int32_t (*num_glyphs)(const sofel_font_t *, uint32_t *) = sofel_font_num_glyphs;
  int32_t (*nominal_glyph)(const sofel_font_t *, uint32_t, uint32_t *) =
      sofel_font_nominal_glyph;
  int32_t (*variation_glyph)(const sofel_font_t *, uint32_t, uint32_t, uint32_t *) =
      sofel_font_variation_glyph;
  int32_t (*h_advance)(const sofel_font_t *, const sofel_location_t *, uint32_t, uint32_t *) =
      sofel_font_h_advance;
  int32_t (*ft_bearing)(const sofel_font_t *, const sofel_location_t *, uint32_t, int16_t *) =
      sofel_font_ft_left_side_bearing;
  int32_t (*ft_h_advance)(const sofel_font_t *, const sofel_location_t *, uint32_t, uint16_t *) =
      sofel_font_ft_h_advance;
  int32_t (*ft_top_bearing)(const sofel_font_t *, const sofel_location_t *, uint32_t, int16_t,
                            int16_t *) = sofel_font_ft_top_side_bearing;
  int32_t (*table)(const sofel_font_t *, uint32_t, const uint8_t **, size_t *) =
      sofel_font_table;
  sofel_outline_scratch_t *(*scratch_create)(void) = sofel_outline_scratch_create;
  void (*scratch_destroy)(sofel_outline_scratch_t *) = sofel_outline_scratch_destroy;
  int32_t (*pixel_scale)(const sofel_font_t *, uint16_t, uint16_t, int32_t *, int32_t *) =
      sofel_outline_pixel_scale;
  int32_t (*load)(const sofel_font_t *, const sofel_location_t *, uint32_t, uint32_t, int32_t, int32_t,
                  sofel_outline_scratch_t *, int32_t *, uint32_t *) = sofel_outline_load;
  int32_t (*decompose)(const sofel_outline_scratch_t *, int32_t, int32_t, int32_t,
                       sofel_move_to_func_t, sofel_line_to_func_t, sofel_quad_to_func_t,
                       sofel_cubic_to_func_t, sofel_close_func_t, void *) =
      sofel_outline_decompose;
  int32_t (*draw)(const sofel_font_t *, const sofel_location_t *, uint32_t, uint32_t, int32_t, int32_t,
                  sofel_outline_scratch_t *, sofel_move_to_func_t, sofel_line_to_func_t,
                  sofel_quad_to_func_t, sofel_cubic_to_func_t, sofel_close_func_t, void *,
                  int32_t *, uint32_t *) = sofel_outline_draw;
  sofel_rasterizer_t *(*raster_create)(void) = sofel_rasterizer_create;
  sofel_rasterizer_t *(*raster_create_cells)(size_t) = sofel_rasterizer_create_with_band_cells;
  void (*raster_destroy)(sofel_rasterizer_t *) = sofel_rasterizer_destroy;
  int32_t (*raster_clear)(sofel_rasterizer_t *) = sofel_rasterizer_clear;
  sofel_move_to_func_t raster_move_to = sofel_rasterizer_move_to;
  sofel_line_to_func_t raster_line_to = sofel_rasterizer_line_to;
  sofel_quad_to_func_t raster_quad_to = sofel_rasterizer_quad_to;
  sofel_cubic_to_func_t raster_cubic_to = sofel_rasterizer_cubic_to;
  sofel_close_func_t raster_close = sofel_rasterizer_close;
  int32_t (*pixel_box)(int32_t, int32_t, int32_t, int32_t, int32_t *, int32_t *) =
      sofel_pixel_box;
  int32_t (*render)(sofel_rasterizer_t *, uint32_t, uint32_t, uint16_t, uint16_t, uint8_t *,
                    size_t, size_t *, int32_t *) = sofel_rasterizer_render;
  int32_t (*render_placed)(sofel_rasterizer_t *, int32_t, int32_t, uint32_t, uint32_t,
                           uint32_t, uint32_t, uint16_t, uint16_t, uint8_t *, size_t,
                           size_t *, int32_t *) = sofel_rasterizer_render_placed;
  int32_t (*render_glyph)(const sofel_font_t *, const sofel_location_t *, uint32_t, uint16_t, sofel_outline_scratch_t *,
                          sofel_rasterizer_t *, uint8_t *, size_t, size_t *, int32_t *) =
      sofel_render_glyph;
  int32_t (*validate)(const uint8_t *, size_t, sofel_diagnostic_func_t, void *) =
      sofel_validate;
  int32_t (*diag_font)(const sofel_diagnostic_t *, uint32_t *) = sofel_diagnostic_font;
  int32_t (*diag_table)(const sofel_diagnostic_t *, uint32_t *) = sofel_diagnostic_table;
  int32_t (*diag_offset)(const sofel_diagnostic_t *, uint32_t *) = sofel_diagnostic_offset;
  int32_t (*diag_severity)(const sofel_diagnostic_t *, int32_t *) = sofel_diagnostic_severity;
  int32_t (*diag_kind)(const sofel_diagnostic_t *, char *, size_t, size_t *) =
      sofel_diagnostic_kind;
  int32_t (*diag_message)(const sofel_diagnostic_t *, char *, size_t, size_t *) =
      sofel_diagnostic_message;
  int32_t (*diag_details)(const sofel_diagnostic_t *, sofel_detail_func_t, void *) =
      sofel_diagnostic_details;
  int32_t (*paint_glyph)(const sofel_font_t *, const sofel_location_t *, uint32_t, uint16_t, const sofel_color_t *, size_t,
                         sofel_color_t, uint16_t, sofel_paint_commands_t *) = sofel_paint_glyph;
  void (*paint_free)(sofel_paint_commands_t *) = sofel_paint_commands_free;
  int32_t (*palette_count)(const sofel_font_t *, uint16_t *) = sofel_palette_count;
  int32_t (*palette_entry_count)(const sofel_font_t *, uint16_t *) = sofel_palette_entry_count;
  int32_t (*palette_name_id)(const sofel_font_t *, uint16_t, uint16_t *) = sofel_palette_name_id;
  int32_t (*palette_flags)(const sofel_font_t *, uint16_t, uint32_t *) = sofel_palette_flags;
  int32_t (*palette_color_name_id)(const sofel_font_t *, uint16_t, uint16_t *) =
      sofel_palette_color_name_id;
  int32_t (*palette_colors)(const sofel_font_t *, uint16_t, sofel_color_t *, size_t, size_t *) =
      sofel_palette_colors;
  int32_t (*find_palette)(const sofel_font_t *, uint32_t, uint16_t *) = sofel_find_palette;
  int32_t (*color_formats)(const sofel_font_t *, uint32_t *) = sofel_color_formats;
  int32_t (*glyph_color_format)(const sofel_font_t *, uint32_t, uint32_t *) =
      sofel_glyph_color_format;
  int32_t (*color_layers)(const sofel_font_t *, uint32_t, uint32_t *, int32_t *, size_t,
                          size_t *) = sofel_color_layers;
  int32_t (*svg_document)(const sofel_font_t *, uint32_t, const uint8_t **, size_t *, uint32_t *,
                          uint32_t *) = sofel_svg_document;
  int32_t (*bitmap_strikes)(const sofel_font_t *, sofel_strike_t *, size_t, size_t *) =
      sofel_bitmap_strikes;
  int32_t (*choose_strike)(const sofel_font_t *, uint16_t, sofel_strike_t *) = sofel_choose_strike;
  int32_t (*match_strike)(const sofel_font_t *, uint32_t, uint32_t, sofel_strike_t *) =
      sofel_match_strike;
  int32_t (*glyph_bitmap)(const sofel_font_t *, uint32_t, uint32_t, sofel_glyph_bitmap_t *) =
      sofel_glyph_bitmap;
  int32_t (*draw_bitmap)(const sofel_font_t *, uint32_t, uint32_t, uint8_t *, size_t,
                         sofel_bitmap_layout_t *) = sofel_draw_bitmap;
  int32_t (*has_outlines)(const sofel_font_t *, uint8_t *) = sofel_font_has_outlines;
  int32_t (*has_bitmaps)(const sofel_font_t *, uint8_t *) = sofel_font_has_bitmaps;
  int32_t (*sin_cos)(float, float *, float *) = sofel_sin_cos;
  float (*tan_f)(float) = sofel_tan;
  int32_t (*normalize)(sofel_color_stop_t *, size_t, float *, float *) = sofel_normalize_color_line;
  int32_t (*reduce)(const float *, float *) = sofel_reduce_linear_anchors;
  int32_t (*tiles)(sofel_color_stop_t *, size_t, uint32_t, float, float, sofel_sweep_tile_func_t,
                   void *) = sofel_sweep_gradient_tiles;
  const void *all[] = {
      (const void *)version,         (const void *)error_string,
      (const void *)mul_fix,         (const void *)div_fix,
      (const void *)mul_div,         (const void *)mul_div_no_round,
      (const void *)transform,       (const void *)vector_length,
      (const void *)font_create,     (const void *)font_destroy,
      (const void *)units_per_em,    (const void *)num_glyphs,
      (const void *)nominal_glyph,   (const void *)variation_glyph,
      (const void *)h_advance,       (const void *)table,
      (const void *)ft_bearing,
      (const void *)ft_h_advance,    (const void *)ft_top_bearing,
      (const void *)scratch_create,  (const void *)scratch_destroy,
      (const void *)pixel_scale,     (const void *)load,
      (const void *)decompose,       (const void *)draw,
      (const void *)raster_create,   (const void *)raster_create_cells,
      (const void *)raster_destroy,  (const void *)raster_clear,
      (const void *)raster_move_to,  (const void *)raster_line_to,
      (const void *)raster_quad_to,  (const void *)raster_cubic_to,
      (const void *)raster_close,    (const void *)pixel_box,
      (const void *)render,          (const void *)render_placed,
      (const void *)render_glyph,    (const void *)validate,
      (const void *)diag_font,       (const void *)diag_table,
      (const void *)diag_offset,     (const void *)diag_severity,
      (const void *)diag_kind,       (const void *)diag_message,
      (const void *)diag_details,
      (const void *)u_version,       (const void *)u_general_category,
      (const void *)u_combining_class, (const void *)u_script,
      (const void *)u_script_from_iso15924, (const void *)u_mirroring,
      (const void *)u_decompose,     (const void *)u_compose,
      (const void *)u_pictographic,  (const void *)u_vertical_orientation,
      (const void *)paint_glyph, (const void *)paint_free, (const void *)palette_count,
      (const void *)palette_entry_count, (const void *)palette_name_id,
      (const void *)palette_flags, (const void *)palette_color_name_id,
      (const void *)palette_colors, (const void *)find_palette, (const void *)color_formats,
      (const void *)glyph_color_format, (const void *)color_layers, (const void *)svg_document,
      (const void *)bitmap_strikes, (const void *)choose_strike, (const void *)match_strike,
      (const void *)glyph_bitmap, (const void *)sin_cos, (const void *)tan_f,
      (const void *)normalize, (const void *)reduce, (const void *)tiles,
      (const void *)draw_bitmap, (const void *)has_outlines, (const void *)has_bitmaps,
  };
  size_t i;
  for (i = 0; i < COUNT(all); i++) CHECK(all[i] != NULL);
}

/* -------------------------------------------- variation location ABI */

static void test_variation_abi(const sofel_font_t *font) {
  sofel_location_t *loc = NULL;
  sofel_variation_t variation = {0, 0.0f};
  char text[16] = {0};
  size_t len = 99;
  uint32_t index = 0;
  uint8_t flag = 99;
  int32_t ft_error = 99;

  CHECK(sofel_variation_parse("wght=500", 8, &variation) == SOFEL_OK);
  CHECK(variation.tag == SOFEL_TAG('w', 'g', 'h', 't') && variation.value == 500.0f);
  CHECK(sofel_variation_format(&variation, text, sizeof text, &len) == SOFEL_OK);
  CHECK(len == 8 && memcmp(text, "wght=500", len + 1) == 0);

  CHECK(sofel_location_create(font, UINT32_MAX, &variation, 1, &loc) == SOFEL_OK);
  CHECK(loc != NULL);
  CHECK(sofel_location_coords_normalized(loc, NULL, 0, &len) == SOFEL_OK && len == 0);
  CHECK(sofel_location_coords_design(loc, NULL, 0, &len) == SOFEL_OK && len == 0);
  CHECK(sofel_location_ft_coords(loc, NULL, 0, &len) == SOFEL_OK && len == 0);
  CHECK(sofel_location_ft_design_coords(loc, NULL, 0, &len) == SOFEL_OK && len == 0);
  CHECK(sofel_location_ft_error(loc, &ft_error) == SOFEL_OK && ft_error == SOFEL_OK);
  CHECK(sofel_location_named_instance_index(loc, &index) == SOFEL_OK && index == UINT32_MAX);
  CHECK(sofel_location_hb_nonzero(loc, &flag) == SOFEL_OK && flag == 0);
  CHECK(sofel_location_ft_default(loc, &flag) == SOFEL_OK && flag == 1);
  sofel_location_destroy(loc);

  CHECK(sofel_location_create_design(font, NULL, 0, &loc) == SOFEL_OK);
  sofel_location_destroy(loc);
  CHECK(sofel_location_create_normalized_2_14(font, NULL, 0, &loc) == SOFEL_OK);
  sofel_location_destroy(loc);
  CHECK(sofel_location_create_normalized_16_16(font, NULL, 0, &loc) == SOFEL_OK);
  sofel_location_destroy(loc);
  CHECK(sofel_location_create_named_instance(font, 0, &loc) == SOFEL_OK);
  sofel_location_destroy(loc);
  sofel_location_destroy(NULL);
}

/* ----------------------------------------------------- recording pens */

#define MAX_COMMANDS 256

struct recorder {
  int32_t data[MAX_COMMANDS];
  size_t len;
};

static void put(struct recorder *r, int32_t v) {
  if (r->len < MAX_COMMANDS) r->data[r->len] = v;
  r->len++;
}

static void rec_move_to(void *user, int32_t x, int32_t y) {
  struct recorder *r = user;
  put(r, 'M');
  put(r, x);
  put(r, y);
}

static void rec_line_to(void *user, int32_t x, int32_t y) {
  struct recorder *r = user;
  put(r, 'L');
  put(r, x);
  put(r, y);
}

static void rec_quad_to(void *user, int32_t cx, int32_t cy, int32_t x, int32_t y) {
  struct recorder *r = user;
  put(r, 'Q');
  put(r, cx);
  put(r, cy);
  put(r, x);
  put(r, y);
}

static void rec_cubic_to(void *user, int32_t c1x, int32_t c1y, int32_t c2x, int32_t c2y,
                         int32_t x, int32_t y) {
  struct recorder *r = user;
  put(r, 'C');
  put(r, c1x);
  put(r, c1y);
  put(r, c2x);
  put(r, c2y);
  put(r, x);
  put(r, y);
}

static void rec_close(void *user) { put(user, 'Z'); }

static int same_commands(const struct recorder *r, const int32_t *expected, size_t len) {
  return r->len == len && (len == 0 || memcmp(r->data, expected, len * sizeof(int32_t)) == 0);
}

/* ------------------------------------------------------ the queries */

/*
 * Every query on a font, compared with the fixture. Returns the number
 * of mismatches, so that threads can run it too. `scratch` and
 * `raster` belong to the caller's thread.
 */
static int run_queries(const sofel_font_t *font, sofel_outline_scratch_t *scratch,
                       sofel_rasterizer_t *raster) {
  int bad = 0;
  size_t i;
  uint16_t upem = 0;
  uint32_t n = 0;

  bad += sofel_font_units_per_em(font, &upem) != SOFEL_OK || upem != FIXTURE_UPEM;
  bad += sofel_font_num_glyphs(font, &n) != SOFEL_OK || n != FIXTURE_NUM_GLYPHS;
  for (i = 0; i < COUNT(nominal_cases); i++) {
    uint32_t g = 12345;
    int32_t s = sofel_font_nominal_glyph(font, nominal_cases[i].codepoint, &g);
    bad += s != nominal_cases[i].status;
    bad += s == SOFEL_OK && g != nominal_cases[i].glyph;
  }
  for (i = 0; i < COUNT(variation_cases); i++) {
    uint32_t g = 12345;
    int32_t s = sofel_font_variation_glyph(font, variation_cases[i].codepoint,
                                           variation_cases[i].selector, &g);
    bad += s != variation_cases[i].status;
    bad += s == SOFEL_OK && g != variation_cases[i].glyph;
  }
  for (i = 0; i < COUNT(advances); i++) {
    uint32_t a = 0;
    uint16_t ft_a = 0;
    bad += sofel_font_h_advance(font, NULL, (uint32_t)i, &a) != SOFEL_OK || a != advances[i];
    int32_t ft_status = sofel_font_ft_h_advance(font, NULL, (uint32_t)i, &ft_a);
    bad += i < FIXTURE_NUM_GLYPHS
               ? ft_status != SOFEL_OK || ft_a != (uint16_t)advances[i]
               : ft_status != SOFEL_ERROR_INVALID_GLYPH;
  }
  {
    int16_t top = 0;
    bad += sofel_font_ft_top_side_bearing(font, NULL, 0, 0, &top) != SOFEL_OK;
  }
  for (i = 0; i < COUNT(ft_bearings); i++) {
    int16_t b = 12345;
    bad += sofel_font_ft_left_side_bearing(font, NULL, (uint32_t)i, &b) != SOFEL_OK ||
           b != ft_bearings[i];
  }
  for (i = 0; i < COUNT(outline_cases); i++) {
    const struct outline_case *c = &outline_cases[i];
    struct recorder rec;
    int32_t cbox[4] = {0, 0, 0, 0};
    uint32_t overlap = 0;
    int32_t s;
    rec.len = 0;
    s = sofel_outline_draw(font, NULL, c->glyph, c->mode, c->x_scale, c->y_scale, scratch,
                           rec_move_to, rec_line_to, rec_quad_to, rec_cubic_to, rec_close,
                           &rec, cbox, &overlap);
    bad += s != c->status;
    bad += !same_commands(&rec, c->commands, c->len);
    if (s == SOFEL_OK) {
      bad += memcmp(cbox, c->cbox, sizeof cbox) != 0 || overlap != c->overlap;
    }
  }
  for (i = 0; i < COUNT(render_cases); i++) {
    const struct render_case *c = &render_cases[i];
    uint8_t buf[RENDER_CAPACITY];
    size_t len = 0;
    int32_t box[4] = {0, 0, 0, 0};
    int32_t s = sofel_render_glyph(font, NULL, c->glyph, c->ppem, scratch, raster, buf, sizeof buf,
                                   &len, box);
    bad += s != c->status;
    if (s == SOFEL_OK) {
      bad += len != c->len || memcmp(box, c->box, sizeof box) != 0;
      bad += len > 0 && memcmp(buf, c->pixels, len) != 0;
    }
  }
  return bad;
}

/* ------------------------------------------------------------- tests */

static void test_version_and_errors(void) {
  CHECK(sofel_version() == SOFEL_VERSION);
  CHECK(strcmp(sofel_error_string(SOFEL_OK), "success") == 0);
  CHECK(strcmp(sofel_error_string(SOFEL_ERROR_NULL), "null pointer argument") == 0);
  CHECK(strcmp(sofel_error_string(SOFEL_ERROR_BUFFER_TOO_SMALL), "buffer too small") == 0);
  CHECK(strcmp(sofel_error_string(-1000), "unknown error") == 0);
  CHECK(strcmp(sofel_error_string(1), "unknown error") == 0);
  {
    int32_t e;
    for (e = SOFEL_ERROR_INVALID_ARGUMENT; e <= SOFEL_OK; e++) {
      CHECK(strcmp(sofel_error_string(e), "unknown error") != 0);
    }
  }
}

static void test_geometry(void) {
  size_t i;
  int32_t x = 300, y = -77;
  for (i = 0; i < COUNT(fix_cases); i++) {
    const struct fix_case *c = &fix_cases[i];
    CHECK(sofel_mul_fix(c->a, c->b) == c->mul_fix);
    CHECK(sofel_div_fix(c->a, c->b) == c->div_fix);
    CHECK(sofel_mul_div(c->a, c->b, c->c) == c->mul_div);
    CHECK(sofel_mul_div_no_round(c->a, c->b, c->c) == c->mul_div_no_round);
    CHECK(sofel_vector_length(c->a, c->b) == c->vector_length);
  }
  CHECK(sofel_transform(&x, &y, 0x10000, 0x4000, -0x8000, 0x18000) == SOFEL_OK);
  CHECK(x == TRANSFORM_X && y == TRANSFORM_Y);
  CHECK(sofel_transform(NULL, &y, 0x10000, 0, 0, 0x10000) == SOFEL_ERROR_NULL);
  CHECK(sofel_transform(&x, NULL, 0x10000, 0, 0, 0x10000) == SOFEL_ERROR_NULL);
}

static void test_unicode(void) {
  size_t i;
  uint32_t a = 7, b = 7, ab = 7, script = 7;
  CHECK(sofel_unicode_version() == UNICODE_VERSION);
  for (i = 0; i < COUNT(unicode_cases); i++) {
    const struct unicode_case *c = &unicode_cases[i];
    uint32_t da = 0, db = 0;
    CHECK(sofel_unicode_general_category(c->codepoint) == c->gc);
    CHECK(sofel_unicode_combining_class(c->codepoint) == c->ccc);
    CHECK(sofel_unicode_script(c->codepoint) == c->script);
    CHECK(sofel_unicode_mirroring(c->codepoint) == c->mirror);
    CHECK(sofel_unicode_decompose(c->codepoint, &da, &db) == c->status);
    CHECK(da == c->a && db == c->b);
    CHECK(sofel_unicode_is_extended_pictographic(c->codepoint) == c->pict);
    CHECK(sofel_unicode_vertical_orientation(c->codepoint) == c->vo);
  }
  CHECK(sofel_unicode_general_category(0x41) == SOFEL_GC_UPPERCASE_LETTER);
  CHECK(sofel_unicode_script(0x41) == 0x4C61746E);
  CHECK(sofel_unicode_vertical_orientation(0x3042) == SOFEL_VO_U);
  for (i = 0; i < COUNT(compose_cases); i++) {
    const struct compose_case *c = &compose_cases[i];
    uint32_t out = 0;
    CHECK(sofel_unicode_compose(c->a, c->b, &out) == c->status);
    CHECK(out == c->ab);
  }
  for (i = 0; i < COUNT(script_cases); i++) {
    const struct script_case *c = &script_cases[i];
    uint32_t out = 0;
    CHECK(sofel_script_from_iso15924((const uint8_t *)c->tag, strlen(c->tag), &out) ==
          c->status);
    CHECK(out == c->script);
  }
  CHECK(sofel_unicode_decompose(0xC5, NULL, &b) == SOFEL_ERROR_NULL);
  CHECK(sofel_unicode_decompose(0xC5, &a, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_unicode_compose(0x41, 0x30A, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_script_from_iso15924(NULL, 4, &script) == SOFEL_ERROR_NULL);
  CHECK(sofel_script_from_iso15924((const uint8_t *)"Latn", 4, NULL) == SOFEL_ERROR_NULL);
  CHECK(a == 7 && b == 7 && ab == 7 && script == 7);
}

static void test_open(void) {
  size_t i;
  int32_t e = 12345;
  for (i = 0; i < COUNT(open_cases); i++) {
    const struct open_case *c = &open_cases[i];
    sofel_font_t *f;
    e = 12345;
    f = sofel_font_create(c->data, c->len, c->index, &e);
    CHECK(e == c->status);
    CHECK((f != NULL) == (c->status == SOFEL_OK));
    sofel_font_destroy(f);
  }
  e = 12345;
  CHECK(sofel_font_create(NULL, 10, 0, &e) == NULL);
  CHECK(e == SOFEL_ERROR_NULL);
  CHECK(sofel_font_create(font_data, sizeof font_data, 0, NULL) == NULL);
  sofel_font_destroy(NULL);
}

static void test_collection(void) {
  int32_t e;
  sofel_font_t *f = sofel_font_create(ttc_data, sizeof ttc_data, 1, &e);
  sofel_outline_scratch_t *scratch = sofel_outline_scratch_create();
  sofel_rasterizer_t *raster = sofel_rasterizer_create();
  CHECK(f != NULL);
  if (f != NULL) CHECK(run_queries(f, scratch, raster) == 0);
  sofel_rasterizer_destroy(raster);
  sofel_outline_scratch_destroy(scratch);
  sofel_font_destroy(f);
}

static void test_font(const sofel_font_t *font) {
  size_t i;
  uint16_t u16v = 0;
  uint32_t u32v = 0;
  const uint8_t *data = NULL;
  size_t len = 0;

  for (i = 0; i < COUNT(table_cases); i++) {
    const struct table_case *c = &table_cases[i];
    int32_t s;
    data = NULL;
    len = 0;
    s = sofel_font_table(font, c->tag, &data, &len);
    CHECK(s == c->status);
    if (s == SOFEL_OK) {
      /* Not copied: it points into the font's data. */
      CHECK(data == font_data + c->offset);
      CHECK(len == c->len);
    }
  }
  CHECK(sofel_font_table(font, SOFEL_TAG('c', 'm', 'a', 'p'), &data, &len) == SOFEL_OK);
  CHECK(sofel_font_table(font, SOFEL_TAG('h', 'e', 'a', 'd'), &data, &len) == SOFEL_OK);
  CHECK(len == 54);

  /* NULL arguments. */
  CHECK(sofel_font_units_per_em(NULL, &u16v) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_units_per_em(font, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_num_glyphs(NULL, &u32v) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_num_glyphs(font, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_nominal_glyph(NULL, 0x41, &u32v) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_nominal_glyph(font, 0x41, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_variation_glyph(NULL, 0x41, 0xFE00, &u32v) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_variation_glyph(font, 0x41, 0xFE00, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_h_advance(NULL, NULL, 1, &u32v) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_h_advance(font, NULL, 1, NULL) == SOFEL_ERROR_NULL);
  {
    int16_t b;
    CHECK(sofel_font_ft_left_side_bearing(NULL, NULL, 1, &b) == SOFEL_ERROR_NULL);
    CHECK(sofel_font_ft_left_side_bearing(font, NULL, 1, NULL) == SOFEL_ERROR_NULL);
  }
  CHECK(sofel_font_table(NULL, SOFEL_TAG('c', 'm', 'a', 'p'), &data, &len) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_table(font, SOFEL_TAG('c', 'm', 'a', 'p'), NULL, &len) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_table(font, SOFEL_TAG('c', 'm', 'a', 'p'), &data, NULL) == SOFEL_ERROR_NULL);
  /* A missing glyph leaves the output alone. */
  u32v = 777;
  CHECK(sofel_font_nominal_glyph(font, 0x20, &u32v) == SOFEL_ERROR_NOT_FOUND);
  CHECK(u32v == 777);
}

static void test_outlines(const sofel_font_t *font) {
  size_t i;
  sofel_outline_scratch_t *scratch = sofel_outline_scratch_create();
  struct recorder rec;
  int32_t cbox[4], xs = 0, ys = 0;
  uint32_t overlap;

  for (i = 0; i < COUNT(scale_cases); i++) {
    CHECK(sofel_outline_pixel_scale(font, scale_cases[i].x_ppem, scale_cases[i].y_ppem, &xs,
                                    &ys) == SOFEL_OK);
    CHECK(xs == scale_cases[i].x_scale && ys == scale_cases[i].y_scale);
  }
  CHECK(sofel_outline_pixel_scale(NULL, 16, 16, &xs, &ys) == SOFEL_ERROR_NULL);
  CHECK(sofel_outline_pixel_scale(font, 16, 16, NULL, &ys) == SOFEL_ERROR_NULL);
  CHECK(sofel_outline_pixel_scale(font, 16, 16, &xs, NULL) == SOFEL_ERROR_NULL);

  /* load + decompose gives what draw gives. */
  for (i = 0; i < COUNT(outline_cases); i++) {
    const struct outline_case *c = &outline_cases[i];
    int32_t s = sofel_outline_load(font, NULL, c->glyph, c->mode, c->x_scale, c->y_scale, scratch,
                                   cbox, &overlap);
    CHECK(s == c->status);
    if (s != SOFEL_OK) continue;
    CHECK(memcmp(cbox, c->cbox, sizeof cbox) == 0 && overlap == c->overlap);
    rec.len = 0;
    CHECK(sofel_outline_decompose(scratch, 0, 0, 1, rec_move_to, rec_line_to, rec_quad_to,
                                  rec_cubic_to, rec_close, &rec) == SOFEL_OK);
    CHECK(same_commands(&rec, c->commands, c->len));
  }

  /* An offset and a multiplier. */
  CHECK(sofel_outline_load(font, NULL, 1, SOFEL_SCALE_FIXED, scale_cases[0].x_scale,
                           scale_cases[0].y_scale, scratch, cbox, &overlap) == SOFEL_OK);
  rec.len = 0;
  CHECK(sofel_outline_decompose(scratch, 100, -50, 4, rec_move_to, rec_line_to, rec_quad_to,
                                rec_cubic_to, rec_close, &rec) == SOFEL_OK);
  CHECK(same_commands(&rec, decomposed, COUNT(decomposed)));

  /* Errors. */
  CHECK(sofel_outline_load(font, NULL, 1, 2, 0, 0, scratch, cbox, &overlap) ==
        SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_outline_load(font, NULL, FIXTURE_NUM_GLYPHS, SOFEL_SCALE_NONE, 0, 0, scratch, cbox,
                           &overlap) == SOFEL_ERROR_INVALID_GLYPH);
  CHECK(sofel_outline_load(NULL, NULL, 1, SOFEL_SCALE_NONE, 0, 0, scratch, cbox, &overlap) ==
        SOFEL_ERROR_NULL);
  CHECK(sofel_outline_load(font, NULL, 1, SOFEL_SCALE_NONE, 0, 0, NULL, cbox, &overlap) ==
        SOFEL_ERROR_NULL);
  CHECK(sofel_outline_load(font, NULL, 1, SOFEL_SCALE_NONE, 0, 0, scratch, NULL, &overlap) ==
        SOFEL_ERROR_NULL);
  CHECK(sofel_outline_load(font, NULL, 1, SOFEL_SCALE_NONE, 0, 0, scratch, cbox, NULL) ==
        SOFEL_ERROR_NULL);
  CHECK(sofel_outline_decompose(NULL, 0, 0, 1, rec_move_to, rec_line_to, rec_quad_to,
                                rec_cubic_to, rec_close, &rec) == SOFEL_ERROR_NULL);
  CHECK(sofel_outline_decompose(scratch, 0, 0, 1, NULL, rec_line_to, rec_quad_to,
                                rec_cubic_to, rec_close, &rec) == SOFEL_ERROR_NULL);
  CHECK(sofel_outline_decompose(scratch, 0, 0, 1, rec_move_to, rec_line_to, rec_quad_to,
                                rec_cubic_to, NULL, &rec) == SOFEL_ERROR_NULL);
  rec.len = 0;
  CHECK(sofel_outline_draw(font, NULL, 1, SOFEL_SCALE_NONE, 0, 0, scratch, rec_move_to, rec_line_to,
                           NULL, rec_cubic_to, rec_close, &rec, cbox,
                           &overlap) == SOFEL_ERROR_NULL);
  CHECK(sofel_outline_draw(NULL, NULL, 1, SOFEL_SCALE_NONE, 0, 0, scratch, rec_move_to, rec_line_to,
                           rec_quad_to, rec_cubic_to, rec_close, &rec, cbox,
                           &overlap) == SOFEL_ERROR_NULL);
  CHECK(sofel_outline_draw(font, NULL, 1, 7, 0, 0, scratch, rec_move_to, rec_line_to, rec_quad_to,
                           rec_cubic_to, rec_close, &rec, cbox,
                           &overlap) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(rec.len == 0);
  /* `user` may be NULL. */
  CHECK(sofel_outline_draw(font, NULL, 0, SOFEL_SCALE_NONE, 0, 0, scratch, rec_move_to, rec_line_to,
                           rec_quad_to, rec_cubic_to, rec_close, NULL, cbox,
                           &overlap) == SOFEL_OK);

  sofel_outline_scratch_destroy(scratch);
  sofel_outline_scratch_destroy(NULL);
}

static void draw_shape(sofel_rasterizer_t *r) {
  sofel_rasterizer_clear(r);
  sofel_rasterizer_move_to(r, 0, 0);
  sofel_rasterizer_line_to(r, 0, 640);
  sofel_rasterizer_line_to(r, 640, 640);
  sofel_rasterizer_line_to(r, 640, 0);
  sofel_rasterizer_line_to(r, 0, 0);
  sofel_rasterizer_close(r);
  sofel_rasterizer_move_to(r, 128, 128);
  sofel_rasterizer_cubic_to(r, 128, 512, 512, 512, 512, 128);
  sofel_rasterizer_quad_to(r, 320, 0, 128, 128);
  sofel_rasterizer_close(r);
}

static void check_bitmap(int32_t s, const struct raster_case *c, const uint8_t *buf,
                         size_t len, const int32_t *box) {
  CHECK(s == c->status);
  if (s != SOFEL_OK) return;
  CHECK(len == c->len);
  CHECK(memcmp(box, c->box, 4 * sizeof(int32_t)) == 0);
  CHECK(len == 0 || memcmp(buf, c->pixels, len) == 0);
}

static void test_raster(const sofel_font_t *font) {
  size_t i;
  sofel_rasterizer_t *r = sofel_rasterizer_create();
  sofel_rasterizer_t *small = sofel_rasterizer_create_with_band_cells(3);
  sofel_outline_scratch_t *scratch = sofel_outline_scratch_create();
  static uint8_t buf[RENDER_CAPACITY];
  size_t len;
  int32_t box[4], shift[2], cbox[4], xs, ys;
  uint32_t overlap;

  for (i = 0; i < COUNT(pixel_box_cases); i++) {
    const struct pixel_box_case *c = &pixel_box_cases[i];
    CHECK(sofel_pixel_box(c->cbox[0], c->cbox[1], c->cbox[2], c->cbox[3], box, shift) ==
          SOFEL_OK);
    CHECK(memcmp(box, c->box, sizeof box) == 0);
    CHECK(memcmp(shift, c->shift, sizeof shift) == 0);
  }
  CHECK(sofel_mono_pixel_box(-32, 33, 32, 95, box, shift) == SOFEL_OK);
  CHECK(box[0] == -1 && box[1] == 2 && box[2] == 2 && box[3] == 1);
  CHECK(shift[0] == 64 && shift[1] == -64);
  CHECK(sofel_pixel_box(0, 0, 64, 64, NULL, shift) == SOFEL_ERROR_NULL);
  CHECK(sofel_pixel_box(0, 0, 64, 64, box, NULL) == SOFEL_ERROR_NULL);

  /* The rasterizer as a pen, with both band sizes. */
  for (i = 0; i < COUNT(raster_cases); i++) {
    const struct raster_case *c = &raster_cases[i];
    int32_t s;
    draw_shape(r);
    s = sofel_rasterizer_render(r, c->fill, c->overlap, c->ppem, c->ppem, buf, sizeof buf,
                                &len, box);
    check_bitmap(s, c, buf, len, box);
    draw_shape(small);
    s = sofel_rasterizer_render(small, c->fill, c->overlap, c->ppem, c->ppem, buf, sizeof buf,
                                &len, box);
    check_bitmap(s, c, buf, len, box);
  }
  draw_shape(r);
  {
    const uint8_t weights[5] = {8, 77, 86, 77, 8};
    CHECK(sofel_rasterizer_render_filtered_lcd(r, 0, SOFEL_LCD_HORIZONTAL, weights, 0, 0,
                                               buf, sizeof buf, &len, box) == SOFEL_OK);
    CHECK(len > 0 && len == (size_t)box[2] * (size_t)box[3]);
    CHECK(sofel_rasterizer_render_filtered_lcd(r, 0, SOFEL_LCD_HORIZONTAL, NULL, 0, 0,
                                               buf, sizeof buf, &len, box) == SOFEL_ERROR_INVALID_ARGUMENT);
  }
  CHECK(sofel_rasterizer_render_harmony(r, 0, 2, 0, 0, buf, sizeof buf, &len, box) ==
        SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_rasterizer_render_harmony(NULL, 0, SOFEL_LCD_VERTICAL, 0, 0, buf, sizeof buf, &len, box) ==
        SOFEL_ERROR_NULL);
  CHECK(sofel_rasterizer_render(r, 2, 0, 0, 0, buf, sizeof buf, &len, box) ==
        SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_rasterizer_render(NULL, 0, 0, 0, 0, buf, sizeof buf, &len, box) ==
        SOFEL_ERROR_NULL);
  CHECK(sofel_rasterizer_render(r, 0, 0, 0, 0, NULL, 10, &len, box) == SOFEL_ERROR_NULL);
  CHECK(sofel_rasterizer_render(r, 0, 0, 0, 0, buf, sizeof buf, NULL, box) == SOFEL_ERROR_NULL);
  CHECK(sofel_rasterizer_render(r, 0, 0, 0, 0, buf, sizeof buf, &len, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_rasterizer_clear(NULL) == SOFEL_ERROR_NULL);
  /* The pen functions ignore NULL. */
  sofel_rasterizer_move_to(NULL, 0, 0);
  sofel_rasterizer_line_to(NULL, 0, 0);
  sofel_rasterizer_quad_to(NULL, 0, 0, 0, 0);
  sofel_rasterizer_cubic_to(NULL, 0, 0, 0, 0, 0, 0);
  sofel_rasterizer_close(NULL);

  /* A too small array: the length and box are written, the array is not. */
  {
    const struct raster_case *c = &raster_cases[0];
    uint8_t tiny[8];
    memset(tiny, 0xAB, sizeof tiny);
    len = 0;
    draw_shape(r);
    CHECK(sofel_rasterizer_render(r, 0, 0, 0, 0, tiny, sizeof tiny, &len, box) ==
          SOFEL_ERROR_BUFFER_TOO_SMALL);
    CHECK(len == c->len && memcmp(box, c->box, sizeof box) == 0);
    for (i = 0; i < sizeof tiny; i++) CHECK(tiny[i] == 0xAB);
    /* NULL with capacity 0 asks for the length, like snprintf. */
    len = 0;
    CHECK(sofel_rasterizer_render(r, 0, 0, 0, 0, NULL, 0, &len, box) ==
          SOFEL_ERROR_BUFFER_TOO_SMALL);
    CHECK(len == c->len);
  }

  /* render_placed composed from load, pixel_box and decompose. */
  CHECK(sofel_outline_pixel_scale(font, 41, 41, &xs, &ys) == SOFEL_OK);
  CHECK(sofel_outline_load(font, NULL, 2, SOFEL_SCALE_FIXED, xs, ys, scratch, cbox, &overlap) ==
        SOFEL_OK);
  CHECK(overlap == 1);
  CHECK(sofel_pixel_box(cbox[0], cbox[1], cbox[2], cbox[3], box, shift) == SOFEL_OK);
  CHECK(sofel_rasterizer_clear(r) == SOFEL_OK);
  CHECK(sofel_outline_decompose(scratch, shift[0], shift[1], 4, sofel_rasterizer_move_to,
                                sofel_rasterizer_line_to, sofel_rasterizer_quad_to,
                                sofel_rasterizer_cubic_to, sofel_rasterizer_close,
                                r) == SOFEL_OK);
  {
    int32_t out[4];
    int32_t s = sofel_rasterizer_render_placed(r, box[0], box[1], (uint32_t)box[2],
                                               (uint32_t)box[3], SOFEL_FILL_EVEN_ODD, 1, 41, 41,
                                               buf, sizeof buf, &len, out);
    check_bitmap(s, &placed_case, buf, len, out);
    CHECK(sofel_rasterizer_render_placed(NULL, 0, 0, 1, 1, 0, 0, 0, 0, buf, sizeof buf, &len,
                                         out) == SOFEL_ERROR_NULL);
    CHECK(sofel_rasterizer_render_placed(r, 0, 0, 1, 1, 5, 0, 0, 0, buf, sizeof buf, &len,
                                         out) == SOFEL_ERROR_INVALID_ARGUMENT);
  }

  /* render_glyph errors. */
  {
    uint8_t tiny[4];
    memset(tiny, 0xAB, sizeof tiny);
    len = 0;
    CHECK(sofel_render_glyph(font, NULL, 1, 16, scratch, r, tiny, sizeof tiny, &len, box) ==
          SOFEL_ERROR_BUFFER_TOO_SMALL);
    CHECK(len == render_cases[2].len);
    for (i = 0; i < sizeof tiny; i++) CHECK(tiny[i] == 0xAB);
    CHECK(sofel_render_glyph(NULL, NULL, 1, 16, scratch, r, buf, sizeof buf, &len, box) ==
          SOFEL_ERROR_NULL);
    CHECK(sofel_render_glyph(font, NULL, 1, 16, NULL, r, buf, sizeof buf, &len, box) ==
          SOFEL_ERROR_NULL);
    CHECK(sofel_render_glyph(font, NULL, 1, 16, scratch, NULL, buf, sizeof buf, &len, box) ==
          SOFEL_ERROR_NULL);
    CHECK(sofel_render_glyph(font, NULL, 1, 16, scratch, r, buf, sizeof buf, NULL, box) ==
          SOFEL_ERROR_NULL);
    CHECK(sofel_render_glyph(font, NULL, 1, 16, scratch, r, buf, sizeof buf, &len, NULL) ==
          SOFEL_ERROR_NULL);
    CHECK(sofel_render_glyph(font, NULL, FIXTURE_NUM_GLYPHS, 16, scratch, r, buf, sizeof buf, &len,
                             box) == SOFEL_ERROR_INVALID_GLYPH);
  }

  sofel_outline_scratch_destroy(scratch);
  sofel_rasterizer_destroy(small);
  sofel_rasterizer_destroy(r);
  sofel_rasterizer_destroy(NULL);
}

/* Collects the diagnostics as the fixture writes them. */
struct seen {
  size_t count;
  const struct diagnostic_case *expected;
  size_t expected_count;
  int bad;
};

struct detail_text {
  char text[512];
  size_t len;
};

static void on_detail(void *user, const char *name, int32_t kind, int64_t value) {
  struct detail_text *d = user;
  int n = snprintf(d->text + d->len, sizeof d->text - d->len, "%s:%d:%lld;", name, (int)kind,
                   (long long)value);
  if (n > 0) d->len += (size_t)n;
}

static void on_diagnostic(void *user, const sofel_diagnostic_t *diagnostic) {
  struct seen *s = user;
  const struct diagnostic_case *c;
  uint32_t font = 0, table = 0, offset = 0;
  int32_t severity = -1, fs;
  char text[256];
  size_t len = 0;
  struct detail_text details;

  if (s->count >= s->expected_count) {
    s->bad++;
    s->count++;
    return;
  }
  c = &s->expected[s->count++];
  fs = sofel_diagnostic_font(diagnostic, &font);
  s->bad += c->font < 0 ? fs != SOFEL_ERROR_NOT_FOUND : fs != SOFEL_OK || font != c->font;
  s->bad += sofel_diagnostic_table(diagnostic, &table) != SOFEL_OK || table != c->table;
  s->bad += sofel_diagnostic_offset(diagnostic, &offset) != SOFEL_OK || offset != c->offset;
  s->bad += sofel_diagnostic_severity(diagnostic, &severity) != SOFEL_OK ||
            severity != c->severity;
  s->bad += sofel_diagnostic_kind(diagnostic, text, sizeof text, &len) != SOFEL_OK ||
            strcmp(text, c->kind) != 0 || len != strlen(c->kind);
  s->bad += sofel_diagnostic_message(diagnostic, text, sizeof text, &len) != SOFEL_OK ||
            strcmp(text, c->message) != 0 || len != strlen(c->message);
  details.len = 0;
  details.text[0] = '\0';
  s->bad += sofel_diagnostic_details(diagnostic, on_detail, &details) != SOFEL_OK ||
            strcmp(details.text, c->details) != 0;

  /* Too small a buffer: the length is given, nothing is written. */
  memset(text, 'x', 4);
  len = 0;
  s->bad += sofel_diagnostic_kind(diagnostic, text, 3, &len) != SOFEL_ERROR_BUFFER_TOO_SMALL ||
            len != strlen(c->kind) || memcmp(text, "xxxx", 4) != 0;
  s->bad += sofel_diagnostic_kind(diagnostic, NULL, 0, &len) != SOFEL_ERROR_BUFFER_TOO_SMALL;
  s->bad += sofel_diagnostic_kind(diagnostic, text, strlen(c->kind), &len) !=
            SOFEL_ERROR_BUFFER_TOO_SMALL;
  s->bad += sofel_diagnostic_message(diagnostic, NULL, 0, &len) !=
                SOFEL_ERROR_BUFFER_TOO_SMALL ||
            len != strlen(c->message);
  /* NULL arguments. */
  s->bad += sofel_diagnostic_font(NULL, &font) != SOFEL_ERROR_NULL;
  s->bad += sofel_diagnostic_table(diagnostic, NULL) != SOFEL_ERROR_NULL;
  s->bad += sofel_diagnostic_offset(NULL, &offset) != SOFEL_ERROR_NULL;
  s->bad += sofel_diagnostic_severity(diagnostic, NULL) != SOFEL_ERROR_NULL;
  s->bad += sofel_diagnostic_kind(diagnostic, NULL, 10, &len) != SOFEL_ERROR_NULL;
  s->bad += sofel_diagnostic_message(diagnostic, text, sizeof text, NULL) != SOFEL_ERROR_NULL;
  s->bad += sofel_diagnostic_details(diagnostic, NULL, NULL) != SOFEL_ERROR_NULL;
  s->bad += sofel_diagnostic_details(NULL, on_detail, &details) != SOFEL_ERROR_NULL;
}

static void check_validate(const uint8_t *data, size_t len,
                           const struct diagnostic_case *expected, size_t count) {
  struct seen s;
  s.count = 0;
  s.expected = expected;
  s.expected_count = count;
  s.bad = 0;
  CHECK(sofel_validate(data, len, on_diagnostic, &s) == SOFEL_OK);
  CHECK(s.count == count);
  CHECK(s.bad == 0);
}

static void count_diagnostic(void *user, const sofel_diagnostic_t *diagnostic) {
  (void)diagnostic;
  (*(size_t *)user)++;
}

static void test_validate(void) {
  size_t n = 0;
  check_validate(invalid_data, sizeof invalid_data, invalid_diagnostics,
                 COUNT(invalid_diagnostics));
  check_validate(ttc_data, sizeof ttc_data, ttc_diagnostics, COUNT(ttc_diagnostics));
  CHECK(sofel_validate(not_font_data, sizeof not_font_data, count_diagnostic, &n) ==
        SOFEL_ERROR_INVALID_FONT);
  CHECK(n == 0);
  CHECK(sofel_validate(NULL, 10, count_diagnostic, &n) == SOFEL_ERROR_NULL);
  CHECK(sofel_validate(font_data, sizeof font_data, NULL, &n) == SOFEL_ERROR_NULL);
}

#if !defined(__wasi__)
struct worker {
  const sofel_font_t *font;
  const sofel_shape_plan_t *plan;
  int bad;
};

static void *work(void *arg) {
  struct worker *w = arg;
  sofel_outline_scratch_t *scratch = sofel_outline_scratch_create();
  sofel_rasterizer_t *raster = sofel_rasterizer_create();
  sofel_buffer_t *buffer = sofel_buffer_create();
  uint32_t gids[2];
  int round;
  for (round = 0; round < 20; round++) {
    w->bad += run_queries(w->font, scratch, raster);
    /* A plan shared between threads, a buffer per thread (ADR 0020). */
    sofel_buffer_clear(buffer);
    sofel_buffer_add_utf8(buffer, "AB", 2, 0, 2);
    w->bad += sofel_shape(w->font, NULL, NULL, w->plan, buffer) != SOFEL_OK ||
              sofel_buffer_glyphs(buffer, gids, NULL, NULL, NULL, 2) != 2 || gids[0] != 1 ||
              gids[1] != 2;
  }
  sofel_buffer_destroy(buffer);
  sofel_rasterizer_destroy(raster);
  sofel_outline_scratch_destroy(scratch);
  return NULL;
}

/* One font and one shaping plan used from several threads at once give
 * the results of one thread. */
static void test_threads(const sofel_font_t *font) {
  pthread_t threads[THREADS];
  struct worker workers[THREADS];
  sofel_segment_t seg = {SOFEL_DIRECTION_LTR, SOFEL_TAG('L', 'a', 't', 'n'), NULL};
  int32_t e = 12345;
  sofel_shape_plan_t *plan = sofel_shape_plan_create(font, NULL, &seg, NULL, 0, &e);
  int i;
  CHECK(e == SOFEL_OK);
  for (i = 0; i < THREADS; i++) {
    workers[i].font = font;
    workers[i].plan = plan;
    workers[i].bad = 0;
    CHECK(pthread_create(&threads[i], NULL, work, &workers[i]) == 0);
  }
  for (i = 0; i < THREADS; i++) {
    CHECK(pthread_join(threads[i], NULL) == 0);
    CHECK(workers[i].bad == 0);
  }
  sofel_shape_plan_destroy(plan);
}
#endif

/* The metrics of ADR 0016, unscaled (scale NULL) and scaled. */
static void check_vglyphs(const sofel_font_t *font, const sofel_scale_t *scale,
                          const struct vglyph_case *cases, size_t count) {
  size_t i;
  for (i = 0; i < count; i++) {
    const struct vglyph_case *c = &cases[i];
    int32_t v = 12345, o[2] = {0, 0}, e[4] = {0, 0, 0, 0}, s;
    if (scale) {
      CHECK(sofel_scale_h_advance(scale, font, NULL, c->glyph, &v) == SOFEL_OK && v == c->h_advance);
      CHECK(sofel_scale_v_advance(scale, font, NULL, c->glyph, &v) == SOFEL_OK && v == c->v_advance);
      CHECK(sofel_scale_v_origin(scale, font, NULL, c->glyph, o) == SOFEL_OK);
      s = sofel_scale_glyph_extents_of(scale, font, NULL, c->glyph, e);
    } else {
      CHECK(sofel_font_v_advance(font, NULL, c->glyph, &v) == SOFEL_OK && v == c->v_advance);
      CHECK(sofel_font_v_origin(font, NULL, c->glyph, o) == SOFEL_OK);
      s = sofel_font_glyph_extents(font, NULL, c->glyph, e);
    }
    CHECK(o[0] == c->origin[0] && o[1] == c->origin[1]);
    CHECK(s == c->extents_status);
    CHECK(s != SOFEL_OK || memcmp(e, c->extents, sizeof e) == 0);
  }
}

static void check_metrics(const sofel_font_t *font, const sofel_scale_t *scale,
                          const struct metric_case *cases, size_t count) {
  size_t i;
  for (i = 0; i < count; i++) {
    const struct metric_case *c = &cases[i];
    int32_t v = 12345, s;
    s = scale ? sofel_scale_metrics_position(scale, font, NULL, c->tag, &v)
              : sofel_font_metrics_position(font, NULL, c->tag, &v);
    CHECK(s == c->status);
    CHECK(s != SOFEL_OK || v == c->value);
    s = scale ? sofel_scale_metrics_position_with_fallback(scale, font, NULL, c->tag, &v)
              : sofel_font_metrics_position_with_fallback(font, NULL, c->tag, &v);
    CHECK(s == SOFEL_OK && v == c->fallback);
  }
}

/* The baselines of ADR 0017, unscaled (scale NULL) and scaled. */
static void check_baselines(const sofel_font_t *font, const sofel_scale_t *scale,
                            const struct baseline_case *cases, size_t count,
                            const struct layout_extents_case *ext_cases, size_t ext_count) {
  size_t i;
  for (i = 0; i < count; i++) {
    const struct baseline_case *c = &cases[i];
    int32_t v = 12345, s;
    s = scale ? sofel_scale_baseline(scale, font, NULL, c->tag, c->direction, c->script, &v)
              : sofel_font_baseline(font, NULL, c->tag, c->direction, c->script, &v);
    CHECK(s == c->status);
    CHECK(s != SOFEL_OK || v == c->value);
    s = scale ? sofel_scale_baseline_with_fallback(scale, font, NULL, c->tag, c->direction, c->script,
                                                   &v)
              : sofel_font_baseline_with_fallback(font, NULL, c->tag, c->direction, c->script, &v);
    CHECK(s == SOFEL_OK && v == c->fallback);
  }
  for (i = 0; i < ext_count; i++) {
    const struct layout_extents_case *c = &ext_cases[i];
    int32_t e[3] = {0, 0, 0}, found = 12345, s;
    s = scale ? sofel_scale_layout_font_extents(scale, font, NULL, c->direction, c->script,
                                                c->language, e, &found)
              : sofel_font_layout_font_extents(font, NULL, c->direction, c->script, c->language, e,
                                               &found);
    CHECK(s == SOFEL_OK && found == c->found && memcmp(e, c->extents, sizeof e) == 0);
  }
}

static void test_names(const sofel_font_t *font) {
  char buf[128];
  size_t len = 0, written = 0, total = 0, count = 0, i;
  uint16_t id = 0, ids[4], u16v = 0;
  const uint8_t *str = NULL;
  int32_t e = 12345, values[11];
  sofel_glyph_names_t *names;

  CHECK(sofel_font_postscript_name(font, NULL, buf, sizeof buf, &len) == SOFEL_OK &&
        len == strlen(FIXTURE_PS_NAME) && memcmp(buf, FIXTURE_PS_NAME, len) == 0);
  CHECK(sofel_font_postscript_name(font, NULL, buf, 2, &len) == SOFEL_ERROR_BUFFER_TOO_SMALL &&
        len == strlen(FIXTURE_PS_NAME));
  CHECK(sofel_font_name_count(font, &count) == SOFEL_OK && count == COUNT(name_list));
  for (i = 0; i < COUNT(name_list); i++) {
    CHECK(sofel_font_name_entry(font, i, &id, buf, sizeof buf, &len) == SOFEL_OK);
    CHECK(id == name_list[i].id && len == strlen(name_list[i].lang) &&
          memcmp(buf, name_list[i].lang, len) == 0);
  }
  CHECK(sofel_font_name_entry(font, COUNT(name_list), &id, buf, sizeof buf, &len) ==
        SOFEL_ERROR_NOT_FOUND);
  for (i = 0; i < COUNT(name_cases); i++) {
    const struct name_case *c = &name_cases[i];
    char t8[16];
    uint16_t t16[16];
    uint32_t t32[16];
    size_t ll = strlen(c->lang);
    CHECK(sofel_font_name_utf8(font, c->id, c->lang, ll, t8, c->cap, &written, &total) ==
          SOFEL_OK);
    CHECK(written == c->n[0] && total == c->n[1] && memcmp(t8, c->utf8, written) == 0);
    CHECK(sofel_font_name_utf16(font, c->id, c->lang, ll, t16, c->cap, &written, &total) ==
          SOFEL_OK);
    CHECK(written == c->n[2] && total == c->n[3] &&
          memcmp(t16, c->utf16, written * sizeof t16[0]) == 0);
    CHECK(sofel_font_name_utf32(font, c->id, c->lang, ll, t32, c->cap, &written, &total) ==
          SOFEL_OK);
    CHECK(written == c->n[4] && total == c->n[5] &&
          memcmp(t32, c->utf32, written * sizeof t32[0]) == 0);
  }
  CHECK(sofel_font_sfnt_name_count(font, &count) == SOFEL_OK && count == COUNT(sfnt_names));
  for (i = 0; i < COUNT(sfnt_names); i++) {
    CHECK(sofel_font_sfnt_name(font, i, ids, &str, &len) == SOFEL_OK);
    CHECK(memcmp(ids, sfnt_names[i].ids, sizeof ids) == 0 && len == sfnt_names[i].len);
  }
  CHECK(sofel_font_sfnt_name(font, count, ids, &str, &len) == SOFEL_ERROR_NOT_FOUND);

  names = sofel_glyph_names_create(font, &e);
  CHECK(e == SOFEL_OK && names != NULL);
  for (i = 0; i < COUNT(glyph_names); i++) {
    const struct glyph_name_case *c = &glyph_names[i];
    uint32_t g = 0;
    int32_t s = sofel_font_glyph_name(font, c->glyph, &str, &len);
    CHECK(c->hb ? s == SOFEL_OK && len == strlen(c->hb) && memcmp(str, c->hb, len) == 0
                : s == SOFEL_ERROR_NOT_FOUND);
    if (c->hb && names) {
      s = sofel_glyph_names_glyph_from_name(names, c->hb, strlen(c->hb), &g);
      CHECK(c->hb_back < 0 ? s == SOFEL_ERROR_NOT_FOUND : s == SOFEL_OK && g == c->hb_back);
    }
    s = sofel_font_ft_glyph_name(font, c->glyph, &str, &len);
    CHECK(c->ft ? s == SOFEL_OK && len == strlen(c->ft) && memcmp(str, c->ft, len) == 0
                : s == SOFEL_ERROR_NOT_FOUND);
    if (c->ft && names) {
      s = sofel_glyph_names_ft_glyph_from_name(names, c->ft, strlen(c->ft), &g);
      CHECK(c->ft_back < 0 ? s == SOFEL_ERROR_NOT_FOUND : s == SOFEL_OK && g == c->ft_back);
    }
    s = sofel_font_ft_v_advance(font, NULL, c->glyph, &u16v);
    CHECK(c->ft_v_advance < 0 ? s == SOFEL_ERROR_INVALID_GLYPH
                              : s == SOFEL_OK && u16v == c->ft_v_advance);
  }
  sofel_glyph_names_destroy(names);
  sofel_glyph_names_destroy(NULL);
  CHECK(sofel_glyph_names_create(NULL, &e) == NULL && e == SOFEL_ERROR_NULL);
  CHECK(sofel_font_ft_face_metrics(font, NULL, values) == SOFEL_OK &&
        memcmp(values, ft_face_metrics, sizeof values) == 0);
  CHECK(sofel_font_ft_face_metrics(font, NULL, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_name_utf8(font, 1, NULL, 2, buf, sizeof buf, &written, &total) ==
        SOFEL_ERROR_NULL);
  CHECK(sofel_font_glyph_name(font, 1, NULL, &len) == SOFEL_ERROR_NULL);
}

/* The boxes and names of a CFF font (ADR 0021). */
static void test_cff(void) {
  int32_t e = 12345;
  size_t i, len = 0;
  const uint8_t *str = NULL;
  sofel_glyph_names_t *names;
  sofel_scale_t *scale;
  sofel_font_t *font = sofel_font_create(cff_font_data, sizeof cff_font_data, 0, &e);
  CHECK(e == SOFEL_OK && font != NULL);
  if (font == NULL) return;
  check_vglyphs(font, NULL, cff_unscaled_glyphs, COUNT(cff_unscaled_glyphs));
  check_metrics(font, NULL, cff_unscaled_metrics, COUNT(cff_unscaled_metrics));
  scale = sofel_scale_create(font, SCALE_X, SCALE_Y, &e);
  CHECK(e == SOFEL_OK && scale != NULL);
  if (scale != NULL) {
    check_vglyphs(font, scale, cff_scaled_glyphs, COUNT(cff_scaled_glyphs));
    check_metrics(font, scale, cff_scaled_metrics, COUNT(cff_scaled_metrics));
  }
  sofel_scale_destroy(scale);
  names = sofel_glyph_names_create(font, &e);
  CHECK(e == SOFEL_OK && names != NULL);
  for (i = 0; i < COUNT(cff_names); i++) {
    const struct cff_name_case *c = &cff_names[i];
    uint32_t g = 0;
    int32_t s = sofel_font_glyph_name(font, c->glyph, &str, &len);
    CHECK(c->hb ? s == SOFEL_OK && len == strlen(c->hb) && memcmp(str, c->hb, len) == 0
                : s == SOFEL_ERROR_NOT_FOUND);
    if (c->hb && names) {
      s = sofel_glyph_names_glyph_from_name(names, c->hb, strlen(c->hb), &g);
      CHECK(c->back < 0 ? s == SOFEL_ERROR_NOT_FOUND : s == SOFEL_OK && g == c->back);
    }
  }
  sofel_glyph_names_destroy(names);
  sofel_font_destroy(font);
}

/* CID queries, font information and CFF outlines (ADR 0021). */
static void test_cff_info(void) {
  int32_t e = 12345, v = -1, sup = 0, cbox[4] = {0, 0, 0, 0};
  uint32_t cid = 99, present = 0, overlap = 9;
  uint16_t fs = 9;
  int64_t values[4] = {0, 0, 0, 0};
  size_t offsets[5] = {0, 0, 0, 0, 0}, len = 0;
  char reg[16], ord[16], small[3], strings[32];
  sofel_outline_scratch_t *scratch = sofel_outline_scratch_create();
  sofel_font_t *cff = sofel_font_create(cff_font_data, sizeof cff_font_data, 0, &e);
  sofel_font_t *cidf = sofel_font_create(cid_font_data, sizeof cid_font_data, 0, &e);
  CHECK(cff != NULL && cidf != NULL && scratch != NULL);
  if (cff == NULL || cidf == NULL || scratch == NULL) return;
  CHECK(sofel_font_is_cff(cff) == 1 && sofel_font_is_cff2(cff) == 0 && sofel_font_is_cff(NULL) == 0);
  CHECK(sofel_font_is_cid_keyed(cff, &v) == SOFEL_OK && v == 0);
  CHECK(sofel_font_cid(cff, 1, &cid) == SOFEL_ERROR_INVALID_ARGUMENT && cid == 0);
  CHECK(sofel_font_ros(cff, reg, sizeof reg, ord, sizeof ord, &sup, &present) ==
        SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_font_is_cid_keyed(cidf, &v) == SOFEL_OK && v == 1);
  CHECK(sofel_font_cid(cidf, 1, &cid) == SOFEL_OK && cid == 100);
  CHECK(sofel_font_cid(cidf, 3, &cid) == SOFEL_ERROR_INVALID_ARGUMENT && cid == 0);
  CHECK(sofel_font_ros(cidf, reg, sizeof reg, ord, sizeof ord, &sup, &present) == SOFEL_OK);
  CHECK(strcmp(reg, "Adobe") == 0 && strcmp(ord, "Japan1") == 0 && sup == 6 && present == 3);
  CHECK(sofel_font_ros(cidf, small, sizeof small, ord, sizeof ord, &sup, &present) ==
        SOFEL_ERROR_BUFFER_TOO_SMALL);
  CHECK(sofel_font_ps_font_info(cidf, values, strings, 2, offsets, &len) ==
        SOFEL_ERROR_BUFFER_TOO_SMALL && len == 5);
  CHECK(sofel_font_ps_font_info(cidf, values, strings, sizeof strings, offsets, &len) == SOFEL_OK);
  CHECK(offsets[0] == (size_t)-1 && offsets[1] == 0 && strcmp(strings, "Note") == 0);
  CHECK(values[0] == 0 && values[1] == 0 && values[2] == -100 && values[3] == 50);
  CHECK(sofel_font_ps_fs_type(cidf, &fs) == SOFEL_OK && fs == 0);
  /* Glyph 1 of the CFF font: rmoveto 10 -20, rlineto 300 700. */
  CHECK(sofel_outline_load(cff, NULL, 1, SOFEL_SCALE_NONE, 0, 0, scratch, cbox, &overlap) == SOFEL_OK);
  CHECK(cbox[0] == 10 && cbox[1] == -20 && cbox[2] == 310 && cbox[3] == 680 && overlap == 0);
  sofel_outline_scratch_destroy(scratch);
  sofel_font_destroy(cff);
  sofel_font_destroy(cidf);
}

static void test_metrics(void) {
  int32_t e = 12345, ext[3] = {0, 0, 0}, box[4] = {30, 700, 401, -703}, v = 0;
  uint16_t u16v = 0;
  int64_t i64v = 0;
  size_t i;
  sofel_font_t *font = sofel_font_create(metrics_font_data, sizeof metrics_font_data, 0, &e);
  sofel_scale_t *scale;
  CHECK(e == SOFEL_OK && font != NULL);
  if (font == NULL) return;

  CHECK(sofel_font_fs_type(font, &u16v) == SOFEL_OK && u16v == FIXTURE_FS_TYPE);
  CHECK(sofel_font_hb_units_per_em(font, &u16v) == SOFEL_OK && u16v == FIXTURE_HB_UPEM);
  for (i = 0; i < COUNT(os2_cases); i++) {
    int32_t s = sofel_font_os2_field(font, os2_cases[i].field, &i64v);
    CHECK(s == os2_cases[i].status);
    CHECK(s != SOFEL_OK || i64v == os2_cases[i].value);
  }
  CHECK(sofel_font_h_extents(font, NULL, ext) == SOFEL_OK &&
        memcmp(ext, unscaled_h_extents, sizeof ext) == 0);
  CHECK(sofel_font_v_extents(font, NULL, ext) == SOFEL_OK &&
        memcmp(ext, unscaled_v_extents, sizeof ext) == 0);
  check_vglyphs(font, NULL, unscaled_glyphs, COUNT(unscaled_glyphs));
  check_metrics(font, NULL, unscaled_metrics, COUNT(unscaled_metrics));
  CHECK(sofel_font_metrics_position(font, NULL, 0x6e6f6e65u, &v) == SOFEL_ERROR_INVALID_ARGUMENT);
  check_baselines(font, NULL, unscaled_baselines, COUNT(unscaled_baselines),
                  unscaled_layout_extents, COUNT(unscaled_layout_extents));
  /* Golem's em box center: Idce, horizontal, hani. */
  CHECK(sofel_font_baseline_with_fallback(font, NULL, SOFEL_BASELINE_IDEO_EMBOX_CENTRAL,
                                          SOFEL_DIRECTION_HORIZONTAL,
                                          SOFEL_TAG('h', 'a', 'n', 'i'), &v) == SOFEL_OK);
  CHECK(sofel_font_baseline(font, NULL, SOFEL_BASELINE_ROMAN, 2, SOFEL_TAG('l', 'a', 't', 'n'), &v) ==
        SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_font_baseline(font, NULL, SOFEL_BASELINE_MATH, SOFEL_DIRECTION_HORIZONTAL,
                            SOFEL_TAG('l', 'a', 't', 'n'), &v) == SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_font_baseline(font, NULL, SOFEL_BASELINE_ROMAN, SOFEL_DIRECTION_HORIZONTAL,
                            SOFEL_TAG('l', 'a', 't', 'n'), NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_layout_font_extents(font, NULL, SOFEL_DIRECTION_VERTICAL, 0, 0, ext, NULL) ==
        SOFEL_ERROR_NULL);
  CHECK(sofel_font_layout_font_extents(font, NULL, -1, 0, 0, ext, &v) == SOFEL_ERROR_INVALID_ARGUMENT);

  scale = sofel_scale_create(font, SCALE_X, SCALE_Y, &e);
  CHECK(e == SOFEL_OK && scale != NULL);
  if (scale != NULL) {
    CHECK(sofel_scale_h_extents(scale, font, NULL, ext) == SOFEL_OK &&
          memcmp(ext, scaled_h_extents, sizeof ext) == 0);
    CHECK(sofel_scale_v_extents(scale, font, NULL, ext) == SOFEL_OK &&
          memcmp(ext, scaled_v_extents, sizeof ext) == 0);
    check_vglyphs(font, scale, scaled_glyphs, COUNT(scaled_glyphs));
    check_metrics(font, scale, scaled_metrics, COUNT(scaled_metrics));
    check_baselines(font, scale, scaled_baselines, COUNT(scaled_baselines),
                    scaled_layout_extents, COUNT(scaled_layout_extents));
    CHECK(sofel_scale_baseline(NULL, font, NULL, SOFEL_BASELINE_ROMAN, SOFEL_DIRECTION_HORIZONTAL, 0,
                               &v) == SOFEL_ERROR_NULL);
    for (i = 0; i < COUNT(em_cases); i++) {
      CHECK(sofel_scale_em_scale_x(scale, em_cases[i].v, &v) == SOFEL_OK && v == em_cases[i].x);
      CHECK(sofel_scale_em_scale_y(scale, em_cases[i].v, &v) == SOFEL_OK && v == em_cases[i].y);
    }
    CHECK(sofel_scale_glyph_extents(scale, box) == SOFEL_OK &&
          memcmp(box, scaled_box, sizeof box) == 0);
    CHECK(sofel_scale_em_scale_x(scale, 1, NULL) == SOFEL_ERROR_NULL);
    CHECK(sofel_scale_h_extents(NULL, font, NULL, ext) == SOFEL_ERROR_NULL);
    CHECK(sofel_scale_v_origin(scale, NULL, NULL, 1, ext) == SOFEL_ERROR_NULL);
    CHECK(sofel_scale_metrics_position(NULL, font, NULL, 0x68617363u, &v) == SOFEL_ERROR_NULL);
  }
  sofel_scale_destroy(scale);
  sofel_scale_destroy(NULL);
  CHECK(sofel_scale_create(NULL, 1, 1, &e) == NULL && e == SOFEL_ERROR_NULL);
  CHECK(sofel_scale_create(font, 1, 1, NULL) == NULL);

  CHECK(sofel_font_h_extents(NULL, NULL, ext) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_h_extents(font, NULL, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_v_origin(font, NULL, 1, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_fs_type(NULL, &u16v) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_os2_field(font, 0, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_metrics_position_with_fallback(font, NULL, 0x68617363u, NULL) == SOFEL_ERROR_NULL);
  test_names(font);
  sofel_font_destroy(font);
}


/* ------------------------------------------------------- color glyphs */

struct text_buf {
  char s[8192];
  size_t len;
  int level;
};

static void tline(struct text_buf *t, const char *fmt, ...) {
  va_list ap;
  int n = snprintf(t->s + t->len, sizeof(t->s) - t->len, "%*s", 2 * t->level, "");
  if (n > 0) t->len += (size_t)n;
  va_start(ap, fmt);
  n = vsnprintf(t->s + t->len, sizeof(t->s) - t->len, fmt, ap);
  va_end(ap);
  if (n > 0) t->len += (size_t)n;
  if (t->len + 1 < sizeof(t->s)) t->s[t->len++] = '\n';
  t->s[t->len] = '\0';
}

static double clean(float v) { return v == 0.0f ? 0.0 : (double)v; }

static void tstops(struct text_buf *t, uint32_t extend, const sofel_color_stop_t *stops, size_t n) {
  size_t i;
  tline(t, "colors %u", extend);
  t->level++;
  for (i = 0; i < n; i++)
    tline(t, "%.3g %u %u %u %u", (double)stops[i].offset, stops[i].color.r, stops[i].color.g,
          stops[i].color.b, stops[i].color.a);
  t->level--;
}

/* The text of sofel-paint (HarfBuzz's test-paint.c) for the commands. */
static void paint_text(const sofel_paint_commands_t *cmds, struct text_buf *t) {
  size_t i;
  t->len = 0;
  t->level = 0;
  t->s[0] = '\0';
  for (i = 0; i < cmds->count; i++) {
    const sofel_paint_command_t *c = &cmds->commands[i];
    const float *v = c->values;
    switch (c->kind) {
    case SOFEL_PAINT_PUSH_TRANSFORM:
      tline(t, "start transform %.3g %.3g %.3g %.3g %.3g %.3g", clean(v[0]), clean(v[1]),
            clean(v[2]), clean(v[3]), clean(v[4]), clean(v[5]));
      t->level++;
      break;
    case SOFEL_PAINT_POP_TRANSFORM:
      t->level--;
      tline(t, "end transform");
      break;
    case SOFEL_PAINT_PUSH_CLIP_GLYPH:
      tline(t, "start clip glyph %u", c->glyph);
      t->level++;
      break;
    case SOFEL_PAINT_PUSH_CLIP_RECTANGLE:
      tline(t, "start clip rectangle %.3g %.3g %.3g %.3g", (double)v[0], (double)v[1],
            (double)v[2], (double)v[3]);
      t->level++;
      break;
    case SOFEL_PAINT_POP_CLIP:
      t->level--;
      tline(t, "end clip");
      break;
    case SOFEL_PAINT_FILL_SOLID:
      tline(t, "solid %u %u %u %u", c->color.r, c->color.g, c->color.b, c->color.a);
      break;
    case SOFEL_PAINT_FILL_LINEAR: {
      float axis[4], mn, mx;
      sofel_color_stop_t stops[16];
      size_t n = c->n_stops < 16 ? c->n_stops : 16;
      CHECK(sofel_reduce_linear_anchors(v, axis) == SOFEL_OK);
      memcpy(stops, c->stops, n * sizeof(stops[0]));
      CHECK(sofel_normalize_color_line(stops, n, &mn, &mx) == SOFEL_OK);
      tline(t, "linear gradient");
      t->level++;
      tline(t, "p0 %.3g %.3g", (double)(axis[0] + mn * (axis[2] - axis[0])),
            (double)(axis[1] + mn * (axis[3] - axis[1])));
      tline(t, "p1 %.3g %.3g", (double)(axis[0] + mx * (axis[2] - axis[0])),
            (double)(axis[1] + mx * (axis[3] - axis[1])));
      tstops(t, c->mode, stops, n);
      t->level--;
      break;
    }
    case SOFEL_PAINT_FILL_RADIAL:
      tline(t, "radial gradient");
      t->level++;
      tline(t, "p0 %.3g %.3g radius %.3g", (double)v[0], (double)v[1], (double)v[2]);
      tline(t, "p1 %.3g %.3g radius %.3g", (double)v[3], (double)v[4], (double)v[5]);
      tstops(t, c->mode, c->stops, c->n_stops);
      t->level--;
      break;
    case SOFEL_PAINT_FILL_SWEEP:
      tline(t, "sweep gradient");
      t->level++;
      tline(t, "center %.3g %.3g", (double)v[0], (double)v[1]);
      tline(t, "angles %.3g %.3g", (double)v[2], (double)v[3]);
      tstops(t, c->mode, c->stops, c->n_stops);
      t->level--;
      break;
    case SOFEL_PAINT_FILL_GLYPH:
      tline(t, "fill glyph %u solid %u %u %u %u", c->glyph, c->color.r, c->color.g, c->color.b,
            c->color.a);
      break;
    case SOFEL_PAINT_PUSH_GROUP:
      tline(t, "push group");
      t->level++;
      break;
    case SOFEL_PAINT_POP_GROUP:
      t->level--;
      tline(t, "pop group mode %u", c->mode);
      break;
    case SOFEL_PAINT_COLOR_GLYPH:
      tline(t, "paint color glyph %u; acting as failed", c->glyph);
      break;
    default:
      tline(t, "kind %u", c->kind);
    }
  }
}

struct tile_sum {
  int tiles;
  float covered;
};

static void sum_tile(void *user, float a0, sofel_color_t c0, float a1, sofel_color_t c1) {
  struct tile_sum *s = (struct tile_sum *)user;
  (void)c0;
  (void)c1;
  s->tiles++;
  s->covered += a1 - a0;
}

/* The EBLC font of the fixture: glyph 1 a 3x2 one-bit bitmap. */
static void test_bitmap_font(void) {
  int32_t error;
  sofel_font_t *font = sofel_font_create(bitmap_font, sizeof(bitmap_font), 0, &error);
  sofel_strike_t strikes[2];
  sofel_bitmap_layout_t layout;
  uint8_t pixels[8], has;
  size_t count;
  CHECK(error == SOFEL_OK);
  CHECK(sofel_font_has_outlines(font, &has) == SOFEL_OK && has == 0);
  CHECK(sofel_font_has_bitmaps(font, &has) == SOFEL_OK && has == 1);
  CHECK(sofel_bitmap_strikes(font, strikes, 2, &count) == SOFEL_OK && count == 1);
  CHECK(strikes[0].x_ppem == 12 && strikes[0].table == 1 && strikes[0].bit_depth == 1);
  CHECK(sofel_draw_bitmap(font, 0, 1, NULL, 0, &layout) == SOFEL_OK);
  CHECK(layout.width == 3 && layout.rows == 2 && layout.pitch == 1 && layout.pixel_mode == 1);
  CHECK(sofel_draw_bitmap(font, 0, 1, pixels, 1, &layout) == SOFEL_ERROR_BUFFER_TOO_SMALL);
  CHECK(sofel_draw_bitmap(font, 0, 1, pixels, 8, &layout) == SOFEL_OK);
  CHECK(pixels[0] == 0xA0 && pixels[1] == 0x40);
  CHECK(layout.hori_bearing_x == 1 && layout.hori_bearing_y == 2 && layout.hori_advance == 4);
  CHECK(sofel_draw_bitmap(font, 0, 2, pixels, 8, &layout) == SOFEL_ERROR_NOT_FOUND);
  sofel_font_destroy(font);
}

static void test_color(void) {
  int32_t error;
  sofel_font_t *font = sofel_font_create(color_font, sizeof(color_font), 0, &error);
  struct text_buf text;
  size_t i, count;
  uint16_t u16;
  uint32_t u32;
  uint32_t glyphs[4];
  int32_t indices[4];
  sofel_color_t colors[8];
  sofel_color_t fg = {10, 20, 30, 255};
  sofel_color_stop_t stops[2];
  float s, c, mn, mx;
  struct tile_sum sum = {0, 0.0f};
  sofel_strike_t strike;
  sofel_glyph_bitmap_t bitmap;
  const uint8_t *data;
  size_t len;
  uint32_t idx, range[2];

  CHECK(error == SOFEL_OK);
  {
    uint8_t page[1024];
    size_t n = 0;
    uint32_t format;
    for (format = SOFEL_VECTOR_SVG; format <= SOFEL_VECTOR_PDF; format++) {
      CHECK(sofel_vector_page(font, NULL, format, 0, 0, 20, 30, &fg, NULL, 0, NULL, 0, &n) ==
            SOFEL_ERROR_BUFFER_TOO_SMALL);
      CHECK(n > 4 && n <= sizeof(page));
      CHECK(sofel_vector_page(font, NULL, format, 0, 0, 20, 30, &fg, NULL, 0, page, sizeof(page), &n) ==
            SOFEL_OK);
      CHECK(memcmp(page, format == SOFEL_VECTOR_SVG ? "<svg" : "%PDF", 4) == 0);
    }
  }
  for (i = 0; i < COUNT(paint_cases); i++) {
    sofel_paint_commands_t cmds;
    int32_t painted = sofel_paint_glyph(font, NULL, paint_cases[i].glyph, paint_cases[i].palette, NULL, 0,
                                        fg, 0, &cmds);
    CHECK(painted == paint_cases[i].painted);
    paint_text(&cmds, &text);
    CHECK(strcmp(text.s, paint_cases[i].text) == 0);
    sofel_paint_commands_free(&cmds);
    CHECK(cmds.commands == NULL && cmds.count == 0);
    sofel_paint_commands_free(&cmds);
  }
  {
    /* A custom palette replaces the font's. */
    sofel_paint_commands_t cmds;
    sofel_color_t custom[2] = {{1, 2, 3, 4}, {5, 6, 7, 8}};
    CHECK(sofel_paint_glyph(font, NULL, 2, 0, custom, 2, fg, 0, &cmds) == SOFEL_PAINTED_COLRV0);
    CHECK(cmds.count == 2 && cmds.commands[0].color.r == 5 && cmds.commands[0].palette_index == 1);
    CHECK(cmds.commands[1].palette_index == -1 && cmds.commands[1].color.b == 30);
    sofel_paint_commands_free(&cmds);
  }
  CHECK(sofel_paint_glyph(NULL, NULL, 2, 0, NULL, 0, fg, 0, NULL) == SOFEL_ERROR_NULL);

  CHECK(sofel_palette_count(font, &u16) == SOFEL_OK && u16 == COLOR_PALETTES);
  CHECK(sofel_palette_entry_count(font, &u16) == SOFEL_OK && u16 == COLOR_ENTRIES);
  CHECK(sofel_palette_name_id(font, 1, &u16) == SOFEL_OK && u16 == 257);
  CHECK(sofel_palette_flags(font, 1, &u32) == SOFEL_OK && u32 == 2);
  CHECK(sofel_palette_color_name_id(font, 1, &u16) == SOFEL_OK && u16 == 301);
  CHECK(sofel_palette_colors(font, 1, colors, 8, &count) == SOFEL_OK && count == 2);
  CHECK(memcmp(&colors[0], color_palette_colors[2], 4) == 0);
  CHECK(sofel_palette_colors(font, 1, colors, 1, &count) == SOFEL_ERROR_BUFFER_TOO_SMALL);
  CHECK(sofel_find_palette(font, 2, &u16) == SOFEL_OK && u16 == 1);
  CHECK(sofel_find_palette(font, 4, &u16) == SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_color_formats(font, &u32) == SOFEL_OK && u32 == COLOR_FORMATS);
  CHECK(sofel_glyph_color_format(font, 2, &u32) == SOFEL_OK && u32 == SOFEL_PAINTED_COLRV0);
  CHECK(sofel_glyph_color_format(font, 3, &u32) == SOFEL_OK && u32 == SOFEL_PAINTED_COLRV1);
  CHECK(sofel_glyph_color_format(font, 1, &u32) == SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_color_layers(font, 2, glyphs, indices, 4, &count) == SOFEL_OK && count == 2);
  CHECK(glyphs[0] == 1 && indices[0] == 1 && indices[1] == -1);
  CHECK(sofel_svg_document(font, 2, &data, &len, &idx, range) == SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_bitmap_strikes(font, NULL, 0, &count) == SOFEL_OK && count == 0);
  CHECK(sofel_choose_strike(font, 0, &strike) == SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_match_strike(font, 1024, 1024, &strike) == SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_glyph_bitmap(font, 0, 1, &bitmap) == SOFEL_ERROR_NOT_FOUND);
  {
    uint8_t has;
    sofel_bitmap_layout_t layout;
    CHECK(sofel_font_has_outlines(font, &has) == SOFEL_OK && has == 1);
    CHECK(sofel_font_has_bitmaps(font, &has) == SOFEL_OK && has == 0);
    CHECK(sofel_draw_bitmap(font, 0, 1, NULL, 0, &layout) == SOFEL_ERROR_NOT_FOUND);
  }
  test_bitmap_font();
  {
    /* Glyph 2's layers are the same square: palette entry 1, then the
       foreground on top. */
    sofel_outline_scratch_t *scratch = sofel_outline_scratch_create();
    sofel_rasterizer_t *raster = sofel_rasterizer_create();
    uint8_t px[1024];
    int32_t box[4];
    uint32_t is_color = 9;
    size_t n, mid;
    CHECK(sofel_render_color_glyph(font, NULL, 2, 10, 0, NULL, 0, fg, scratch, raster, px, sizeof px, &n,
                                   box, &is_color) == SOFEL_OK);
    CHECK(is_color == 1 && box[2] > 0 && box[3] > 0 && n == (size_t)box[2] * box[3] * 4);
    mid = ((size_t)box[3] / 2 * box[2] + box[2] / 2) * 4;
    CHECK(px[mid] == 30 && px[mid + 1] == 20 && px[mid + 2] == 10 && px[mid + 3] == 255);
    CHECK(sofel_render_color_glyph(font, NULL, 2, 10, 0, NULL, 0, fg, scratch, raster, px, 4, &n, box,
                                   &is_color) == SOFEL_ERROR_BUFFER_TOO_SMALL);
    /* Not a COLRv0 glyph: the gray outline. */
    CHECK(sofel_render_color_glyph(font, NULL, 1, 10, 0, NULL, 0, fg, scratch, raster, px, sizeof px, &n,
                                   box, &is_color) == SOFEL_OK);
    CHECK(is_color == 0 && n == (size_t)box[2] * box[3]);
    CHECK(sofel_render_color_glyph(font, NULL, 2, 10, 0, NULL, 0, fg, scratch, raster, px, sizeof px, &n,
                                   box, NULL) == SOFEL_ERROR_NULL);
    sofel_rasterizer_destroy(raster);
    sofel_outline_scratch_destroy(scratch);
  }
  {
    uint8_t img[64];
    const uint8_t cov[2] = {255, 0};
    const uint8_t cov2[1] = {255};
    int32_t box[4] = {0, 0, 0, 0};
    size_t n = 0;
    sofel_color_t red = {255, 0, 0, 255};
    sofel_color_t blue = {0, 0, 255, 255};
    CHECK(sofel_blend_layer(img, sizeof img, &n, box, 1, 2, 2, 1, cov, red) == SOFEL_OK);
    CHECK(n == 8 && box[0] == 1 && box[1] == 2 && box[2] == 2 && box[3] == 1);
    CHECK(img[0] == 0 && img[2] == 255 && img[3] == 255 && img[7] == 0);
    /* The box grows to the left and down. */
    CHECK(sofel_blend_layer(img, sizeof img, &n, box, 0, 1, 1, 1, cov2, blue) == SOFEL_OK);
    CHECK(n == 24 && box[0] == 0 && box[1] == 2 && box[2] == 3 && box[3] == 2);
    CHECK(img[4 + 2] == 255 && img[12] == 255 && img[15] == 255);
    CHECK(sofel_blend_layer(img, 24, &n, box, -5, 1, 1, 1, cov2, blue) ==
          SOFEL_ERROR_BUFFER_TOO_SMALL);
    CHECK(n == 64);
    n = 24;
    CHECK(sofel_blend_layer(img, 8, &n, box, 0, 1, 1, 1, cov2, blue) == SOFEL_ERROR_INVALID_ARGUMENT);
    CHECK(sofel_blend_layer(img, sizeof img, &n, box, 0, 1, 1, 1, NULL, blue) == SOFEL_ERROR_NULL);
  }

  CHECK(sofel_sin_cos(0.0f, &s, &c) == SOFEL_OK && s == 0.0f && c == 1.0f);
  CHECK(sofel_tan(0.0f) == 0.0f);
  memset(stops, 0, sizeof(stops));
  stops[0].offset = 2.0f;
  stops[1].offset = 1.0f;
  stops[1].color.a = 255;
  CHECK(sofel_normalize_color_line(stops, 2, &mn, &mx) == SOFEL_OK);
  CHECK(mn == 1.0f && mx == 2.0f && stops[0].offset == 0.0f && stops[0].color.a == 255);
  CHECK(sofel_sweep_gradient_tiles(stops, 2, SOFEL_EXTEND_PAD, 0.0f, 3.14159265f, sum_tile, &sum) ==
        SOFEL_OK);
  CHECK(sum.tiles >= 2 && sum.covered > 6.28f && sum.covered < 6.29f);
  CHECK(sofel_sweep_gradient_tiles(stops, 2, 7, 0.0f, 1.0f, sum_tile, &sum) ==
        SOFEL_ERROR_INVALID_ARGUMENT);
  sofel_font_destroy(font);
}

/* ------------------------------------------------------------ shaping */

/* The shaping functions of sofel.h (ADR 0020) against their declared
 * types, as check_declarations does for the others. */
static void check_shape_declarations(void) {
  sofel_scale_t *(*scale_with_ppem)(const sofel_font_t *, int32_t, int32_t, uint32_t,
                                     uint32_t, int32_t *) = sofel_scale_create_with_ppem;
  int32_t (*direction_from_string)(const char *, size_t) = sofel_direction_from_string;
  const char *(*direction_to_string)(int32_t) = sofel_direction_to_string;
  int32_t (*script_from_string)(const char *, size_t, uint32_t *) = sofel_script_from_string;
  int32_t (*script_direction)(uint32_t) = sofel_script_horizontal_direction;
  sofel_language_t *(*language_create)(const char *, size_t, int32_t *) = sofel_language_create;
  void (*language_destroy)(sofel_language_t *) = sofel_language_destroy;
  int32_t (*language_string)(const sofel_language_t *, const char **, size_t *) =
      sofel_language_string;
  int32_t (*tags_from)(uint32_t, const sofel_language_t *, uint32_t *, size_t *, uint32_t *,
                       size_t *) = sofel_ot_tags_from_script_and_language;
  int32_t (*tags_to)(uint32_t, uint32_t, uint32_t *, sofel_language_t **) =
      sofel_ot_tags_to_script_and_language;
  uint32_t (*tag_to_script)(uint32_t) = sofel_ot_tag_to_script;
  int32_t (*tag_to_language)(uint32_t, sofel_language_t **) = sofel_ot_tag_to_language;
  int32_t (*feature_parse)(const char *, size_t, sofel_feature_t *) = sofel_feature_parse;
  int32_t (*feature_format)(const sofel_feature_t *, char *, size_t, size_t *) =
      sofel_feature_format;
  int32_t (*segment_guess)(const char *, size_t, const sofel_segment_t *,
                           const sofel_language_t *, sofel_segment_t *) = sofel_segment_guess;
  sofel_shape_plan_t *(*plan_create)(const sofel_font_t *, const sofel_location_t *, const sofel_segment_t *,
                                     const sofel_feature_t *, size_t, int32_t *) =
      sofel_shape_plan_create;
  void (*plan_destroy)(sofel_shape_plan_t *) = sofel_shape_plan_destroy;
  int32_t (*plan_shaper)(const sofel_shape_plan_t *, int32_t *) = sofel_shape_plan_shaper;
  sofel_buffer_t *(*buffer_create)(void) = sofel_buffer_create;
  sofel_buffer_t *(*buffer_similar)(const sofel_buffer_t *, int32_t *) =
      sofel_buffer_create_similar;
  void (*buffer_destroy)(sofel_buffer_t *) = sofel_buffer_destroy;
  int32_t (*buffer_clear)(sofel_buffer_t *) = sofel_buffer_clear;
  int32_t (*buffer_length)(const sofel_buffer_t *, size_t *) = sofel_buffer_length;
  int32_t (*buffer_add)(sofel_buffer_t *, uint32_t, uint32_t) = sofel_buffer_add;
  int32_t (*add_utf8)(sofel_buffer_t *, const char *, size_t, size_t, size_t) =
      sofel_buffer_add_utf8;
  int32_t (*add_utf16)(sofel_buffer_t *, const uint16_t *, size_t, size_t, size_t) =
      sofel_buffer_add_utf16;
  int32_t (*add_utf32)(sofel_buffer_t *, const uint32_t *, size_t, size_t, size_t) =
      sofel_buffer_add_utf32;
  int32_t (*add_codepoints)(sofel_buffer_t *, const uint32_t *, size_t, size_t, size_t) =
      sofel_buffer_add_codepoints;
  int32_t (*add_latin1)(sofel_buffer_t *, const uint8_t *, size_t, size_t, size_t) =
      sofel_buffer_add_latin1;
  int32_t (*append)(sofel_buffer_t *, const sofel_buffer_t *, size_t, size_t) =
      sofel_buffer_append;
  int32_t (*number_clusters)(sofel_buffer_t *) = sofel_buffer_number_clusters;
  int32_t (*codepoints)(const sofel_buffer_t *, uint32_t *, uint32_t *, size_t, size_t *) =
      sofel_buffer_codepoints;
  int32_t (*context)(const sofel_buffer_t *, int32_t, uint32_t[5], size_t *) =
      sofel_buffer_context;
  int32_t (*get_flags)(const sofel_buffer_t *, uint32_t *) = sofel_buffer_get_flags;
  int32_t (*set_flags)(sofel_buffer_t *, uint32_t) = sofel_buffer_set_flags;
  int32_t (*get_level)(const sofel_buffer_t *, int32_t *) = sofel_buffer_get_cluster_level;
  int32_t (*set_level)(sofel_buffer_t *, int32_t) = sofel_buffer_set_cluster_level;
  int32_t (*get_replacement)(const sofel_buffer_t *, uint32_t *) =
      sofel_buffer_get_replacement_codepoint;
  int32_t (*set_replacement)(sofel_buffer_t *, uint32_t) = sofel_buffer_set_replacement_codepoint;
  int32_t (*get_invisible)(const sofel_buffer_t *, uint32_t *) = sofel_buffer_get_invisible_glyph;
  int32_t (*set_invisible)(sofel_buffer_t *, uint32_t) = sofel_buffer_set_invisible_glyph;
  int32_t (*get_not_found)(const sofel_buffer_t *, uint32_t *) = sofel_buffer_get_not_found_glyph;
  int32_t (*set_not_found)(sofel_buffer_t *, uint32_t) = sofel_buffer_set_not_found_glyph;
  int32_t (*get_vs)(const sofel_buffer_t *, uint32_t *) =
      sofel_buffer_get_not_found_variation_selector_glyph;
  int32_t (*set_vs)(sofel_buffer_t *, uint32_t) =
      sofel_buffer_set_not_found_variation_selector_glyph;
  int32_t (*get_random)(const sofel_buffer_t *, uint32_t *) = sofel_buffer_get_random_state;
  int32_t (*set_random)(sofel_buffer_t *, uint32_t) = sofel_buffer_set_random_state;
  int32_t (*shape)(const sofel_font_t *, const sofel_location_t *, const sofel_scale_t *, const sofel_shape_plan_t *,
                   sofel_buffer_t *) = sofel_shape;
  int32_t (*shape_fallback)(const sofel_font_t *, const sofel_location_t *, const sofel_scale_t *, const sofel_segment_t *,
                            sofel_buffer_t *) = sofel_shape_fallback;
  int64_t (*buffer_glyphs)(const sofel_buffer_t *, uint32_t *, uint32_t *, uint32_t *, int32_t *,
                           size_t) = sofel_buffer_glyphs;
  int32_t (*buffer_direction)(const sofel_buffer_t *, int32_t *) = sofel_buffer_direction;
  int32_t (*normalize)(sofel_buffer_t *) = sofel_buffer_normalize_glyphs;
  int32_t (*diff)(const sofel_buffer_t *, const sofel_buffer_t *, uint32_t, uint32_t,
                  uint32_t *) = sofel_buffer_diff;
  uint32_t (*format_from)(const char *, size_t) = sofel_serialize_format_from_string;
  const char *(*format_to)(uint32_t) = sofel_serialize_format_to_string;
  int32_t (*serialize)(const sofel_buffer_t *, size_t, size_t, char *, size_t, size_t *, size_t *,
                       const sofel_font_t *, const sofel_scale_t *, uint32_t, uint32_t) =
      sofel_buffer_serialize;
  int32_t (*deserialize_glyphs)(sofel_buffer_t *, const char *, size_t, size_t *,
                                const sofel_font_t *, const sofel_glyph_names_t *, int32_t,
                                uint32_t) = sofel_buffer_deserialize_glyphs;
  int32_t (*deserialize_unicode)(sofel_buffer_t *, const char *, size_t, size_t *, uint32_t) =
      sofel_buffer_deserialize_unicode;
  int32_t (*glyph_to_string)(const sofel_font_t *, uint32_t, char *, size_t, size_t *) =
      sofel_font_glyph_to_string;
  int32_t (*glyph_from_string)(const sofel_font_t *, const sofel_glyph_names_t *, const char *,
                               size_t, uint32_t *) = sofel_font_glyph_from_string;
  int32_t (*nominal_raw)(const sofel_font_t *, uint32_t, uint32_t *) =
      sofel_font_nominal_glyph_raw;
  int32_t (*variation_raw)(const sofel_font_t *, uint32_t, uint32_t, uint32_t *) =
      sofel_font_variation_glyph_raw;
  uint8_t (*mcc)(uint32_t) = sofel_unicode_modified_combining_class;
  uint8_t (*ignorable)(uint32_t) = sofel_unicode_is_default_ignorable;
  uint8_t (*vs)(uint32_t) = sofel_unicode_is_variation_selector;
  uint8_t (*ri)(uint32_t) = sofel_unicode_is_regional_indicator;
  uint8_t (*space)(uint32_t) = sofel_unicode_space_fallback_type;
  uint32_t (*vertical)(uint32_t) = sofel_unicode_vertical_char_for;
  const void *all[] = {
      (const void *)scale_with_ppem,
      (const void *)direction_from_string, (const void *)direction_to_string,
      (const void *)script_from_string, (const void *)script_direction,
      (const void *)language_create, (const void *)language_destroy,
      (const void *)language_string, (const void *)tags_from, (const void *)tags_to,
      (const void *)tag_to_script, (const void *)tag_to_language, (const void *)feature_parse,
      (const void *)feature_format, (const void *)segment_guess, (const void *)plan_create,
      (const void *)plan_destroy, (const void *)plan_shaper, (const void *)buffer_create,
      (const void *)buffer_similar, (const void *)buffer_destroy, (const void *)buffer_clear,
      (const void *)buffer_length, (const void *)buffer_add, (const void *)add_utf8,
      (const void *)add_utf16, (const void *)add_utf32, (const void *)add_codepoints,
      (const void *)add_latin1, (const void *)append, (const void *)number_clusters,
      (const void *)codepoints, (const void *)context, (const void *)get_flags,
      (const void *)set_flags, (const void *)get_level, (const void *)set_level,
      (const void *)get_replacement, (const void *)set_replacement, (const void *)get_invisible,
      (const void *)set_invisible, (const void *)get_not_found, (const void *)set_not_found,
      (const void *)get_vs, (const void *)set_vs, (const void *)get_random,
      (const void *)set_random, (const void *)shape, (const void *)shape_fallback,
      (const void *)buffer_glyphs, (const void *)buffer_direction, (const void *)normalize,
      (const void *)diff, (const void *)format_from, (const void *)format_to,
      (const void *)serialize, (const void *)deserialize_glyphs,
      (const void *)deserialize_unicode, (const void *)glyph_to_string,
      (const void *)glyph_from_string, (const void *)nominal_raw, (const void *)variation_raw,
      (const void *)mcc, (const void *)ignorable, (const void *)vs, (const void *)ri,
      (const void *)space, (const void *)vertical,
  };
  size_t i;
  for (i = 0; i < COUNT(all); i++) CHECK(all[i] != NULL);
}

static int same_string(const char *s, size_t len, const char *expected) {
  return s != NULL && len == strlen(expected) && memcmp(s, expected, len) == 0;
}

/* Directions, scripts, languages, OpenType tags, features, and
 * HarfBuzz's own character properties. */
static void test_shape_values(void) {
  uint32_t script = 7, tags[4], ltags[4];
  size_t n = 7, ln = 7;
  int32_t e = 12345;
  const char *s = NULL;
  char text[32];
  sofel_language_t *ja, *en, *l = NULL;
  sofel_feature_t f;

  CHECK(sofel_direction_from_string("RTL", 3) == SOFEL_DIRECTION_RTL);
  CHECK(sofel_direction_from_string("ttb", 3) == SOFEL_DIRECTION_TTB);
  CHECK(sofel_direction_from_string("x", 1) == 0);
  CHECK(sofel_direction_from_string(NULL, 0) == 0);
  CHECK(strcmp(sofel_direction_to_string(SOFEL_DIRECTION_BTT), "btt") == 0);
  CHECK(sofel_direction_to_string(0) == NULL);
  CHECK(SOFEL_DIRECTION_IS_VALID(SOFEL_DIRECTION_LTR) && !SOFEL_DIRECTION_IS_VALID(0));
  CHECK(SOFEL_DIRECTION_IS_HORIZONTAL(SOFEL_DIRECTION_RTL));
  CHECK(SOFEL_DIRECTION_IS_VERTICAL(SOFEL_DIRECTION_BTT));
  CHECK(SOFEL_DIRECTION_IS_FORWARD(SOFEL_DIRECTION_TTB));
  CHECK(SOFEL_DIRECTION_IS_BACKWARD(SOFEL_DIRECTION_RTL));
  CHECK(SOFEL_DIRECTION_REVERSE(SOFEL_DIRECTION_LTR) == SOFEL_DIRECTION_RTL);
  CHECK(SOFEL_DIRECTION_REVERSE(SOFEL_DIRECTION_BTT) == SOFEL_DIRECTION_TTB);

  CHECK(sofel_script_from_string("arab", 4, &script) == SOFEL_OK);
  CHECK(script == SOFEL_TAG('A', 'r', 'a', 'b'));
  CHECK(sofel_script_from_string("Latin", 5, &script) == SOFEL_OK);
  CHECK(script == SOFEL_TAG('L', 'a', 't', 'i'));
  CHECK(sofel_script_from_string("ab", 2, &script) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_script_from_string(NULL, 4, &script) == SOFEL_ERROR_NULL);
  CHECK(script == SOFEL_TAG('L', 'a', 't', 'i'));
  CHECK(sofel_script_horizontal_direction(SOFEL_TAG('A', 'r', 'a', 'b')) == SOFEL_DIRECTION_RTL);
  CHECK(sofel_script_horizontal_direction(SOFEL_TAG('L', 'a', 't', 'n')) == SOFEL_DIRECTION_LTR);
  CHECK(sofel_script_horizontal_direction(SOFEL_TAG('R', 'u', 'n', 'r')) == 0);
  CHECK(sofel_script_horizontal_direction(0) == SOFEL_DIRECTION_LTR);

  ja = sofel_language_create("ja_JP.UTF-8", 11, &e);
  CHECK(e == SOFEL_OK && ja != NULL);
  CHECK(sofel_language_string(ja, &s, &n) == SOFEL_OK && same_string(s, n, "ja-jp"));
  CHECK(sofel_language_create("", 0, &e) == NULL && e == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_language_create(NULL, 2, &e) == NULL && e == SOFEL_ERROR_NULL);
  CHECK(sofel_language_create("en", 2, NULL) == NULL);
  en = sofel_language_create("en", 2, &e);

  n = 4;
  ln = 4;
  CHECK(sofel_ot_tags_from_script_and_language(SOFEL_TAG('H', 'a', 'n', 'i'), ja, tags, &n,
                                               ltags, &ln) == SOFEL_OK);
  CHECK(n == 1 && tags[0] == SOFEL_TAG('h', 'a', 'n', 'i'));
  CHECK(ln == 1 && ltags[0] == SOFEL_TAG('J', 'A', 'N', ' '));
  n = 0;
  ln = SOFEL_OT_MAX_TAGS_PER_LANGUAGE;
  CHECK(sofel_ot_tags_from_script_and_language(0, en, NULL, &n, ltags, &ln) == SOFEL_OK);
  CHECK(n == 0 && ln == 1 && ltags[0] == SOFEL_TAG('E', 'N', 'G', ' '));
  ln = 0;
  CHECK(sofel_ot_tags_from_script_and_language(1, NULL, NULL, &n, NULL, &ln) ==
        SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_ot_tags_from_script_and_language(0, NULL, NULL, NULL, NULL, &ln) ==
        SOFEL_ERROR_NULL);
  CHECK(sofel_ot_tags_to_script_and_language(SOFEL_TAG('l', 'a', 't', 'n'),
                                             SOFEL_TAG('E', 'N', 'G', ' '), &script,
                                             &l) == SOFEL_OK);
  CHECK(script == SOFEL_TAG('L', 'a', 't', 'n'));
  CHECK(sofel_language_string(l, &s, &n) == SOFEL_OK && same_string(s, n, "en"));
  sofel_language_destroy(l);
  CHECK(sofel_ot_tags_to_script_and_language(SOFEL_OT_TAG_DEFAULT_SCRIPT, 0, &script, &l) ==
        SOFEL_OK);
  CHECK(script == 0 && l == NULL);
  CHECK(sofel_ot_tag_to_script(SOFEL_TAG('k', 'a', 'n', 'a')) == SOFEL_TAG('K', 'a', 'n', 'a'));
  CHECK(sofel_ot_tag_to_script(SOFEL_OT_TAG_DEFAULT_SCRIPT) == 0);
  CHECK(sofel_ot_tag_to_language(SOFEL_TAG('J', 'A', 'N', ' '), &l) == SOFEL_OK);
  CHECK(sofel_language_string(l, &s, &n) == SOFEL_OK && same_string(s, n, "ja"));
  sofel_language_destroy(l);
  CHECK(sofel_ot_tag_to_language(SOFEL_OT_TAG_DEFAULT_LANGUAGE, &l) == SOFEL_OK && l == NULL);
  CHECK(sofel_ot_tag_to_language(0, NULL) == SOFEL_ERROR_NULL);
  sofel_language_destroy(NULL);

  CHECK(sofel_feature_parse("kern[3:5]=2", 11, &f) == SOFEL_OK);
  CHECK(f.tag == SOFEL_TAG('k', 'e', 'r', 'n') && f.value == 2 && f.start == 3 && f.end == 5);
  CHECK(sofel_feature_format(&f, text, sizeof text, &n) == SOFEL_OK);
  CHECK(n == 11 && strcmp(text, "kern[3:5]=2") == 0);
  CHECK(sofel_feature_parse("-liga", 5, &f) == SOFEL_OK);
  CHECK(f.value == 0 && f.start == SOFEL_FEATURE_GLOBAL_START && f.end == SOFEL_FEATURE_GLOBAL_END);
  memset(text, 'x', sizeof text);
  CHECK(sofel_feature_format(&f, text, 5, &n) == SOFEL_ERROR_BUFFER_TOO_SMALL);
  CHECK(n == 5 && text[0] == 'x');
  CHECK(sofel_feature_format(&f, text, 6, &n) == SOFEL_OK && strcmp(text, "-liga") == 0);
  CHECK(sofel_feature_parse("kern=", 5, &f) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_feature_parse("kern", 4, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_feature_format(NULL, text, sizeof text, &n) == SOFEL_ERROR_NULL);

  CHECK(sofel_unicode_modified_combining_class(0x0F39) == 127);
  CHECK(sofel_unicode_modified_combining_class(0x0301) == 230);
  CHECK(sofel_unicode_is_default_ignorable(0x200B) == 1);
  CHECK(sofel_unicode_is_default_ignorable(0x115F) == 0);
  CHECK(sofel_unicode_is_variation_selector(0xFE0F) == 1);
  CHECK(sofel_unicode_is_variation_selector(0x180B) == 0);
  CHECK(sofel_unicode_is_regional_indicator(0x1F1E6) == 1);
  CHECK(sofel_unicode_space_fallback_type(0x2003) == SOFEL_SPACE_EM);
  CHECK(sofel_unicode_space_fallback_type(0x0020) == SOFEL_SPACE_SPACE);
  CHECK(sofel_unicode_space_fallback_type(0x0041) == SOFEL_SPACE_NOT_SPACE);
  CHECK(sofel_unicode_vertical_char_for(0x3001) == 0xFE11);
  CHECK(sofel_unicode_vertical_char_for(0x0041) == 0x0041);

  sofel_language_destroy(en);
  sofel_language_destroy(ja);
}

/* Segment properties, buffers of characters, their settings, and their
 * serialization. */
static void test_shape_buffers(void) {
  sofel_buffer_t *b = sofel_buffer_create(), *c;
  sofel_segment_t in = {0, 0, NULL}, out;
  sofel_language_t *ja;
  int32_t e = 12345, level = 7;
  uint32_t cps[16], cls[16], ctx[5], u32 = 7, flags = 7;
  const uint16_t utf16[] = {0x61, 0xD800, 0x62};
  const uint32_t utf32[] = {0x61, 0xD800, 0x110000};
  const uint8_t latin1[] = {0x41, 0xE9};
  size_t n = 7, consumed = 7, count = 7;
  char text[64];

  ja = sofel_language_create("ja", 2, &e);
  CHECK(sofel_segment_guess("12 abc", 6, &in, NULL, &out) == SOFEL_OK);
  CHECK(out.direction == SOFEL_DIRECTION_LTR && out.script == SOFEL_TAG('L', 'a', 't', 'n'));
  CHECK(out.language == NULL);
  CHECK(sofel_segment_guess("\330\247", 2, &in, ja, &out) == SOFEL_OK);
  CHECK(out.direction == SOFEL_DIRECTION_RTL && out.script == SOFEL_TAG('A', 'r', 'a', 'b'));
  CHECK(out.language == ja);
  in.direction = SOFEL_DIRECTION_TTB;
  CHECK(sofel_segment_guess("123", 3, &in, NULL, &out) == SOFEL_OK);
  CHECK(out.direction == SOFEL_DIRECTION_TTB && out.script == 0);
  in.script = 1;
  CHECK(sofel_segment_guess("123", 3, &in, NULL, &out) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_segment_guess("123", 3, NULL, NULL, &out) == SOFEL_ERROR_NULL);

  CHECK(b != NULL);
  CHECK(sofel_buffer_add_utf8(b, "a\377b", 3, 0, SIZE_MAX) == SOFEL_OK);
  CHECK(sofel_buffer_codepoints(b, cps, cls, 8, &n) == SOFEL_OK && n == 3);
  CHECK(cps[0] == 0x61 && cps[1] == 0xFFFD && cps[2] == 0x62);
  CHECK(cls[0] == 0 && cls[1] == 1 && cls[2] == 2);
  cps[0] = 7;
  CHECK(sofel_buffer_codepoints(b, cps, NULL, 2, &n) == SOFEL_ERROR_BUFFER_TOO_SMALL);
  CHECK(n == 3 && cps[0] == 7);
  CHECK(sofel_buffer_clear(b) == SOFEL_OK);
  CHECK(sofel_buffer_add_utf16(b, utf16, 3, 0, SIZE_MAX) == SOFEL_OK);
  CHECK(sofel_buffer_add_utf32(b, utf32, 3, 0, SIZE_MAX) == SOFEL_OK);
  CHECK(sofel_buffer_add_codepoints(b, utf32, 3, 1, 1) == SOFEL_OK);
  CHECK(sofel_buffer_add_latin1(b, latin1, 2, 0, SIZE_MAX) == SOFEL_OK);
  CHECK(sofel_buffer_add(b, 0x1F600, 99) == SOFEL_OK);
  CHECK(sofel_buffer_codepoints(b, cps, cls, 16, &n) == SOFEL_OK && n == 10);
  CHECK(cps[0] == 0x61 && cps[1] == 0xFFFD && cps[2] == 0x62 && cps[4] == 0xFFFD);
  CHECK(cps[5] == 0xFFFD && cps[6] == 0xD800 && cps[7] == 0x41 && cps[8] == 0xE9);
  CHECK(cps[9] == 0x1F600 && cls[1] == 1 && cls[6] == 1 && cls[8] == 1 && cls[9] == 99);
  CHECK(sofel_buffer_length(b, &n) == SOFEL_OK && n == 10);
  CHECK(sofel_buffer_number_clusters(b) == SOFEL_OK);
  CHECK(sofel_buffer_codepoints(b, NULL, cls, 10, &n) == SOFEL_OK && cls[9] == 9);
  CHECK(sofel_buffer_add_utf8(b, NULL, 1, 0, 1) == SOFEL_ERROR_NULL);

  /* Context: the characters around the item. */
  CHECK(sofel_buffer_clear(b) == SOFEL_OK);
  CHECK(sofel_buffer_add_utf8(b, "xyzabcd", 7, 3, 2) == SOFEL_OK);
  CHECK(sofel_buffer_context(b, 0, ctx, &n) == SOFEL_OK && n == 3);
  CHECK(ctx[0] == 'z' && ctx[1] == 'y' && ctx[2] == 'x');
  CHECK(sofel_buffer_context(b, 1, ctx, &n) == SOFEL_OK && n == 2);
  CHECK(ctx[0] == 'c' && ctx[1] == 'd');
  CHECK(sofel_buffer_context(b, 2, ctx, &n) == SOFEL_ERROR_INVALID_ARGUMENT);

  /* Settings, kept by sofel_buffer_create_similar and sofel_buffer_clear. */
  CHECK(sofel_buffer_set_flags(b, SOFEL_BUFFER_FLAG_BOT | SOFEL_BUFFER_FLAG_EOT) == SOFEL_OK);
  CHECK(sofel_buffer_set_cluster_level(b, SOFEL_CLUSTER_LEVEL_CHARACTERS) == SOFEL_OK);
  CHECK(sofel_buffer_set_cluster_level(b, 4) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_buffer_set_replacement_codepoint(b, 0x3F) == SOFEL_OK);
  CHECK(sofel_buffer_set_invisible_glyph(b, 3) == SOFEL_OK);
  CHECK(sofel_buffer_set_not_found_glyph(b, 4) == SOFEL_OK);
  CHECK(sofel_buffer_set_not_found_variation_selector_glyph(b, 5) == SOFEL_OK);
  CHECK(sofel_buffer_set_random_state(b, 6) == SOFEL_OK);
  c = sofel_buffer_create_similar(b, &e);
  CHECK(e == SOFEL_OK && c != NULL);
  CHECK(sofel_buffer_get_flags(c, &flags) == SOFEL_OK && flags == 3);
  CHECK(sofel_buffer_get_cluster_level(c, &level) == SOFEL_OK &&
        level == SOFEL_CLUSTER_LEVEL_CHARACTERS);
  CHECK(sofel_buffer_get_replacement_codepoint(c, &u32) == SOFEL_OK && u32 == 0x3F);
  CHECK(sofel_buffer_get_invisible_glyph(c, &u32) == SOFEL_OK && u32 == 3);
  CHECK(sofel_buffer_get_not_found_glyph(c, &u32) == SOFEL_OK && u32 == 4);
  CHECK(sofel_buffer_get_not_found_variation_selector_glyph(c, &u32) == SOFEL_OK && u32 == 5);
  CHECK(sofel_buffer_get_random_state(c, &u32) == SOFEL_OK && u32 == 1);
  CHECK(sofel_buffer_get_random_state(b, &u32) == SOFEL_OK && u32 == 6);
  CHECK(sofel_buffer_set_not_found_variation_selector_glyph(c, UINT32_MAX) == SOFEL_OK);
  CHECK(sofel_buffer_get_not_found_variation_selector_glyph(c, &u32) == SOFEL_OK &&
        u32 == UINT32_MAX);
  CHECK(sofel_buffer_clear(b) == SOFEL_OK);
  CHECK(sofel_buffer_get_flags(b, &flags) == SOFEL_OK && flags == 3);
  CHECK(sofel_buffer_get_random_state(b, &u32) == SOFEL_OK && u32 == 1);

  /* Append the middle of another buffer, with its context. */
  CHECK(sofel_buffer_add_utf8(c, "pqrst", 5, 0, SIZE_MAX) == SOFEL_OK);
  CHECK(sofel_buffer_append(b, c, 1, 3) == SOFEL_OK);
  CHECK(sofel_buffer_codepoints(b, cps, cls, 8, &n) == SOFEL_OK && n == 2);
  CHECK(cps[0] == 'q' && cps[1] == 'r' && cls[0] == 1);
  CHECK(sofel_buffer_context(b, 0, ctx, &n) == SOFEL_OK && n == 1 && ctx[0] == 'p');
  CHECK(sofel_buffer_append(b, b, 0, 1) == SOFEL_ERROR_INVALID_ARGUMENT);

  /* Characters serialized and read back. */
  CHECK(sofel_buffer_clear(c) == SOFEL_OK);
  CHECK(sofel_serialize_format_from_string("TEXTS", 5) == SOFEL_SERIALIZE_FORMAT_TEXT);
  CHECK(sofel_serialize_format_from_string("json", 4) == SOFEL_SERIALIZE_FORMAT_JSON);
  CHECK(sofel_serialize_format_from_string("xml", 3) == 0);
  CHECK(strcmp(sofel_serialize_format_to_string(SOFEL_SERIALIZE_FORMAT_JSON), "json") == 0);
  CHECK(sofel_serialize_format_to_string(0) == NULL);
  CHECK(sofel_buffer_serialize(c, 0, SIZE_MAX, text, sizeof text, &consumed, &count, NULL, NULL,
                               SOFEL_SERIALIZE_FORMAT_TEXT, 0) == SOFEL_OK);
  CHECK(consumed == 2 && count == 0 && strcmp(text, "!!") == 0);
  CHECK(sofel_buffer_serialize(b, 0, SIZE_MAX, text, sizeof text, &consumed, &count, NULL, NULL,
                               SOFEL_SERIALIZE_FORMAT_TEXT, 0) == SOFEL_OK);
  CHECK(count == 2 && strcmp(text, "<U+0071=1|U+0072=2>") == 0 && consumed == strlen(text));
  CHECK(sofel_buffer_serialize(b, 0, SIZE_MAX, text, sizeof text, &consumed, &count, NULL, NULL,
                               SOFEL_SERIALIZE_FORMAT_JSON, 0) == SOFEL_OK);
  CHECK(strcmp(text, "[{\"u\":113,\"cl\":1},{\"u\":114,\"cl\":2}]") == 0);
  CHECK(sofel_buffer_serialize(b, 0, SIZE_MAX, text, 12, &consumed, &count, NULL, NULL,
                               SOFEL_SERIALIZE_FORMAT_TEXT, 0) == SOFEL_OK);
  CHECK(count == 1 && strcmp(text, "<U+0071=1") == 0);
  CHECK(sofel_buffer_serialize(b, 0, SIZE_MAX, text, sizeof text, &consumed, &count, NULL, NULL,
                               1, 0) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_buffer_deserialize_unicode(c, "<U+0071=1|U+0072=2>", 19, &consumed,
                                         SOFEL_SERIALIZE_FORMAT_TEXT) == SOFEL_OK);
  CHECK(consumed == 19);
  CHECK(sofel_buffer_diff(c, b, UINT32_MAX, 0, &flags) == SOFEL_OK &&
        flags == SOFEL_DIFF_FLAG_EQUAL);
  CHECK(sofel_buffer_deserialize_unicode(c, "x", 1, &consumed, SOFEL_SERIALIZE_FORMAT_TEXT) ==
        SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(consumed == 0);
  /* An item cut short at the end is not read. */
  CHECK(sofel_buffer_deserialize_unicode(c, "<U+0073=3|U+0074", 16, &consumed,
                                         SOFEL_SERIALIZE_FORMAT_TEXT) == SOFEL_OK);
  CHECK(consumed == 16);
  CHECK(sofel_buffer_length(c, &n) == SOFEL_OK && n == 3);
  CHECK(sofel_buffer_diff(c, b, UINT32_MAX, 0, &flags) == SOFEL_OK &&
        flags == SOFEL_DIFF_FLAG_LENGTH_MISMATCH);

  sofel_buffer_destroy(c);
  sofel_buffer_destroy(b);
  sofel_buffer_destroy(NULL);
  sofel_language_destroy(ja);
}

static sofel_buffer_t *configured_buffer(void) {
  sofel_buffer_t *b = sofel_buffer_create();
  CHECK(b != NULL);
  CHECK(sofel_buffer_set_flags(b, SOFEL_BUFFER_FLAG_BOT) == SOFEL_OK);
  CHECK(sofel_buffer_set_cluster_level(b, SOFEL_CLUSTER_LEVEL_CHARACTERS) == SOFEL_OK);
  CHECK(sofel_buffer_set_replacement_codepoint(b, 63) == SOFEL_OK);
  CHECK(sofel_buffer_set_invisible_glyph(b, 3) == SOFEL_OK);
  CHECK(sofel_buffer_set_not_found_glyph(b, 4) == SOFEL_OK);
  CHECK(sofel_buffer_set_not_found_variation_selector_glyph(b, 5) == SOFEL_OK);
  CHECK(sofel_buffer_set_random_state(b, 6) == SOFEL_OK);
  return b;
}

static void check_reusable_buffer(sofel_buffer_t *b) {
  sofel_buffer_t *control = configured_buffer();
  uint32_t got, want;
  int32_t level, control_level;
  size_t n;
  CHECK(sofel_buffer_clear(control) == SOFEL_OK);
  CHECK(sofel_buffer_clear(b) == SOFEL_OK);
  CHECK(sofel_buffer_length(b, &n) == SOFEL_OK && n == 0);
  CHECK(sofel_buffer_get_flags(control, &want) == SOFEL_OK);
  CHECK(sofel_buffer_get_flags(b, &got) == SOFEL_OK && got == want);
  CHECK(sofel_buffer_get_cluster_level(control, &control_level) == SOFEL_OK);
  CHECK(sofel_buffer_get_cluster_level(b, &level) == SOFEL_OK && level == control_level);
  CHECK(sofel_buffer_get_replacement_codepoint(control, &want) == SOFEL_OK);
  CHECK(sofel_buffer_get_replacement_codepoint(b, &got) == SOFEL_OK && got == want);
  CHECK(sofel_buffer_get_invisible_glyph(control, &want) == SOFEL_OK);
  CHECK(sofel_buffer_get_invisible_glyph(b, &got) == SOFEL_OK && got == want);
  CHECK(sofel_buffer_get_not_found_glyph(control, &want) == SOFEL_OK);
  CHECK(sofel_buffer_get_not_found_glyph(b, &got) == SOFEL_OK && got == want);
  CHECK(sofel_buffer_get_not_found_variation_selector_glyph(control, &want) == SOFEL_OK);
  CHECK(sofel_buffer_get_not_found_variation_selector_glyph(b, &got) == SOFEL_OK && got == want);
  CHECK(sofel_buffer_get_random_state(control, &want) == SOFEL_OK);
  CHECK(sofel_buffer_get_random_state(b, &got) == SOFEL_OK && got == want);
  CHECK(got == 1);
  CHECK(sofel_buffer_add(b, 0x41, 0) == SOFEL_OK);
  sofel_buffer_destroy(control);
  sofel_buffer_destroy(b);
}

static void inv1_clear_after_deserialize(const sofel_font_t *font) {
  const uint32_t formats[] = {SOFEL_SERIALIZE_FORMAT_TEXT, SOFEL_SERIALIZE_FORMAT_JSON};
  const char *items[] = {"[gid1]", "[{\"g\":1}]"};
  sofel_segment_t seg = {SOFEL_DIRECTION_LTR, 0, NULL};
  for (size_t f = 0; f < 2; ++f) {
    for (int mode = 0; mode < 3; ++mode) {
      sofel_buffer_t *b = configured_buffer();
      if (mode == 2) {
        CHECK(sofel_buffer_add(b, 0x41, 0) == SOFEL_OK);
        CHECK(sofel_shape_fallback(font, NULL, NULL, &seg, b) == SOFEL_OK);
      }
      for (int i = 0; i < (mode == 1 ? 2 : 1); ++i) {
        size_t consumed;
        CHECK(sofel_buffer_deserialize_glyphs(b, items[f], strlen(items[f]), &consumed, NULL,
                                              NULL, seg.direction, formats[f]) == SOFEL_OK);
        CHECK(consumed == strlen(items[f]));
      }
      check_reusable_buffer(b);
    }
  }
}

static void inv2_clear_after_parse_error(void) {
  const struct {
    uint32_t format;
    const char *input;
    size_t consumed, glyphs;
  } cases[] = {
      {SOFEL_SERIALIZE_FORMAT_TEXT, "", 0, 0},
      {SOFEL_SERIALIZE_FORMAT_TEXT, ",", 0, 0},
      {SOFEL_SERIALIZE_FORMAT_TEXT, "[1=0|2=?]", 7, 1},
      {SOFEL_SERIALIZE_FORMAT_JSON, "", 0, 0},
      {SOFEL_SERIALIZE_FORMAT_JSON, "!", 0, 0},
      {SOFEL_SERIALIZE_FORMAT_JSON, "[{\"g\":1},?]", 9, 1},
  };
  for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); ++i) {
    sofel_buffer_t *b = configured_buffer();
    size_t consumed, n;
    CHECK(sofel_buffer_deserialize_glyphs(b, cases[i].input, strlen(cases[i].input), &consumed,
                                          NULL, NULL, SOFEL_DIRECTION_LTR, cases[i].format) ==
          SOFEL_ERROR_INVALID_ARGUMENT);
    CHECK(consumed == cases[i].consumed);
    CHECK(sofel_buffer_length(b, &n) == SOFEL_OK && n == cases[i].glyphs);
    if (n) {
      uint32_t gid;
      CHECK(sofel_buffer_glyphs(b, &gid, NULL, NULL, NULL, 1) == 1 && gid == 1);
    }
    check_reusable_buffer(b);
  }
}

/* Glyphs read from their serialization, written back and compared. */
static void test_shape_glyph_buffers(const sofel_font_t *font) {
  static const char glyphs_text[] = "[gid1=0+500|gid2=1@10,20+0]";
  sofel_buffer_t *b = sofel_buffer_create(), *u = sofel_buffer_create();
  sofel_glyph_names_t *names;
  uint32_t gids[4], cls[4], fl[4], flags = 7, g = 7;
  int32_t pos[16], dir = 0, e = 12345;
  size_t consumed = 7, count = 7, n = 7;
  char text[64];

  CHECK(sofel_buffer_deserialize_glyphs(b, glyphs_text, strlen(glyphs_text), &consumed, NULL,
                                        NULL, SOFEL_DIRECTION_RTL,
                                        SOFEL_SERIALIZE_FORMAT_TEXT) == SOFEL_OK);
  CHECK(consumed == strlen(glyphs_text));
  CHECK(sofel_buffer_glyphs(b, gids, cls, fl, pos, 4) == 2);
  CHECK(gids[0] == 1 && gids[1] == 2 && cls[0] == 0 && cls[1] == 1 && fl[0] == 0);
  CHECK(pos[0] == 500 && pos[1] == 0 && pos[4] == 0 && pos[6] == 10 && pos[7] == 20);
  CHECK(sofel_buffer_direction(b, &dir) == SOFEL_OK && dir == SOFEL_DIRECTION_RTL);
  CHECK(sofel_buffer_serialize(b, 0, SIZE_MAX, text, sizeof text, &consumed, &count, NULL, NULL,
                               SOFEL_SERIALIZE_FORMAT_TEXT, 0) == SOFEL_OK);
  CHECK(count == 2 && strcmp(text, glyphs_text) == 0);
  CHECK(sofel_buffer_serialize(b, 1, 2, text, sizeof text, &consumed, &count, NULL, NULL,
                               SOFEL_SERIALIZE_FORMAT_TEXT,
                               SOFEL_SERIALIZE_FLAG_NO_POSITIONS) == SOFEL_OK);
  CHECK(count == 1 && strcmp(text, "|gid2=1]") == 0);
  CHECK(sofel_buffer_serialize(b, 0, SIZE_MAX, text, 0, &consumed, &count, NULL, NULL,
                               SOFEL_SERIALIZE_FORMAT_TEXT, 0) == SOFEL_OK);
  CHECK(count == 0 && consumed == 0);
  CHECK(sofel_buffer_normalize_glyphs(b) == SOFEL_OK);
  CHECK(sofel_buffer_add(b, 0x41, 0) == SOFEL_ERROR_BUFFER_CONTENT);
  CHECK(sofel_buffer_set_flags(b, 0) == SOFEL_ERROR_BUFFER_CONTENT);
  CHECK(sofel_buffer_create_similar(b, &e) == NULL && e == SOFEL_ERROR_BUFFER_CONTENT);

  /* A second copy compares equal; characters compare as another kind. */
  CHECK(sofel_buffer_deserialize_glyphs(u, glyphs_text, strlen(glyphs_text), &consumed, NULL,
                                        NULL, SOFEL_DIRECTION_RTL,
                                        SOFEL_SERIALIZE_FORMAT_TEXT) == SOFEL_OK);
  CHECK(sofel_buffer_diff(u, b, UINT32_MAX, 0, &flags) == SOFEL_OK &&
        flags == SOFEL_DIFF_FLAG_EQUAL);
  CHECK(sofel_buffer_deserialize_glyphs(u, "[gid0=2+100]", 12, &consumed, NULL, NULL,
                                        SOFEL_DIRECTION_RTL,
                                        SOFEL_SERIALIZE_FORMAT_TEXT) == SOFEL_OK);
  CHECK(sofel_buffer_length(u, &n) == SOFEL_OK && n == 3);
  CHECK(sofel_buffer_diff(b, u, 2, 0, &flags) == SOFEL_OK &&
        flags == (SOFEL_DIFF_FLAG_LENGTH_MISMATCH | SOFEL_DIFF_FLAG_NOTDEF_PRESENT |
                  SOFEL_DIFF_FLAG_DOTTED_CIRCLE_PRESENT));
  CHECK(sofel_buffer_clear(u) == SOFEL_OK);
  CHECK(sofel_buffer_diff(u, b, UINT32_MAX, 0, &flags) == SOFEL_OK &&
        flags == SOFEL_DIFF_FLAG_LENGTH_MISMATCH);
  CHECK(sofel_buffer_add(u, 0x41, 0) == SOFEL_OK);
  CHECK(sofel_buffer_diff(u, b, UINT32_MAX, 0, &flags) == SOFEL_OK &&
        flags == SOFEL_DIFF_FLAG_CONTENT_TYPE_MISMATCH);
  CHECK(sofel_buffer_glyphs(u, gids, NULL, NULL, NULL, 4) == SOFEL_ERROR_BUFFER_CONTENT);
  CHECK(sofel_buffer_glyphs(NULL, gids, NULL, NULL, NULL, 4) == SOFEL_ERROR_NULL);
  CHECK(sofel_buffer_deserialize_glyphs(u, "[gid1]", 6, &consumed, NULL, NULL,
                                        SOFEL_DIRECTION_LTR, SOFEL_SERIALIZE_FORMAT_TEXT) ==
        SOFEL_ERROR_BUFFER_CONTENT);
  CHECK(sofel_buffer_deserialize_glyphs(b, "[gid1=x]", 8, &consumed, NULL, NULL,
                                        SOFEL_DIRECTION_LTR, SOFEL_SERIALIZE_FORMAT_TEXT) ==
        SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_buffer_deserialize_glyphs(b, "[gid1]", 6, &consumed, NULL, NULL, 0,
                                        SOFEL_SERIALIZE_FORMAT_TEXT) ==
        SOFEL_ERROR_INVALID_ARGUMENT);

  /* Glyph strings: names of the font, ids, and uni<hex> through the cmap. */
  CHECK(sofel_font_glyph_to_string(NULL, 5, text, sizeof text, &n) == SOFEL_OK);
  CHECK(n == 4 && strcmp(text, "gid5") == 0);
  CHECK(sofel_font_glyph_to_string(NULL, 5, text, 4, &n) == SOFEL_ERROR_BUFFER_TOO_SMALL);
  CHECK(sofel_font_glyph_from_string(NULL, NULL, "gid7", 4, &g) == SOFEL_OK && g == 7);
  CHECK(sofel_font_glyph_from_string(NULL, NULL, "12", 2, &g) == SOFEL_OK && g == 12);
  CHECK(sofel_font_glyph_from_string(font, NULL, "uni0042", 7, &g) == SOFEL_OK && g == 2);
  CHECK(sofel_font_glyph_from_string(NULL, NULL, "uni0042", 7, &g) == SOFEL_ERROR_NOT_FOUND);
  names = sofel_glyph_names_create(font, &e);
  if (names != NULL) {
    CHECK(sofel_font_glyph_from_string(font, names, "foo", 3, &g) == SOFEL_ERROR_NOT_FOUND);
    sofel_glyph_names_destroy(names);
  }
  CHECK(sofel_font_nominal_glyph_raw(font, 0x41, &g) == SOFEL_OK && g == 1);
  CHECK(sofel_font_nominal_glyph_raw(font, 0x5A, &g) == SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_font_variation_glyph_raw(font, 0x41, 0xFE00, &g) == SOFEL_OK && g == 3);

  sofel_buffer_destroy(u);
  sofel_buffer_destroy(b);
}

/* Shaping the fixture font, which has no layout tables: plans, the
 * buffer's two kinds of content, and the fallback shaper. */
static void test_shape_fixture_font(const sofel_font_t *font) {
  sofel_buffer_t *b = sofel_buffer_create();
  sofel_segment_t seg = {SOFEL_DIRECTION_LTR, 0, NULL}, guessed;
  sofel_shape_plan_t *plan;
  sofel_feature_t f = {SOFEL_TAG('k', 'e', 'r', 'n'), 0, SOFEL_FEATURE_GLOBAL_START,
                       SOFEL_FEATURE_GLOBAL_END};
  uint32_t gids[4] = {7, 7, 7, 7};
  int32_t pos[16], shaper = -1, e = 12345;
  size_t n = 7;

  CHECK(sofel_buffer_add_utf8(b, "AB", 2, 0, 2) == SOFEL_OK);
  CHECK(sofel_segment_guess("AB", 2, &seg, NULL, &guessed) == SOFEL_OK);
  plan = sofel_shape_plan_create(font, NULL, &guessed, &f, 1, &e);
  CHECK(e == SOFEL_OK && plan != NULL);
  CHECK(sofel_shape_plan_shaper(plan, &shaper) == SOFEL_OK && shaper == SOFEL_SHAPER_DEFAULT);
  CHECK(sofel_shape(font, NULL, NULL, plan, b) == SOFEL_OK);
  CHECK(sofel_buffer_glyphs(b, gids, NULL, NULL, pos, 4) == 2);
  CHECK(gids[0] == 1 && gids[1] == 2 && pos[0] == (int32_t)advances[1] &&
        pos[4] == (int32_t)advances[2]);
  CHECK(sofel_shape(font, NULL, NULL, plan, b) == SOFEL_ERROR_BUFFER_CONTENT);
  CHECK(sofel_buffer_clear(b) == SOFEL_OK);
  CHECK(sofel_buffer_length(b, &n) == SOFEL_OK && n == 0);
  CHECK(sofel_buffer_add_utf8(b, "BA", 2, 0, 2) == SOFEL_OK);
  seg.direction = SOFEL_DIRECTION_RTL;
  CHECK(sofel_shape_fallback(font, NULL, NULL, &seg, b) == SOFEL_OK);
  CHECK(sofel_buffer_glyphs(b, gids, NULL, NULL, pos, 4) == 2);
  CHECK(gids[0] == 1 && gids[1] == 2 && pos[0] == (int32_t)advances[1]);
  CHECK(sofel_shape(NULL, NULL, NULL, plan, b) == SOFEL_ERROR_NULL);
  sofel_shape_plan_destroy(plan);

  seg.direction = 0;
  CHECK(sofel_shape_plan_create(font, NULL, &seg, NULL, 0, &e) == NULL &&
        e == SOFEL_ERROR_INVALID_ARGUMENT);
  seg.direction = SOFEL_DIRECTION_RTL;
  seg.script = SOFEL_TAG('A', 'r', 'a', 'b');
  plan = sofel_shape_plan_create(font, NULL, &seg, NULL, 0, &e);
  CHECK(sofel_shape_plan_shaper(plan, &shaper) == SOFEL_OK && shaper == SOFEL_SHAPER_ARABIC);
  sofel_shape_plan_destroy(plan);
  CHECK(sofel_shape_plan_create(NULL, NULL, &seg, NULL, 0, &e) == NULL && e == SOFEL_ERROR_NULL);
  sofel_shape_plan_destroy(NULL);
  sofel_buffer_destroy(b);
}

static void check_ppem_shape(const sofel_font_t *font, const sofel_scale_t *scale,
                             const sofel_shape_plan_t *plan, const sofel_segment_t *seg,
                             const int32_t normal[4], const int32_t fallback[4]) {
  sofel_buffer_t *buffer = sofel_buffer_create();
  uint32_t glyphs[1];
  int32_t positions[4];
  size_t i;
  CHECK(buffer != NULL);
  CHECK(sofel_buffer_add_utf8(buffer, "A", 1, 0, 1) == SOFEL_OK);
  CHECK(sofel_shape(font, NULL, scale, plan, buffer) == SOFEL_OK);
  CHECK(sofel_buffer_glyphs(buffer, glyphs, NULL, NULL, positions, 1) == 1);
  CHECK(glyphs[0] == 1);
  for (i = 0; i < 4; i++) CHECK(positions[i] == normal[i]);
  CHECK(sofel_buffer_clear(buffer) == SOFEL_OK);
  CHECK(sofel_buffer_add_utf8(buffer, "A", 1, 0, 1) == SOFEL_OK);
  CHECK(sofel_shape_fallback(font, NULL, scale, seg, buffer) == SOFEL_OK);
  CHECK(sofel_buffer_glyphs(buffer, glyphs, NULL, NULL, positions, 1) == 1);
  CHECK(glyphs[0] == 1);
  for (i = 0; i < 4; i++) CHECK(positions[i] == fallback[i]);
  sofel_buffer_destroy(buffer);
}

static void inv1_legacy_scale(const sofel_font_t *font, const sofel_shape_plan_t *plan,
                               const sofel_segment_t *seg) {
  int32_t error = -1;
  sofel_scale_t *legacy = sofel_scale_create(font, 1000, 2000, &error);
  CHECK(legacy != NULL && error == SOFEL_OK);
  if (legacy != NULL) {
    check_ppem_shape(font, legacy, plan, seg, ppem_cases[0].normal, ppem_cases[0].fallback);
    sofel_scale_destroy(legacy);
  }
}

static void inv2_axis_ppem(const sofel_font_t *font, const sofel_shape_plan_t *plan,
                            const sofel_segment_t *seg) {
  int32_t error = -1;
  size_t i;
  for (i = 0; i < COUNT(ppem_cases); i++) {
    const struct ppem_case *c = &ppem_cases[i];
    sofel_scale_t *scale = sofel_scale_create_with_ppem(font, 1000, 2000, c->x, c->y, &error);
    CHECK(scale != NULL && error == SOFEL_OK);
    if (scale != NULL) {
      check_ppem_shape(font, scale, plan, seg, c->normal, c->fallback);
      sofel_scale_destroy(scale);
    }
  }
  CHECK(sofel_scale_create_with_ppem(NULL, 1, 1, 12, 16, &error) == NULL &&
        error == SOFEL_ERROR_NULL);
  CHECK(sofel_scale_create_with_ppem(font, 1, 1, 12, 16, NULL) == NULL);
}

static void inv3_null_scale(const sofel_font_t *font, const sofel_shape_plan_t *plan,
                            const sofel_segment_t *seg) {
  check_ppem_shape(font, NULL, plan, seg, ppem_null_normal, ppem_null_fallback);
}

static void test_shape_ppem(void) {
  int32_t error = -1;
  sofel_font_t *font = sofel_font_create(ppem_font_data, sizeof ppem_font_data, 0, &error);
  sofel_segment_t seg = {SOFEL_DIRECTION_LTR, SOFEL_TAG('L', 'a', 't', 'n'), NULL};
  sofel_shape_plan_t *plan;
  CHECK(font != NULL && error == SOFEL_OK);
  if (font == NULL) return;
  plan = sofel_shape_plan_create(font, NULL, &seg, NULL, 0, &error);
  CHECK(plan != NULL && error == SOFEL_OK);
  if (plan == NULL) { sofel_font_destroy(font); return; }
  inv1_legacy_scale(font, plan, &seg);
  inv2_axis_ppem(font, plan, &seg);
  inv3_null_scale(font, plan, &seg);
  sofel_shape_plan_destroy(plan);
  sofel_font_destroy(font);
}

/* Golem's golem_font_shape (modules/golem-font/cbits/golem/golem_font.c)
 * with sofel in place of HarfBuzz: UTF-8 text, LTR or TTB, the script
 * and language as strings, comma-separated features. Returns the glyph
 * count, or -(count) - 1 when the arrays of max_glyphs are too small. */
static int golem_shape(const sofel_font_t *f, const char *utf8, int len, const char *script,
                       const char *lang, int vertical, const char *features,
                       uint32_t *out_gids, uint32_t *out_clusters, uint32_t *out_flags,
                       int32_t *out_pos, int max_glyphs) {
  sofel_buffer_t *buf = sofel_buffer_create();
  sofel_segment_t in = {vertical ? SOFEL_DIRECTION_TTB : SOFEL_DIRECTION_LTR, 0, NULL}, props;
  sofel_language_t *language = NULL;
  sofel_shape_plan_t *plan;
  sofel_feature_t feats[32];
  size_t nfeat = 0;
  int32_t e = 0;
  int64_t count;
  sofel_buffer_add_utf8(buf, utf8, (size_t)len, 0, (size_t)len);
  if (script && *script) sofel_script_from_string(script, strlen(script), &in.script);
  if (lang && *lang) in.language = language = sofel_language_create(lang, strlen(lang), &e);
  sofel_segment_guess(utf8, (size_t)len, &in, NULL, &props);
  if (features && *features) {
    const char *p = features;
    while (*p && nfeat < 32) {
      const char *end = strchr(p, ',');
      size_t n = end ? (size_t)(end - p) : strlen(p);
      if (sofel_feature_parse(p, n, &feats[nfeat]) == SOFEL_OK) nfeat++;
      if (!end) break;
      p = end + 1;
    }
  }
  plan = sofel_shape_plan_create(f, NULL, &props, feats, nfeat, &e);
  CHECK(e == SOFEL_OK);
  CHECK(sofel_shape(f, NULL, NULL, plan, buf) == SOFEL_OK);
  count = sofel_buffer_glyphs(buf, out_gids, out_clusters, out_flags, out_pos,
                              (size_t)max_glyphs);
  sofel_shape_plan_destroy(plan);
  sofel_language_destroy(language);
  sofel_buffer_destroy(buf);
  if (count > max_glyphs) return -(int)count - 1;
  return (int)count;
}

/* The bytes of a file of the test data (scripts/fetch-testdata.sh) in
 * $SOFEL_TESTDATA, or NULL. */
static uint8_t *read_testdata(const char *path, size_t *len) {
  const char *dir = getenv("SOFEL_TESTDATA");
  char full[1024];
  FILE *fp;
  uint8_t *data = NULL;
  long size;
  if (dir == NULL || (size_t)snprintf(full, sizeof full, "%s/%s", dir, path) >= sizeof full)
    return NULL;
  fp = fopen(full, "rb");
  if (fp == NULL) return NULL;
  if (fseek(fp, 0, SEEK_END) == 0 && (size = ftell(fp)) > 0 && fseek(fp, 0, SEEK_SET) == 0 &&
      (data = malloc((size_t)size)) != NULL) {
    *len = fread(data, 1, (size_t)size, fp);
    if (*len != (size_t)size) {
      free(data);
      data = NULL;
    }
  }
  fclose(fp);
  return data;
}

/* The shaping cases of fixture_shape.h, called as Golem calls HarfBuzz,
 * on real fonts of the test data; skipped without the test data unless
 * SOFEL_REQUIRE_TESTDATA is set. */
static void test_golem_shape(void) {
  size_t i, j;
  for (i = 0; i < COUNT(shape_cases); i++) {
    const struct shape_case *c = &shape_cases[i];
    uint32_t gids[16], clusters[16], flags[16];
    int32_t pos[64];
    size_t len = 0;
    int32_t e = 12345;
    int n;
    uint8_t *data = read_testdata(c->font, &len);
    sofel_font_t *font;
    if (data == NULL) {
      fprintf(stderr, "%s: no test data; shaping skipped\n", c->font);
      CHECK(getenv("SOFEL_REQUIRE_TESTDATA") == NULL);
      return;
    }
    font = sofel_font_create(data, len, 0, &e);
    CHECK(e == SOFEL_OK);
    /* The needed count when the arrays are too small, with nothing written. */
    gids[0] = 12345;
    n = golem_shape(font, c->text, (int)c->text_len, c->script, c->language, c->vertical,
                    c->features, gids, clusters, flags, pos, 1);
    CHECK(n == -(int)c->n - 1 && gids[0] == 12345);
    n = golem_shape(font, c->text, (int)c->text_len, c->script, c->language, c->vertical,
                    c->features, gids, clusters, flags, pos, 16);
    CHECK(n == (int)c->n);
    for (j = 0; j < c->n && n == (int)c->n; j++) {
      CHECK(gids[j] == c->glyphs[j]);
      CHECK(clusters[j] == c->clusters[j]);
      CHECK(flags[j] == c->flags[j]);
      CHECK(memcmp(&pos[4 * j], &c->positions[4 * j], 4 * sizeof(int32_t)) == 0);
    }
    sofel_font_destroy(font);
    free(data);
  }
}

/* ---------------------------------------------------------- subsetting */

static void check_subset_declarations(void) {
  void (*bytes_destroy)(sofel_bytes_t *) = sofel_bytes_destroy;
  int32_t (*input_create)(sofel_subset_input_t **) = sofel_subset_input_create;
  void (*input_destroy)(sofel_subset_input_t *) = sofel_subset_input_destroy;
  int32_t (*keep_everything)(sofel_subset_input_t *) = sofel_subset_input_keep_everything;
  int32_t (*set_add)(sofel_subset_input_t *, sofel_subset_set_t, uint32_t) =
      sofel_subset_input_set_add;
  int32_t (*set_add_range)(sofel_subset_input_t *, sofel_subset_set_t, uint32_t, uint32_t) =
      sofel_subset_input_set_add_range;
  int32_t (*set_remove)(sofel_subset_input_t *, sofel_subset_set_t, uint32_t) =
      sofel_subset_input_set_remove;
  int32_t (*set_clear)(sofel_subset_input_t *, sofel_subset_set_t) = sofel_subset_input_set_clear;
  int32_t (*set_invert)(sofel_subset_input_t *, sofel_subset_set_t) =
      sofel_subset_input_set_invert;
  int32_t (*set_contains)(const sofel_subset_input_t *, sofel_subset_set_t, uint32_t) =
      sofel_subset_input_set_contains;
  int32_t (*set_count)(const sofel_subset_input_t *, sofel_subset_set_t, uint64_t *) =
      sofel_subset_input_set_count;
  int32_t (*set_next)(const sofel_subset_input_t *, sofel_subset_set_t, uint32_t *) =
      sofel_subset_input_set_next;
  int32_t (*get_flags)(const sofel_subset_input_t *, uint32_t *) = sofel_subset_input_get_flags;
  int32_t (*set_flags)(sofel_subset_input_t *, uint32_t) = sofel_subset_input_set_flags;
  int32_t (*set_map)(sofel_subset_input_t *, uint32_t, uint32_t) =
      sofel_subset_input_set_glyph_mapping;
  int32_t (*clear_map)(sofel_subset_input_t *) = sofel_subset_input_clear_glyph_mapping;
  int32_t (*get_axis)(const sofel_subset_input_t *, uint32_t, float *, float *, float *) =
      sofel_subset_input_get_axis_range;
  int32_t (*pin_all)(sofel_subset_input_t *, const sofel_font_t *) =
      sofel_subset_input_pin_all_axes_to_default;
  int32_t (*pin_default)(sofel_subset_input_t *, const sofel_font_t *, uint32_t) =
      sofel_subset_input_pin_axis_to_default;
  int32_t (*pin_location)(sofel_subset_input_t *, const sofel_font_t *, uint32_t, float) =
      sofel_subset_input_pin_axis_location;
  int32_t (*set_axis)(sofel_subset_input_t *, const sofel_font_t *, uint32_t, float, float,
                      float) = sofel_subset_input_set_axis_range;
  int32_t (*override_name)(sofel_subset_input_t *, uint16_t, uint16_t, uint16_t, uint16_t,
                           const char *, size_t) = sofel_subset_input_override_name_table;
  int32_t (*to_args)(const sofel_subset_input_t *, char *, size_t, size_t *) =
      sofel_subset_input_to_args;
  int32_t (*axis_from)(const char *, size_t, float *, float *, float *) =
      sofel_subset_axis_range_from_string;
  int32_t (*axis_to)(const sofel_subset_input_t *, uint32_t, char *, size_t, size_t *) =
      sofel_subset_axis_range_to_string;
  int32_t (*plan_create)(const sofel_font_t *, const sofel_subset_prepared_t *,
                         const sofel_subset_input_t *, sofel_subset_plan_t **) =
      sofel_subset_plan_create;
  void (*plan_destroy)(sofel_subset_plan_t *) = sofel_subset_plan_destroy;
  int32_t (*plan_execute)(const sofel_subset_plan_t *, const sofel_font_t *, sofel_bytes_t *) =
      sofel_subset_plan_execute;
  int32_t (*plan_count)(const sofel_subset_plan_t *, uint32_t *) =
      sofel_subset_plan_num_output_glyphs;
  int32_t (*old_to_new)(const sofel_subset_plan_t *, uint32_t, uint32_t *) =
      sofel_subset_plan_old_to_new_glyph;
  int32_t (*new_to_old)(const sofel_subset_plan_t *, uint32_t, uint32_t *) =
      sofel_subset_plan_new_to_old_glyph;
  int32_t (*unicode_to_old)(const sofel_subset_plan_t *, uint32_t, uint32_t *) =
      sofel_subset_plan_unicode_to_old_glyph;
  int32_t (*old_to_news)(const sofel_subset_plan_t *, uint32_t *, uint32_t *, size_t, size_t *) =
      sofel_subset_plan_old_to_new_glyphs;
  int32_t (*new_to_olds)(const sofel_subset_plan_t *, uint32_t *, uint32_t *, size_t, size_t *) =
      sofel_subset_plan_new_to_old_glyphs;
  int32_t (*unicode_to_olds)(const sofel_subset_plan_t *, uint32_t *, uint32_t *, size_t,
                             size_t *) = sofel_subset_plan_unicode_to_old_glyphs;
  int32_t (*subset)(const sofel_font_t *, const sofel_subset_input_t *, sofel_bytes_t *) =
      sofel_subset;
  int32_t (*serialize)(uint32_t, const sofel_serialize_object_t *, size_t, sofel_bytes_t *) =
      sofel_serialize_objects;
  int32_t (*prepared_create)(const sofel_font_t *, sofel_subset_prepared_t **) =
      sofel_subset_prepared_create;
  void (*prepared_destroy)(sofel_subset_prepared_t *) = sofel_subset_prepared_destroy;
  int32_t (*cs_index)(const sofel_font_t *, const uint8_t **, size_t *) =
      sofel_subset_cff_charstrings_index;
  int32_t (*cs_data)(const sofel_font_t *, uint32_t, const uint8_t **, size_t *) =
      sofel_subset_cff_charstring_data;
  const void *all[] = {
      (const void *)bytes_destroy, (const void *)input_create,  (const void *)input_destroy,
      (const void *)keep_everything, (const void *)set_add,     (const void *)set_add_range,
      (const void *)set_remove,    (const void *)set_clear,     (const void *)set_invert,
      (const void *)set_contains,  (const void *)set_count,     (const void *)set_next,
      (const void *)get_flags,     (const void *)set_flags,     (const void *)set_map,
      (const void *)clear_map,     (const void *)get_axis,      (const void *)override_name,
      (const void *)to_args,       (const void *)axis_from,     (const void *)axis_to,
      (const void *)plan_create,   (const void *)plan_destroy,  (const void *)plan_execute,
      (const void *)plan_count,    (const void *)old_to_new,    (const void *)new_to_old,
      (const void *)unicode_to_old, (const void *)old_to_news,  (const void *)new_to_olds,
      (const void *)unicode_to_olds, (const void *)subset,      (const void *)serialize,
      (const void *)pin_all,       (const void *)pin_default,   (const void *)pin_location,
      (const void *)set_axis,      (const void *)prepared_create, (const void *)prepared_destroy,
      (const void *)cs_index,      (const void *)cs_data,
  };
  size_t i;
  for (i = 0; i < COUNT(all); i++) CHECK(all[i] != NULL);
}

static uint64_t fnv(const uint8_t *b, size_t n) {
  uint64_t h = 0xcbf29ce484222325u;
  size_t i;
  for (i = 0; i < n; i++) h = (h ^ b[i]) * 0x100000001b3u;
  return h;
}

/* Checks the pairs of a mapping against the fixture's flat array. */
static void check_pairs(const sofel_subset_plan_t *plan,
                        int32_t (*f)(const sofel_subset_plan_t *, uint32_t *, uint32_t *, size_t,
                                     size_t *),
                        size_t n, const uint32_t *expected) {
  uint32_t a[64], b[64];
  size_t len = 12345, i;
  a[0] = 777;
  if (n > 0) {
    CHECK(f(plan, a, b, n - 1, &len) == SOFEL_ERROR_BUFFER_TOO_SMALL && len == n && a[0] == 777);
  }
  CHECK(f(plan, NULL, NULL, 0, &len) == (n ? SOFEL_ERROR_BUFFER_TOO_SMALL : SOFEL_OK) && len == n);
  CHECK(f(plan, a, b, 64, &len) == SOFEL_OK && len == n);
  for (i = 0; i < n && i < 64; i++) CHECK(a[i] == expected[2 * i] && b[i] == expected[2 * i + 1]);
}

/* The input functions against to_args, which the fixture computed for
 * the same input built with the Rust API. */
static void test_subset_input(void) {
  sofel_subset_input_t *in = NULL;
  char args[4096];
  size_t len = 0;
  uint64_t count = 0;
  uint32_t v, flags = 1;
  float mn = 1, df = 1, mx = 1;
  CHECK(sofel_subset_input_create(&in) == SOFEL_OK && in != NULL);
  CHECK(sofel_subset_input_to_args(in, args, sizeof args, &len) == SOFEL_OK && len == 0);
  CHECK(sofel_subset_input_set_add_range(in, SOFEL_SUBSET_SET_UNICODES, 0x41, 0x43) == SOFEL_OK);
  CHECK(sofel_subset_input_set_remove(in, SOFEL_SUBSET_SET_UNICODES, 0x42) == SOFEL_OK);
  CHECK(sofel_subset_input_set_add(in, SOFEL_SUBSET_SET_GLYPHS, 2) == SOFEL_OK);
  CHECK(sofel_subset_input_set_add(in, SOFEL_SUBSET_SET_DROP_TABLES, SOFEL_TAG('G', 'P', 'O', 'S')) ==
        SOFEL_OK);
  CHECK(sofel_subset_input_set_clear(in, SOFEL_SUBSET_SET_NAME_IDS) == SOFEL_OK);
  CHECK(sofel_subset_input_set_add(in, SOFEL_SUBSET_SET_NAME_IDS, 1) == SOFEL_OK);
  CHECK(sofel_subset_input_set_add(in, SOFEL_SUBSET_SET_NAME_IDS, 2) == SOFEL_OK);
  CHECK(sofel_subset_input_set_invert(in, SOFEL_SUBSET_SET_LAYOUT_FEATURES) == SOFEL_OK);
  CHECK(sofel_subset_input_set_remove(in, SOFEL_SUBSET_SET_LAYOUT_SCRIPTS,
                                      SOFEL_TAG('l', 'a', 't', 'n')) == SOFEL_OK);
  CHECK(sofel_subset_input_set_flags(
            in, SOFEL_SUBSET_FLAGS_RETAIN_GIDS | SOFEL_SUBSET_FLAGS_NO_HINTING) == SOFEL_OK);
  CHECK(sofel_subset_input_set_glyph_mapping(in, 5, 9) == SOFEL_OK);
  CHECK(sofel_subset_input_set_glyph_mapping(in, 5, 7) == SOFEL_OK); /* replaces */
  /* to_args: the needed length when the array is too small. */
  args[0] = 'x';
  CHECK(sofel_subset_input_to_args(in, args, 3, &len) == SOFEL_ERROR_BUFFER_TOO_SMALL &&
        len == strlen(SUBSET_ARGS) && args[0] == 'x');
  CHECK(sofel_subset_input_to_args(in, args, sizeof args, &len) == SOFEL_OK &&
        len == strlen(SUBSET_ARGS) && memcmp(args, SUBSET_ARGS, len) == 0);

  /* Reading the sets and flags. */
  CHECK(sofel_subset_input_set_contains(in, SOFEL_SUBSET_SET_UNICODES, 0x41) == 1);
  CHECK(sofel_subset_input_set_contains(in, SOFEL_SUBSET_SET_UNICODES, 0x42) == 0);
  CHECK(sofel_subset_input_set_count(in, SOFEL_SUBSET_SET_UNICODES, &count) == SOFEL_OK &&
        count == 2);
  CHECK(sofel_subset_input_set_count(in, SOFEL_SUBSET_SET_LAYOUT_SCRIPTS, &count) == SOFEL_OK &&
        count == 0xFFFFFFFEu);
  v = SOFEL_SUBSET_SET_INVALID;
  CHECK(sofel_subset_input_set_next(in, SOFEL_SUBSET_SET_UNICODES, &v) == 1 && v == 0x41);
  CHECK(sofel_subset_input_set_next(in, SOFEL_SUBSET_SET_UNICODES, &v) == 1 && v == 0x43);
  CHECK(sofel_subset_input_set_next(in, SOFEL_SUBSET_SET_UNICODES, &v) == 0 &&
        v == SOFEL_SUBSET_SET_INVALID);
  CHECK(sofel_subset_input_get_flags(in, &flags) == SOFEL_OK &&
        flags == (SOFEL_SUBSET_FLAGS_RETAIN_GIDS | SOFEL_SUBSET_FLAGS_NO_HINTING));
  CHECK(sofel_subset_input_clear_glyph_mapping(in) == SOFEL_OK);
  CHECK(sofel_subset_input_to_args(in, args, sizeof args, &len) == SOFEL_OK &&
        strstr(SUBSET_ARGS, " --gid-map=5:7") != NULL &&
        len == strlen(SUBSET_ARGS) - strlen(" --gid-map=5:7"));
  /* keep_everything changes the flags and the sets. */
  CHECK(sofel_subset_input_keep_everything(in) == SOFEL_OK);
  CHECK(sofel_subset_input_set_contains(in, SOFEL_SUBSET_SET_GLYPHS, 12345) == 1);
  CHECK(sofel_subset_input_set_count(in, SOFEL_SUBSET_SET_DROP_TABLES, &count) == SOFEL_OK &&
        count == 0);

  /* No axes yet: not found, and nothing written. */
  CHECK(sofel_subset_input_get_axis_range(in, SOFEL_TAG('w', 'g', 'h', 't'), &mn, &df, &mx) ==
            SOFEL_ERROR_NOT_FOUND &&
        mn == 1 && df == 1 && mx == 1);
  CHECK(sofel_subset_axis_range_to_string(in, SOFEL_TAG('w', 'g', 'h', 't'), args, sizeof args,
                                          &len) == SOFEL_ERROR_NOT_FOUND);

  /* Name overrides: HarfBuzz refuses a Macintosh name that is not ASCII. */
  CHECK(sofel_subset_input_override_name_table(in, 1, 3, 1, 0x409, "Sofel", 5) == SOFEL_OK);
  CHECK(sofel_subset_input_override_name_table(in, 1, 3, 1, 0x409, NULL, 0) == SOFEL_OK);
  CHECK(sofel_subset_input_override_name_table(in, 1, 1, 0, 0, "\303\251", 2) ==
        SOFEL_ERROR_INVALID_ARGUMENT);

  /* Errors: NULL, invalid flags, invalid set kinds. */
  CHECK(sofel_subset_input_create(NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_subset_input_set_add(NULL, SOFEL_SUBSET_SET_GLYPHS, 1) == SOFEL_ERROR_NULL);
  CHECK(sofel_subset_input_set_flags(NULL, 0) == SOFEL_ERROR_NULL);
  CHECK(sofel_subset_input_get_flags(in, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_subset_input_set_flags(in, 0x10000u) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_subset_input_get_flags(in, &flags) == SOFEL_OK &&
        (flags & SOFEL_SUBSET_FLAGS_GLYPH_NAMES) != 0);
  CHECK(sofel_subset_input_set_add(in, 8, 1) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_subset_input_set_add_range(in, 99, 1, 2) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_subset_input_set_remove(in, 8, 1) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_subset_input_set_clear(in, 8) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_subset_input_set_invert(in, 8) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_subset_input_set_contains(in, 8, 1) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_subset_input_set_count(in, 8, &count) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_subset_input_set_next(in, 8, &v) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_subset_input_to_args(NULL, args, sizeof args, &len) == SOFEL_ERROR_NULL);
  CHECK(sofel_subset_input_keep_everything(NULL) == SOFEL_ERROR_NULL);
  sofel_subset_input_destroy(in);
  sofel_subset_input_destroy(NULL);
}

static void test_axis_strings(void) {
  size_t i;
  for (i = 0; i < COUNT(axis_string_cases); i++) {
    const struct axis_string_case *c = &axis_string_cases[i];
    float v[3] = {7, 7, 7};
    uint32_t bits[3];
    int32_t s = sofel_subset_axis_range_from_string(c->s, strlen(c->s), &v[0], &v[1], &v[2]);
    CHECK(s == c->status);
    if (s == SOFEL_OK) {
      memcpy(bits, v, sizeof bits);
      CHECK(memcmp(bits, c->bits, sizeof bits) == 0);
    } else {
      CHECK(v[0] == 7 && v[1] == 7 && v[2] == 7);
    }
  }
  CHECK(sofel_subset_axis_range_from_string(NULL, 1, NULL, NULL, NULL) == SOFEL_ERROR_NULL);
}

/* The inputs of fixture.rs's plan_inputs, built with the C functions. */
static sofel_subset_input_t *plan_input(size_t i) {
  sofel_subset_input_t *in = NULL;
  CHECK(sofel_subset_input_create(&in) == SOFEL_OK);
  if (i == 0) {
    sofel_subset_input_set_add(in, SOFEL_SUBSET_SET_UNICODES, 0x41);
    sofel_subset_input_set_add(in, SOFEL_SUBSET_SET_UNICODES, 0x43);
  } else if (i == 1) {
    sofel_subset_input_set_add(in, SOFEL_SUBSET_SET_GLYPHS, 1);
    sofel_subset_input_set_add(in, SOFEL_SUBSET_SET_GLYPHS, 2);
    sofel_subset_input_set_glyph_mapping(in, 1, 2);
    sofel_subset_input_set_glyph_mapping(in, 2, 1);
  } else {
    sofel_subset_input_set_add(in, SOFEL_SUBSET_SET_UNICODES, 0x41);
    sofel_subset_input_override_name_table(in, 1, 3, 1, 0x409, "Sofel", 5);
  }
  return in;
}

/* sofel_subset and the plan give the same bytes as the Rust API, and the
 * plan's mappings are the Rust plan's. */
static void test_subset_plans(const sofel_font_t *font) {
  size_t i;
  sofel_subset_prepared_t *prepared = NULL;
  CHECK(sofel_subset_prepared_create(font, &prepared) == SOFEL_OK && prepared != NULL);
  for (i = 0; i < COUNT(subset_cases); i++) {
    const struct subset_case *c = &subset_cases[i];
    sofel_subset_input_t *in = plan_input(i);
    sofel_subset_plan_t *plan = NULL;
    sofel_bytes_t a = {NULL, 0}, b = {NULL, 0};
    uint32_t n = 0, g = 0;
    CHECK(sofel_subset(font, in, &a) == c->status);
    if (c->status != SOFEL_OK) {
      CHECK(a.data == NULL);
      sofel_subset_input_destroy(in);
      continue;
    }
    CHECK(a.len == c->len && fnv(a.data, a.len) == c->fnv);
    {
      /* A plan made with the prepared value gives the same bytes. */
      sofel_subset_plan_t *fast = NULL;
      sofel_bytes_t f = {NULL, 0};
      CHECK(sofel_subset_plan_create(font, prepared, in, &fast) == SOFEL_OK && fast != NULL);
      CHECK(sofel_subset_plan_execute(fast, font, &f) == SOFEL_OK);
      CHECK(f.len == a.len && memcmp(a.data, f.data, a.len) == 0);
      sofel_subset_plan_destroy(fast);
      sofel_bytes_destroy(&f);
    }
    CHECK(sofel_subset_plan_create(font, NULL, in, &plan) == SOFEL_OK && plan != NULL);
    sofel_subset_input_destroy(in); /* the plan does not borrow the input */
    CHECK(sofel_subset_plan_execute(plan, font, &b) == SOFEL_OK);
    CHECK(b.len == a.len && memcmp(a.data, b.data, a.len) == 0);
    CHECK(sofel_subset_plan_num_output_glyphs(plan, &n) == SOFEL_OK &&
          n == c->num_output_glyphs);
    check_pairs(plan, sofel_subset_plan_old_to_new_glyphs, c->n_old_new, c->old_new);
    check_pairs(plan, sofel_subset_plan_new_to_old_glyphs, c->n_new_old, c->new_old);
    check_pairs(plan, sofel_subset_plan_unicode_to_old_glyphs, c->n_unicode_old, c->unicode_old);
    if (c->n_old_new > 0) {
      CHECK(sofel_subset_plan_old_to_new_glyph(plan, c->old_new[0], &g) == SOFEL_OK &&
            g == c->old_new[1]);
      CHECK(sofel_subset_plan_new_to_old_glyph(plan, c->new_old[2], &g) == SOFEL_OK &&
            g == c->new_old[3]);
    }
    if (c->n_unicode_old > 0) {
      CHECK(sofel_subset_plan_unicode_to_old_glyph(plan, c->unicode_old[0], &g) == SOFEL_OK &&
            g == c->unicode_old[1]);
    }
    g = 777;
    CHECK(sofel_subset_plan_old_to_new_glyph(plan, 100000, &g) == SOFEL_ERROR_NOT_FOUND &&
          g == 777);
    CHECK(sofel_subset_plan_new_to_old_glyph(plan, 100000, &g) == SOFEL_ERROR_NOT_FOUND);
    CHECK(sofel_subset_plan_unicode_to_old_glyph(plan, 0x10FFFF, &g) == SOFEL_ERROR_NOT_FOUND);
    CHECK(sofel_subset_plan_execute(plan, NULL, &b) == SOFEL_ERROR_NULL);
    CHECK(sofel_subset_plan_execute(plan, font, NULL) == SOFEL_ERROR_NULL);
    sofel_subset_plan_destroy(plan);
    sofel_bytes_destroy(&a);
    sofel_bytes_destroy(&b);
    CHECK(a.data == NULL && a.len == 0);
    sofel_bytes_destroy(&a); /* again: nothing */
  }
  sofel_bytes_destroy(NULL);
  sofel_subset_plan_destroy(NULL);
  {
    /* RETAIN_GIDS with a glyph mapping: refused as by HarfBuzz. */
    sofel_subset_input_t *in = plan_input(1);
    sofel_subset_plan_t *plan = NULL;
    sofel_bytes_t out = {NULL, 0};
    sofel_subset_prepared_t *other = NULL;
    int32_t e = 0;
    sofel_font_t *cff = sofel_font_create(cff_font_data, sizeof cff_font_data, 0, &e);
    sofel_subset_input_set_flags(in, SOFEL_SUBSET_FLAGS_RETAIN_GIDS);
    CHECK(sofel_subset_plan_create(font, NULL, in, &plan) == SUBSET_INVALID_INPUT &&
          plan == NULL);
    CHECK(sofel_subset(font, in, &out) == SUBSET_INVALID_INPUT && out.data == NULL);
    /* Errors: NULL, and a prepared value of another font. */
    CHECK(sofel_subset(NULL, in, &out) == SOFEL_ERROR_NULL);
    CHECK(sofel_subset(font, NULL, &out) == SOFEL_ERROR_NULL);
    CHECK(sofel_subset(font, in, NULL) == SOFEL_ERROR_NULL);
    CHECK(sofel_subset_plan_create(font, NULL, NULL, &plan) == SOFEL_ERROR_NULL);
    CHECK(sofel_subset_plan_create(font, NULL, in, NULL) == SOFEL_ERROR_NULL);
    CHECK(sofel_subset_plan_num_output_glyphs(NULL, NULL) == SOFEL_ERROR_NULL);
    CHECK(sofel_subset_prepared_create(NULL, &other) == SOFEL_ERROR_NULL && other == NULL);
    CHECK(sofel_subset_prepared_create(font, NULL) == SOFEL_ERROR_NULL);
    CHECK(cff != NULL && sofel_subset_prepared_create(cff, &other) == SOFEL_OK);
    sofel_subset_input_set_flags(in, 0);
    CHECK(sofel_subset_plan_create(font, other, in, &plan) == SUBSET_INVALID_INPUT &&
          plan == NULL);
    sofel_subset_prepared_destroy(other);
    sofel_subset_prepared_destroy(NULL);
    sofel_font_destroy(cff);
    sofel_subset_input_destroy(in);
  }
  sofel_subset_prepared_destroy(prepared);
}

/* The CFF charstring accessors: the INDEX and glyph 1's charstring point
 * into the font's data. */
static void test_subset_cff(const sofel_font_t *font) {
  int32_t e = 0;
  const uint8_t *data = NULL, *index = NULL;
  size_t len = 0, index_len = 0;
  sofel_font_t *cff = sofel_font_create(cff_font_data, sizeof cff_font_data, 0, &e);
  CHECK(cff != NULL);
  if (cff == NULL) return;
  CHECK(sofel_subset_cff_charstrings_index(cff, &index, &index_len) == SOFEL_OK &&
        index_len == CFF_CHARSTRINGS_INDEX_LEN && index > cff_font_data &&
        index + index_len <= cff_font_data + sizeof cff_font_data);
  CHECK(sofel_subset_cff_charstring_data(cff, 1, &data, &len) == SOFEL_OK &&
        len == sizeof cff_charstring_1 && memcmp(data, cff_charstring_1, len) == 0 &&
        data > index && data + len <= index + index_len);
  len = 7;
  CHECK(sofel_subset_cff_charstring_data(cff, 100000, &data, &len) == SOFEL_ERROR_NOT_FOUND &&
        len == 7);
  CHECK(sofel_subset_cff_charstrings_index(font, &data, &len) == SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_subset_cff_charstring_data(font, 0, &data, &len) == SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_subset_cff_charstrings_index(NULL, &data, &len) == SOFEL_ERROR_NULL);
  CHECK(sofel_subset_cff_charstrings_index(cff, NULL, &len) == SOFEL_ERROR_NULL);
  CHECK(sofel_subset_cff_charstring_data(cff, 1, &data, NULL) == SOFEL_ERROR_NULL);
  sofel_font_destroy(cff);
}

/* sofel_serialize_objects: a root with a 16-bit offset to a child, a
 * shared child and the argument errors. */
static void test_serialize_objects(void) {
  static const uint8_t leaf[] = {0xAA, 0xBB};
  static const uint8_t root[] = {0x01, 0x00, 0x00, 0x00, 0x00};
  sofel_serialize_link_t links[2] = {{2, 1, 1, 0, SOFEL_SERIALIZE_WHENCE_HEAD, 0},
                                     {2, 3, 1, 0, SOFEL_SERIALIZE_WHENCE_HEAD, 0}};
  sofel_serialize_object_t objs[2] = {{leaf, sizeof leaf, NULL, 0, NULL, 0},
                                      {root, sizeof root, links, 2, NULL, 0}};
  static const uint8_t expected[] = {0x01, 0x00, 0x05, 0x00, 0x05, 0xAA, 0xBB};
  sofel_bytes_t out = {NULL, 0};
  CHECK(sofel_serialize_objects(SOFEL_TAG('t', 'e', 's', 't'), objs, 2, &out) == SOFEL_OK);
  CHECK(out.len == sizeof expected && memcmp(out.data, expected, out.len) == 0);
  sofel_bytes_destroy(&out);
  CHECK(sofel_serialize_objects(0, NULL, 0, &out) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_serialize_objects(0, objs, 2, NULL) == SOFEL_ERROR_NULL);
  links[1].width = 5;
  CHECK(sofel_serialize_objects(0, objs, 2, &out) == SOFEL_ERROR_INVALID_ARGUMENT);
  links[1].width = 2;
  links[1].objidx = 3;
  CHECK(sofel_serialize_objects(0, objs, 2, &out) == SOFEL_ERROR_INVALID_ARGUMENT);
  links[1].objidx = 1;
  links[1].position = 4; /* past the end of the root */
  CHECK(sofel_serialize_objects(0, objs, 2, &out) == SOFEL_ERROR_INVALID_ARGUMENT);
  links[1].position = 3;
  links[1].whence = 3;
  CHECK(sofel_serialize_objects(0, objs, 2, &out) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(out.data == NULL);
}

/* Golem's golem_font_subset with sofel (ADR 0022): the glyphs, RETAIN_GIDS
 * and sofel_subset, on real fonts of the test data; the fixture's
 * bytes are those of hb-dump --subset (fixture.rs checks them when
 * hb-dump is built). Skipped without the test data unless
 * SOFEL_REQUIRE_TESTDATA is set. */
static void test_golem_subset(void) {
  size_t i, j;
  for (i = 0; i < COUNT(golem_cases); i++) {
    const struct golem_case *c = &golem_cases[i];
    size_t len = 0;
    int32_t e = 12345;
    sofel_subset_input_t *in = NULL;
    sofel_bytes_t out = {NULL, 0};
    uint8_t *data = read_testdata(c->font, &len);
    sofel_font_t *font;
    if (data == NULL) {
      fprintf(stderr, "%s: no test data; subsetting skipped\n", c->font);
      CHECK(getenv("SOFEL_REQUIRE_TESTDATA") == NULL);
      return;
    }
    font = sofel_font_create(data, len, 0, &e);
    CHECK(e == SOFEL_OK);
    CHECK(sofel_subset_input_create(&in) == SOFEL_OK);
    for (j = 0; j < c->n; j++)
      CHECK(sofel_subset_input_set_add(in, SOFEL_SUBSET_SET_GLYPHS, c->glyphs[j]) == SOFEL_OK);
    CHECK(sofel_subset_input_set_flags(in, SOFEL_SUBSET_FLAGS_RETAIN_GIDS) == SOFEL_OK);
    CHECK(sofel_subset(font, in, &out) == SOFEL_OK);
    sofel_subset_input_destroy(in);
    CHECK(out.len == c->len && fnv(out.data, out.len) == c->fnv);
    sofel_bytes_destroy(&out);
    sofel_font_destroy(font);
    free(data);
  }
}

/* The axis functions on a variable font of the test data: the ranges
 * the Rust API sets, the errors, and the subset with every axis pinned
 * to its default. Skipped without the test data unless
 * SOFEL_REQUIRE_TESTDATA is set. */
static void test_subset_axes(const sofel_font_t *static_font) {
  const uint32_t wght = SOFEL_TAG('w', 'g', 'h', 't');
  size_t len = 0, i;
  int32_t e = 12345;
  sofel_subset_input_t *in = NULL;
  sofel_bytes_t out = {NULL, 0};
  float v[3] = {7, 7, 7};
  uint32_t bits[3];
  uint8_t *data = read_testdata(AXIS_FONT, &len);
  sofel_font_t *font;

  /* A font without fvar has no axes: nothing changes. */
  CHECK(sofel_subset_input_create(&in) == SOFEL_OK);
  CHECK(sofel_subset_input_pin_all_axes_to_default(in, static_font) == SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_subset_input_pin_axis_to_default(in, static_font, wght) == SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_subset_input_get_axis_range(in, wght, &v[0], &v[1], &v[2]) ==
        SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_subset_input_pin_all_axes_to_default(NULL, static_font) == SOFEL_ERROR_NULL);
  CHECK(sofel_subset_input_pin_all_axes_to_default(in, NULL) == SOFEL_ERROR_NULL);
  CHECK(sofel_subset_input_set_axis_range(in, NULL, wght, 1, 2, 1) == SOFEL_ERROR_NULL);
  sofel_subset_input_destroy(in);

  if (data == NULL) {
    fprintf(stderr, "%s: no test data; axes skipped\n", AXIS_FONT);
    CHECK(getenv("SOFEL_REQUIRE_TESTDATA") == NULL);
    return;
  }
  font = sofel_font_create(data, len, 0, &e);
  CHECK(e == SOFEL_OK);
  for (i = 0; i < 3; i++) {
    int32_t s;
    CHECK(sofel_subset_input_create(&in) == SOFEL_OK);
    if (i == 0)
      s = sofel_subset_input_pin_axis_to_default(in, font, wght);
    else if (i == 1)
      s = sofel_subset_input_pin_axis_location(in, font, wght, 1000);
    else
      s = sofel_subset_input_set_axis_range(in, font, wght, NAN, 500, NAN);
    CHECK(s == SOFEL_OK);
    CHECK(sofel_subset_input_get_axis_range(in, wght, &v[0], &v[1], &v[2]) == SOFEL_OK);
    memcpy(bits, v, sizeof bits);
    CHECK(memcmp(bits, axis_ranges[i], sizeof bits) == 0);
    sofel_subset_input_destroy(in);
  }

  /* No such axis; a minimum above the maximum. The input is unchanged. */
  CHECK(sofel_subset_input_create(&in) == SOFEL_OK);
  CHECK(sofel_subset_input_pin_axis_location(in, font, SOFEL_TAG('Z', 'Z', 'Z', 'Z'), 1) ==
        SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_subset_input_set_axis_range(in, font, SOFEL_TAG('Z', 'Z', 'Z', 'Z'), 1, 2, 1) ==
        SOFEL_ERROR_NOT_FOUND);
  CHECK(sofel_subset_input_set_axis_range(in, font, wght, 700, 300, NAN) ==
        SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_subset_input_get_axis_range(in, wght, &v[0], &v[1], &v[2]) ==
        SOFEL_ERROR_NOT_FOUND);

  /* Every axis pinned to its default: the Rust API's bytes. */
  CHECK(sofel_subset_input_set_add_range(in, SOFEL_SUBSET_SET_UNICODES, 0x41, 0x43) == SOFEL_OK);
  CHECK(sofel_subset_input_pin_all_axes_to_default(in, font) == SOFEL_OK);
  CHECK(sofel_subset(font, in, &out) == SOFEL_OK);
  CHECK(out.len == axis_pinned_len && fnv(out.data, out.len) == axis_pinned_fnv);
  sofel_bytes_destroy(&out);
  sofel_subset_input_destroy(in);
  sofel_font_destroy(font);
  free(data);
}

/* ADR 0062 I2: an empty page leaves the caller's sentinel intact. */
static void test_layout_queries(const sofel_font_t *font) {
  int32_t value = -7;
  float style_value = -7.0f;
  size_t stored = 99, total = 99;
  int32_t carets[2] = {-91, -92};
  sofel_math_kern_entry_t kerns[1] = {{-91, -92}};
  sofel_math_glyph_variant_t variants[1] = {{91, -92}};
  sofel_math_glyph_part_t parts[1] = {{91, -92, -93, -94, 7}};
  CHECK(sofel_math_has_data(font, &value) == SOFEL_OK && value == 0);
  CHECK(sofel_style_value(font, NULL, NULL, SOFEL_TAG('w', 'g', 'h', 't'), &style_value) == SOFEL_OK && style_value == 700.0f);
  CHECK(sofel_style_value(font, NULL, NULL, SOFEL_TAG('w', 'd', 't', 'h'), &style_value) == SOFEL_OK && style_value == 100.0f);
  int32_t scale_error = -1;
  sofel_scale_t *point_scale = sofel_scale_create_with_ptem(font, 1000, 1000, 0, 0, 23.5f, &scale_error);
  CHECK(point_scale != NULL && scale_error == SOFEL_OK);
  if (point_scale != NULL) {
    CHECK(sofel_style_value(font, NULL, point_scale, SOFEL_TAG('o', 'p', 's', 'z'), &style_value) == SOFEL_OK && style_value == 23.5f);
    sofel_scale_destroy(point_scale);
  }
  CHECK(sofel_math_constant(font, NULL, NULL, 4, &value) == SOFEL_OK && value == 0);
  CHECK(sofel_math_italics_correction(font, NULL, NULL, 3, &value) == SOFEL_OK && value == 0);
  CHECK(sofel_math_top_accent_attachment(font, NULL, NULL, 3, &value) == SOFEL_OK);
  CHECK(sofel_math_is_extended_shape(font, 3, &value) == SOFEL_OK && value == 0);
  CHECK(sofel_math_kerning(font, NULL, NULL, 3, 0, 100, &value) == SOFEL_OK && value == 0);
  CHECK(sofel_math_min_connector_overlap(font, NULL, NULL, SOFEL_DIRECTION_VERTICAL, &value) == SOFEL_OK && value == 0);
  CHECK(sofel_layout_ligature_carets(font, NULL, NULL, 3, SOFEL_DIRECTION_HORIZONTAL,
                                      0, carets, 1, &stored, &total) == SOFEL_OK);
  CHECK(stored == 0 && total == 0 && carets[0] == -91 && carets[1] == -92);
  CHECK(sofel_math_kernings(font, NULL, NULL, 3, 0, 0, kerns, 1, &stored, &total) == SOFEL_OK);
  CHECK(stored == 0 && total == 0 && kerns[0].kern_value == -92);
  CHECK(sofel_math_variants(font, NULL, NULL, 3, SOFEL_DIRECTION_VERTICAL,
                             0, variants, 1, &stored, &total) == SOFEL_OK);
  CHECK(stored == 0 && total == 0 && variants[0].advance == -92);
  CHECK(sofel_math_assembly(font, NULL, NULL, 3, SOFEL_DIRECTION_VERTICAL,
                             0, parts, 1, &stored, &total, &value) == SOFEL_OK);
  CHECK(stored == 0 && total == 0 && value == 0 && parts[0].full_advance == -94);

  int32_t error = -99;
  sofel_font_t *math_font = sofel_font_create(layout_query_math_data,
                                              sizeof layout_query_math_data, 0, &error);
  CHECK(math_font != NULL && error == SOFEL_OK);
  if (math_font == NULL) return;
  CHECK(sofel_math_has_data(math_font, &value) == SOFEL_OK && value == 1);
  CHECK(sofel_math_kernings(math_font, NULL, NULL, 3, 0, 0,
                            kerns, 1, &stored, &total) == SOFEL_OK);
  CHECK(stored == 1 && total == 2 && kerns[0].max_correction_height == 100 && kerns[0].kern_value == -10);
  kerns[0].kern_value = -92;
  CHECK(sofel_math_kernings(math_font, NULL, NULL, 3, 0, 1,
                            kerns, 1, &stored, &total) == SOFEL_OK);
  CHECK(stored == 1 && total == 2 && kerns[0].max_correction_height == INT32_MAX && kerns[0].kern_value == -20);
  kerns[0].kern_value = -92;
  CHECK(sofel_math_kernings(math_font, NULL, NULL, 3, 0, 2,
                            kerns, 1, &stored, &total) == SOFEL_OK);
  CHECK(stored == 0 && total == 2 && kerns[0].kern_value == -92);
  CHECK(sofel_math_kernings(math_font, NULL, NULL, 3, 0, 0,
                            NULL, 0, &stored, &total) == SOFEL_OK);
  CHECK(stored == 0 && total == 2);
  sofel_math_kern_entry_t all_kerns[3] = {{-1, -1}, {-1, -1}, {-91, -92}};
  CHECK(sofel_math_kernings(math_font, NULL, NULL, 3, 0, 0,
                            all_kerns, 3, &stored, &total) == SOFEL_OK);
  CHECK(stored == 2 && total == 2 && all_kerns[0].kern_value == -10 &&
        all_kerns[1].kern_value == -20 && all_kerns[2].kern_value == -92);
  CHECK(sofel_math_variants(math_font, NULL, NULL, 10, SOFEL_DIRECTION_VERTICAL,
                             1, variants, 1, &stored, &total) == SOFEL_OK);
  CHECK(stored == 1 && total == 2 && variants[0].glyph == 13 && variants[0].advance == 1500);
  sofel_math_glyph_variant_t all_variants[3] = {{0, 0}, {0, 0}, {91, -92}};
  CHECK(sofel_math_variants(math_font, NULL, NULL, 10, SOFEL_DIRECTION_VERTICAL,
                             0, all_variants, 3, &stored, &total) == SOFEL_OK);
  CHECK(stored == 2 && total == 2 && all_variants[0].glyph == 12 &&
        all_variants[1].glyph == 13 && all_variants[2].advance == -92);
  CHECK(sofel_math_assembly(math_font, NULL, NULL, 10, SOFEL_DIRECTION_VERTICAL,
                             1, parts, 1, &stored, &total, &value) == SOFEL_OK);
  CHECK(stored == 1 && total == 2 && value == 5 && parts[0].glyph == 15 && parts[0].full_advance == 300);
  sofel_math_glyph_part_t all_parts[3] = {{0}, {0}, {91, -92, -93, -94, 7}};
  CHECK(sofel_math_assembly(math_font, NULL, NULL, 10, SOFEL_DIRECTION_VERTICAL,
                             0, all_parts, 3, &stored, &total, &value) == SOFEL_OK);
  CHECK(stored == 2 && total == 2 && all_parts[0].glyph == 14 &&
        all_parts[1].glyph == 15 && all_parts[2].full_advance == -94);
  sofel_font_destroy(math_font);

  sofel_font_t *caret_font = sofel_font_create(layout_query_gdef_data,
                                               sizeof layout_query_gdef_data, 0, &error);
  CHECK(caret_font != NULL && error == SOFEL_OK);
  if (caret_font == NULL) return;
  sofel_scale_t *caret_scale = sofel_scale_create_with_ppem(caret_font, 1000, 1000, 12, 13, &error);
  CHECK(caret_scale != NULL && error == SOFEL_OK);
  if (caret_scale != NULL) {
    int32_t all_carets[3] = {-91, -92, -93};
    CHECK(sofel_layout_ligature_carets(caret_font, NULL, caret_scale, 3,
                                        SOFEL_DIRECTION_HORIZONTAL, 0, all_carets, 3,
                                        &stored, &total) == SOFEL_OK);
    CHECK(stored == 2 && total == 2 && all_carets[0] == 100 &&
          all_carets[1] == 283 && all_carets[2] == -93);
    CHECK(sofel_layout_ligature_carets(caret_font, NULL, caret_scale, 3,
                                        SOFEL_DIRECTION_HORIZONTAL, 1, carets, 1,
                                        &stored, &total) == SOFEL_OK);
    CHECK(stored == 1 && total == 2 && carets[0] == 283 && carets[1] == -92);
    carets[0] = -91;
    CHECK(sofel_layout_ligature_carets(caret_font, NULL, caret_scale, 3,
                                        SOFEL_DIRECTION_VERTICAL, 2, carets, 1,
                                        &stored, &total) == SOFEL_OK);
    CHECK(stored == 0 && total == 2 && carets[0] == -91);
    sofel_scale_destroy(caret_scale);
  }
  sofel_font_destroy(caret_font);

  sofel_font_t *variable_font = sofel_font_create(layout_query_variable_data,
                                                   sizeof layout_query_variable_data, 0, &error);
  CHECK(variable_font != NULL && error == SOFEL_OK);
  if (variable_font == NULL) return;
  sofel_location_t *location = NULL;
  float design_coord = 1.0f;
  CHECK(sofel_location_create_design(variable_font, &design_coord, 1, &location) == SOFEL_OK);
  if (location != NULL) {
    CHECK(sofel_math_constant(variable_font, location, NULL, 4, &value) == SOFEL_OK && value == 21);
    CHECK(sofel_style_value(variable_font, location, NULL, SOFEL_TAG('w', 'g', 'h', 't'),
                             &style_value) == SOFEL_OK && style_value == 1.0f);
    sofel_location_destroy(location);
  }
  sofel_font_destroy(variable_font);
}

int main(void) {
  int32_t e = 12345;
  sofel_font_t *font;
  sofel_outline_scratch_t *scratch;
  sofel_rasterizer_t *raster;

  check_declarations();
  check_shape_declarations();
  check_subset_declarations();
  test_version_and_errors();
  test_geometry();
  test_unicode();
  test_open();

  font = sofel_font_create(font_data, sizeof font_data, 0, &e);
  CHECK(e == SOFEL_OK);
  if (font == NULL) {
    fprintf(stderr, "cannot open the fixture font\n");
    return 1;
  }
  scratch = sofel_outline_scratch_create();
  raster = sofel_rasterizer_create();
  CHECK(run_queries(font, scratch, raster) == 0);
  sofel_rasterizer_destroy(raster);
  sofel_outline_scratch_destroy(scratch);

  test_font(font);
  test_layout_queries(font);
  test_variation_abi(font);
  test_collection();
  test_outlines(font);
  test_raster(font);
  test_validate();
  test_metrics();
  test_cff();
  test_cff_info();
  test_color();
  test_shape_values();
  test_shape_buffers();
  inv1_clear_after_deserialize(font);
  inv2_clear_after_parse_error();
  test_shape_glyph_buffers(font);
  test_shape_fixture_font(font);
  test_shape_ppem();
  test_golem_shape();
  test_subset_input();
  test_axis_strings();
  test_subset_plans(font);
  test_subset_cff(font);
  test_serialize_objects();
  test_golem_subset();
  test_subset_axes(font);
#if !defined(__wasi__)
  test_threads(font);
#endif
  sofel_font_destroy(font);

  if (failures != 0) {
    fprintf(stderr, "%d checks failed\n", failures);
    return 1;
  }
  printf("ok: sofel C ABI\n");
  return 0;
}
