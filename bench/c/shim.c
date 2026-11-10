/*
 * Thin wrappers that the benchmark calls through FFI (ADR 0013). They
 * hide FreeType's and HarfBuzz's struct layouts behind plain functions,
 * so that bench/src/ffi.rs only declares these.
 */

#include <limits.h>
#include <stdlib.h>
#include <string.h>

#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_ADVANCES_H
#include FT_OUTLINE_H

#include <hb.h>
#include <hb-ot.h>

/* ---------------------------------------------------------- FreeType */

static FT_Library library;

void *bft_open(const unsigned char *data, long len, long index)
{
  FT_Face face;
  if (!library && FT_Init_FreeType(&library) != 0)
    return NULL;
  if (FT_New_Memory_Face(library, data, len, index, &face) != 0)
    return NULL;
  return face;
}

void bft_close(void *face) { FT_Done_Face((FT_Face)face); }

long bft_num_glyphs(void *face) { return ((FT_Face)face)->num_glyphs; }

int bft_set_pixel_sizes(void *face, unsigned ppem)
{
  return FT_Set_Pixel_Sizes((FT_Face)face, 0, ppem);
}

unsigned bft_char_index(void *face, unsigned long c)
{
  return FT_Get_Char_Index((FT_Face)face, c);
}

long bft_advance(void *face, unsigned gid)
{
  FT_Fixed a = 0;
  if (FT_Get_Advance((FT_Face)face, gid, FT_LOAD_NO_SCALE, &a) != 0)
    return -1;
  return (long)a;
}

static int count_move(const FT_Vector *to, void *user) { (void)to; ++*(long *)user; return 0; }
static int count_line(const FT_Vector *to, void *user) { (void)to; ++*(long *)user; return 0; }
static int count_conic(const FT_Vector *c, const FT_Vector *to, void *user)
{
  (void)c; (void)to; ++*(long *)user; return 0;
}
static int count_cubic(const FT_Vector *c1, const FT_Vector *c2, const FT_Vector *to, void *user)
{
  (void)c1; (void)c2; (void)to; ++*(long *)user; return 0;
}
static const FT_Outline_Funcs counting = { count_move, count_line, count_conic, count_cubic, 0, 0 };

/* Loads (NO_HINTING | NO_BITMAP, plus NO_SCALE when unscaled) and
 * decomposes a glyph; returns the number of commands, or -1. */
long bft_outline(void *face, unsigned gid, int unscaled)
{
  FT_Face f = (FT_Face)face;
  FT_Int32 flags = FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP;
  long n = 0;
  if (unscaled)
    flags |= FT_LOAD_NO_SCALE;
  if (FT_Load_Glyph(f, gid, flags) != 0 || f->glyph->format != FT_GLYPH_FORMAT_OUTLINE)
    return -1;
  if (FT_Outline_Decompose(&f->glyph->outline, &counting, &n) != 0)
    return -1;
  return n;
}

/* Loads (NO_HINTING | NO_BITMAP) and renders (NORMAL) a glyph at the
 * current size; returns the sum of the pixels, or -1. */
long bft_render(void *face, unsigned gid)
{
  FT_Face f = (FT_Face)face;
  FT_Bitmap *b;
  unsigned r, c;
  long sum = 0;
  if (FT_Load_Glyph(f, gid, FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP) != 0)
    return -1;
  if (FT_Render_Glyph(f->glyph, FT_RENDER_MODE_NORMAL) != 0)
    return -1;
  b = &f->glyph->bitmap;
  for (r = 0; r < b->rows; r++)
    for (c = 0; c < b->width; c++)
      sum += b->buffer[(long)r * b->pitch + c];
  return sum;
}

/* ---------------------------------------------------------- HarfBuzz */

void *bhb_open(const unsigned char *data, unsigned len, unsigned index)
{
  hb_blob_t *blob = hb_blob_create_or_fail((const char *)data, len, HB_MEMORY_MODE_READONLY, NULL, NULL);
  hb_face_t *face;
  hb_font_t *font;
  if (!blob)
    return NULL;
  face = hb_face_create(blob, index);
  hb_blob_destroy(blob);
  font = hb_font_create(face);
  hb_face_destroy(face);
  return font;
}

void bhb_close(void *font) { hb_font_destroy((hb_font_t *)font); }

/* Scale 0 means unscaled (the default scale is unitsPerEm). */
void bhb_set_ppem(void *font, unsigned ppem)
{
  hb_font_t *f = (hb_font_t *)font;
  if (ppem == 0) {
    unsigned upem = hb_face_get_upem(hb_font_get_face(f));
    hb_font_set_scale(f, (int)upem, (int)upem);
  } else {
    hb_font_set_scale(f, (int)ppem * 64, (int)ppem * 64);
  }
}

unsigned bhb_nominal_glyph(void *font, unsigned c)
{
  hb_codepoint_t g = 0;
  if (!hb_font_get_nominal_glyph((hb_font_t *)font, c, &g))
    return 0;
  return g;
}

int bhb_advance(void *font, unsigned gid)
{
  return hb_font_get_glyph_h_advance((hb_font_t *)font, gid);
}

int bhb_v_advance(void *font, unsigned gid)
{
  return hb_font_get_glyph_v_advance((hb_font_t *)font, gid);
}

/* The sum of the vertical origin's x and y. */
long bhb_v_origin(void *font, unsigned gid)
{
  hb_position_t x = 0, y = 0;
  hb_font_get_glyph_v_origin((hb_font_t *)font, gid, &x, &y);
  return (long)x + (long)y;
}

static void hb_move(hb_draw_funcs_t *d, void *data, hb_draw_state_t *s, float x, float y, void *u)
{
  (void)d; (void)s; (void)x; (void)y; (void)u; ++*(long *)data;
}
static void hb_line(hb_draw_funcs_t *d, void *data, hb_draw_state_t *s, float x, float y, void *u)
{
  (void)d; (void)s; (void)x; (void)y; (void)u; ++*(long *)data;
}
static void hb_quad(hb_draw_funcs_t *d, void *data, hb_draw_state_t *s, float cx, float cy, float x, float y, void *u)
{
  (void)d; (void)s; (void)cx; (void)cy; (void)x; (void)y; (void)u; ++*(long *)data;
}
static void hb_cubic(hb_draw_funcs_t *d, void *data, hb_draw_state_t *s, float c1x, float c1y,
                     float c2x, float c2y, float x, float y, void *u)
{
  (void)d; (void)s; (void)c1x; (void)c1y; (void)c2x; (void)c2y; (void)x; (void)y; (void)u;
  ++*(long *)data;
}
static void hb_close(hb_draw_funcs_t *d, void *data, hb_draw_state_t *s, void *u)
{
  (void)d; (void)data; (void)s; (void)u;
}

static hb_draw_funcs_t *draw_funcs;

/* Draws a glyph with counting functions; returns the number of move,
 * line, quadratic and cubic commands, or -1. */
long bhb_draw(void *font, unsigned gid)
{
  long n = 0;
  if (!draw_funcs) {
    draw_funcs = hb_draw_funcs_create();
    hb_draw_funcs_set_move_to_func(draw_funcs, hb_move, NULL, NULL);
    hb_draw_funcs_set_line_to_func(draw_funcs, hb_line, NULL, NULL);
    hb_draw_funcs_set_quadratic_to_func(draw_funcs, hb_quad, NULL, NULL);
    hb_draw_funcs_set_cubic_to_func(draw_funcs, hb_cubic, NULL, NULL);
    hb_draw_funcs_set_close_path_func(draw_funcs, hb_close, NULL, NULL);
    hb_draw_funcs_make_immutable(draw_funcs);
  }
  if (!hb_font_draw_glyph_or_fail((hb_font_t *)font, gid, draw_funcs, &n))
    return -1;
  return n;
}

/* The sum of the glyph box's four values, or LONG_MIN when there is
 * none (ADR 0021). */
long bhb_glyph_extents(void *font, unsigned gid)
{
  hb_glyph_extents_t e;
  if (!hb_font_get_glyph_extents((hb_font_t *)font, gid, &e))
    return LONG_MIN;
  return (long)e.x_bearing + (long)e.y_bearing + (long)e.width + (long)e.height;
}

/* The length of the glyph's name as hb_font_get_glyph_name writes it
 * into 256 bytes, or -1 without a name. */
long bhb_glyph_name(void *font, unsigned gid, char *buf)
{
  if (!hb_font_get_glyph_name((hb_font_t *)font, gid, buf, 256))
    return -1;
  return (long)strlen(buf);
}

/* The glyph with the name, or -1. */
long bhb_glyph_from_name(void *font, const char *name, int len)
{
  hb_codepoint_t g = 0;
  if (!hb_font_get_glyph_from_name((hb_font_t *)font, name, len, &g))
    return -1;
  return (long)g;
}

/* Opens the data (hb_face_create, hb_font_create) and gets the box of
 * one glyph: what a first query costs, accelerators included. */
long bhb_open_extents(const unsigned char *data, unsigned len, unsigned gid)
{
  void *font = bhb_open(data, len, 0);
  long v;
  if (!font)
    return LONG_MIN;
  v = bhb_glyph_extents(font, gid);
  bhb_close(font);
  return v;
}
