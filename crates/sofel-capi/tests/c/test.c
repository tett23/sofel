/*
 * Tests of the C ABI (ADR 0015), run by scripts/test-capi.sh natively,
 * with ThreadSanitizer and AddressSanitizer, and on wasm32-wasip1.
 *
 * Every function of sofel.h is called at least once. The expected
 * values come from fixture.h, which crates/sofel-capi/tests/fixture.rs
 * computes with the Rust API.
 */

#include <stdio.h>
#include <string.h>

#include "sofel.h"
#include "fixture.h"

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
  int32_t (*h_advance)(const sofel_font_t *, uint32_t, uint16_t *) = sofel_font_h_advance;
  int32_t (*table)(const sofel_font_t *, uint32_t, const uint8_t **, size_t *) =
      sofel_font_table;
  sofel_outline_scratch_t *(*scratch_create)(void) = sofel_outline_scratch_create;
  void (*scratch_destroy)(sofel_outline_scratch_t *) = sofel_outline_scratch_destroy;
  int32_t (*pixel_scale)(const sofel_font_t *, uint16_t, uint16_t, int32_t *, int32_t *) =
      sofel_outline_pixel_scale;
  int32_t (*load)(const sofel_font_t *, uint32_t, uint32_t, int32_t, int32_t,
                  sofel_outline_scratch_t *, int32_t *, uint32_t *) = sofel_outline_load;
  int32_t (*decompose)(const sofel_outline_scratch_t *, int32_t, int32_t, int32_t,
                       sofel_move_to_func_t, sofel_line_to_func_t, sofel_quad_to_func_t,
                       sofel_cubic_to_func_t, sofel_close_func_t, void *) =
      sofel_outline_decompose;
  int32_t (*draw)(const sofel_font_t *, uint32_t, uint32_t, int32_t, int32_t,
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
  int32_t (*render_glyph)(const sofel_font_t *, uint32_t, uint16_t, sofel_outline_scratch_t *,
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
  const void *all[] = {
      (const void *)version,         (const void *)error_string,
      (const void *)mul_fix,         (const void *)div_fix,
      (const void *)mul_div,         (const void *)mul_div_no_round,
      (const void *)transform,       (const void *)vector_length,
      (const void *)font_create,     (const void *)font_destroy,
      (const void *)units_per_em,    (const void *)num_glyphs,
      (const void *)nominal_glyph,   (const void *)variation_glyph,
      (const void *)h_advance,       (const void *)table,
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
  };
  size_t i;
  for (i = 0; i < COUNT(all); i++) CHECK(all[i] != NULL);
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
    uint16_t a = 0;
    bad += sofel_font_h_advance(font, (uint32_t)i, &a) != SOFEL_OK || a != advances[i];
  }
  for (i = 0; i < COUNT(outline_cases); i++) {
    const struct outline_case *c = &outline_cases[i];
    struct recorder rec;
    int32_t cbox[4] = {0, 0, 0, 0};
    uint32_t overlap = 0;
    int32_t s;
    rec.len = 0;
    s = sofel_outline_draw(font, c->glyph, c->mode, c->x_scale, c->y_scale, scratch,
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
    int32_t s = sofel_render_glyph(font, c->glyph, c->ppem, scratch, raster, buf, sizeof buf,
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
  CHECK(sofel_font_h_advance(NULL, 1, &u16v) == SOFEL_ERROR_NULL);
  CHECK(sofel_font_h_advance(font, 1, NULL) == SOFEL_ERROR_NULL);
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
    int32_t s = sofel_outline_load(font, c->glyph, c->mode, c->x_scale, c->y_scale, scratch,
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
  CHECK(sofel_outline_load(font, 1, SOFEL_SCALE_FIXED, scale_cases[0].x_scale,
                           scale_cases[0].y_scale, scratch, cbox, &overlap) == SOFEL_OK);
  rec.len = 0;
  CHECK(sofel_outline_decompose(scratch, 100, -50, 4, rec_move_to, rec_line_to, rec_quad_to,
                                rec_cubic_to, rec_close, &rec) == SOFEL_OK);
  CHECK(same_commands(&rec, decomposed, COUNT(decomposed)));

  /* Errors. */
  CHECK(sofel_outline_load(font, 1, 2, 0, 0, scratch, cbox, &overlap) ==
        SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(sofel_outline_load(font, FIXTURE_NUM_GLYPHS, SOFEL_SCALE_NONE, 0, 0, scratch, cbox,
                           &overlap) == SOFEL_ERROR_INVALID_GLYPH);
  CHECK(sofel_outline_load(NULL, 1, SOFEL_SCALE_NONE, 0, 0, scratch, cbox, &overlap) ==
        SOFEL_ERROR_NULL);
  CHECK(sofel_outline_load(font, 1, SOFEL_SCALE_NONE, 0, 0, NULL, cbox, &overlap) ==
        SOFEL_ERROR_NULL);
  CHECK(sofel_outline_load(font, 1, SOFEL_SCALE_NONE, 0, 0, scratch, NULL, &overlap) ==
        SOFEL_ERROR_NULL);
  CHECK(sofel_outline_load(font, 1, SOFEL_SCALE_NONE, 0, 0, scratch, cbox, NULL) ==
        SOFEL_ERROR_NULL);
  CHECK(sofel_outline_decompose(NULL, 0, 0, 1, rec_move_to, rec_line_to, rec_quad_to,
                                rec_cubic_to, rec_close, &rec) == SOFEL_ERROR_NULL);
  CHECK(sofel_outline_decompose(scratch, 0, 0, 1, NULL, rec_line_to, rec_quad_to,
                                rec_cubic_to, rec_close, &rec) == SOFEL_ERROR_NULL);
  CHECK(sofel_outline_decompose(scratch, 0, 0, 1, rec_move_to, rec_line_to, rec_quad_to,
                                rec_cubic_to, NULL, &rec) == SOFEL_ERROR_NULL);
  rec.len = 0;
  CHECK(sofel_outline_draw(font, 1, SOFEL_SCALE_NONE, 0, 0, scratch, rec_move_to, rec_line_to,
                           NULL, rec_cubic_to, rec_close, &rec, cbox,
                           &overlap) == SOFEL_ERROR_NULL);
  CHECK(sofel_outline_draw(NULL, 1, SOFEL_SCALE_NONE, 0, 0, scratch, rec_move_to, rec_line_to,
                           rec_quad_to, rec_cubic_to, rec_close, &rec, cbox,
                           &overlap) == SOFEL_ERROR_NULL);
  CHECK(sofel_outline_draw(font, 1, 7, 0, 0, scratch, rec_move_to, rec_line_to, rec_quad_to,
                           rec_cubic_to, rec_close, &rec, cbox,
                           &overlap) == SOFEL_ERROR_INVALID_ARGUMENT);
  CHECK(rec.len == 0);
  /* `user` may be NULL. */
  CHECK(sofel_outline_draw(font, 0, SOFEL_SCALE_NONE, 0, 0, scratch, rec_move_to, rec_line_to,
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
  CHECK(sofel_outline_load(font, 2, SOFEL_SCALE_FIXED, xs, ys, scratch, cbox, &overlap) ==
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
    CHECK(sofel_render_glyph(font, 1, 16, scratch, r, tiny, sizeof tiny, &len, box) ==
          SOFEL_ERROR_BUFFER_TOO_SMALL);
    CHECK(len == render_cases[2].len);
    for (i = 0; i < sizeof tiny; i++) CHECK(tiny[i] == 0xAB);
    CHECK(sofel_render_glyph(NULL, 1, 16, scratch, r, buf, sizeof buf, &len, box) ==
          SOFEL_ERROR_NULL);
    CHECK(sofel_render_glyph(font, 1, 16, NULL, r, buf, sizeof buf, &len, box) ==
          SOFEL_ERROR_NULL);
    CHECK(sofel_render_glyph(font, 1, 16, scratch, NULL, buf, sizeof buf, &len, box) ==
          SOFEL_ERROR_NULL);
    CHECK(sofel_render_glyph(font, 1, 16, scratch, r, buf, sizeof buf, NULL, box) ==
          SOFEL_ERROR_NULL);
    CHECK(sofel_render_glyph(font, 1, 16, scratch, r, buf, sizeof buf, &len, NULL) ==
          SOFEL_ERROR_NULL);
    CHECK(sofel_render_glyph(font, FIXTURE_NUM_GLYPHS, 16, scratch, r, buf, sizeof buf, &len,
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
  int bad;
};

static void *work(void *arg) {
  struct worker *w = arg;
  sofel_outline_scratch_t *scratch = sofel_outline_scratch_create();
  sofel_rasterizer_t *raster = sofel_rasterizer_create();
  int round;
  for (round = 0; round < 20; round++) w->bad += run_queries(w->font, scratch, raster);
  sofel_rasterizer_destroy(raster);
  sofel_outline_scratch_destroy(scratch);
  return NULL;
}

/* One font queried from several threads at once gives the results of
 * one thread. */
static void test_threads(const sofel_font_t *font) {
  pthread_t threads[THREADS];
  struct worker workers[THREADS];
  int i;
  for (i = 0; i < THREADS; i++) {
    workers[i].font = font;
    workers[i].bad = 0;
    CHECK(pthread_create(&threads[i], NULL, work, &workers[i]) == 0);
  }
  for (i = 0; i < THREADS; i++) {
    CHECK(pthread_join(threads[i], NULL) == 0);
    CHECK(workers[i].bad == 0);
  }
}
#endif

int main(void) {
  int32_t e = 12345;
  sofel_font_t *font;
  sofel_outline_scratch_t *scratch;
  sofel_rasterizer_t *raster;

  check_declarations();
  test_version_and_errors();
  test_geometry();
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
  test_collection();
  test_outlines(font);
  test_raster(font);
  test_validate();
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
