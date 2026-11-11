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
#include FT_CID_H
#include FT_COLOR_H

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

/* FT_Get_CID_From_Glyph_Index; -1 on error (ADR 0021). */
long bft_cid(void *face, unsigned gid)
{
  FT_UInt cid = 0;
  if (FT_Get_CID_From_Glyph_Index((FT_Face)face, gid, &cid) != 0)
    return -1;
  return (long)cid;
}

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

int bhb_math_constant(void *font, unsigned index)
{
  return hb_ot_math_get_constant((hb_font_t *)font, (hb_ot_math_constant_t)index);
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

/* ------------------------------------------------ color glyphs (ADR 0023) */

/* FreeType: the number of COLRv0 layers of a glyph. */
long bft_layers(void *face, unsigned gid)
{
  FT_LayerIterator it;
  FT_UInt g, c;
  long n = 0;
  it.p = NULL;
  while (FT_Get_Color_Glyph_Layer((FT_Face)face, gid, &g, &c, &it))
    n++;
  return n;
}

static long ft_walk(FT_Face face, FT_OpaquePaint op, int depth)
{
  FT_COLR_Paint p;
  long n = 1;
  if (depth > 64 || !FT_Get_Paint(face, op, &p))
    return 0;
  switch (p.format) {
  case FT_COLR_PAINTFORMAT_COLR_LAYERS: {
    FT_OpaquePaint l = {NULL, 0};
    while (FT_Get_Paint_Layers(face, &p.u.colr_layers.layer_iterator, &l))
      n += ft_walk(face, l, depth + 1);
    break;
  }
  case FT_COLR_PAINTFORMAT_LINEAR_GRADIENT:
  case FT_COLR_PAINTFORMAT_RADIAL_GRADIENT:
  case FT_COLR_PAINTFORMAT_SWEEP_GRADIENT: {
    FT_ColorStop stop;
    FT_ColorLine *line = p.format == FT_COLR_PAINTFORMAT_LINEAR_GRADIENT
                             ? &p.u.linear_gradient.colorline
                             : p.format == FT_COLR_PAINTFORMAT_RADIAL_GRADIENT
                                   ? &p.u.radial_gradient.colorline
                                   : &p.u.sweep_gradient.colorline;
    while (FT_Get_Colorline_Stops(face, &stop, &line->color_stop_iterator))
      n++;
    break;
  }
  case FT_COLR_PAINTFORMAT_GLYPH: n += ft_walk(face, p.u.glyph.paint, depth + 1); break;
  case FT_COLR_PAINTFORMAT_TRANSFORM: n += ft_walk(face, p.u.transform.paint, depth + 1); break;
  case FT_COLR_PAINTFORMAT_TRANSLATE: n += ft_walk(face, p.u.translate.paint, depth + 1); break;
  case FT_COLR_PAINTFORMAT_SCALE: n += ft_walk(face, p.u.scale.paint, depth + 1); break;
  case FT_COLR_PAINTFORMAT_ROTATE: n += ft_walk(face, p.u.rotate.paint, depth + 1); break;
  case FT_COLR_PAINTFORMAT_SKEW: n += ft_walk(face, p.u.skew.paint, depth + 1); break;
  case FT_COLR_PAINTFORMAT_COMPOSITE:
    n += ft_walk(face, p.u.composite.source_paint, depth + 1);
    n += ft_walk(face, p.u.composite.backdrop_paint, depth + 1);
    break;
  default: break;
  }
  return n;
}

/* FreeType: the number of nodes and stops of a glyph's COLRv1 paint
 * tree, walked with FT_Get_Paint (the walk is the benchmark's). */
long bft_colr_walk(void *face, unsigned gid)
{
  FT_OpaquePaint op = {NULL, 0};
  if (!FT_Get_Color_Glyph_Paint((FT_Face)face, gid, FT_COLOR_NO_ROOT_TRANSFORM, &op))
    return 0;
  return ft_walk((FT_Face)face, op, 0);
}

/* HarfBuzz: whether a glyph has a paint, times 65536, plus its layers. */
long bhb_color_format(void *font, unsigned gid)
{
  hb_face_t *face = hb_font_get_face((hb_font_t *)font);
  return (long)hb_ot_color_glyph_has_paint(face, gid) * 65536 +
         (long)hb_ot_color_glyph_get_layers(face, gid, 0, NULL, NULL);
}

long bhb_color_layers(void *font, unsigned gid)
{
  hb_face_t *face = hb_font_get_face((hb_font_t *)font);
  unsigned n = hb_ot_color_glyph_get_layers(face, gid, 0, NULL, NULL);
  hb_ot_color_layer_t layers[64];
  unsigned count = n < 64 ? n : 64;
  hb_ot_color_glyph_get_layers(face, gid, 0, &count, layers);
  return (long)n;
}

long bhb_svg_len(void *font, unsigned gid)
{
  hb_blob_t *b = hb_ot_color_glyph_reference_svg(hb_font_get_face((hb_font_t *)font), gid);
  long n = (long)hb_blob_get_length(b);
  hb_blob_destroy(b);
  return n;
}

struct paint_count { long commands; long image_len; };

#define PC ((struct paint_count *)data)
static void pc_push_transform(hb_paint_funcs_t *f, void *data, float a, float b, float c, float d, float e, float g, void *u) { PC->commands++; }
static void pc_pop(hb_paint_funcs_t *f, void *data, void *u) { PC->commands++; }
static void pc_clip_glyph(hb_paint_funcs_t *f, void *data, hb_codepoint_t g, hb_font_t *font, void *u) { PC->commands++; }
static void pc_clip_rect(hb_paint_funcs_t *f, void *data, float a, float b, float c, float d, void *u) { PC->commands++; }
static void pc_color(hb_paint_funcs_t *f, void *data, hb_bool_t fg, hb_color_t c, void *u) { PC->commands++; }
static void pc_fill_glyph(hb_paint_funcs_t *f, void *data, hb_codepoint_t g, hb_font_t *font, hb_bool_t fg, hb_color_t c, void *u) { PC->commands++; }
static hb_bool_t pc_image(hb_paint_funcs_t *f, void *data, hb_blob_t *b, unsigned w, unsigned h, hb_tag_t t, float s, hb_glyph_extents_t *e, void *u)
{ PC->commands++; PC->image_len += (long)hb_blob_get_length(b); return 1; }
static void pc_stops(void *data, hb_color_line_t *cl)
{
  hb_color_stop_t stops[64];
  unsigned n = 64;
  hb_color_line_get_color_stops(cl, 0, &n, stops);
  PC->commands++;
}
static void pc_linear(hb_paint_funcs_t *f, void *data, hb_color_line_t *cl, float a, float b, float c, float d, float e, float g, void *u) { pc_stops(data, cl); }
static void pc_radial(hb_paint_funcs_t *f, void *data, hb_color_line_t *cl, float a, float b, float c, float d, float e, float g, void *u) { pc_stops(data, cl); }
static void pc_sweep(hb_paint_funcs_t *f, void *data, hb_color_line_t *cl, float a, float b, float c, float d, void *u) { pc_stops(data, cl); }
static void pc_group(hb_paint_funcs_t *f, void *data, void *u) { PC->commands++; }
static void pc_pop_group(hb_paint_funcs_t *f, void *data, hb_paint_composite_mode_t m, void *u) { PC->commands++; }
#undef PC

static hb_paint_funcs_t *count_funcs;

/* HarfBuzz: the number of paint calls of hb_font_paint_glyph_or_fail
 * plus the bytes of the images; -1 when it is not a color glyph. */
long bhb_paint(void *font, unsigned gid)
{
  struct paint_count c = {0, 0};
  if (!count_funcs) {
    hb_paint_funcs_t *f = hb_paint_funcs_create();
    hb_paint_funcs_set_push_transform_func(f, pc_push_transform, NULL, NULL);
    hb_paint_funcs_set_pop_transform_func(f, pc_pop, NULL, NULL);
    hb_paint_funcs_set_push_clip_glyph_func(f, pc_clip_glyph, NULL, NULL);
    hb_paint_funcs_set_push_clip_rectangle_func(f, pc_clip_rect, NULL, NULL);
    hb_paint_funcs_set_pop_clip_func(f, pc_pop, NULL, NULL);
    hb_paint_funcs_set_color_func(f, pc_color, NULL, NULL);
    hb_paint_funcs_set_fill_glyph_func(f, pc_fill_glyph, NULL, NULL);
    hb_paint_funcs_set_image_func(f, pc_image, NULL, NULL);
    hb_paint_funcs_set_linear_gradient_func(f, pc_linear, NULL, NULL);
    hb_paint_funcs_set_radial_gradient_func(f, pc_radial, NULL, NULL);
    hb_paint_funcs_set_sweep_gradient_func(f, pc_sweep, NULL, NULL);
    hb_paint_funcs_set_push_group_func(f, pc_group, NULL, NULL);
    hb_paint_funcs_set_pop_group_func(f, pc_pop_group, NULL, NULL);
    hb_paint_funcs_make_immutable(f);
    count_funcs = f;
  }
  if (!hb_font_paint_glyph_or_fail((hb_font_t *)font, gid, count_funcs, &c, 0, HB_COLOR(0, 0, 0, 255)))
    return -1;
  return c.commands + c.image_len;
}

/* HarfBuzz's strike choice comes from the font's ppem. */
void bhb_set_bitmap_ppem(void *font, unsigned ppem)
{
  hb_font_set_ppem((hb_font_t *)font, ppem, ppem);
}

/* --------------------------------------------------- shaping (ADR 0020) */

/* The direction after hb_buffer_guess_segment_properties: 0 keeps the
 * guessed one, as perf/benchmark-shape.cc without --direction. */
static void shape_line(hb_font_t *font, hb_buffer_t *buf, const char *text, unsigned text_len,
                       unsigned line_len, int direction)
{
  hb_buffer_clear_contents(buf);
  hb_buffer_add_utf8(buf, text, (int)text_len, 0, (int)line_len);
  hb_buffer_guess_segment_properties(buf);
  /* No language from the process locale (sofel does not read it). */
  hb_buffer_set_language(buf, HB_LANGUAGE_INVALID);
  if (direction)
    hb_buffer_set_direction(buf, (hb_direction_t)direction);
  hb_shape(font, buf, NULL, 0);
}

/* The checksum of the glyphs the sofel side computes the same way. */
static unsigned long long shape_sum(hb_buffer_t *buf)
{
  unsigned n = 0, i;
  hb_glyph_info_t *info = hb_buffer_get_glyph_infos(buf, &n);
  hb_glyph_position_t *pos = hb_buffer_get_glyph_positions(buf, &n);
  unsigned long long h = 0;
  for (i = 0; i < n; i++) {
    h = h * 31u + info[i].codepoint;
    h = h * 31u + info[i].cluster;
    h = h * 31u + (unsigned)hb_glyph_info_get_glyph_flags(&info[i]);
    h = h * 31u + (unsigned long long)(long long)pos[i].x_advance;
    h = h * 31u + (unsigned long long)(long long)pos[i].y_advance;
    h = h * 31u + (unsigned long long)(long long)pos[i].x_offset;
    h = h * 31u + (unsigned long long)(long long)pos[i].y_offset;
  }
  return h;
}

void *bhb_buffer_create(void) { return hb_buffer_create(); }

void bhb_buffer_destroy(void *buf) { hb_buffer_destroy((hb_buffer_t *)buf); }

/* Shapes the line text[0..line_len] (the rest of the text is its
 * post-context) and writes up to cap glyphs: ids, clusters, flags and
 * four positions each. Returns the glyph count. */
long bhb_shape_line(void *font, void *buf, const char *text, unsigned text_len,
                    unsigned line_len, int direction, unsigned *glyphs, unsigned *clusters,
                    unsigned *flags, int *positions, unsigned cap)
{
  hb_buffer_t *b = (hb_buffer_t *)buf;
  unsigned n = 0, i;
  hb_glyph_info_t *info;
  hb_glyph_position_t *pos;
  shape_line((hb_font_t *)font, b, text, text_len, line_len, direction);
  info = hb_buffer_get_glyph_infos(b, &n);
  pos = hb_buffer_get_glyph_positions(b, &n);
  for (i = 0; i < n && i < cap; i++) {
    glyphs[i] = info[i].codepoint;
    clusters[i] = info[i].cluster;
    flags[i] = (unsigned)hb_glyph_info_get_glyph_flags(&info[i]);
    positions[4 * i] = pos[i].x_advance;
    positions[4 * i + 1] = pos[i].y_advance;
    positions[4 * i + 2] = pos[i].x_offset;
    positions[4 * i + 3] = pos[i].y_offset;
  }
  return (long)n;
}

/* perf/benchmark-shape.cc's shape(): every line ended by '\n', each with
 * its guessed properties. Returns the checksums of the lines, summed. */
unsigned long long bhb_shape_text(void *font, void *buf, const char *text, unsigned text_len,
                                  int direction)
{
  const char *end;
  unsigned long long sum = 0;
  while ((end = (const char *)memchr(text, '\n', text_len))) {
    unsigned skip = (unsigned)(end - text) + 1;
    shape_line((hb_font_t *)font, (hb_buffer_t *)buf, text, text_len, (unsigned)(end - text),
               direction);
    sum += shape_sum((hb_buffer_t *)buf);
    text_len -= skip;
    text += skip;
  }
  return sum;
}

/* From the font's bytes to the first line shaped, cold: the blob, face,
 * font and buffer are made and destroyed. Returns the line's checksum. */
unsigned long long bhb_first_shape(const unsigned char *data, unsigned len, const char *text,
                                   unsigned text_len, unsigned line_len, int direction)
{
  hb_font_t *font = (hb_font_t *)bhb_open(data, len, 0);
  hb_buffer_t *buf = hb_buffer_create();
  unsigned long long h;
  shape_line(font, buf, text, text_len, line_len, direction);
  h = shape_sum(buf);
  hb_buffer_destroy(buf);
  hb_font_destroy(font);
  return h;
}

/* Loads (COLOR | NO_HINTING | NO_BITMAP | NO_SVG) and renders (NORMAL)
 * a glyph at the current size, COLR version 0 layers blended in palette
 * 0; returns the sum of the bytes of its pixels (4 a pixel for BGRA),
 * or -1. */
long bft_colr_render(void *face, unsigned gid)
{
  FT_Face f = (FT_Face)face;
  FT_Bitmap *b;
  unsigned r, c, bytes;
  long sum = 0;
  if (FT_Load_Glyph(f, gid, FT_LOAD_COLOR | FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP | FT_LOAD_NO_SVG) != 0)
    return -1;
  if (FT_Render_Glyph(f->glyph, FT_RENDER_MODE_NORMAL) != 0)
    return -1;
  b = &f->glyph->bitmap;
  if (!b->buffer)
    return 0;
  bytes = b->pixel_mode == FT_PIXEL_MODE_BGRA ? 4 : 1;
  for (r = 0; r < b->rows; r++)
    for (c = 0; c < b->width * bytes; c++)
      sum += b->buffer[(long)r * b->pitch + c];
  return sum;
}

/* FreeType: an embedded bitmap glyph loaded with FT_LOAD_DEFAULT at the
 * current size: the sum of its bytes plus its width and rows; -1 when
 * it is not a bitmap. */
long bft_sbit(void *face, unsigned gid)
{
  FT_Face f = (FT_Face)face;
  long sum;
  unsigned r, k;
  if (FT_Load_Glyph(f, gid, FT_LOAD_DEFAULT) != 0 || f->glyph->format != FT_GLYPH_FORMAT_BITMAP)
    return -1;
  sum = (long)f->glyph->bitmap.width + (long)f->glyph->bitmap.rows;
  for (r = 0; r < f->glyph->bitmap.rows; r++)
    for (k = 0; k < (unsigned)abs(f->glyph->bitmap.pitch); k++)
      sum += f->glyph->bitmap.buffer[(long)r * f->glyph->bitmap.pitch + k];
  return sum;
}

/* ------------------------------------------------- subsetting (ADR 0022) */

#include <hb-subset.h>

/* hb_subset_or_fail and hb_face_reference_blob on a face, with
 * an input made here: the glyphs or the characters given, and the
 * flags. Returns the FNV-1a 64-bit hash of the subset font, with its
 * length in *len; 0 with *len 0 when subsetting fails. */
unsigned long long bhb_subset_face(void *face_, const unsigned *glyphs, unsigned num_glyphs,
                                   const unsigned *unicodes, unsigned num_unicodes,
                                   unsigned flags, unsigned *len)
{
  hb_face_t *face = (hb_face_t *)face_;
  hb_subset_input_t *input = hb_subset_input_create_or_fail();
  hb_set_t *gs, *us;
  hb_face_t *out;
  unsigned long long h = 0xcbf29ce484222325ull;
  unsigned i;
  *len = 0;
  if (!input)
    return 0;
  gs = hb_subset_input_glyph_set(input);
  us = hb_subset_input_unicode_set(input);
  for (i = 0; i < num_glyphs; i++)
    hb_set_add(gs, glyphs[i]);
  for (i = 0; i < num_unicodes; i++)
    hb_set_add(us, unicodes[i]);
  hb_subset_input_set_flags(input, flags);
  out = hb_subset_or_fail(face, input);
  hb_subset_input_destroy(input);
  if (!out)
    return 0;
  {
    hb_blob_t *blob = hb_face_reference_blob(out);
    unsigned n = 0;
    const char *d = hb_blob_get_data(blob, &n);
    for (i = 0; i < n; i++)
      h = (h ^ (unsigned char)d[i]) * 0x100000001b3ull;
    *len = n;
    hb_blob_destroy(blob);
  }
  hb_face_destroy(out);
  return h;
}

/* bhb_subset_face on the font's face. */
unsigned long long bhb_subset(void *font, const unsigned *glyphs, unsigned num_glyphs,
                              const unsigned *unicodes, unsigned num_unicodes, unsigned flags,
                              unsigned *len)
{
  return bhb_subset_face(hb_font_get_face((hb_font_t *)font), glyphs, num_glyphs, unicodes,
                         num_unicodes, flags, len);
}

/* hb_subset_preprocess of the font's face: a new face, for
 * bhb_subset_face and bhb_face_destroy. */
void *bhb_subset_preprocess(void *font)
{
  return hb_subset_preprocess(hb_font_get_face((hb_font_t *)font));
}

/* The glyph count of a face. */
unsigned bhb_face_glyph_count(void *face)
{
  return hb_face_get_glyph_count((hb_face_t *)face);
}

void bhb_face_destroy(void *face)
{
  hb_face_destroy((hb_face_t *)face);
}
