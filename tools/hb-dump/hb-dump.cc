/*
 * hb-dump: write what HarfBuzz gives for a font's metrics, for
 * comparison with sofel (ADR 0016). Built as one translation unit with
 * HarfBuzz's src/harfbuzz.cc, so that internal functions can be called
 * as well; no FreeType integration.
 *
 * Usage: hb-dump [MODE] FILE
 *
 * MODE selects what is written; each mode is one function in the table
 * `modes` at the bottom, so that new dumps are added as new entries.
 *
 * Mode --metrics (the default):
 * Output is line-oriented text; numbers are decimal. For each face
 * index i of the file (hb_face_count; a TTC/OTC has several):
 *
 *   face <i>
 *   num_glyphs <hb_face_get_glyph_count>
 *   upem <hb_face_get_upem>
 *   then for each scale, in this order: (upem, upem), (2 upem, 2 upem),
 *   (upem / 2, upem / 2), (768, 768), (-upem, -upem), (768, -1536):
 *   scale <x_scale> <y_scale>
 *   h_extents <ascender> <descender> <line_gap>
 *                     (hb_font_get_extents_for_direction, LTR)
 *   v_extents <ascender> <descender> <line_gap>   (likewise, TTB)
 *   metric <tag> <found> <value> <fallback>
 *                     (for each hb_ot_metrics_tag_t: <found> and <value>
 *                     from hb_ot_metrics_get_position, <value> 0 when not
 *                     found; <fallback> from
 *                     hb_ot_metrics_get_position_with_fallback)
 *   glyph <g> <h_advance> <v_advance> <v_origin_x> <v_origin_y>
 *         <has_extents> <x_bearing> <y_bearing> <width> <height>
 *                     (for every glyph below num_glyphs:
 *                     hb_font_get_glyph_h_advance, _v_advance,
 *                     _v_origin and _extents; the extents are 0 when
 *                     <has_extents> is 0)
 *   end
 *
 * Exit status: 0 on success, 1 when the file cannot be read, 2 on wrong
 * usage.
 */

#include "harfbuzz.cc"

#include <stdio.h>
#include <string.h>

static const hb_ot_metrics_tag_t metric_tags[] = {
  HB_OT_METRICS_TAG_HORIZONTAL_ASCENDER,
  HB_OT_METRICS_TAG_HORIZONTAL_DESCENDER,
  HB_OT_METRICS_TAG_HORIZONTAL_LINE_GAP,
  HB_OT_METRICS_TAG_HORIZONTAL_CLIPPING_ASCENT,
  HB_OT_METRICS_TAG_HORIZONTAL_CLIPPING_DESCENT,
  HB_OT_METRICS_TAG_VERTICAL_ASCENDER,
  HB_OT_METRICS_TAG_VERTICAL_DESCENDER,
  HB_OT_METRICS_TAG_VERTICAL_LINE_GAP,
  HB_OT_METRICS_TAG_HORIZONTAL_CARET_RISE,
  HB_OT_METRICS_TAG_HORIZONTAL_CARET_RUN,
  HB_OT_METRICS_TAG_HORIZONTAL_CARET_OFFSET,
  HB_OT_METRICS_TAG_VERTICAL_CARET_RISE,
  HB_OT_METRICS_TAG_VERTICAL_CARET_RUN,
  HB_OT_METRICS_TAG_VERTICAL_CARET_OFFSET,
  HB_OT_METRICS_TAG_X_HEIGHT,
  HB_OT_METRICS_TAG_CAP_HEIGHT,
  HB_OT_METRICS_TAG_SUBSCRIPT_EM_X_SIZE,
  HB_OT_METRICS_TAG_SUBSCRIPT_EM_Y_SIZE,
  HB_OT_METRICS_TAG_SUBSCRIPT_EM_X_OFFSET,
  HB_OT_METRICS_TAG_SUBSCRIPT_EM_Y_OFFSET,
  HB_OT_METRICS_TAG_SUPERSCRIPT_EM_X_SIZE,
  HB_OT_METRICS_TAG_SUPERSCRIPT_EM_Y_SIZE,
  HB_OT_METRICS_TAG_SUPERSCRIPT_EM_X_OFFSET,
  HB_OT_METRICS_TAG_SUPERSCRIPT_EM_Y_OFFSET,
  HB_OT_METRICS_TAG_STRIKEOUT_SIZE,
  HB_OT_METRICS_TAG_STRIKEOUT_OFFSET,
  HB_OT_METRICS_TAG_UNDERLINE_SIZE,
  HB_OT_METRICS_TAG_UNDERLINE_OFFSET,
};

static void print_tag (hb_tag_t t)
{
  char s[5];
  hb_tag_to_string (t, s);
  s[4] = '\0';
  printf ("%s", s);
}

static void dump_scale (hb_font_t *font, unsigned num_glyphs, int x, int y)
{
  hb_font_set_scale (font, x, y);
  printf ("scale %d %d\n", x, y);

  hb_font_extents_t e;
  hb_font_get_extents_for_direction (font, HB_DIRECTION_LTR, &e);
  printf ("h_extents %d %d %d\n", e.ascender, e.descender, e.line_gap);
  hb_font_get_extents_for_direction (font, HB_DIRECTION_TTB, &e);
  printf ("v_extents %d %d %d\n", e.ascender, e.descender, e.line_gap);

  for (unsigned i = 0; i < sizeof (metric_tags) / sizeof (metric_tags[0]); i++)
  {
    hb_position_t v = 0, fallback = 0;
    hb_bool_t found = hb_ot_metrics_get_position (font, metric_tags[i], &v);
    if (!found) v = 0;
    hb_ot_metrics_get_position_with_fallback (font, metric_tags[i], &fallback);
    printf ("metric ");
    print_tag (metric_tags[i]);
    printf (" %d %d %d\n", found ? 1 : 0, v, fallback);
  }

  for (unsigned g = 0; g < num_glyphs; g++)
  {
    hb_position_t ox = 0, oy = 0;
    hb_glyph_extents_t ext = {0, 0, 0, 0};
    hb_position_t ha = hb_font_get_glyph_h_advance (font, g);
    hb_position_t va = hb_font_get_glyph_v_advance (font, g);
    hb_font_get_glyph_v_origin (font, g, &ox, &oy);
    hb_bool_t has = hb_font_get_glyph_extents (font, g, &ext);
    if (!has) ext.x_bearing = ext.y_bearing = ext.width = ext.height = 0;
    printf ("glyph %u %d %d %d %d %d %d %d %d %d\n", g, ha, va, ox, oy,
	    has ? 1 : 0, ext.x_bearing, ext.y_bearing, ext.width, ext.height);
  }
}

static int dump_metrics (hb_blob_t *blob)
{
  unsigned count = hb_face_count (blob);
  if (!count) count = 1;
  for (unsigned i = 0; i < count; i++)
  {
    hb_face_t *face = hb_face_create (blob, i);
    hb_font_t *font = hb_font_create (face);
    unsigned num_glyphs = hb_face_get_glyph_count (face);
    int upem = (int) hb_face_get_upem (face);

    printf ("face %u\n", i);
    printf ("num_glyphs %u\n", num_glyphs);
    printf ("upem %d\n", upem);
    const int scales[][2] = {
      {upem, upem}, {2 * upem, 2 * upem}, {upem / 2, upem / 2},
      {768, 768}, {-upem, -upem}, {768, -1536},
    };
    for (unsigned s = 0; s < sizeof (scales) / sizeof (scales[0]); s++)
      dump_scale (font, num_glyphs, scales[s][0], scales[s][1]);
    printf ("end\n");

    hb_font_destroy (font);
    hb_face_destroy (face);
  }
  return 0;
}

struct dump_mode_t
{
  const char *name;
  int (*dump) (hb_blob_t *blob);
};

static const dump_mode_t modes[] = {
  {"--metrics", dump_metrics},
};

int main (int argc, char **argv)
{
  const dump_mode_t *mode = &modes[0];
  if (argc == 3)
  {
    mode = nullptr;
    for (unsigned i = 0; i < sizeof (modes) / sizeof (modes[0]); i++)
      if (strcmp (argv[1], modes[i].name) == 0)
	mode = &modes[i];
  }
  if (!mode || argc < 2 || argc > 3)
  {
    fprintf (stderr, "usage: hb-dump [MODE] FILE; MODE is one of:");
    for (unsigned i = 0; i < sizeof (modes) / sizeof (modes[0]); i++)
      fprintf (stderr, " %s", modes[i].name);
    fprintf (stderr, "\n");
    return 2;
  }
  const char *path = argv[argc - 1];
  hb_blob_t *blob = hb_blob_create_from_file_or_fail (path);
  if (!blob)
  {
    fprintf (stderr, "hb-dump: cannot read %s\n", path);
    return 1;
  }
  int status = mode->dump (blob);
  hb_blob_destroy (blob);
  return status;
}
