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
 * Mode --names: for each face index i,
 *
 *   face <i>
 *   name <name_id> <language>    (hb_ot_name_list_names, in its order)
 *   utf8 <name_id> <language> <capacity> <total> <written> <hex>
 *   utf16 ... and utf32 ...      (hb_ot_name_get_utf8/16/32 for every
 *                                name id and every language listed, and
 *                                for HB_LANGUAGE_INVALID written as `-`;
 *                                with a capacity of total + 1 and of 4;
 *                                <written> is the text_size HarfBuzz
 *                                returns, <hex> the units written, two,
 *                                four or eight hex digits each, `-` for
 *                                none)
 *   glyph_name <g> <found> <hex> (hb_font_get_glyph_name with 256 bytes,
 *                                every glyph)
 *   glyph_from_name <g> <found> <glyph>
 *                                (hb_font_get_glyph_from_name of that
 *                                name, for every glyph with a name)
 *   end
 *
 * Mode --cmap: for each face index i,
 *
 *   face <i>
 *   num_glyphs <hb_face_get_glyph_count>
 *   map <code point> <glyph>     (hb_font_get_nominal_glyph, for every
 *                                code point up to U+10FFFF it maps)
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

template <typename T>
static void print_units (const T *u, unsigned n, int digits)
{
  if (!n) { printf (" -"); return; }
  printf (" ");
  for (unsigned i = 0; i < n; i++) printf ("%0*lx", digits, (unsigned long) u[i]);
}

template <typename T, typename F>
static void dump_utf (const char *kind, hb_face_t *face, unsigned id, hb_language_t lang,
		      int digits, F get)
{
  unsigned total = get (face, id, lang, nullptr, nullptr);
  unsigned caps[2] = {total + 1, 4};
  for (unsigned c = 0; c < 2; c++)
  {
    T buf[1024];
    unsigned cap = caps[c] < 1024 ? caps[c] : 1024;
    unsigned size = cap;
    unsigned t = get (face, id, lang, &size, buf);
    printf ("%s %u %s %u %u %u", kind, id, lang ? hb_language_to_string (lang) : "-", cap, t, size);
    print_units (buf, size, digits);
    printf ("\n");
  }
}

static int dump_names (hb_blob_t *blob)
{
  unsigned count = hb_face_count (blob);
  if (!count) count = 1;
  for (unsigned i = 0; i < count; i++)
  {
    hb_face_t *face = hb_face_create (blob, i);
    hb_font_t *font = hb_font_create (face);
    printf ("face %u\n", i);
    unsigned n = 0;
    const hb_ot_name_entry_t *entries = hb_ot_name_list_names (face, &n);
    for (unsigned e = 0; e < n; e++)
      printf ("name %u %s\n", entries[e].name_id, hb_language_to_string (entries[e].language));
    for (unsigned e = 0; e < n; e++)
    {
      if (e && entries[e].name_id == entries[e - 1].name_id) continue;
      unsigned id = entries[e].name_id;
      hb_language_t langs[1025];
      unsigned nl = 0;
      langs[nl++] = HB_LANGUAGE_INVALID;
      for (unsigned l = 0; l < n && nl < 1025; l++)
      {
	bool seen = false;
	for (unsigned k = 0; k < nl; k++) seen |= langs[k] == entries[l].language;
	if (!seen) langs[nl++] = entries[l].language;
      }
      for (unsigned l = 0; l < nl; l++)
      {
	dump_utf<uint8_t> ("utf8", face, id, langs[l], 2,
			   [] (hb_face_t *f, unsigned id, hb_language_t lang, unsigned *size, uint8_t *t)
			   { return hb_ot_name_get_utf8 (f, id, lang, size, (char *) t); });
	dump_utf<uint16_t> ("utf16", face, id, langs[l], 4, hb_ot_name_get_utf16);
	dump_utf<uint32_t> ("utf32", face, id, langs[l], 8, hb_ot_name_get_utf32);
      }
    }
    unsigned num_glyphs = hb_face_get_glyph_count (face);
    for (unsigned g = 0; g < num_glyphs; g++)
    {
      char name[256];
      hb_bool_t found = hb_font_get_glyph_name (font, g, name, sizeof name);
      unsigned len = found ? (unsigned) strlen (name) : 0;
      printf ("glyph_name %u %d", g, found ? 1 : 0);
      print_units ((const uint8_t *) name, len, 2);
      printf ("\n");
      if (found && len)
      {
	hb_codepoint_t gid = 0;
	hb_bool_t ok = hb_font_get_glyph_from_name (font, name, (int) len, &gid);
	printf ("glyph_from_name %u %d %u\n", g, ok ? 1 : 0, ok ? gid : 0);
      }
    }
    printf ("end\n");
    hb_font_destroy (font);
    hb_face_destroy (face);
  }
  return 0;
}

static int dump_cmap (hb_blob_t *blob)
{
  unsigned count = hb_face_count (blob);
  if (!count) count = 1;
  for (unsigned i = 0; i < count; i++)
  {
    hb_face_t *face = hb_face_create (blob, i);
    hb_font_t *font = hb_font_create (face);
    printf ("face %u\nnum_glyphs %u\n", i, hb_face_get_glyph_count (face));
    for (hb_codepoint_t c = 0; c <= 0x10FFFFu; c++)
    {
      hb_codepoint_t g = 0;
      if (hb_font_get_nominal_glyph (font, c, &g))
	printf ("map %u %u\n", c, g);
    }
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
  {"--names", dump_names},
  {"--cmap", dump_cmap},
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
