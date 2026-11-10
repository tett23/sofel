/*
 * hb-dump: write what HarfBuzz gives for a font's metrics (ADR 0016) and
 * for the Unicode character data (ADR 0019), for comparison with sofel.
 * Built as one translation unit with
 * HarfBuzz's src/harfbuzz.cc, so that internal functions can be called
 * as well; no FreeType integration.
 *
 * Usage: hb-dump [MODE] FILE, or hb-dump MODE for the modes without
 * a file
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
 * Mode --baselines (ADR 0017): for each face index i,
 *
 *   face <i>
 *   upem <hb_face_get_upem>
 *   then for each scale of --metrics, in the same order:
 *   scale <x_scale> <y_scale>
 *   baseline <dir> <script> <tag> <found> <value> <fallback>
 *                     (<dir> h for HB_DIRECTION_LTR, v for TTB; for
 *                     every script tag of `base_scripts` and every
 *                     script tag in the BASE table, and every
 *                     hb_ot_layout_baseline_tag_t: <found> and <value>
 *                     from hb_ot_layout_get_baseline, <value> 0 when not
 *                     found; <fallback> from
 *                     hb_ot_layout_get_baseline_with_fallback; the
 *                     language is HB_OT_TAG_DEFAULT_LANGUAGE)
 *   extents <dir> <script> <language> <found> <ascender> <descender>
 *           <line_gap>
 *                     (hb_ot_layout_get_font_extents for the same
 *                     scripts, and for `dflt` and every language system
 *                     tag in the BASE table)
 *   end
 *
 * Tags are written as eight hex digits (big-endian), since tags in a
 * font may have spaces or any other bytes.
 * Mode --vowel-constraints (ADR 0019); FILE is HarfBuzz's
 * src/ms-use/IndicShapingInvalidCluster.txt, the input of the table:
 *
 *   vowel <script> <in> <out>
 *                     (for each script of the first characters of the
 *                     file's sequences, in the order of first appearance,
 *                     and for every sequence of length 1 to 3 over the
 *                     characters of that script's sequences, in ascending
 *                     order: <in> the sequence, <out> the code points
 *                     after _hb_preprocess_text_vowel_constraints with the
 *                     buffer's script set; both hex joined by `,`)
 *
 * Modes without FILE (ADR 0019):
 *
 * --unicode: for every code point 0..0x10FFFF and 0x110000, 0x7FFFFFFF,
 * 0xFFFFFFFF, through hb_unicode_funcs_get_default ():
 *
 *   <cp> <gc> <ccc> <script> <mirroring> <decomposed> <a> <b> <ep>
 *        <indic> <use> <joining>
 *                     (hb_unicode_general_category as the number of
 *                     hb_unicode_general_category_t,
 *                     hb_unicode_combining_class, hb_unicode_script as its
 *                     four-letter tag, hb_unicode_mirroring,
 *                     hb_unicode_decompose (<a> <b> 0 0 when not
 *                     decomposed), _hb_unicode_is_emoji_Extended_Pictographic,
 *                     hb_indic_get_categories, hb_use_get_category, and
 *                     get_joining_type of hb-ot-shaper-arabic.cc; numbers
 *                     in decimal)
 *
 *   then, for every pair (a, b) with a in the first and b in the second
 *   characters of the two-character decompositions, with 0 and the
 *   Hangul L jamo (and the LV syllables) added to the first, 0 and the
 *   V and T jamo added to the second, both in ascending order:
 *
 *   compose <a> <b> <composed> <ab>
 *                     (hb_unicode_compose; <ab> 0 when not composed)
 *
 * --arabic-pua: for every code point 0..0xFFFF,
 *
 *   pua <cp> <simp> <trad>
 *                     (_hb_arabic_pua_simp_map and _hb_arabic_pua_trad_map)
 *
 * Exit status: 0 on success, 1 when the file cannot be read, 2 on wrong
 * usage.
 */

#include "harfbuzz.cc"

#include <stdio.h>
#include <stdlib.h>
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

/* The script tags written for every font: the ones Golem uses, the
 * default, and every tag (old and new) of a script with a hanging
 * character in hb_ot_layout_get_baseline_with_fallback. */
static const char *const base_scripts[] = {
  "hani", "latn", "deva", "DFLT", "kana", "hang", "cyrl", "math",
  "dev2", "dev3", "bng2", "gjr2", "gur2", "knd2",
  "beng", "gujr", "guru", "tibt", "limb", "sylo", "phag", "mtei", "shrd",
  "takr", "modi", "sidd", "tirh", "marc", "newa", "soyo", "zanb", "dogr",
  "gong", "nand",
};

static const hb_ot_layout_baseline_tag_t baseline_tags[] = {
  HB_OT_LAYOUT_BASELINE_TAG_ROMAN,
  HB_OT_LAYOUT_BASELINE_TAG_HANGING,
  HB_OT_LAYOUT_BASELINE_TAG_IDEO_FACE_BOTTOM_OR_LEFT,
  HB_OT_LAYOUT_BASELINE_TAG_IDEO_FACE_TOP_OR_RIGHT,
  HB_OT_LAYOUT_BASELINE_TAG_IDEO_FACE_CENTRAL,
  HB_OT_LAYOUT_BASELINE_TAG_IDEO_EMBOX_BOTTOM_OR_LEFT,
  HB_OT_LAYOUT_BASELINE_TAG_IDEO_EMBOX_TOP_OR_RIGHT,
  HB_OT_LAYOUT_BASELINE_TAG_IDEO_EMBOX_CENTRAL,
  HB_OT_LAYOUT_BASELINE_TAG_MATH,
};

struct tag_list_t
{
  hb_tag_t tags[1024];
  unsigned count;
  void add (hb_tag_t t)
  {
    for (unsigned i = 0; i < count; i++)
      if (tags[i] == t) return;
    if (count < sizeof (tags) / sizeof (tags[0])) tags[count++] = t;
  }
};

static unsigned be16 (const uint8_t *d, unsigned len, unsigned at, bool *ok)
{
  if (at + 2 > len) { *ok = false; return 0; }
  return (d[at] << 8) | d[at + 1];
}

static hb_tag_t be32 (const uint8_t *d, unsigned len, unsigned at, bool *ok)
{
  if (at + 4 > len) { *ok = false; return 0; }
  return ((hb_tag_t) d[at] << 24) | (d[at + 1] << 16) | (d[at + 2] << 8) | d[at + 3];
}

/* The script and language system tags of the raw BASE bytes, read
 * independently of HarfBuzz's parser (bounds-checked; whatever cannot
 * be read is skipped). */
static void base_tags (hb_face_t *face, tag_list_t *scripts, tag_list_t *languages)
{
  hb_blob_t *blob = hb_face_reference_table (face, HB_TAG ('B','A','S','E'));
  unsigned len = 0;
  const uint8_t *d = (const uint8_t *) hb_blob_get_data (blob, &len);
  bool ok = true;
  for (unsigned field = 4; field <= 6; field += 2)
  {
    unsigned axis = be16 (d, len, field, &ok);
    if (!ok || !axis) { ok = true; continue; }
    unsigned list = be16 (d, len, axis + 2, &ok);
    if (!ok || !list) { ok = true; continue; }
    list += axis;
    unsigned n = be16 (d, len, list, &ok);
    for (unsigned i = 0; ok && i < n; i++)
    {
      unsigned rec = list + 2 + 6 * i;
      hb_tag_t t = be32 (d, len, rec, &ok);
      unsigned script = be16 (d, len, rec + 4, &ok);
      if (!ok) break;
      scripts->add (t);
      if (!script) continue;
      script += list;
      unsigned m = be16 (d, len, script + 4, &ok);
      for (unsigned j = 0; ok && j < m; j++)
      {
	hb_tag_t l = be32 (d, len, script + 6 + 6 * j, &ok);
	if (ok) languages->add (l);
      }
      ok = true;
    }
    ok = true;
  }
  hb_blob_destroy (blob);
}

static void dump_baseline_scale (hb_font_t *font, const tag_list_t &scripts,
				 const tag_list_t &languages, int x, int y)
{
  hb_font_set_scale (font, x, y);
  printf ("scale %d %d\n", x, y);
  const hb_direction_t dirs[2] = {HB_DIRECTION_LTR, HB_DIRECTION_TTB};
  for (unsigned d = 0; d < 2; d++)
  {
    const char *dir = d ? "v" : "h";
    for (unsigned s = 0; s < scripts.count; s++)
    {
      for (unsigned b = 0; b < sizeof (baseline_tags) / sizeof (baseline_tags[0]); b++)
      {
	hb_position_t v = 0, fallback = 0;
	hb_bool_t found = hb_ot_layout_get_baseline (font, baseline_tags[b], dirs[d],
						     scripts.tags[s], HB_OT_TAG_DEFAULT_LANGUAGE, &v);
	if (!found) v = 0;
	hb_ot_layout_get_baseline_with_fallback (font, baseline_tags[b], dirs[d],
						 scripts.tags[s], HB_OT_TAG_DEFAULT_LANGUAGE,
						 &fallback);
	printf ("baseline %s %08x %08x %d %d %d\n", dir, (unsigned) scripts.tags[s],
		(unsigned) baseline_tags[b], found ? 1 : 0, v, fallback);
      }
      for (unsigned l = 0; l < languages.count; l++)
      {
	hb_font_extents_t e = {0, 0, 0};
	hb_bool_t found = hb_ot_layout_get_font_extents (font, dirs[d], scripts.tags[s],
							 languages.tags[l], &e);
	printf ("extents %s %08x %08x %d %d %d %d\n", dir, (unsigned) scripts.tags[s],
		(unsigned) languages.tags[l], found ? 1 : 0, e.ascender, e.descender,
		e.line_gap);
      }
    }
  }
}

static int dump_baselines (hb_blob_t *blob)
{
  unsigned count = hb_face_count (blob);
  if (!count) count = 1;
  for (unsigned i = 0; i < count; i++)
  {
    hb_face_t *face = hb_face_create (blob, i);
    hb_font_t *font = hb_font_create (face);
    int upem = (int) hb_face_get_upem (face);
    tag_list_t scripts = {{0}, 0}, languages = {{0}, 0};
    for (unsigned s = 0; s < sizeof (base_scripts) / sizeof (base_scripts[0]); s++)
      scripts.add (hb_tag_from_string (base_scripts[s], 4));
    languages.add (HB_OT_TAG_DEFAULT_LANGUAGE);
    base_tags (face, &scripts, &languages);

    printf ("face %u\n", i);
    printf ("upem %d\n", upem);
    const int scales[][2] = {
      {upem, upem}, {2 * upem, 2 * upem}, {upem / 2, upem / 2},
      {768, 768}, {-upem, -upem}, {768, -1536},
    };
    for (unsigned s = 0; s < sizeof (scales) / sizeof (scales[0]); s++)
      dump_baseline_scale (font, scripts, languages, scales[s][0], scales[s][1]);
    printf ("end\n");

    hb_font_destroy (font);
    hb_face_destroy (face);
  }
  return 0;
}
static void print_hex_list (const hb_codepoint_t *u, unsigned n)
{
  for (unsigned i = 0; i < n; i++) printf ("%s%04X", i ? "," : "", u[i]);
}

/* The characters of the sequences of IndicShapingInvalidCluster.txt, by
 * the script of each sequence's first character. */
struct vowel_script_t
{
  hb_script_t script;
  hb_codepoint_t chars[256];
  unsigned count;
};

static int compare_codepoints (const void *a, const void *b)
{
  hb_codepoint_t x = *(const hb_codepoint_t *) a, y = *(const hb_codepoint_t *) b;
  return x < y ? -1 : x > y;
}

static int dump_vowel_constraints (hb_blob_t *blob)
{
  hb_unicode_funcs_t *ufuncs = hb_unicode_funcs_get_default ();
  unsigned len = 0;
  const char *data = hb_blob_get_data (blob, &len);
  static vowel_script_t scripts[64];
  unsigned num_scripts = 0;
  for (unsigned i = 0; i < len;)
  {
    unsigned end = i;
    while (end < len && data[end] != '\n') end++;
    /* The code points before `;` (and before `#`). */
    hb_codepoint_t seq[16];
    unsigned n = 0;
    for (unsigned j = i; j < end && data[j] != ';' && data[j] != '#';)
    {
      char *stop = nullptr;
      char word[16] = {0};
      unsigned w = 0;
      while (j < end && (data[j] == ' ' || data[j] == '\t' || data[j] == '\r')) j++;
      while (j < end && w < 15 && ((data[j] >= '0' && data[j] <= '9') ||
				   (data[j] >= 'A' && data[j] <= 'F') ||
				   (data[j] >= 'a' && data[j] <= 'f')))
	word[w++] = data[j++];
      if (!w) break;
      if (n < 16) seq[n++] = (hb_codepoint_t) strtoul (word, &stop, 16);
    }
    i = end + 1;
    if (!n) continue;
    hb_script_t sc = hb_unicode_script (ufuncs, seq[0]);
    unsigned s = 0;
    while (s < num_scripts && scripts[s].script != sc) s++;
    if (s == num_scripts)
    {
      if (num_scripts == 64) return 1;
      scripts[num_scripts].script = sc;
      scripts[num_scripts].count = 0;
      num_scripts++;
    }
    for (unsigned k = 0; k < n; k++)
    {
      bool seen = false;
      for (unsigned c = 0; c < scripts[s].count; c++) seen |= scripts[s].chars[c] == seq[k];
      if (!seen && scripts[s].count < 256) scripts[s].chars[scripts[s].count++] = seq[k];
    }
  }

  hb_buffer_t *buffer = hb_buffer_create ();
  for (unsigned s = 0; s < num_scripts; s++)
  {
    vowel_script_t &v = scripts[s];
    qsort (v.chars, v.count, sizeof (v.chars[0]), compare_codepoints);
    char tag[5];
    hb_tag_to_string (v.script, tag);
    tag[4] = '\0';
    for (unsigned length = 1; length <= 3; length++)
    {
      unsigned total = 1;
      for (unsigned k = 0; k < length; k++) total *= v.count;
      for (unsigned index = 0; index < total; index++)
      {
	hb_codepoint_t seq[3];
	unsigned rest = index;
	for (unsigned k = length; k-- > 0;)
	{
	  seq[k] = v.chars[rest % v.count];
	  rest /= v.count;
	}
	hb_buffer_clear_contents (buffer);
	hb_buffer_add_utf32 (buffer, seq, (int) length, 0, (int) length);
	hb_buffer_set_script (buffer, v.script);
	hb_buffer_set_direction (buffer, HB_DIRECTION_LTR);
	_hb_preprocess_text_vowel_constraints (nullptr, buffer, nullptr);
	unsigned out_len = 0;
	hb_glyph_info_t *info = hb_buffer_get_glyph_infos (buffer, &out_len);
	hb_codepoint_t out[8];
	unsigned m = out_len < 8 ? out_len : 8;
	for (unsigned k = 0; k < m; k++) out[k] = info[k].codepoint;
	printf ("vowel %s ", tag);
	print_hex_list (seq, length);
	printf (" ");
	print_hex_list (out, m);
	printf ("\n");
      }
    }
  }
  hb_buffer_destroy (buffer);
  return 0;
}

static void dump_unicode_value (hb_unicode_funcs_t *ufuncs, hb_codepoint_t u)
{
  hb_unicode_general_category_t gc = hb_unicode_general_category (ufuncs, u);
  hb_codepoint_t a = 0, b = 0;
  hb_bool_t decomposed = hb_unicode_decompose (ufuncs, u, &a, &b);
  if (!decomposed) a = b = 0;
  char tag[5];
  hb_tag_to_string (hb_unicode_script (ufuncs, u), tag);
  tag[4] = '\0';
  printf ("%u %u %u %s %u %d %u %u %d %u %u %u\n", u, (unsigned) gc,
	  hb_unicode_combining_class (ufuncs, u), tag,
	  hb_unicode_mirroring (ufuncs, u), decomposed ? 1 : 0, a, b,
	  _hb_unicode_is_emoji_Extended_Pictographic (u) ? 1 : 0,
	  (unsigned) hb_indic_get_categories (u), (unsigned) hb_use_get_category (u),
	  get_joining_type (u, gc));
}

static int dump_unicode ()
{
  hb_unicode_funcs_t *ufuncs = hb_unicode_funcs_get_default ();
  for (hb_codepoint_t u = 0; u <= 0x10FFFFu; u++)
    dump_unicode_value (ufuncs, u);
  const hb_codepoint_t beyond[] = {0x110000u, 0x7FFFFFFFu, 0xFFFFFFFFu};
  for (unsigned i = 0; i < 3; i++)
    dump_unicode_value (ufuncs, beyond[i]);

  /* Membership of the first and second characters of the pairs. */
  static bool first[0x110000], second[0x110000];
  first[0] = second[0] = true;
  for (hb_codepoint_t u = 0; u <= 0x10FFFFu; u++)
  {
    hb_codepoint_t a = 0, b = 0;
    if (hb_unicode_decompose (ufuncs, u, &a, &b) && b)
    {
      if (a < 0x110000u) first[a] = true;
      if (b < 0x110000u) second[b] = true;
    }
  }
  for (hb_codepoint_t l = 0x1100u; l < 0x1100u + 19; l++) first[l] = true;
  for (hb_codepoint_t s = 0xAC00u; s < 0xAC00u + 11172; s += 28) first[s] = true;
  for (hb_codepoint_t v = 0x1161u; v < 0x1161u + 21; v++) second[v] = true;
  for (hb_codepoint_t t = 0x11A7u; t < 0x11A7u + 28; t++) second[t] = true;
  static hb_codepoint_t seconds[0x110000];
  unsigned num_seconds = 0;
  for (hb_codepoint_t b = 0; b <= 0x10FFFFu; b++)
    if (second[b]) seconds[num_seconds++] = b;
  for (hb_codepoint_t a = 0; a <= 0x10FFFFu; a++)
  {
    if (!first[a]) continue;
    for (unsigned i = 0; i < num_seconds; i++)
    {
      hb_codepoint_t b = seconds[i];
      hb_codepoint_t ab = 0;
      hb_bool_t composed = hb_unicode_compose (ufuncs, a, b, &ab);
      printf ("compose %u %u %d %u\n", a, b, composed ? 1 : 0, composed ? ab : 0);
    }
  }
  return 0;
}

static int dump_arabic_pua ()
{
  for (unsigned u = 0; u <= 0xFFFFu; u++)
    printf ("pua %u %u %u\n", u, (unsigned) _hb_arabic_pua_simp_map (u),
	    (unsigned) _hb_arabic_pua_trad_map (u));
  return 0;
}

/* The modes that read no file. */
struct fileless_mode_t
{
  const char *name;
  int (*dump) ();
};

static const fileless_mode_t fileless_modes[] = {
  {"--unicode", dump_unicode},
  {"--arabic-pua", dump_arabic_pua},
};

struct dump_mode_t
{
  const char *name;
  int (*dump) (hb_blob_t *blob);
};

static const dump_mode_t modes[] = {
  {"--metrics", dump_metrics},
  {"--names", dump_names},
  {"--cmap", dump_cmap},
  {"--baselines", dump_baselines},
  {"--vowel-constraints", dump_vowel_constraints},
};

int main (int argc, char **argv)
{
  if (argc == 2)
    for (unsigned i = 0; i < sizeof (fileless_modes) / sizeof (fileless_modes[0]); i++)
      if (strcmp (argv[1], fileless_modes[i].name) == 0)
	return fileless_modes[i].dump ();
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
    fprintf (stderr, "; or hb-dump MODE, MODE one of:");
    for (unsigned i = 0; i < sizeof (fileless_modes) / sizeof (fileless_modes[0]); i++)
      fprintf (stderr, " %s", fileless_modes[i].name);
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
