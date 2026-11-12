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
 * Mode --sampled-metrics FONT PLAN (ADR 0031): PLAN contains one
 * `<face-index> <glyph-id>` pair per line, at most 128 sorted glyphs per
 * face. Writes the --metrics lines at the units-per-em scale only, for
 * those glyphs. The `glyph` line retains its actual glyph ID.
 * Mode --cmap-digest (ADR 0031): for each face, a 64-bit FNV-1a digest
 * of the nominal glyph ID as four little-endian bytes for every Unicode
 * scalar value in ascending order. Unmapped scalars contribute glyph 0.
 * Mode --corpus-shape FONT FACE HEX-CODEPOINTS SCRIPT LANGUAGE DIRECTION
 * FEATURES VARIATIONS (ADR 0031): one shaping case with explicit buffer
 * properties, feature settings and design coordinates. Use `-` for no
 * feature or variation setting. The output is --shape's glyph text.
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
 * Mode --layout-map (ADR 0020): for each face i,
 *
 *   face <i>
 *   sel <script> <language> (<found> <script index> <chosen> <language index>)x2
 *                     (for every script of the code points, Zmth and
 *                     HB_SCRIPT_INVALID (0), as eight hex digits, and
 *                     every language: `*` for none and
 *                     hb_ot_tag_to_language of every language system
 *                     tag of GSUB and GPOS; the tags of
 *                     hb_ot_tags_from_script_and_language, then for
 *                     GSUB and GPOS hb_ot_layout_table_select_script
 *                     (found 0 or 1, the index, the chosen tag in hex)
 *                     and hb_ot_layout_script_select_language)
 *   end
 *
 * Tags are written as eight hex digits (big-endian), since tags in a
 * font may have spaces or any other bytes.
 * Mode --paint (ADR 0023), or --paint=SLANT with
 * hb_font_set_synthetic_slant (font, SLANT): for each face index i,
 *
 *   face <i>
 *   paint <palette> <ppem> <glyph> <n>
 *                     (hb_font_paint_glyph with HB_COLOR (0, 0, 0, 255)
 *                     as the foreground, for every palette (palette 0
 *                     when there are none), every ppem in 0 and the
 *                     strikes' ppems of CBLC and sbix (set with
 *                     hb_font_set_ppem), and every glyph; then <n> lines
 *                     of the text of HarfBuzz's test/api/test-paint.c,
 *                     whose image line ends with an empty line. An image
 *                     without extents (SVG) writes `extents -`)
 *   end
 *
 * Mode --color (ADR 0023): for each face index i,
 *
 *   face <i>
 *   has <layers> <paint> <palettes> <svg> <png>
 *                     (hb_ot_color_has_*)
 *   palettes <count>  (hb_ot_color_palette_get_count)
 *   palette <p> <name_id> <flags> <colors>
 *                     (hb_ot_color_palette_get_name_id, _get_flags and
 *                     _get_colors: RRGGBBAA hex joined by `,`, `-` for
 *                     none)
 *   color_name <c> <name_id>
 *                     (hb_ot_color_palette_color_get_name_id for every
 *                     color of palette 0's count)
 *   glyph <g> <has_paint> <layers> <svg_index> <svg_start> <svg_end>
 *         <svg_length> <png_length>
 *                     (hb_ot_color_glyph_has_paint;
 *                     hb_ot_color_glyph_get_layers as glyph:index joined
 *                     by `,`, `-` for none;
 *                     hb_ot_color_glyph_get_svg_document_index and
 *                     hb_ot_color_get_svg_document_glyph_range, `-` for
 *                     none; the lengths of hb_ot_color_glyph_reference_svg
 *                     and hb_ot_color_glyph_reference_png; every glyph)
 *   end
 *
 * Mode --extents (ADR 0023): for each face index i,
 *
 *   face <i>
 *   extents <g> <found> <x_bearing> <y_bearing> <width> <height>
 *                     (hb_font_get_glyph_extents at the default scale,
 *                     every glyph; 0 when not found)
 *   end
 *
 * Mode --syllables MACHINE ALPHABET COUNT SEED (ADR 0024): HarfBuzz's
 * syllable machine MACHINE (indic, khmer, myanmar or use;
 * find_syllables_<MACHINE>) run on buffers whose shaper categories are
 * set directly (all other fields zero). ALPHABET is the category values,
 * decimal, joined by `,`. For every sequence over ALPHABET of length 1
 * to 4 (shorter first; within a length in lexicographic order of the
 * positions in ALPHABET):
 *
 *   e <syllables>
 *
 * then for COUNT sequences of length 5 to 32 from the pseudo-random
 * numbers of --shape-random seeded with SEED (the length is 5 plus the
 * next number modulo 28; each value is ALPHABET[next number modulo its
 * size]):
 *
 *   r <values> <syllables>
 *
 * <values> are the categories, hex joined by `,`; <syllables> is the
 * syllable byte of every item, two hex digits each, concatenated.
 *
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
 *        <indic> <use> <joining> <mcc> <di> <space> <vert>
 *                     (hb_unicode_general_category as the number of
 *                     hb_unicode_general_category_t,
 *                     hb_unicode_combining_class, hb_unicode_script as its
 *                     four-letter tag, hb_unicode_mirroring,
 *                     hb_unicode_decompose (<a> <b> 0 0 when not
 *                     decomposed), _hb_unicode_is_emoji_Extended_Pictographic,
 *                     hb_indic_get_categories, hb_use_get_category, and
 *                     get_joining_type of hb-ot-shaper-arabic.cc, and
 *                     (ADR 0020) hb_unicode_funcs_t's
 *                     modified_combining_class, is_default_ignorable
 *                     (0 or 1), space_fallback_type and vertical_char_for;
 *                     numbers in decimal)
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
 * --ot-tags (ADR 0020): HarfBuzz's conversion between scripts and
 * languages and OpenType tags (hb-ot-tag.cc). Tags are eight hex digits,
 * a list of them `-` for none or joined by `,`; a language is
 * hb_language_to_string, `*` for none.
 *
 *   scripts <tags>   the scripts: those of every code point, and
 *                    HB_SCRIPT_INVALID (0), Zmth and Hrkt, sorted
 *   branch <kind> <s>
 *                    the strings the branches of the generated
 *                    hb_ot_tags_from_complex_language test, read by
 *                    running a copy of it with recording tests: <kind>
 *                    l lang_matches and e strcmp (<s> the whole
 *                    language), n strncmp (<s> the prefix), s and p
 *                    subtag_matches from the language and from its
 *                    first dash (<s> the subtag)
 *   f <input> <language> <language tags> <script tags>...
 *                    hb_ot_tags_from_script_and_language with
 *                    HB_OT_MAX_TAGS_PER_SCRIPT and _PER_LANGUAGE, for
 *                    the language hb_language_from_string (<input>) and
 *                    every script, in the order of `scripts`. The
 *                    inputs, sorted: every language of the tables and
 *                    of hb_ot_ambiguous_tag_to_language, the branches'
 *                    whole languages, every primary language of the
 *                    branches (and und, en) x none or every other
 *                    subtag of the branches x none or every region
 *                    subtag of the branches x no private use, -x-...
 *                    and -u-...-x-hbsc-latn, and the languages of
 *                    test/api/test-ot-tag.c and more edge cases
 *   c <input> <script> <script count> <language count> <script tags>
 *     <language tags>
 *                    the same with the counts 0 to 4, for those last
 *                    languages and the scripts 0, Latn, Deva, Mymr,
 *                    Hira and Mlym
 *   s <script tag> <script>
 *                    hb_ot_tag_to_script, for every script tag of the
 *                    `f` lines and DFLT
 *   l <language tag> <language>
 *                    hb_ot_tag_to_language, for every language tag of
 *                    the `f` lines and of the tables, and dflt
 *   r <script tag> <language tag> <script> <language>
 *                    hb_ot_tags_to_script_and_language for every pair
 *                    of those
 *   end
 * --features (ADR 0020): for every string of the feature string set
 * (feature_tags and the other tables below: their direct product, then
 * feature_extras),
 *
 *   feature =<string> 1 <tag> <value> <start> <end> =<to_string>
 *   feature =<string> 0
 *                     (hb_feature_from_string, then hb_feature_to_string
 *                     when it parses; <tag> as eight hex digits)
 *
 * Strings are written after a `=`, with the bytes 0x21..0x7E other than
 * `\` as they are and any other byte as `\xHH`.
 *
 * Modes with a font and a second file (ADR 0020):
 *
 * --serialize FONT TEXTFILE: for every line of TEXTFILE (without its
 * newline), in a buffer of its UTF-8 with
 * hb_buffer_guess_segment_properties, shaped with hb_shape and no
 * features on face 0 at the default scale:
 *
 *   line <n> =<text>
 *   unicode <cp>,<cluster> ...   (the buffer before shaping)
 *   serialize unicode <format> <flags> =<string>
 *   deserialize unicode <format> <ok> <end> <len> <cp>,<cluster> ...
 *                     (for text and json and every flags value 0..63:
 *                     hb_buffer_serialize as hb-shape calls it, then
 *                     hb_buffer_deserialize_unicode of that string into
 *                     a new buffer: the result, end_ptr - buf and the
 *                     items)
 *   glyphs <g>,<cluster>,<mask>,<x_advance>,<y_advance>,<x_offset>,<y_offset> ...
 *                     (the shaped buffer, the mask as it is)
 *   serialize glyphs <format> <flags> =<string>
 *   deserialize glyphs <format> <ok> <end> <len> <glyph items> ...
 *                     (likewise with the font, and
 *                     hb_buffer_deserialize_glyphs with the font)
 *
 * then, for every input of test-buffer.c's serialization tests and every
 * prefix of it (the lengths 0 to the whole),
 *
 *   corpus =<string>
 *   deserialize glyphs text ..., deserialize glyphs json ...,
 *   deserialize unicode text ..., deserialize unicode json ...
 *
 * --deserialize FONT CORPUS: the `corpus` block above for every line of
 * CORPUS, a string written as above without the `=`.
 * --shape FONT TEXTFILE DIRECTION FEATURES (ADR 0020): for every line of
 * TEXTFILE (split at `\n`, a last empty piece dropped), a buffer of
 * its UTF-8 (hb_buffer_add_utf8 of the line alone), the direction set
 * unless DIRECTION is `auto` (hb_direction_from_string),
 * hb_buffer_guess_segment_properties, and hb_shape with FEATURES (a
 * list as hb-shape's --features reads it; `` for none) on face 0 at the
 * default scale; one line each, as hb-shape writes it with
 * --no-glyph-names --show-flags and UTF-8 cluster values.
 *
 * --shape-random FONT COUNT SEED (ADR 0020): for each face i,
 *
 *   face <i>
 *   script <tag> <n>   (for each script (hb_unicode_script, eight hex
 *                      digits) of the n code points 0..U+10FFFF that
 *                      hb_font_get_nominal_glyph maps, in tag order)
 *   s <cps> <glyphs>   (COUNT strings: with state = SEED at the start of
 *                      each script and next () = ((state = state *
 *                      1103515245 + 12345) >> 16) & 0x7FFF, a length
 *                      1 + next () % 16 and that many code points
 *                      cps[next () % n] of the script, in hex joined by
 *                      `,`; shaped with hb_buffer_add_codepoints, the
 *                      flags BOT, EOT and PRODUCE_UNSAFE_TO_CONCAT,
 *                      hb_buffer_guess_segment_properties and hb_shape,
 *                      written as --shape writes them)
 *   end
 *
 * --roundf (ADR 0026): reads lines from stdin, each `f <bits>` (a
 * `float`, eight hex digits of its bits) or `d <bits>` (a `double`,
 * sixteen hex digits), and writes for each the sixteen hex digits of the
 * bits of HarfBuzz's `roundf` of the value (the macro of hb-algs.hh,
 * `_hb_roundf`, whose result is a `double`).
 *
 * --mult (ADR 0026): reads lines `<upem> <scale>` from stdin and writes
 * for each the `x_mult` of an hb_font_t of a face with that upem after
 * hb_font_set_scale (font, scale, scale), in decimal.
 *
 * --repack FILE (ADR 0022, 再配置): FILE holds object graphs, all
 * numbers big-endian: a u32 count, then per graph a u32 tag, a u32 kind
 * and a u32 object count, then per object (the root last) a u32 length
 * and its bytes, a u32 count of real links, each `width signed whence 0`
 * (u8 each) and u32 bias, position and objidx (1 is the first object),
 * and a u32 count of virtual links, each a u32 objidx. Kind 0 serializes
 * the objects with hb_serialize_context_t (push, the bytes, add_link,
 * add_virtual_link, pop_pack (false), end_serialize) and repacks as
 * _hb_subset_repack does (hb_resolve_overflows with its defaults when an
 * offset overflowed); kind 1 calls hb_subset_serialize_or_fail (whose
 * links have only width, position and objidx). Writes per graph
 *
 *   graph <i> ok <length> <fnv>   (the FNV-1a 64-bit hash of the bytes,
 *                                 16 hex digits)
 *   graph <i> fail
 *
 * --subset OPTION... (ADR 0022, 突き合わせ): subsets a font as
 * HarfBuzz's util/hb-subset does with these options, and writes the font
 * hb_subset_or_fail and hb_face_reference_blob give. hb-dump is built
 * with HB_EXPERIMENTAL_API (tools/hb-dump/build.sh), as HarfBuzz's
 * subsetting tests are, so that --iftb-requirements and
 * --retain-num-glyphs work. The options are those of hb-subset, applied
 * in order with the same semantics: --font-file=FILE (required),
 * --face-index=N, --output-file=FILE or -o FILE (required), --gids,
 * --unicodes, --text, --glyphs, --name-IDs, --name-languages,
 * --layout-features, --layout-scripts and --drop-tables (each also with
 * a `+` or `-` suffix, and `*`), --gid-map, --variations and
 * --instance, --keep-everything, every flag option, --preprocess and
 * --preprocess-face, and --num-iterations=N. An option's value follows
 * `=` or comes as the next argument. The *-file options and the
 * positional FONT-FILE and TEXT are not accepted.
 * Exit status: 0 when the font was written, 1 when subsetting failed
 * (nothing is written) or a file cannot be read or written, 2 on a wrong
 * option.
 *
 * Exit status: 0 on success, 1 when the file cannot be read, 2 on wrong
 * usage.
 */

#include "harfbuzz.cc"
/* The subsetter: the files of harfbuzz-subset.cc that harfbuzz.cc does
 * not include. */
#include "graph/gsubgpos-context.cc"
#include "hb-depend.cc"
#include "hb-subset-cff-common.cc"
#include "hb-subset-cff1.cc"
#include "hb-subset-cff2.cc"
#include "hb-subset-input.cc"
#include "hb-subset-instancer-iup.cc"
#include "hb-subset-instancer-solver.cc"
#include "hb-subset-plan-layout.cc"
#include "hb-subset-plan-var.cc"
#include "hb-subset-plan.cc"
#include "hb-subset-serialize.cc"
#include "hb-subset-table-cff.cc"
#include "hb-subset-table-color.cc"
#include "hb-subset-table-layout.cc"
#include "hb-subset-table-other.cc"
#include "hb-subset-table-var.cc"
#include "hb-subset.cc"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>

#include <algorithm>
#include <cmath>
#include <string>
#include <vector>

static unsigned reference_named = HB_FONT_NO_VAR_NAMED_INSTANCE;
static std::vector<hb_variation_t> reference_variations;

static hb_font_t *var_create_font (hb_face_t *face)
{
  hb_font_t *font = hb_font_create (face);
  if (reference_named != HB_FONT_NO_VAR_NAMED_INSTANCE)
    hb_font_set_var_named_instance (font, reference_named);
  if (!reference_variations.empty())
    hb_font_set_variations (font, reference_variations.data(), reference_variations.size());
  return font;
}

/* Remove variation options while preserving each mode's positional arguments. */
static bool reference_options (int *argc, char **argv)
{
  int write = 2;
  for (int read = 2; read < *argc; read++)
  {
    const char *arg = argv[read];
    if (strncmp (arg, "--named-instance=", 17) == 0)
    {
      char *end;
      unsigned long value = strtoul (arg + 17, &end, 10);
      if (!arg[17] || *end || value > 0xFFFFFFFFul) return false;
      reference_named = (unsigned) value;
    }
    else if (strncmp (arg, "--variations=", 13) == 0)
    {
      const char *p = arg + 13;
      if (!*p) return false;
      while (*p)
      {
        const char *end = strchr (p, ',');
        unsigned len = end ? (unsigned) (end - p) : (unsigned) strlen (p);
        hb_variation_t item;
        if (!hb_variation_from_string (p, (int) len, &item)) return false;
        reference_variations.push_back (item);
        if (!end) break;
        p = end + 1;
        if (!*p) return false;
      }
    }
    else argv[write++] = argv[read];
  }
  *argc = write;
  return true;
}

/* ADR 0027 task 2: keep the exact float input bits in reference output. */
static unsigned var_float_bits (float value)
{
  unsigned bits;
  memcpy (&bits, &value, sizeof bits);
  return bits;
}

static bool var_options (int argc, char **argv, unsigned *named,
                         std::vector<hb_variation_t> *variations)
{
  *named = HB_FONT_NO_VAR_NAMED_INSTANCE;
  for (int i = 2; i < argc - 1; i++)
  {
    if (strncmp (argv[i], "--named-instance=", 17) == 0)
    {
      char *end;
      unsigned long value = strtoul (argv[i] + 17, &end, 10);
      if (!* (argv[i] + 17) || *end || value > 0xFFFFFFFFul) return false;
      *named = (unsigned) value;
    }
    else if (strncmp (argv[i], "--variations=", 13) == 0)
    {
      const char *p = argv[i] + 13;
      if (!*p) return false;
      while (*p)
      {
        const char *end = strchr (p, ',');
        unsigned len = end ? (unsigned) (end - p) : (unsigned) strlen (p);
        hb_variation_t item;
        if (!hb_variation_from_string (p, (int) len, &item)) return false;
        variations->push_back (item);
        if (!end) break;
        p = end + 1;
        if (!*p) return false;
      }
    }
    else return false;
  }
  return true;
}

static int dump_var_reference (int argc, char **argv)
{
  bool normalize = strcmp (argv[1], "--var-normalize") == 0;
  unsigned named;
  std::vector<hb_variation_t> variations;
  if (argc < 3 || !var_options (argc, argv, &named, &variations))
  {
    fprintf (stderr, "usage: hb-dump --var[\u002dnormalize] [--named-instance=N] [--variations=TAG=VALUE,...] FILE\n");
    return 2;
  }
  hb_blob_t *blob = hb_blob_create_from_file_or_fail (argv[argc - 1]);
  if (!blob) return 1;
  unsigned faces = hb_face_count (blob);
  for (unsigned i = 0; i < faces; i++)
  {
    hb_face_t *face = hb_face_create (blob, i);
    hb_font_t *font = var_create_font (face);
    hb_ot_font_set_funcs (font);
    if (named != HB_FONT_NO_VAR_NAMED_INSTANCE)
      hb_font_set_var_named_instance (font, named);
    if (!variations.empty())
      hb_font_set_variations (font, variations.data(), variations.size());
    unsigned count = hb_ot_var_get_axis_count (face);
    std::vector<hb_ot_var_axis_info_t> axes (count);
    unsigned actual = count;
    hb_ot_var_get_axis_infos (face, 0, &actual, axes.data());
    axes.resize (actual);
    printf ("face %u\naxis_count %u\n", i, count);
    if (!normalize)
    {
      printf ("has_data %u\n", (unsigned) hb_ot_var_has_data (face));
      for (const auto &axis : axes)
      {
        hb_ot_var_axis_info_t found;
        hb_bool_t ok = hb_ot_var_find_axis_info (face, axis.tag, &found);
        printf ("axis %u %08x %u %u %08x %08x %08x\n",
                axis.axis_index, axis.tag, axis.name_id, (unsigned) axis.flags,
                var_float_bits (axis.min_value),
                var_float_bits (axis.default_value),
                var_float_bits (axis.max_value));
        printf ("found_axis %08x %u %u\n", axis.tag, (unsigned) ok,
                ok ? found.axis_index : 0);
      }
      unsigned named_count = hb_ot_var_get_named_instance_count (face);
      printf ("named_count %u\n", named_count);
      for (unsigned n = 0; n < named_count; n++)
      {
        unsigned length = count;
        std::vector<float> coords (count);
        hb_ot_var_named_instance_get_design_coords (face, n, &length, coords.data());
        printf ("named %u %u %u", n,
                hb_ot_var_named_instance_get_subfamily_name_id (face, n),
                hb_ot_var_named_instance_get_postscript_name_id (face, n));
        for (unsigned a = 0; a < length; a++)
          printf (" %08x", var_float_bits (coords[a]));
        printf ("\n");
      }
      unsigned length = 0;
      const int *normalized = hb_font_get_var_coords_normalized (font, &length);
      printf ("coords_normalized");
      for (unsigned a = 0; a < length; a++) printf (" %d", normalized[a]);
      printf ("\n");
      std::vector<int> normalized_variations (count);
      hb_ot_var_normalize_variations (face, variations.data(), variations.size(),
                                      normalized_variations.data(), count);
      printf ("normalized_variations");
      for (unsigned a = 0; a < count; a++)
        printf (" %d", normalized_variations[a]);
      printf ("\n");
      const float *design = hb_font_get_var_coords_design (font, &length);
      printf ("coords_design");
      for (unsigned a = 0; a < length; a++)
        printf (" %08x", var_float_bits (design[a]));
      printf ("\n");
    }
    else
    {
      std::vector<float> coords (axes.size());
      std::vector<int> result (axes.size());
      for (unsigned a = 0; a < axes.size(); a++)
      {
        for (unsigned b = 0; b < axes.size(); b++)
          coords[b] = axes[b].default_value;
        float min = axes[a].min_value, max = axes[a].max_value;
        for (unsigned k = 0; k <= 256; k++)
        {
          coords[a] = min + (max - min) * (float) k / 256.f;
          hb_ot_var_normalize_coords (face, coords.size(), coords.data(), result.data());
          printf ("sample %u %u %08x %d\n", a, k,
                  var_float_bits (coords[a]), result[a]);
        }
        const char *labels[] = {"outside_min", "outside_max", "half_min", "half_max"};
        float span = max - min;
        float def = axes[a].default_value;
        float values[] = {min - span, max + span,
                          def - (def - min) / 32768.f,
                          def + (max - def) / 32768.f};
        for (unsigned k = 0; k < 4; k++)
        {
          coords[a] = values[k];
          hb_ot_var_normalize_coords (face, coords.size(), coords.data(), result.data());
          printf ("sample %u %s %08x %d\n", a, labels[k],
                  var_float_bits (coords[a]), result[a]);
        }
      }
    }
    printf ("end\n");
    hb_font_destroy (font);
    hb_face_destroy (face);
  }
  hb_blob_destroy (blob);
  return 0;
}

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

static void dump_scale (hb_font_t *font, unsigned num_glyphs, int x, int y,
			const unsigned *selected = nullptr, unsigned selected_count = 0)
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

  for (unsigned n = 0; n < (selected ? selected_count : num_glyphs); n++)
  {
    unsigned g = selected ? selected[n] : n;
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

/* Raw MVAR records, including tags outside hb_ot_metrics_tag_t's named constants. */
static int dump_metrics_variation (hb_blob_t *blob)
{
  unsigned count = hb_face_count (blob);
  for (unsigned i = 0; i < count; i++)
  {
    hb_face_t *face = hb_face_create (blob, i);
    hb_font_t *font = var_create_font (face);
    hb_font_set_scale (font, 768, -1536);
    hb_blob_t *table = hb_face_reference_table (face, HB_TAG ('M','V','A','R'));
    unsigned len;
    const unsigned char *data = (const unsigned char *) hb_blob_get_data (table, &len);
    printf ("face %u\n", i);
    if (len >= 12)
    {
      unsigned stride = data[6] * 256u + data[7];
      unsigned records = data[8] * 256u + data[9];
      if (stride >= 8 && records <= (len - 12) / stride)
        for (unsigned j = 0; j < records; j++)
        {
          const unsigned char *p = data + 12 + stride * j;
          auto tag = (hb_ot_metrics_tag_t) HB_TAG (p[0], p[1], p[2], p[3]);
          float delta = hb_ot_metrics_get_variation (font, tag);
          uint32_t bits;
          memcpy (&bits, &delta, sizeof bits);
          printf ("%08x %08x %d %d\n", (unsigned) tag, (unsigned) bits,
                  hb_ot_metrics_get_x_variation (font, tag),
                  hb_ot_metrics_get_y_variation (font, tag));
        }
    }
    puts ("end");
    hb_blob_destroy (table);
    hb_font_destroy (font);
    hb_face_destroy (face);
  }
  return 0;
}

static int dump_metrics (hb_blob_t *blob)
{
  unsigned count = hb_face_count (blob);
  if (!count) count = 1;
  for (unsigned i = 0; i < count; i++)
  {
    hb_face_t *face = hb_face_create (blob, i);
    hb_font_t *font = var_create_font (face);
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

static int dump_sampled_metrics (hb_blob_t *blob, const char *plan)
{
  unsigned faces = hb_face_count (blob);
  if (!faces) faces = 1;
  for (unsigned i = 0; i < faces; i++)
  {
    FILE *fp = fopen (plan, "r");
    if (!fp) { fprintf (stderr, "hb-dump: cannot read %s\n", plan); return 1; }
    hb_face_t *face = hb_face_create (blob, i);
    hb_font_t *font = var_create_font (face);
    unsigned num_glyphs = hb_face_get_glyph_count (face);
    unsigned selected[128], count = 0, face_index = 0, glyph = 0;
    int scanned = 0;
    while ((scanned = fscanf (fp, "%u %u", &face_index, &glyph)) == 2)
    {
      if (face_index >= faces || glyph >= num_glyphs ||
	  (face_index == i && (count == 128 || (count && selected[count - 1] >= glyph))))
      {
	fprintf (stderr, "hb-dump: invalid sampled glyph %u %u\n", face_index, glyph);
	fclose (fp); hb_font_destroy (font); hb_face_destroy (face); return 1;
      }
      if (face_index == i) selected[count++] = glyph;
    }
    fclose (fp);
    if (scanned != EOF || (num_glyphs && !count))
    {
      fprintf (stderr, "hb-dump: invalid or empty plan for face %u\n", i);
      hb_font_destroy (font); hb_face_destroy (face); return 1;
    }
    int upem = (int) hb_face_get_upem (face);
    printf ("face %u\nnum_glyphs %u\nupem %d\n", i, num_glyphs, upem);
    dump_scale (font, num_glyphs, upem, upem, selected, count);
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
    hb_font_t *font = var_create_font (face);
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
    hb_font_t *font = var_create_font (face);
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

static int dump_cmap_digest (hb_blob_t *blob)
{
  unsigned count = hb_face_count (blob);
  if (!count) count = 1;
  for (unsigned i = 0; i < count; i++)
  {
    hb_face_t *face = hb_face_create (blob, i);
    hb_font_t *font = var_create_font (face);
    uint64_t hash = UINT64_C (14695981039346656037);
    for (hb_codepoint_t c = 0; c <= 0x10FFFFu; c++)
    {
      if (c >= 0xD800u && c <= 0xDFFFu) continue;
      hb_codepoint_t g = 0;
      hb_font_get_nominal_glyph (font, c, &g);
      for (unsigned b = 0; b < 4; b++)
      {
	hash ^= (g >> (b * 8)) & 0xFFu;
	hash *= UINT64_C (1099511628211);
      }
    }
    printf ("face %u cmap %016llx\n", i, (unsigned long long) hash);
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
    hb_font_t *font = var_create_font (face);
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
  printf ("%u %u %u %s %u %d %u %u %d %u %u %u %u %d %u %u\n", u, (unsigned) gc,
	  hb_unicode_combining_class (ufuncs, u), tag,
	  hb_unicode_mirroring (ufuncs, u), decomposed ? 1 : 0, a, b,
	  _hb_unicode_is_emoji_Extended_Pictographic (u) ? 1 : 0,
	  (unsigned) hb_indic_get_categories (u), (unsigned) hb_use_get_category (u),
	  get_joining_type (u, gc),
	  ufuncs->modified_combining_class (u),
	  hb_unicode_funcs_t::is_default_ignorable (u) ? 1 : 0,
	  (unsigned) hb_unicode_funcs_t::space_fallback_type (u),
	  hb_unicode_funcs_t::vertical_char_for (u));
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

/* --ot-tags (ADR 0020) */

/* The strings the branches of hb_ot_tags_from_complex_language test,
 * read from HarfBuzz's compiled code rather than from the header's
 * text: a second copy of hb-ot-tag-table.hh with the four tests
 * replaced by recorders that fail (strncmp succeeds, so that the
 * subtag_matches after it is reached), called once per first byte. */
namespace ot_tag_probe {
struct record_t { char kind; char first; const char *s; };
static record_t records[4096];
static unsigned num_records;
static char first_byte;
static void record (char kind, const char *s)
{
  if (num_records < sizeof (records) / sizeof (records[0]))
  {
    records[num_records].kind = kind;
    records[num_records].first = first_byte;
    records[num_records].s = s;
    num_records++;
  }
}
/* kind: p subtag_matches (p, ...), s subtag_matches (lang_str, ...),
 * l lang_matches, n strncmp, e strcmp */
static bool subtag (const char *from, const char *s) { record (from[0] == 'p' ? 'p' : 's', s); return false; }
static bool lang (const char *s) { record ('l', s); return false; }
static int ncmp (const char *s) { record ('n', s); return 0; }
static int cmp (const char *s) { record ('e', s); return 1; }
#undef strcmp
#undef strncmp
#define subtag_matches(a, b, s, n) ::ot_tag_probe::subtag (#a, s)
#define lang_matches(a, b, s, n) ::ot_tag_probe::lang (s)
#define strncmp(a, s, n) ::ot_tag_probe::ncmp (s)
#define strcmp(a, s) ::ot_tag_probe::cmp (s)
#undef HB_OT_TAG_TABLE_HH
#include "hb-ot-tag-table.hh"
#undef subtag_matches
#undef lang_matches
#undef strncmp
#undef strcmp
}

struct str_list_t
{
  char **v;
  unsigned n, cap;
};

static void str_add (str_list_t *l, const char *s)
{
  if (l->n == l->cap)
  {
    l->cap = l->cap ? 2 * l->cap : 256;
    l->v = (char **) realloc (l->v, l->cap * sizeof (char *));
    if (!l->v) abort ();
  }
  size_t len = strlen (s);
  char *c = (char *) malloc (len + 1);
  if (!c) abort ();
  memcpy (c, s, len + 1);
  l->v[l->n++] = c;
}

static int compare_strs (const void *a, const void *b)
{
  return strcmp (*(char *const *) a, *(char *const *) b);
}

static void str_sort_unique (str_list_t *l)
{
  qsort (l->v, l->n, sizeof (char *), compare_strs);
  unsigned j = 0;
  for (unsigned i = 0; i < l->n; i++)
    if (!j || strcmp (l->v[j - 1], l->v[i]) != 0)
      l->v[j++] = l->v[i];
    else
      free (l->v[i]);
  l->n = j;
}

struct ot_tag_set_t
{
  hb_tag_t *v;
  unsigned n, cap;
};

static void ot_tag_add (ot_tag_set_t *s, hb_tag_t t)
{
  if (s->n == s->cap)
  {
    s->cap = s->cap ? 2 * s->cap : 256;
    s->v = (hb_tag_t *) realloc (s->v, s->cap * sizeof (hb_tag_t));
    if (!s->v) abort ();
  }
  s->v[s->n++] = t;
}

static int compare_ot_tags (const void *a, const void *b)
{
  hb_tag_t x = *(const hb_tag_t *) a, y = *(const hb_tag_t *) b;
  return x < y ? -1 : x > y ? 1 : 0;
}

static void ot_tag_sort_unique (ot_tag_set_t *s)
{
  qsort (s->v, s->n, sizeof (hb_tag_t), compare_ot_tags);
  unsigned j = 0;
  for (unsigned i = 0; i < s->n; i++)
    if (!j || s->v[j - 1] != s->v[i])
      s->v[j++] = s->v[i];
  s->n = j;
}

/* Tags as `-` for none, else eight hex digits joined by `,`. */
static void print_ot_tags (const hb_tag_t *t, unsigned n)
{
  if (!n) { printf ("-"); return; }
  for (unsigned i = 0; i < n; i++)
    printf (i ? ",%08x" : "%08x", t[i]);
}

static void print_ot_language (hb_language_t l)
{
  printf ("%s", l ? hb_language_to_string (l) : "*");
}

/* The languages of test/api/test-ot-tag.c and further edge cases. */
static const char *const ot_tag_fixed_languages[] = {
  "en", "x-hbsc", "x-hbsc-", "x-hbsc-1", "x-hbsc-1a", "x-hbsc-1a2b3c4x",
  "x-hbsc-326c6f6e67", "x-hbscabc", "x-hbscdeva", "x-hbscdev2", "x-hbscdev3",
  "x-hbsc-64657633", "x-hbotpap0-hbsccopt", "en-x-hbsc", "en-x-hbscabc",
  "en-x-hbscdeva", "en-x-hbscdev2", "en-x-hbscdev3", "en-x-hbsc-64657633",
  "en-x-hbotpap0-hbsccopt", "alt", "ar", "az", "az-ir", "az-az", "en_US",
  "cja-x-hbot-434a4120", "cjm-x-hbot-434a4d20", "cjm", "eve", "cfm", "flm",
  "hy", "hyw", "bgr", "cbl", "cka", "cmr", "cnb", "cnh", "cnk", "cnw", "csh",
  "csy", "ctd", "czt", "dao", "hlt", "mrh", "pck", "sez", "tcp", "tcz", "yos",
  "zom", "fa", "fa_IR", "man", "aii", "syr", "amw", "cld", "syc", "tru", "zh",
  "zh-cn", "zh-sg", "zh-mo", "zh-hant-mo", "zh-hans-mo", "zh-HK", "zH-HanT-hK",
  "zH-HanS-hK", "zh-tw", "zh-Hans", "zh-Hant", "zh-xx", "zh-Hans-TW", "yue",
  "yue-Hant", "yue-Hans", "abc-x-hbot-41424320", "x-hbot-41424344",
  "asdf-asdf-wer-x-hbotabc-zxc", "asdf-asdf-wer-x-hbotabc",
  "asdf-asdf-wer-x-hbotabcd", "asdf-asdf-wer-x-hbot-41424320-zxc",
  "asdf-asdf-wer-x-hbot-41424320", "asdf-asdf-wer-x-hbot-41424344",
  "asdf-asdf-wer-x-hbot", "asdf-asdf-wer-x-hbot-zxc",
  "asdf-asdf-wer-x-hbot-zxc-414243", "asdf-asdf-wer-x-hbot-414243",
  "asdf-asdf-wer-x-hbot-4142432", "xy", "xyz", "xyz-qw", "-fonipa",
  "en-fonipax", "en-x-fonipa", "en-a-fonipa", "en-a-qwe-b-fonipa", "en-fonipa",
  "en-fonipax-fonipa", "rm-CH-fonipa-sursilv-x-foobar", "und-fonipa",
  "zh-fonipa", "en-fonnapa", "chr-fonnapa", "und-fonnapa", "ka-Geok",
  "und-Geok", "ga-Latg", "ghc", "ro-MD", "el-polyton", "el-CY-polyton",
  "aii-Syre", "de-Syre", "syr-Syre", "und-Syre", "aii-Syrj", "de-Syrj",
  "syr-Syrj", "und-Syrj", "aii-Syrn", "de-Syrn", "syr-Syrn", "und-Syrn",
  "fa-x-hbotabc-hbot-41686121-zxc", "fa-ir-x-hbotabc-hbot-41686121-zxc",
  "zh-x-hbotabc-hbot-41686121-zxc", "zh-cn-x-hbotabc-hbot-41686121-zxc",
  "zh-xy-x-hbotabc-hbot-41686121-zxc", "xyz-xy-x-hbotabc-hbot-41686121-zxc",
  "fa-x-hbot-41686121-hbotabc-zxc", "fa-ir-x-hbot-41686121-hbotabc-zxc",
  "zh-x-hbot-41686121-hbotabc-zxc", "zh-cn-x-hbot-41686121-hbotabc-zxc",
  "zh-xy-x-hbot-41686121-hbotabc-zxc", "xyz-xy-x-hbot-41686121-hbotabc-zxc",
  "x-hbot", "x-hbot-", "x-hbot-1", "x-hbot-1a", "x-hbot-1a2b3c4x",
  "x-hbot-326c6f6e67", "ar-aao", "art-lojban", "kok-gom", "i-lux", "drh",
  "ar-ary", "ar-ary-DZ", "no-bok", "no-nyn", "i-hak", "zh-guoyu", "zh-min",
  "zh-min-nan", "zh-xiang", "als", "far", "ar-001", "tr@foo=bar",
  "und", "en-x-hbscdflt", "x-hbot1234-hbsc5678", "x-hbsc5678-hbot1234", "ml",
  /* Further edge cases. */
  "x", "x-", "-", "--", "a", "a-", "-a", "a--b", "a-b-c-d", "en-u-hk",
  "zh-u-co-x-hbsc-hant", "zh-a-b-c-hk", "zh-x-u-hk", "x-hbscDFLT",
  "x-hbotdflt", "x-hbot-64666c74", "x-hbsc-44464c54", "x-hbot-FFFFFFFF",
  "en-x-hbsc-6C61746E", "en-x-hbscl", "en-x-hbsclatnn", "en-x-hbot-zz",
  "zh-yue", "zh-yue-hk", "zh-cmn-hant", "zh-cmn-hans-hk", "zh-hak", "sgn-x-a",
  "abcd", "abcde-fg", "ab-cde", "ab-cdef", "ab-1de", "ab-cd1", "a1-b2",
  "123", "12", "ab_cd_ef", "EN", "zh-hantx-hk", "zh-hk-hant", "zh-hkx",
  "cdo-hant-hkx", "cdo-hant-hk-x", "art-lojban-x", "art-lojbanx",
  "zh-min-nan-x-hbot-41424344", "hy-arevmda", "oc-provenc", "nv-navajo",
  "lb-lux", "mnw-th", "ro-md-x-hbsc-6c61746e", "zh-cn-x-hbsc", "en-fonupa",
};

static const unsigned ot_tag_counts_scripts[] = {0, 1, 2, 3, 4};

static void add_primary_parts (str_list_t *primaries, str_list_t *seconds,
			       str_list_t *regions, const char *full)
{
  char buf[64];
  snprintf (buf, sizeof buf, "%s", full);
  char *save = buf;
  bool first = true;
  for (char *tok = buf; tok; tok = save)
  {
    char *dash = strchr (tok, '-');
    save = dash ? dash + 1 : nullptr;
    if (dash) *dash = '\0';
    if (!*tok) continue;
    if (first) { str_add (primaries, tok); first = false; continue; }
    size_t len = strlen (tok);
    bool region = (len == 2 && ISALPHA (tok[0]) && ISALPHA (tok[1])) ||
		  (len == 3 && tok[0] >= '0' && tok[0] <= '9');
    str_add (region ? regions : seconds, tok);
  }
}

static int dump_ot_tags ()
{
  /* Scripts: those of the characters, then HB_SCRIPT_INVALID, Math and
   * Hrkt, which no character has. */
  ot_tag_set_t scripts = {nullptr, 0, 0};
  hb_unicode_funcs_t *ufuncs = hb_unicode_funcs_get_default ();
  for (hb_codepoint_t u = 0; u <= 0x10FFFFu; u++)
    ot_tag_add (&scripts, (hb_tag_t) hb_unicode_script (ufuncs, u));
  ot_tag_add (&scripts, (hb_tag_t) HB_SCRIPT_INVALID);
  ot_tag_add (&scripts, (hb_tag_t) HB_SCRIPT_MATH);
  ot_tag_add (&scripts, HB_TAG ('H','r','k','t'));
  ot_tag_sort_unique (&scripts);
  printf ("scripts ");
  print_ot_tags (scripts.v, scripts.n);
  printf ("\n");

  /* The strings of the generated functions' branches. */
  for (int c = 1; c < 128; c++)
  {
    char buf[32];
    snprintf (buf, sizeof buf, "%caaaaaa-bbbbbbbbbbbb", c);
    ot_tag_probe::first_byte = (char) c;
    unsigned count = 3;
    hb_tag_t tags[3];
    ot_tag_probe::hb_ot_tags_from_complex_language (buf, buf + strlen (buf), &count, tags);
  }
  str_list_t primaries = {nullptr, 0, 0}, seconds = {nullptr, 0, 0},
	     regions = {nullptr, 0, 0}, languages = {nullptr, 0, 0};
  str_add (&primaries, "und");
  str_add (&primaries, "en");
  for (unsigned i = 0; i < ot_tag_probe::num_records; i++)
  {
    const ot_tag_probe::record_t &r = ot_tag_probe::records[i];
    /* The branches before the switch run for every first byte. */
    if (r.kind == 'p' && r.first != 1) continue;
    char full[64];
    snprintf (full, sizeof full, "%c%s", r.first, r.s);
    switch (r.kind)
    {
    case 'l': case 'e':
      printf ("branch %c %s\n", r.kind, full);
      str_add (&languages, full);
      add_primary_parts (&primaries, &seconds, &regions, full);
      break;
    case 'n':
      printf ("branch n %s\n", full);
      add_primary_parts (&primaries, &seconds, &regions, full);
      break;
    case 's': case 'p':
      printf ("branch %c %s\n", r.kind, r.s);
      snprintf (full, sizeof full, "und%s", r.s);
      add_primary_parts (&primaries, &seconds, &regions, full);
      break;
    }
  }
  str_sort_unique (&primaries);
  str_sort_unique (&seconds);
  str_sort_unique (&regions);

  /* primary x script or other subtag x region x private use. */
  static const char *const privates[] = {"", "-x-hant-hk-fonipa", "-u-hk-x-hbsc-latn"};
  for (unsigned p = 0; p < primaries.n; p++)
    for (unsigned s = 0; s <= seconds.n; s++)
      for (unsigned r = 0; r <= regions.n; r++)
	for (unsigned x = 0; x < 3; x++)
	{
	  char buf[128];
	  snprintf (buf, sizeof buf, "%s%s%s%s%s%s", primaries.v[p],
		    s ? "-" : "", s ? seconds.v[s - 1] : "",
		    r ? "-" : "", r ? regions.v[r - 1] : "", privates[x]);
	  str_add (&languages, buf);
	}

  /* Every language of the tables, and the ambiguous tags' languages. */
  ot_tag_set_t table_tags = {nullptr, 0, 0};
  char buf[8];
  for (unsigned i = 0; i < ARRAY_LENGTH (ot_languages2); i++)
  {
    hb_tag_to_string (ot_languages2[i].language, buf);
    buf[2] = '\0';
    str_add (&languages, buf);
    ot_tag_add (&table_tags, ot_languages2[i].tag);
  }
  for (unsigned i = 0; i < ARRAY_LENGTH (ot_languages3); i++)
  {
    hb_tag_to_string (ot_languages3[i].language, buf);
    buf[3] = '\0';
    str_add (&languages, buf);
    ot_tag_add (&table_tags, ot_languages3[i].tag);
  }
  for (unsigned i = 0; i < ARRAY_LENGTH (ot_languages3_multi); i++)
  {
    hb_tag_to_string (ot_languages3_multi[i].language, buf);
    buf[3] = '\0';
    str_add (&languages, buf);
  }
  for (unsigned i = 0; i < ARRAY_LENGTH (ot_languages3_multi_values); i++)
    ot_tag_add (&table_tags, ot_languages3_multi_values[i]);
  for (unsigned i = 0; i < ARRAY_LENGTH (ot_languages3_blocked); i++)
  {
    hb_tag_to_string (ot_languages3_blocked[i], buf);
    buf[3] = '\0';
    str_add (&languages, buf);
  }
  ot_tag_sort_unique (&table_tags);
  for (unsigned i = 0; i < table_tags.n; i++)
  {
    hb_language_t l = hb_ot_ambiguous_tag_to_language (table_tags.v[i]);
    if (l) str_add (&languages, hb_language_to_string (l));
  }
  for (unsigned i = 0; i < ARRAY_LENGTH (ot_tag_fixed_languages); i++)
    str_add (&languages, ot_tag_fixed_languages[i]);
  str_sort_unique (&languages);

  /* Forward, with HB_OT_MAX_TAGS_PER_SCRIPT and _PER_LANGUAGE:
   *   f <input> <language> <language tags> <script tags of each script>
   */
  ot_tag_set_t seen_scripts = {nullptr, 0, 0}, seen_languages = {nullptr, 0, 0};
  for (unsigned i = 0; i < languages.n; i++)
  {
    hb_language_t lang = hb_language_from_string (languages.v[i], -1);
    if (!lang) continue;
    hb_tag_t lt0[HB_OT_MAX_TAGS_PER_LANGUAGE];
    unsigned lc0 = 0;
    printf ("f %s ", languages.v[i]);
    print_ot_language (lang);
    for (unsigned s = 0; s < scripts.n; s++)
    {
      hb_tag_t st[HB_OT_MAX_TAGS_PER_SCRIPT], lt[HB_OT_MAX_TAGS_PER_LANGUAGE];
      unsigned sc = HB_OT_MAX_TAGS_PER_SCRIPT, lc = HB_OT_MAX_TAGS_PER_LANGUAGE;
      hb_ot_tags_from_script_and_language ((hb_script_t) scripts.v[s], lang,
					   &sc, st, &lc, lt);
      if (!s)
      {
	lc0 = lc;
	memcpy (lt0, lt, lc * sizeof (hb_tag_t));
	printf (" ");
	print_ot_tags (lt, lc);
	for (unsigned k = 0; k < lc; k++) ot_tag_add (&seen_languages, lt[k]);
      }
      else if (lc != lc0 || memcmp (lt, lt0, lc * sizeof (hb_tag_t)))
      {
	fprintf (stderr, "hb-dump: the language tags of %s depend on the script\n",
		 languages.v[i]);
	return 1;
      }
      printf (" ");
      print_ot_tags (st, sc);
      for (unsigned k = 0; k < sc; k++) ot_tag_add (&seen_scripts, st[k]);
    }
    printf ("\n");
  }

  /* Other counts, for the fixed languages and a few scripts:
   *   c <input> <script> <script count> <language count> <script tags>
   *     <language tags>
   */
  static const hb_tag_t count_scripts[] = {
    0, HB_TAG ('L','a','t','n'), HB_TAG ('D','e','v','a'), HB_TAG ('M','y','m','r'),
    HB_TAG ('H','i','r','a'), HB_TAG ('M','l','y','m'),
  };
  for (unsigned i = 0; i < ARRAY_LENGTH (ot_tag_fixed_languages); i++)
  {
    hb_language_t lang = hb_language_from_string (ot_tag_fixed_languages[i], -1);
    for (unsigned s = 0; s < ARRAY_LENGTH (count_scripts); s++)
      for (unsigned scount = 0; scount <= 4; scount++)
	for (unsigned lcount = 0; lcount <= 4; lcount++)
	{
	  hb_tag_t st[4], lt[4];
	  unsigned sc = scount, lc = lcount;
	  hb_ot_tags_from_script_and_language ((hb_script_t) count_scripts[s], lang,
					       &sc, st, &lc, lt);
	  printf ("c %s %08x %u %u ", ot_tag_fixed_languages[i], count_scripts[s],
		  scount, lcount);
	  print_ot_tags (st, sc);
	  printf (" ");
	  print_ot_tags (lt, lc);
	  printf ("\n");
	}
  }

  /* Backward, for every tag that appeared and the tables' tags:
   *   s <script tag> <hb_ot_tag_to_script>
   *   l <language tag> <hb_ot_tag_to_language, * for none>
   *   r <script tag> <language tag> <script> <language>
   *     (hb_ot_tags_to_script_and_language)
   */
  ot_tag_add (&seen_scripts, HB_OT_TAG_DEFAULT_SCRIPT);
  ot_tag_sort_unique (&seen_scripts);
  for (unsigned i = 0; i < table_tags.n; i++)
    ot_tag_add (&seen_languages, table_tags.v[i]);
  ot_tag_add (&seen_languages, HB_OT_TAG_DEFAULT_LANGUAGE);
  ot_tag_sort_unique (&seen_languages);
  for (unsigned i = 0; i < seen_scripts.n; i++)
    printf ("s %08x %08x\n", seen_scripts.v[i], (unsigned) hb_ot_tag_to_script (seen_scripts.v[i]));
  for (unsigned i = 0; i < seen_languages.n; i++)
  {
    printf ("l %08x ", seen_languages.v[i]);
    print_ot_language (hb_ot_tag_to_language (seen_languages.v[i]));
    printf ("\n");
  }
  for (unsigned i = 0; i < seen_scripts.n; i++)
    for (unsigned j = 0; j < seen_languages.n; j++)
    {
      hb_script_t script;
      hb_language_t language;
      hb_ot_tags_to_script_and_language (seen_scripts.v[i], seen_languages.v[j],
					 &script, &language);
      printf ("r %08x %08x %08x ", seen_scripts.v[i], seen_languages.v[j], (unsigned) script);
      print_ot_language (language);
      printf ("\n");
    }
  printf ("end\n");
  return 0;
}

/* Bytes 0x21..0x7E but `\` as they are, anything else as \xHH, after a
 * `=`, so that an empty string is `=`. */
static void print_escaped (const char *s, unsigned len)
{
  putchar ('=');
  for (unsigned i = 0; i < len; i++)
  {
    unsigned char c = (unsigned char) s[i];
    if (c > 0x20 && c < 0x7F && c != '\\')
      putchar (c);
    else
      printf ("\\x%02x", c);
  }
}

/* The strings of --features (ADR 0020): the direct product below, then
 * the extra cases. sofel-conformance builds the same list. */
static const char *feature_tags[] = {
  "kern", "aalt", "a", "abc", "abcde", "'liga'", "\"liga\"", "'abc'",
  "\"abcde\"", "'liga\"",
};
static const char *feature_prefixes[] = {"", "+", "-"};
static const char *feature_ranges[] = {
  "", "[]", "[:]", "[3]", "[3:]", "[:5]", "[3:5]", "[3", "[x]",
};
static const char *feature_values[] = {
  "", "=0", "=2", " on", " off", " 2", "=", "=on", " xyz", "=-1",
};
static const char *feature_spaces[][2] = {
  {"", ""}, {" ", ""}, {"", " "}, {"\t ", " \n"},
};
static const char *feature_extras[] = {
  "abcd", "abcd=1", "+abcd", "abcd=0", "-abcd", "abcd=2", "+abcd=2",
  "-abcd=2", "\"abcd\" on", "\"abcd\" off", "\"abcd\" 1", "\"abcd\" 0",
  "\"abcd\" 2", "abcd[0]", "abcd[1]", "abcd[1]=1", "abcd[1]=2",
  "abcd[1]=0", "abcd[]", "abcd[:]", "abcd[1:]", "abcd[:1]", "abcd[1:3]",
  "abcd[1:2]=1", "abcd[1:4]=2", "aalt[3:5]=2",
  "kern\t", "kern\t=2", "kern[ 3 : 5 ]", "kern[3;5]", "kern[-1]",
  "kern[-1:]", "kern[4294967295]", "kern[2147483647]", "kern[2147483648]",
  "kern=2147483647", "kern=2147483648", "kern=-2147483648", "kern=+2",
  "kern = 2", "kern=0x10", "kern=010", "kern  ON", "kern OfF", "kern onx",
  "kern=off", "kern=ON", "-+kern", "+-kern", "--kern", "", " ", "=", "[]",
  "-", "+", "'kern' 'x'", "'ke\"n'", "\"kern'", "kern'", "k=rn", "k[rn",
  "'k rn'", "'k[rn'", "'k=rn'", "k:rn", "k]rn", "k\trn", "\xc3\xa9t\xc3",
  "\xc3\xa9t\xc3\xa9", "\xff\xfe", "\v kern \f", "kern[3:5]=2 ",
  "kern[3:5] =2", "kern [3:5]", "kern[3:5]x", "kern[3:5", "kern[3:5]]",
  "kern[:]=3", "kern[0:]", "kern[0:1]", "kern[5:3]", "kern[3:3]",
  "kern==2", "kern=2=3", "kern 2 3", "kern on off", "'kern'on",
  "'kern'=2", "\"kern\"[1:2]", "''", "\"\"", "'", "\"", "kern'x",
  "liga=4294967295", "liga=-0", "liga[+3]", "liga[ -2 ]", "liga[3:-1]",
};

static void dump_feature (const char *s, unsigned len)
{
  hb_feature_t f;
  hb_bool_t ok = hb_feature_from_string (s, (int) len, &f);
  printf ("feature ");
  print_escaped (s, len);
  if (ok)
  {
    char buf[128];
    hb_feature_to_string (&f, buf, sizeof (buf));
    printf (" 1 %08x %u %u %u ", f.tag, f.value, f.start, f.end);
    print_escaped (buf, strlen (buf));
  }
  else
    printf (" 0");
  printf ("\n");
}

#define ARRAY_COUNT(a) (sizeof (a) / sizeof ((a)[0]))

static int dump_features ()
{
  for (unsigned t = 0; t < ARRAY_COUNT (feature_tags); t++)
    for (unsigned p = 0; p < ARRAY_COUNT (feature_prefixes); p++)
      for (unsigned r = 0; r < ARRAY_COUNT (feature_ranges); r++)
	for (unsigned v = 0; v < ARRAY_COUNT (feature_values); v++)
	  for (unsigned w = 0; w < ARRAY_COUNT (feature_spaces); w++)
	  {
	    char s[128];
	    int n = snprintf (s, sizeof (s), "%s%s%s%s%s%s",
			      feature_spaces[w][0], feature_prefixes[p],
			      feature_tags[t], feature_ranges[r],
			      feature_values[v], feature_spaces[w][1]);
	    dump_feature (s, (unsigned) n);
	  }
  for (unsigned i = 0; i < ARRAY_COUNT (feature_extras); i++)
    dump_feature (feature_extras[i], strlen (feature_extras[i]));
  return 0;
}

/* --serialize and --deserialize (ADR 0020). */
static const hb_buffer_serialize_format_t serialize_formats[] = {
  HB_BUFFER_SERIALIZE_FORMAT_TEXT, HB_BUFFER_SERIALIZE_FORMAT_JSON,
};
static const char *serialize_format_names[] = {"text", "json"};

static void print_glyph_items (hb_buffer_t *b)
{
  unsigned n = hb_buffer_get_length (b);
  hb_glyph_info_t *info = hb_buffer_get_glyph_infos (b, nullptr);
  hb_glyph_position_t *pos = hb_buffer_get_glyph_positions (b, nullptr);
  for (unsigned i = 0; i < n; i++)
    printf (" %u,%u,%u,%d,%d,%d,%d", info[i].codepoint, info[i].cluster,
	    info[i].mask, pos[i].x_advance, pos[i].y_advance,
	    pos[i].x_offset, pos[i].y_offset);
}

static void print_unicode_items (hb_buffer_t *b)
{
  unsigned n = hb_buffer_get_length (b);
  hb_glyph_info_t *info = hb_buffer_get_glyph_infos (b, nullptr);
  for (unsigned i = 0; i < n; i++)
    printf (" %u,%u", info[i].codepoint, info[i].cluster);
}

/* What hb-shape does (util/shape-format.hh, serialize). */
static std::string serialize_all (hb_buffer_t *b, hb_font_t *font,
				  hb_buffer_serialize_format_t format,
				  hb_buffer_serialize_flags_t flags)
{
  std::string out;
  unsigned n = hb_buffer_get_length (b), start = 0;
  while (start < n)
  {
    char buf[32768];
    unsigned consumed;
    start += hb_buffer_serialize (b, start, n, buf, sizeof (buf), &consumed,
				  font, format, flags);
    if (!consumed)
      break;
    out.append (buf, consumed);
  }
  return out;
}

static void dump_deserialize_glyphs (const char *s, unsigned len, hb_font_t *font,
				     unsigned f)
{
  hb_buffer_t *b = hb_buffer_create ();
  const char *end = s;
  hb_bool_t ok = hb_buffer_deserialize_glyphs (b, s, (int) len, &end, font,
					       serialize_formats[f]);
  printf ("deserialize glyphs %s %d %u %u", serialize_format_names[f], ok ? 1 : 0,
	  (unsigned) (end - s), hb_buffer_get_length (b));
  print_glyph_items (b);
  printf ("\n");
  hb_buffer_destroy (b);
}

static void dump_deserialize_unicode (const char *s, unsigned len, unsigned f)
{
  hb_buffer_t *b = hb_buffer_create ();
  const char *end = s;
  hb_bool_t ok = hb_buffer_deserialize_unicode (b, s, (int) len, &end,
						serialize_formats[f]);
  printf ("deserialize unicode %s %d %u %u", serialize_format_names[f], ok ? 1 : 0,
	  (unsigned) (end - s), hb_buffer_get_length (b));
  print_unicode_items (b);
  printf ("\n");
  hb_buffer_destroy (b);
}

static void dump_corpus_entry (const char *s, unsigned len, hb_font_t *font)
{
  printf ("corpus ");
  print_escaped (s, len);
  printf ("\n");
  for (unsigned f = 0; f < 2; f++)
    dump_deserialize_glyphs (s, len, font, f);
  for (unsigned f = 0; f < 2; f++)
    dump_deserialize_unicode (s, len, f);
}

/* The inputs of test_buffer_serialize_deserialize and
 * test_buffer_deserialize2 in HarfBuzz's test/api/test-buffer.c. */
static const char *test_buffer_inputs[] = {
  "<U+0640=0|U+0635=1>",
  "[{\"u\":1600,\"cl\":0},{\"u\":1589,\"cl\":1}]",
  "[{\"u\":1600,\"cl\":0},{\"u\":1589,\"cl\":1}",
  "[{\"u\":1600,\"cl\":0},{\"g\":1589,\"cl\":1}]",
  "<U+0640=0|uni0635=1>",
  "<U+0041=0|U+0042=1>", "[1=0@2,3+4,5|2=1+6]",
  "[{\"u\":65,\"cl\":0},{\"u\":66,\"cl\":1}]",
  "[{\"g\":1,\"cl\":0,\"dx\":2,\"dy\":3,\"ax\":4,\"ay\":5},{\"g\":2,\"cl\":1}]",
  "<U+0041=0>", "[icon0=0+10]",
  "[{\"u\":65}]", "[{\"g\":\"icon0\",\"ax\":10}]",
  "<U+0041=0|U+0042=?>", "[1=0|2=?]",
  "[{\"u\":65},?]", "[{\"g\":1},?]",
  "<U+0041=0|U+0042=4294967296>", "[1=0|2=4294967296]",
  "[{\"u\":65},{\"u\":4294967296}]",
  "[{\"g\":1},{\"g\":2,\"ax\":2147483648}]",
  "", "[1=0]", "[{\"g\":1}]",
};

static int dump_serialize (hb_blob_t *blob, const char *text_path)
{
  FILE *fp = fopen (text_path, "rb");
  if (!fp)
  {
    fprintf (stderr, "hb-dump: cannot read %s\n", text_path);
    return 1;
  }
  hb_face_t *face = hb_face_create (blob, 0);
  hb_font_t *font = var_create_font (face);
  char line[4096];
  unsigned line_no = 0;
  while (fgets (line, sizeof (line), fp))
  {
    unsigned len = strlen (line);
    if (len && line[len - 1] == '\n')
      line[--len] = '\0';
    printf ("line %u ", line_no++);
    print_escaped (line, len);
    printf ("\n");
    hb_buffer_t *b = hb_buffer_create ();
    hb_buffer_add_utf8 (b, line, (int) len, 0, (int) len);
    hb_buffer_guess_segment_properties (b);
    printf ("unicode");
    print_unicode_items (b);
    printf ("\n");
    for (unsigned f = 0; f < 2; f++)
      for (unsigned flags = 0; flags < 64; flags++)
      {
	std::string s = serialize_all (b, nullptr, serialize_formats[f],
				       (hb_buffer_serialize_flags_t) flags);
	printf ("serialize unicode %s %u ", serialize_format_names[f], flags);
	print_escaped (s.data (), s.size ());
	printf ("\n");
	dump_deserialize_unicode (s.data (), s.size (), f);
      }
    hb_shape (font, b, nullptr, 0);
    printf ("glyphs");
    print_glyph_items (b);
    printf ("\n");
    for (unsigned f = 0; f < 2; f++)
      for (unsigned flags = 0; flags < 64; flags++)
      {
	std::string s = serialize_all (b, font, serialize_formats[f],
				       (hb_buffer_serialize_flags_t) flags);
	printf ("serialize glyphs %s %u ", serialize_format_names[f], flags);
	print_escaped (s.data (), s.size ());
	printf ("\n");
	dump_deserialize_glyphs (s.data (), s.size (), font, f);
      }
    hb_buffer_destroy (b);
  }
  fclose (fp);
  for (unsigned i = 0; i < ARRAY_COUNT (test_buffer_inputs); i++)
  {
    const char *s = test_buffer_inputs[i];
    unsigned len = strlen (s);
    for (unsigned n = 0; n <= len; n++)
      dump_corpus_entry (s, n, font);
  }
  hb_font_destroy (font);
  hb_face_destroy (face);
  return 0;
}

static int dump_deserialize (hb_blob_t *blob, const char *corpus_path)
{
  FILE *fp = fopen (corpus_path, "rb");
  if (!fp)
  {
    fprintf (stderr, "hb-dump: cannot read %s\n", corpus_path);
    return 1;
  }
  hb_face_t *face = hb_face_create (blob, 0);
  hb_font_t *font = var_create_font (face);
  std::string line;
  int c;
  while ((c = fgetc (fp)) != EOF)
  {
    if (c != '\n')
    {
      line.push_back ((char) c);
      continue;
    }
    /* Undo print_escaped (without its `=`). */
    std::string s;
    for (unsigned i = 0; i < line.size (); i++)
    {
      if (line[i] == '\\' && i + 3 < line.size () && line[i + 1] == 'x')
      {
	char hex[3] = {line[i + 2], line[i + 3], '\0'};
	s.push_back ((char) strtoul (hex, nullptr, 16));
	i += 3;
      }
      else
	s.push_back (line[i]);
    }
    dump_corpus_entry (s.data (), s.size (), font);
    line.clear ();
  }
  fclose (fp);
  hb_font_destroy (font);
  hb_face_destroy (face);
  return 0;
}

/* ADR 0023: color glyphs. */

struct paint_text_t
{
  int level;
  hb_vector_t<char> text;
  void line (const char *fmt, ...)
  {
    char buf[1024];
    int n = snprintf (buf, sizeof (buf), "%*s", 2 * level, "");
    va_list ap;
    va_start (ap, fmt);
    vsnprintf (buf + n, sizeof (buf) - n, fmt, ap);
    va_end (ap);
    for (const char *p = buf; *p; p++) text.push (*p);
    text.push ('\n');
  }
};

static float clean_float (float v) { return v == 0.f ? 0.f : v; }

#define PD ((paint_text_t *) data)
static void pt_push_transform (hb_paint_funcs_t *, void *data, float xx, float yx, float xy, float yy, float dx, float dy, void *)
{
  PD->line ("start transform %.3g %.3g %.3g %.3g %.3g %.3g", clean_float (xx), clean_float (yx),
	    clean_float (xy), clean_float (yy), clean_float (dx), clean_float (dy));
  PD->level++;
}
static void pt_pop_transform (hb_paint_funcs_t *, void *data, void *) { PD->level--; PD->line ("end transform"); }
static void pt_fill_glyph (hb_paint_funcs_t *, void *data, hb_codepoint_t g, hb_font_t *, hb_bool_t, hb_color_t c, void *)
{
  PD->line ("fill glyph %u solid %d %d %d %d", g, hb_color_get_red (c), hb_color_get_green (c),
	    hb_color_get_blue (c), hb_color_get_alpha (c));
}
static hb_bool_t pt_color_glyph (hb_paint_funcs_t *, void *data, hb_codepoint_t g, hb_font_t *, void *)
{
  PD->line ("paint color glyph %u; acting as failed", g);
  return false;
}
static void pt_push_clip_glyph (hb_paint_funcs_t *, void *data, hb_codepoint_t g, hb_font_t *, void *)
{
  PD->line ("start clip glyph %u", g);
  PD->level++;
}
static void pt_push_clip_rectangle (hb_paint_funcs_t *, void *data, float a, float b, float c, float d, void *)
{
  PD->line ("start clip rectangle %.3g %.3g %.3g %.3g", a, b, c, d);
  PD->level++;
}
static void pt_pop_clip (hb_paint_funcs_t *, void *data, void *) { PD->level--; PD->line ("end clip"); }
static void pt_color (hb_paint_funcs_t *, void *data, hb_bool_t, hb_color_t c, void *)
{
  PD->line ("solid %d %d %d %d", hb_color_get_red (c), hb_color_get_green (c), hb_color_get_blue (c),
	    hb_color_get_alpha (c));
}
static hb_bool_t pt_image (hb_paint_funcs_t *, void *data, hb_blob_t *, unsigned w, unsigned h,
			   hb_tag_t format, float slant, hb_glyph_extents_t *e, void *)
{
  char buf[5] = {0};
  hb_tag_to_string (format, buf);
  if (e)
    PD->line ("image type %s size %u %u slant %.3g extents %d %d %d %d\n", buf, w, h, slant,
	      e->x_bearing, e->y_bearing, e->width, e->height);
  else
    PD->line ("image type %s size %u %u slant %.3g extents -\n", buf, w, h, slant);
  return true;
}
static void pt_stops (paint_text_t *d, hb_paint_extend_t extend, hb_color_stop_t *stops, unsigned len)
{
  d->line ("colors %d", extend);
  d->level++;
  for (unsigned i = 0; i < len; i++)
    d->line ("%.3g %d %d %d %d", stops[i].offset, hb_color_get_red (stops[i].color),
	     hb_color_get_green (stops[i].color), hb_color_get_blue (stops[i].color),
	     hb_color_get_alpha (stops[i].color));
  d->level--;
}
static void pt_line (paint_text_t *d, hb_color_line_t *cl)
{
  unsigned len = hb_color_line_get_color_stops (cl, 0, NULL, NULL);
  hb_color_stop_t *stops = (hb_color_stop_t *) malloc ((len ? len : 1) * sizeof (hb_color_stop_t));
  hb_color_line_get_color_stops (cl, 0, &len, stops);
  pt_stops (d, hb_color_line_get_extend (cl), stops, len);
  free (stops);
}
static void pt_linear (hb_paint_funcs_t *, void *data, hb_color_line_t *cl, float x0, float y0,
		       float x1, float y1, float x2, float y2, void *)
{
  float xx0, yy0, xx1, yy1, mn, mx;
  hb_paint_reduce_linear_anchors (x0, y0, x1, y1, x2, y2, &xx0, &yy0, &xx1, &yy1);
  unsigned len = hb_color_line_get_color_stops (cl, 0, NULL, NULL);
  hb_color_stop_t *stops = (hb_color_stop_t *) malloc ((len ? len : 1) * sizeof (hb_color_stop_t));
  hb_color_line_get_color_stops (cl, 0, &len, stops);
  hb_paint_normalize_color_line (stops, len, &mn, &mx);
  PD->line ("linear gradient");
  PD->level++;
  PD->line ("p0 %.3g %.3g", xx0 + mn * (xx1 - xx0), yy0 + mn * (yy1 - yy0));
  PD->line ("p1 %.3g %.3g", xx0 + mx * (xx1 - xx0), yy0 + mx * (yy1 - yy0));
  pt_stops (PD, hb_color_line_get_extend (cl), stops, len);
  PD->level--;
  free (stops);
}
static void pt_radial (hb_paint_funcs_t *, void *data, hb_color_line_t *cl, float x0, float y0,
		       float r0, float x1, float y1, float r1, void *)
{
  PD->line ("radial gradient");
  PD->level++;
  PD->line ("p0 %.3g %.3g radius %.3g", x0, y0, r0);
  PD->line ("p1 %.3g %.3g radius %.3g", x1, y1, r1);
  pt_line (PD, cl);
  PD->level--;
}
static void pt_sweep (hb_paint_funcs_t *, void *data, hb_color_line_t *cl, float cx, float cy,
		      float a0, float a1, void *)
{
  PD->line ("sweep gradient");
  PD->level++;
  PD->line ("center %.3g %.3g", cx, cy);
  PD->line ("angles %.3g %.3g", a0, a1);
  pt_line (PD, cl);
  PD->level--;
}
static void pt_push_group (hb_paint_funcs_t *, void *data, void *) { PD->line ("push group"); PD->level++; }
static void pt_pop_group (hb_paint_funcs_t *, void *data, hb_paint_composite_mode_t m, void *)
{
  PD->level--;
  PD->line ("pop group mode %d", m);
}
#undef PD

static hb_paint_funcs_t *paint_text_funcs ()
{
  hb_paint_funcs_t *f = hb_paint_funcs_create ();
  hb_paint_funcs_set_push_transform_func (f, pt_push_transform, NULL, NULL);
  hb_paint_funcs_set_pop_transform_func (f, pt_pop_transform, NULL, NULL);
  hb_paint_funcs_set_fill_glyph_func (f, pt_fill_glyph, NULL, NULL);
  hb_paint_funcs_set_color_glyph_func (f, pt_color_glyph, NULL, NULL);
  hb_paint_funcs_set_push_clip_glyph_func (f, pt_push_clip_glyph, NULL, NULL);
  hb_paint_funcs_set_push_clip_rectangle_func (f, pt_push_clip_rectangle, NULL, NULL);
  hb_paint_funcs_set_pop_clip_func (f, pt_pop_clip, NULL, NULL);
  hb_paint_funcs_set_push_group_func (f, pt_push_group, NULL, NULL);
  hb_paint_funcs_set_pop_group_func (f, pt_pop_group, NULL, NULL);
  hb_paint_funcs_set_color_func (f, pt_color, NULL, NULL);
  hb_paint_funcs_set_image_func (f, pt_image, NULL, NULL);
  hb_paint_funcs_set_linear_gradient_func (f, pt_linear, NULL, NULL);
  hb_paint_funcs_set_radial_gradient_func (f, pt_radial, NULL, NULL);
  hb_paint_funcs_set_sweep_gradient_func (f, pt_sweep, NULL, NULL);
  hb_paint_funcs_make_immutable (f);
  return f;
}

/* The ppems of the strikes of CBLC (the larger of x and y) and sbix. */
static void strike_ppems (hb_face_t *face, hb_vector_t<unsigned> *ppems)
{
  bool ok = true;
  hb_blob_t *b = hb_face_reference_table (face, HB_TAG ('C','B','L','C'));
  unsigned len = 0;
  const uint8_t *d = (const uint8_t *) hb_blob_get_data (b, &len);
  if (len >= 8)
  {
    unsigned n = be32 (d, len, 4, &ok);
    for (unsigned i = 0; i < n && 8 + 48 * (i + 1) <= len; i++)
    {
      unsigned x = d[8 + 48 * i + 44], y = d[8 + 48 * i + 45];
      unsigned p = x > y ? x : y;
      if (!ppems->lfind (p)) ppems->push (p);
    }
  }
  hb_blob_destroy (b);
  b = hb_face_reference_table (face, HB_TAG ('s','b','i','x'));
  d = (const uint8_t *) hb_blob_get_data (b, &len);
  if (len >= 8)
  {
    unsigned n = be32 (d, len, 4, &ok);
    for (unsigned i = 0; i < n && 8 + 4 * (i + 1) <= len; i++)
    {
      bool ok2 = true;
      unsigned off = be32 (d, len, 8 + 4 * i, &ok2);
      unsigned p = be16 (d, len, off, &ok2);
      if (ok2 && !ppems->lfind (p)) ppems->push (p);
    }
  }
  hb_blob_destroy (b);
}

static float paint_slant = 0.f;

static int dump_paint (hb_blob_t *blob)
{
  hb_paint_funcs_t *funcs = paint_text_funcs ();
  unsigned count = hb_face_count (blob);
  if (!count) count = 1;
  for (unsigned i = 0; i < count; i++)
  {
    hb_face_t *face = hb_face_create (blob, i);
    hb_font_t *font = var_create_font (face);
    hb_font_set_synthetic_slant (font, paint_slant);
    unsigned num_glyphs = hb_face_get_glyph_count (face);
    unsigned palettes = hb_ot_color_palette_get_count (face);
    if (!palettes) palettes = 1;
    hb_vector_t<unsigned> ppems;
    ppems.push (0);
    strike_ppems (face, &ppems);
    printf ("face %u\n", i);
    for (unsigned pal = 0; pal < palettes; pal++)
      for (unsigned pp = 0; pp < ppems.length; pp++)
      {
	hb_font_set_ppem (font, ppems[pp], ppems[pp]);
	for (unsigned g = 0; g < num_glyphs; g++)
	{
	  paint_text_t t;
	  t.level = 0;
	  hb_font_paint_glyph (font, g, funcs, &t, pal, HB_COLOR (0, 0, 0, 255));
	  unsigned lines = 0;
	  for (unsigned k = 0; k < t.text.length; k++) lines += t.text[k] == '\n';
	  printf ("paint %u %u %u %u\n", pal, ppems[pp], g, lines);
	  fwrite (t.text.arrayZ, 1, t.text.length, stdout);
	}
      }
    printf ("end\n");
    hb_font_destroy (font);
    hb_face_destroy (face);
  }
  hb_paint_funcs_destroy (funcs);
  return 0;
}

/* ADR 0031: the bytes returned for each sbix PNG at the default ppem. */
static int dump_png_digest (int, char **argv)
{
  hb_blob_t *blob = hb_blob_create_from_file_or_fail (argv[2]);
  if (!blob) { fprintf (stderr, "hb-dump: cannot read %s\n", argv[2]); return 1; }
  hb_face_t *face = hb_face_create (blob, 0);
  hb_font_t *font = var_create_font (face);
  hb_font_set_ppem (font, 0, 0);
  unsigned count = hb_face_get_glyph_count (face);
  for (unsigned glyph = 0; glyph < count; glyph++)
  {
    hb_blob_t *png = hb_ot_color_glyph_reference_png (font, glyph);
    unsigned length = hb_blob_get_length (png);
    const char *data = hb_blob_get_data (png, nullptr);
    unsigned long long hash = length ? 0xcbf29ce484222325ULL : 0;
    for (unsigned i = 0; i < length; i++)
      hash = (hash ^ (unsigned char) data[i]) * 0x100000001b3ULL;
    printf ("glyph %u %u %016llx\n", glyph, length, hash);
    hb_blob_destroy (png);
  }
  hb_font_destroy (font);
  hb_face_destroy (face);
  hb_blob_destroy (blob);
  return 0;
}

/* ADR 0031: one color paint at explicit design coordinates, as FNV-1a. */
static int dump_paint_var (int, char **argv)
{
  hb_blob_t *blob = hb_blob_create_from_file_or_fail (argv[2]);
  if (!blob) { fprintf (stderr, "hb-dump: cannot read %s\n", argv[2]); return 1; }
  hb_face_t *face = hb_face_create (blob, 0);
  hb_font_t *font = var_create_font (face);
  char *end = nullptr;
  unsigned long glyph = strtoul (argv[3], &end, 10);
  if (!*argv[3] || *end || glyph >= hb_face_get_glyph_count (face))
  {
    hb_font_destroy (font); hb_face_destroy (face); hb_blob_destroy (blob); return 2;
  }
  if (strcmp (argv[4], "-") != 0)
  {
    std::vector<hb_variation_t> variations;
    std::string coordinates (argv[4]);
    size_t begin = 0;
    while (begin < coordinates.size ())
    {
      size_t stop = coordinates.find (',', begin);
      if (stop == std::string::npos) stop = coordinates.size ();
      hb_variation_t variation;
      if (!hb_variation_from_string (coordinates.data () + begin, (int) (stop - begin), &variation))
      {
        hb_font_destroy (font); hb_face_destroy (face); hb_blob_destroy (blob); return 2;
      }
      variations.push_back (variation);
      begin = stop + 1;
    }
    hb_font_set_variations (font, variations.data (), (unsigned) variations.size ());
  }
  paint_text_t text;
  text.level = 0;
  hb_paint_funcs_t *funcs = paint_text_funcs ();
  hb_font_paint_glyph (font, glyph, funcs, &text, 0, HB_COLOR (0, 0, 0, 255));
  unsigned long long hash = 0xcbf29ce484222325ULL;
  for (unsigned i = 0; i < text.text.length; i++)
    hash = (hash ^ (unsigned char) text.text[i]) * 0x100000001b3ULL;
  printf ("%016llx\n", hash);
  hb_paint_funcs_destroy (funcs);
  hb_font_destroy (font);
  hb_face_destroy (face);
  hb_blob_destroy (blob);
  return 0;
}

/* ADR 0031: one sampled glyph's metrics at explicit design coordinates. */
static int dump_var_metrics (int, char **argv)
{
  hb_blob_t *blob = hb_blob_create_from_file_or_fail (argv[2]);
  if (!blob) { fprintf (stderr, "hb-dump: cannot read %s\n", argv[2]); return 1; }
  hb_face_t *face = hb_face_create (blob, 0);
  hb_font_t *font = var_create_font (face);
  char *end = nullptr;
  unsigned long glyph = strtoul (argv[3], &end, 10);
  if (!*argv[3] || *end || glyph >= hb_face_get_glyph_count (face))
  {
    hb_font_destroy (font); hb_face_destroy (face); hb_blob_destroy (blob); return 2;
  }
  if (strcmp (argv[4], "-") != 0)
  {
    std::vector<hb_variation_t> variations;
    std::string coordinates (argv[4]);
    size_t begin = 0;
    while (begin < coordinates.size ())
    {
      size_t stop = coordinates.find (',', begin);
      if (stop == std::string::npos) stop = coordinates.size ();
      hb_variation_t variation;
      if (!hb_variation_from_string (coordinates.data () + begin, (int) (stop - begin), &variation))
      {
        hb_font_destroy (font); hb_face_destroy (face); hb_blob_destroy (blob); return 2;
      }
      variations.push_back (variation);
      begin = stop + 1;
    }
    hb_font_set_variations (font, variations.data (), (unsigned) variations.size ());
  }
  unsigned selected = (unsigned) glyph;
  printf ("face 0\nnum_glyphs %u\nupem %u\n", hb_face_get_glyph_count (face),
          hb_face_get_upem (face));
  dump_scale (font, hb_face_get_glyph_count (face), (int) hb_face_get_upem (face),
              (int) hb_face_get_upem (face), &selected, 1);
  printf ("end\n");
  hb_font_destroy (font);
  hb_face_destroy (face);
  hb_blob_destroy (blob);
  return 0;
}

static int dump_color (hb_blob_t *blob)
{
  unsigned count = hb_face_count (blob);
  if (!count) count = 1;
  for (unsigned i = 0; i < count; i++)
  {
    hb_face_t *face = hb_face_create (blob, i);
    hb_font_t *font = var_create_font (face);
    unsigned num_glyphs = hb_face_get_glyph_count (face);
    printf ("face %u\n", i);
    printf ("has %d %d %d %d %d\n", hb_ot_color_has_layers (face) ? 1 : 0,
	    hb_ot_color_has_paint (face) ? 1 : 0, hb_ot_color_has_palettes (face) ? 1 : 0,
	    hb_ot_color_has_svg (face) ? 1 : 0, hb_ot_color_has_png (face) ? 1 : 0);
    unsigned palettes = hb_ot_color_palette_get_count (face);
    printf ("palettes %u\n", palettes);
    for (unsigned p = 0; p < palettes; p++)
    {
      unsigned n = hb_ot_color_palette_get_colors (face, p, 0, NULL, NULL);
      hb_vector_t<hb_color_t> colors;
      colors.resize (n);
      unsigned got = n;
      hb_ot_color_palette_get_colors (face, p, 0, &got, colors.arrayZ);
      printf ("palette %u %u %u ", p, hb_ot_color_palette_get_name_id (face, p),
	      (unsigned) hb_ot_color_palette_get_flags (face, p));
      if (!got) printf ("-");
      for (unsigned k = 0; k < got; k++)
	printf ("%s%02x%02x%02x%02x", k ? "," : "", hb_color_get_red (colors[k]),
		hb_color_get_green (colors[k]), hb_color_get_blue (colors[k]),
		hb_color_get_alpha (colors[k]));
      printf ("\n");
    }
    unsigned ncolors = palettes ? hb_ot_color_palette_get_colors (face, 0, 0, NULL, NULL) : 0;
    for (unsigned c = 0; c < ncolors; c++)
      printf ("color_name %u %u\n", c, hb_ot_color_palette_color_get_name_id (face, c));
    for (unsigned g = 0; g < num_glyphs; g++)
    {
      printf ("glyph %u %d ", g, hb_ot_color_glyph_has_paint (face, g) ? 1 : 0);
      unsigned n = hb_ot_color_glyph_get_layers (face, g, 0, NULL, NULL);
      hb_vector_t<hb_ot_color_layer_t> layers;
      layers.resize (n);
      unsigned got = n;
      hb_ot_color_glyph_get_layers (face, g, 0, &got, layers.arrayZ);
      if (!got) printf ("-");
      for (unsigned k = 0; k < got; k++)
	printf ("%s%u:%u", k ? "," : "", layers[k].glyph, layers[k].color_index);
      unsigned index = 0;
      if (hb_ot_color_glyph_get_svg_document_index (face, g, &index))
      {
	hb_codepoint_t s = 0, e = 0;
	if (hb_ot_color_get_svg_document_glyph_range (face, index, &s, &e))
	  printf (" %u %u %u", index, s, e);
	else
	  printf (" %u - -", index);
      }
      else
	printf (" - - -");
      hb_blob_t *svg = hb_ot_color_glyph_reference_svg (face, g);
      hb_blob_t *png = hb_ot_color_glyph_reference_png (font, g);
      printf (" %u %u\n", hb_blob_get_length (svg), hb_blob_get_length (png));
      hb_blob_destroy (svg);
      hb_blob_destroy (png);
    }
    printf ("end\n");
    hb_font_destroy (font);
    hb_face_destroy (face);
  }
  return 0;
}

static int dump_extents (hb_blob_t *blob)
{
  unsigned count = hb_face_count (blob);
  if (!count) count = 1;
  for (unsigned i = 0; i < count; i++)
  {
    hb_face_t *face = hb_face_create (blob, i);
    hb_font_t *font = var_create_font (face);
    unsigned num_glyphs = hb_face_get_glyph_count (face);
    printf ("face %u\n", i);
    for (unsigned g = 0; g < num_glyphs; g++)
    {
      hb_glyph_extents_t e = {0, 0, 0, 0};
      hb_bool_t found = hb_font_get_glyph_extents (font, g, &e);
      if (!found) e.x_bearing = e.y_bearing = e.width = e.height = 0;
      printf ("extents %u %d %d %d %d %d\n", g, found ? 1 : 0, e.x_bearing, e.y_bearing, e.width,
	      e.height);
    }
    printf ("end\n");
    hb_font_destroy (font);
    hb_face_destroy (face);
  }
  return 0;
}

static int dump_roundf ()
{
  char kind;
  unsigned long long bits;
  while (scanf (" %c %llx", &kind, &bits) == 2)
  {
    double r;
    if (kind == 'f')
    {
      uint32_t b = (uint32_t) bits;
      float v;
      memcpy (&v, &b, sizeof (v));
      r = roundf (v);
    }
    else
    {
      uint64_t b = bits;
      double v;
      memcpy (&v, &b, sizeof (v));
      r = roundf (v);
    }
    uint64_t out;
    memcpy (&out, &r, sizeof (out));
    printf ("%016llx\n", (unsigned long long) out);
  }
  return 0;
}

static int dump_mult ()
{
  unsigned upem;
  int scale;
  while (scanf ("%u %d", &upem, &scale) == 2)
  {
    hb_face_t *face = hb_face_builder_create ();
    hb_face_set_upem (face, upem);
    hb_font_t *font = var_create_font (face);
    hb_font_set_scale (font, scale, scale);
    printf ("%lld\n", (long long) font->x_mult);
    hb_font_destroy (font);
    hb_face_destroy (face);
  }
  return 0;
}

/* --shape, --shape-random and --layout-map (ADR 0020). */

/* hb-shape's parse_features: the list without surrounding quotes, split
 * at `,` and ` `, items that do not parse dropped. */
static void parse_feature_list (const char *arg, std::vector<hb_feature_t> *out)
{
  std::string s (arg);
  if (s.size () >= 2 && s[0] == s[s.size () - 1] && (s[0] == '"' || s[0] == '\''))
    s = s.substr (1, s.size () - 2);
  size_t start = 0;
  while (start <= s.size ())
  {
    size_t end = s.find_first_of (", ", start);
    if (end == std::string::npos) end = s.size ();
    hb_feature_t f;
    if (end > start && hb_feature_from_string (s.data () + start, (int) (end - start), &f))
      out->push_back (f);
    start = end + 1;
  }
}

/* What hb-shape writes for the glyphs (shape_format_options_t::serialize),
 * with glyph ids, UTF-8 cluster values and the flags. */
static void print_shaped (hb_buffer_t *b, hb_font_t *font)
{
  std::string s = serialize_all (b, font, HB_BUFFER_SERIALIZE_FORMAT_TEXT,
				 (hb_buffer_serialize_flags_t) (HB_BUFFER_SERIALIZE_FLAG_NO_GLYPH_NAMES |
								HB_BUFFER_SERIALIZE_FLAG_GLYPH_FLAGS));
  fwrite (s.data (), 1, s.size (), stdout);
  printf ("\n");
}

static int dump_shape (int argc, char **argv)
{
  const char *font_path = argv[2], *text_path = argv[3], *dir = argv[4];
  hb_blob_t *blob = hb_blob_create_from_file_or_fail (font_path);
  FILE *fp = blob ? fopen (text_path, "rb") : nullptr;
  if (!fp)
  {
    fprintf (stderr, "hb-dump: cannot read %s\n", blob ? text_path : font_path);
    hb_blob_destroy (blob);
    return 1;
  }
  std::vector<hb_feature_t> features;
  parse_feature_list (argv[5], &features);
  hb_face_t *face = hb_face_create (blob, 0);
  hb_font_t *font = var_create_font (face);
  hb_buffer_t *b = hb_buffer_create ();
  std::string line;
  int c;
  bool more = true;
  while (more)
  {
    c = fgetc (fp);
    if (c != EOF && c != '\n')
    {
      line.push_back ((char) c);
      continue;
    }
    more = c != EOF;
    if (!more && line.empty ())
      break;
    hb_buffer_clear_contents (b);
    hb_buffer_add_utf8 (b, line.data (), (int) line.size (), 0, (int) line.size ());
    if (strcmp (dir, "auto") != 0)
      hb_buffer_set_direction (b, hb_direction_from_string (dir, -1));
    hb_buffer_guess_segment_properties (b);
    hb_shape (font, b, features.data (), (unsigned) features.size ());
    print_shaped (b, font);
    line.clear ();
  }
  fclose (fp);
  hb_buffer_destroy (b);
  hb_font_destroy (font);
  hb_face_destroy (face);
  hb_blob_destroy (blob);
  return 0;
}

static int dump_corpus_shape (int argc, char **argv)
{
  hb_blob_t *blob = hb_blob_create_from_file_or_fail (argv[2]);
  if (!blob) { fprintf (stderr, "hb-dump: cannot read %s\n", argv[2]); return 1; }
  unsigned face_index = (unsigned) strtoul (argv[3], nullptr, 10);
  unsigned faces = hb_face_count (blob);
  if (!faces) faces = 1;
  if (face_index >= faces) { hb_blob_destroy (blob); return 2; }
  hb_face_t *face = hb_face_create (blob, face_index);
  hb_font_t *font = var_create_font (face);
  if (strcmp (argv[9], "-") != 0)
  {
    std::vector<hb_variation_t> variations;
    std::string coordinates (argv[9]);
    size_t begin = 0;
    while (begin < coordinates.size ())
    {
      size_t end = coordinates.find (',', begin);
      if (end == std::string::npos) end = coordinates.size ();
      hb_variation_t variation;
      if (!hb_variation_from_string (coordinates.data () + begin, (int) (end - begin), &variation))
      {
	hb_font_destroy (font); hb_face_destroy (face); hb_blob_destroy (blob); return 2;
      }
      variations.push_back (variation);
      begin = end + 1;
    }
    hb_font_set_variations (font, variations.data (), (unsigned) variations.size ());
  }
  std::vector<hb_feature_t> features;
  if (strcmp (argv[8], "-") != 0) parse_feature_list (argv[8], &features);
  hb_buffer_t *buffer = hb_buffer_create ();
  std::vector<hb_codepoint_t> points;
  std::string hex (argv[4]);
  size_t begin = 0;
  while (begin < hex.size ())
  {
    size_t end = hex.find (',', begin);
    if (end == std::string::npos) end = hex.size ();
    std::string part = hex.substr (begin, end - begin);
    char *tail = nullptr;
    unsigned long value = strtoul (part.c_str (), &tail, 16);
    if (!tail || *tail || value > 0x10FFFFu || (value >= 0xD800u && value <= 0xDFFFu))
    {
      hb_buffer_destroy (buffer); hb_font_destroy (font); hb_face_destroy (face);
      hb_blob_destroy (blob); return 2;
    }
    points.push_back ((hb_codepoint_t) value);
    begin = end + 1;
  }
  hb_buffer_add_utf32 (buffer, points.data (), (int) points.size (), 0, (int) points.size ());
  if (argc == 13) /* AAT corpus comparisons also check the optional glyph flags. */
  {
    if (strcmp (argv[10], "-") != 0)
    {
      int scale = atoi (argv[10]);
      hb_font_set_scale (font, scale, scale);
    }
    hb_font_set_ptem (font, strtof (argv[11], nullptr));
    unsigned flags = HB_BUFFER_FLAG_PRODUCE_UNSAFE_TO_CONCAT | HB_BUFFER_FLAG_PRODUCE_SAFE_TO_INSERT_TATWEEL;
    if (strcmp (argv[12], "remove") == 0) flags |= HB_BUFFER_FLAG_REMOVE_DEFAULT_IGNORABLES;
    hb_buffer_set_flags (buffer, (hb_buffer_flags_t) flags);
    if (strcmp (argv[6], "-") != 0)
      hb_buffer_set_language (buffer, hb_language_from_string (argv[6], -1));
    hb_buffer_guess_segment_properties (buffer);
  }
  else
  {
    hb_buffer_set_script (buffer, hb_script_from_string (argv[5], -1));
    hb_buffer_set_language (buffer, hb_language_from_string (argv[6], -1));
    hb_buffer_set_direction (buffer, hb_direction_from_string (argv[7], -1));
  }
  hb_shape (font, buffer, features.data (), (unsigned) features.size ());
  print_shaped (buffer, font);
  hb_buffer_destroy (buffer);
  hb_font_destroy (font);
  hb_face_destroy (face);
  hb_blob_destroy (blob);
  return 0;
}

/* The pseudo-random numbers of --shape-random: the C standard's example
 * LCG, the high 15 bits of the state. */
static unsigned random_next (uint32_t *state)
{
  *state = *state * 1103515245u + 12345u;
  return (*state >> 16) & 0x7FFFu;
}

static int compare_u32 (const void *a, const void *b)
{
  uint32_t x = *(const uint32_t *) a, y = *(const uint32_t *) b;
  return x < y ? -1 : x > y;
}

static int dump_shape_random (int argc, char **argv)
{
  hb_blob_t *blob = hb_blob_create_from_file_or_fail (argv[2]);
  if (!blob)
  {
    fprintf (stderr, "hb-dump: cannot read %s\n", argv[2]);
    return 1;
  }
  unsigned count = (unsigned) strtoul (argv[3], nullptr, 10);
  uint32_t seed = (uint32_t) strtoul (argv[4], nullptr, 10);
  unsigned faces = hb_face_count (blob);
  if (!faces) faces = 1;
  hb_unicode_funcs_t *ufuncs = hb_unicode_funcs_get_default ();
  hb_buffer_t *b = hb_buffer_create ();
  for (unsigned i = 0; i < faces; i++)
  {
    hb_face_t *face = hb_face_create (blob, i);
    hb_font_t *font = var_create_font (face);
    printf ("face %u\n", i);
    /* The mapped characters, grouped by script, scripts in tag order. */
    std::vector<std::pair<uint32_t, std::vector<hb_codepoint_t>>> groups;
    std::vector<uint32_t> tags;
    std::vector<std::pair<uint32_t, hb_codepoint_t>> chars;
    for (hb_codepoint_t u = 0; u <= 0x10FFFFu; u++)
    {
      hb_codepoint_t g;
      if (hb_font_get_nominal_glyph (font, u, &g))
	chars.push_back (std::make_pair ((uint32_t) hb_unicode_script (ufuncs, u), u));
    }
    for (auto &p : chars) tags.push_back (p.first);
    if (!tags.empty ())
    {
      qsort (tags.data (), tags.size (), sizeof (tags[0]), compare_u32);
      tags.erase (std::unique (tags.begin (), tags.end ()), tags.end ());
    }
    for (uint32_t tag : tags)
    {
      std::vector<hb_codepoint_t> cps;
      for (auto &p : chars)
	if (p.first == tag) cps.push_back (p.second);
      printf ("script %08x %u\n", tag, (unsigned) cps.size ());
      uint32_t state = seed;
      for (unsigned n = 0; n < count; n++)
      {
	unsigned len = 1 + random_next (&state) % 16;
	std::vector<hb_codepoint_t> text;
	for (unsigned k = 0; k < len; k++)
	  text.push_back (cps[random_next (&state) % cps.size ()]);
	hb_buffer_clear_contents (b);
	hb_buffer_set_flags (b, (hb_buffer_flags_t) (HB_BUFFER_FLAG_BOT | HB_BUFFER_FLAG_EOT |
						     HB_BUFFER_FLAG_PRODUCE_UNSAFE_TO_CONCAT));
	hb_buffer_add_codepoints (b, text.data (), (int) text.size (), 0, (int) text.size ());
	hb_buffer_guess_segment_properties (b);
	hb_shape (font, b, nullptr, 0);
	printf ("s ");
	for (unsigned k = 0; k < len; k++)
	  printf (k ? ",%x" : "%x", text[k]);
	printf (" ");
	print_shaped (b, font);
      }
    }
    printf ("end\n");
    hb_font_destroy (font);
    hb_face_destroy (face);
  }
  hb_buffer_destroy (b);
  hb_blob_destroy (blob);
  return 0;
}

/* --syllables: the syllable machines on given categories. */
static void syllables_run (const char *machine, hb_buffer_t *b,
			   const unsigned char *cats, unsigned len)
{
  hb_buffer_clear_contents (b);
  for (unsigned i = 0; i < len; i++)
    hb_buffer_add (b, 0, i);
  hb_glyph_info_t *info = b->info;
  for (unsigned i = 0; i < len; i++)
  {
    info[i].ot_shaper_var_u8_category () = cats[i];
    info[i].syllable () = 0;
  }
  if (strcmp (machine, "indic") == 0) find_syllables_indic (b);
  else if (strcmp (machine, "khmer") == 0) find_syllables_khmer (b);
  else if (strcmp (machine, "myanmar") == 0) find_syllables_myanmar (b);
  else find_syllables_use (b);
}

static void syllables_print (hb_buffer_t *b)
{
  for (unsigned i = 0; i < b->len; i++)
    printf ("%02x", b->info[i].syllable ());
  printf ("\n");
}

static int dump_syllables (int argc HB_UNUSED, char **argv)
{
  const char *machine = argv[2];
  if (strcmp (machine, "indic") && strcmp (machine, "khmer") &&
      strcmp (machine, "myanmar") && strcmp (machine, "use"))
  {
    fprintf (stderr, "hb-dump: unknown machine %s\n", machine);
    return 2;
  }
  std::vector<unsigned char> alphabet;
  for (const char *p = argv[3]; *p;)
  {
    alphabet.push_back ((unsigned char) strtoul (p, (char **) &p, 10));
    if (*p == ',') p++;
  }
  if (alphabet.empty ()) return 2;
  unsigned count = (unsigned) strtoul (argv[4], nullptr, 10);
  uint32_t state = (uint32_t) strtoul (argv[5], nullptr, 10);
  hb_buffer_t *b = hb_buffer_create ();
  unsigned n = (unsigned) alphabet.size ();
  unsigned char cats[32];
  for (unsigned len = 1; len <= 4; len++)
  {
    unsigned total = 1;
    for (unsigned k = 0; k < len; k++) total *= n;
    for (unsigned idx = 0; idx < total; idx++)
    {
      unsigned v = idx;
      for (unsigned k = len; k-- > 0;)
      {
	cats[k] = alphabet[v % n];
	v /= n;
      }
      syllables_run (machine, b, cats, len);
      printf ("e ");
      syllables_print (b);
    }
  }
  for (unsigned c = 0; c < count; c++)
  {
    unsigned len = 5 + random_next (&state) % 28;
    for (unsigned k = 0; k < len; k++)
      cats[k] = alphabet[random_next (&state) % n];
    syllables_run (machine, b, cats, len);
    printf ("r ");
    for (unsigned k = 0; k < len; k++)
      printf (k ? ",%x" : "%x", cats[k]);
    printf (" ");
    syllables_print (b);
  }
  hb_buffer_destroy (b);
  return 0;
}

/* --layout-map: the script and language system hb_ot_map_builder_t picks
 * in GSUB and GPOS. */
static int dump_layout_map (hb_blob_t *blob)
{
  ot_tag_set_t scripts = {nullptr, 0, 0};
  hb_unicode_funcs_t *ufuncs = hb_unicode_funcs_get_default ();
  for (hb_codepoint_t u = 0; u <= 0x10FFFFu; u++)
    ot_tag_add (&scripts, (hb_tag_t) hb_unicode_script (ufuncs, u));
  ot_tag_add (&scripts, (hb_tag_t) HB_SCRIPT_INVALID);
  ot_tag_add (&scripts, (hb_tag_t) HB_SCRIPT_MATH);
  ot_tag_sort_unique (&scripts);
  static const hb_tag_t table_tags[2] = {HB_OT_TAG_GSUB, HB_OT_TAG_GPOS};
  unsigned faces = hb_face_count (blob);
  if (!faces) faces = 1;
  for (unsigned i = 0; i < faces; i++)
  {
    hb_face_t *face = hb_face_create (blob, i);
    printf ("face %u\n", i);
    /* The languages: none, and those of the language system tags of
     * both tables. */
    std::vector<hb_language_t> languages;
    languages.push_back (HB_LANGUAGE_INVALID);
    for (unsigned t = 0; t < 2; t++)
    {
      unsigned sc = hb_ot_layout_table_get_script_tags (face, table_tags[t], 0, nullptr, nullptr);
      for (unsigned s = 0; s < sc; s++)
      {
	hb_tag_t ltags[64];
	unsigned start = 0, n;
	do
	{
	  n = 64;
	  hb_ot_layout_script_get_language_tags (face, table_tags[t], s, start, &n, ltags);
	  for (unsigned k = 0; k < n; k++)
	  {
	    hb_language_t l = hb_ot_tag_to_language (ltags[k]);
	    if (l && std::find (languages.begin (), languages.end (), l) == languages.end ())
	      languages.push_back (l);
	  }
	  start += n;
	} while (n == 64);
      }
    }
    for (unsigned s = 0; s < scripts.n; s++)
      for (hb_language_t lang : languages)
      {
	unsigned script_count = HB_OT_MAX_TAGS_PER_SCRIPT, language_count = HB_OT_MAX_TAGS_PER_LANGUAGE;
	hb_tag_t script_tags[HB_OT_MAX_TAGS_PER_SCRIPT], language_tags[HB_OT_MAX_TAGS_PER_LANGUAGE];
	hb_ot_tags_from_script_and_language ((hb_script_t) scripts.v[s], lang,
					     &script_count, script_tags,
					     &language_count, language_tags);
	printf ("sel %08x %s", scripts.v[s], lang ? hb_language_to_string (lang) : "*");
	for (unsigned t = 0; t < 2; t++)
	{
	  unsigned script_index, language_index;
	  hb_tag_t chosen;
	  hb_bool_t found = hb_ot_layout_table_select_script (face, table_tags[t], script_count, script_tags,
							      &script_index, &chosen);
	  hb_ot_layout_script_select_language (face, table_tags[t], script_index,
					       language_count, language_tags, &language_index);
	  printf (" %d %u %08x %u", found ? 1 : 0, script_index, chosen, language_index);
	}
	printf ("\n");
      }
    printf ("end\n");
    hb_face_destroy (face);
  }
  free (scripts.v);
  return 0;
}

/* --repack: see the top of the file. */
struct repack_reader_t
{
  const unsigned char *p, *end;
  bool ok;
  unsigned u8 () { if (p >= end) { ok = false; return 0; } return *p++; }
  unsigned u32 ()
  {
    unsigned v = 0;
    for (unsigned i = 0; i < 4; i++) v = (v << 8) | u8 ();
    return v;
  }
};

struct repack_link_t
{
  unsigned width, is_signed, whence, bias, position, objidx;
};

struct repack_object_t
{
  std::vector<char> bytes;
  std::vector<repack_link_t> real_links;
  std::vector<unsigned> virtual_links;
};

static void print_repack_result (unsigned i, hb_blob_t *blob)
{
  if (!blob)
  {
    printf ("graph %u fail\n", i);
    return;
  }
  unsigned len = 0;
  const char *data = hb_blob_get_data (blob, &len);
  uint64_t h = 0xcbf29ce484222325ull;
  for (unsigned k = 0; k < len; k++)
  {
    h ^= (unsigned char) data[k];
    h *= 0x100000001b3ull;
  }
  printf ("graph %u ok %u %016llx\n", i, len, (unsigned long long) h);
  hb_blob_destroy (blob);
}

static hb_blob_t *repack_serializer (hb_tag_t tag, const std::vector<repack_object_t> &objs)
{
  size_t total = 1024;
  for (const auto &o : objs) total += o.bytes.size ();
  std::vector<char> buf (total);
  hb_serialize_context_t c (buf.data (), total);
  c.start_serialize<char> ();
  for (unsigned i = 0; i < objs.size (); i++)
  {
    const auto &o = objs[i];
    c.push ();
    char *head = c.allocate_size<char> (o.bytes.size ());
    if (!head) return nullptr;
    if (o.bytes.size ()) memcpy (head, o.bytes.data (), o.bytes.size ());
    for (const auto &l : o.real_links)
    {
      char *at = head + l.position;
      hb_serialize_context_t::whence_t w = (hb_serialize_context_t::whence_t) l.whence;
      if (l.width == 2 && l.is_signed) c.add_link (*(OT::Offset<OT::HBINT16> *) at, l.objidx, w, l.bias);
      else if (l.width == 2) c.add_link (*(OT::Offset16 *) at, l.objidx, w, l.bias);
      else if (l.width == 3) c.add_link (*(OT::Offset24 *) at, l.objidx, w, l.bias);
      else if (l.is_signed) c.add_link (*(OT::Offset<OT::HBINT32> *) at, l.objidx, w, l.bias);
      else c.add_link (*(OT::Offset32 *) at, l.objidx, w, l.bias);
    }
    for (unsigned v : o.virtual_links) c.add_virtual_link (v);
    if (c.pop_pack (false) != i + 1) return nullptr;
  }
  c.end_serialize ();
  if (c.in_error () && !c.only_offset_overflow ()) return nullptr;
  if (!c.offset_overflow ()) return c.copy_blob ();
  return hb_resolve_overflows (c.object_graph (), tag);
}

static hb_blob_t *repack_or_fail (hb_tag_t tag, const std::vector<repack_object_t> &objs)
{
  std::vector<hb_subset_serialize_object_t> hb (objs.size ());
  std::vector<std::vector<hb_subset_serialize_link_t>> real (objs.size ()), virt (objs.size ());
  for (unsigned i = 0; i < objs.size (); i++)
  {
    for (const auto &l : objs[i].real_links)
      real[i].push_back (hb_subset_serialize_link_t {l.width, l.position, l.objidx});
    for (unsigned v : objs[i].virtual_links)
      virt[i].push_back (hb_subset_serialize_link_t {0, 0, v});
    hb[i].head = (char *) objs[i].bytes.data ();
    hb[i].tail = hb[i].head + objs[i].bytes.size ();
    hb[i].num_real_links = real[i].size ();
    hb[i].real_links = real[i].data ();
    hb[i].num_virtual_links = virt[i].size ();
    hb[i].virtual_links = virt[i].data ();
  }
  return hb_subset_serialize_or_fail (tag, hb.data (), hb.size ());
}

static int dump_repack (hb_blob_t *blob)
{
  unsigned len = 0;
  const char *data = hb_blob_get_data (blob, &len);
  repack_reader_t r = {(const unsigned char *) data, (const unsigned char *) data + len, true};
  unsigned count = r.u32 ();
  for (unsigned i = 0; i < count && r.ok; i++)
  {
    hb_tag_t tag = r.u32 ();
    unsigned kind = r.u32 ();
    unsigned n = r.u32 ();
    std::vector<repack_object_t> objs (n);
    for (auto &o : objs)
    {
      unsigned size = r.u32 ();
      if ((size_t) (r.end - r.p) < size) { r.ok = false; break; }
      o.bytes.assign ((const char *) r.p, (const char *) r.p + size);
      r.p += size;
      unsigned nr = r.u32 ();
      for (unsigned k = 0; k < nr && r.ok; k++)
      {
        repack_link_t l;
        l.width = r.u8 ();
        l.is_signed = r.u8 ();
        l.whence = r.u8 ();
        r.u8 ();
        l.bias = r.u32 ();
        l.position = r.u32 ();
        l.objidx = r.u32 ();
        o.real_links.push_back (l);
      }
      unsigned nv = r.u32 ();
      for (unsigned k = 0; k < nv && r.ok; k++)
        o.virtual_links.push_back (r.u32 ());
    }
    if (!r.ok) break;
    print_repack_result (i, kind ? repack_or_fail (tag, objs) : repack_serializer (tag, objs));
  }
  if (!r.ok)
  {
    fprintf (stderr, "hb-dump: truncated --repack file\n");
    return 1;
  }
  return 0;
}

/* --subset: util/hb-subset.cc's option callbacks without GLib. */

struct subset_args_t
{
  hb_face_t *face = nullptr;
  hb_font_t *font = nullptr;
  hb_subset_input_t *input = nullptr;
};

static bool subset_fail (const char *fmt, const char *s)
{
  fprintf (stderr, "hb-dump --subset: ");
  fprintf (stderr, fmt, s);
  fprintf (stderr, "\n");
  return false;
}

/* The suffix of an option name: '+', '-' or 0. */
static char subset_suffix (const char *name)
{
  size_t n = strlen (name);
  char c = n ? name[n - 1] : 0;
  return c == '+' || c == '-' ? c : 0;
}

/* The start of every set option: clears the set without a suffix, and
 * handles `*`; returns true when the value is done. */
static bool subset_set_start (hb_set_t *set, char suffix, const char *arg)
{
  if (!suffix) hb_set_clear (set);
  if (strcmp (arg, "*") != 0) return false;
  hb_set_clear (set);
  if (suffix != '-') hb_set_invert (set);
  return true;
}

static void subset_set_put (hb_set_t *set, char suffix, hb_codepoint_t a, hb_codepoint_t b)
{
  if (suffix != '-') hb_set_add_range (set, a, b);
  else hb_set_del_range (set, a, b);
}

/* parse_gids (base 10, ", ") and parse_unicodes (base 16, its
 * delimiters). */
static bool subset_numbers (hb_set_t *set, char suffix, const char *arg, int base,
			    const char *delimiters, bool reject_minus)
{
  if (subset_set_start (set, suffix, arg)) return true;
  char *s = (char *) arg;
  char *p;
  while (s && *s)
  {
    while (*s && strchr (delimiters, *s)) s++;
    if (!*s) break;
    errno = 0;
    hb_codepoint_t start = strtoul (s, &p, base);
    if ((reject_minus && s[0] == '-') || errno || s == p)
      return subset_fail ("failed parsing at: '%s'", s);
    hb_codepoint_t end = start;
    if (p && p[0] == '-')
    {
      s = ++p;
      end = strtoul (s, &p, base);
      if (s[0] == '-' || errno || s == p)
	return subset_fail ("failed parsing at: '%s'", s);
      if (end < start) return subset_fail ("invalid range at: '%s'", s);
    }
    subset_set_put (set, suffix, start, end);
    s = p;
  }
  return true;
}

/* parse_nameids and parse_name_languages: base 10, no ranges. */
static bool subset_ids (hb_set_t *set, char suffix, const char *arg)
{
  if (subset_set_start (set, suffix, arg)) return true;
  char *s = (char *) arg;
  char *p;
  while (s && *s)
  {
    while (*s && strchr (", ", *s)) s++;
    if (!*s) break;
    errno = 0;
    hb_codepoint_t u = strtoul (s, &p, 10);
    if (errno || s == p) return subset_fail ("failed parsing id at: '%s'", s);
    subset_set_put (set, suffix, u, u);
    s = p;
  }
  return true;
}

/* parse_layout_tag_list and parse_drop_tables. */
static bool subset_tags (hb_set_t *set, char suffix, const char *arg)
{
  if (subset_set_start (set, suffix, arg)) return true;
  std::string copy (arg);
  for (char *s = strtok (&copy[0], ", "); s; s = strtok (nullptr, ", "))
  {
    if (strlen (s) > 4) return subset_fail ("failed parsing tag at: '%s'", s);
    hb_tag_t tag = hb_tag_from_string (s, strlen (s));
    subset_set_put (set, suffix, tag, tag);
  }
  return true;
}

/* parse_text, with g_utf8_get_char's decoding of valid UTF-8. */
static bool subset_text (hb_set_t *set, char suffix, const char *arg)
{
  if (subset_set_start (set, suffix, arg)) return true;
  const unsigned char *c = (const unsigned char *) arg;
  while (*c)
  {
    unsigned n = *c < 0x80 ? 1 : *c < 0xE0 ? 2 : *c < 0xF0 ? 3 : 4;
    hb_codepoint_t cp = n == 1 ? *c : *c & (0x7F >> n);
    for (unsigned i = 1; i < n && c[i]; i++)
      cp = (cp << 6) | (c[i] & 0x3F);
    subset_set_put (set, suffix, cp, cp);
    for (unsigned i = 0; i < n && *c; i++) c++;
  }
  return true;
}

/* parse_glyphs. */
static bool subset_glyphs (subset_args_t *a, char suffix, const char *arg)
{
  hb_set_t *set = hb_subset_input_glyph_set (a->input);
  if (subset_set_start (set, suffix, arg)) return true;
  if (!a->font) a->font = var_create_font (a->face);
  const char *p = arg, *p_end = arg + strlen (arg);
  while (p < p_end)
  {
    while (p < p_end && (*p == ' ' || *p == ',')) p++;
    const char *end = p;
    while (end < p_end && *end != ' ' && *end != ',') end++;
    if (p < end)
    {
      hb_codepoint_t gid;
      if (!hb_font_glyph_from_string (a->font, p, end - p, &gid))
	return subset_fail ("failed parsing glyph name: '%s'", p);
      subset_set_put (set, suffix, gid, gid);
    }
    p = end + 1;
  }
  return true;
}

/* parse_glyph_map. */
static bool subset_gid_map (subset_args_t *a, const char *arg)
{
  hb_set_t *glyphs = hb_subset_input_glyph_set (a->input);
  char *s = (char *) arg;
  char *p;
  while (s && *s)
  {
    while (*s && strchr (", ", *s)) s++;
    if (!*s) break;
    errno = 0;
    hb_codepoint_t from = strtoul (s, &p, 10);
    if (s[0] == '-' || errno || s == p || !p || p[0] != ':')
      return subset_fail ("failed parsing glyph map at: '%s'", s);
    s = ++p;
    hb_codepoint_t to = strtoul (s, &p, 10);
    if (s[0] == '-' || errno || s == p)
      return subset_fail ("failed parsing glyph map at: '%s'", s);
    hb_set_add (glyphs, from);
    hb_map_set (hb_subset_input_old_to_new_glyph_mapping (a->input), from, to);
    s = p;
  }
  return true;
}

/* util/helper-subset.hh's parse_instancing_spec. */
static bool subset_instance (subset_args_t *a, const char *arg)
{
  std::string copy (arg);
  char *rest = &copy[0];
  char *s;
  while ((s = strtok (rest, "=")))
  {
    rest = nullptr;
    unsigned len = strlen (s);
    if (len > 4) return subset_fail ("failed parsing axis tag at: '%s'", s);
    if (strcmp (s, "*") == 0)
    {
      s = strtok (nullptr, ", ");
      if (!s || strcmp (s, "drop") != 0)
	return subset_fail ("failed parsing axis position at: '%s'", s ? s : "");
      if (!hb_subset_input_pin_all_axes_to_default (a->input, a->face))
	return subset_fail ("%s", "failed pinning all axes to default");
      continue;
    }
    hb_tag_t axis = hb_tag_from_string (s, len);
    s = strtok (nullptr, ", ");
    if (!s) return subset_fail ("value not specified for axis '%s'", "");
    if (strcmp (s, "drop") == 0)
    {
      if (!hb_subset_input_pin_axis_to_default (a->input, a->face, axis))
	return subset_fail ("cannot pin axis at: '%s'", s);
      continue;
    }
    float min, def, max;
    if (!hb_subset_axis_range_from_string (s, -1, &min, &max, &def))
      return subset_fail ("failed parsing axis range: '%s'", s);
    if (!hb_subset_input_set_axis_range (a->input, a->face, axis, min, max, def))
      return subset_fail ("cannot set axis range at: '%s'", s);
  }
  return true;
}

struct subset_flag_t
{
  const char *name;
  hb_subset_flags_t flag;
};

static const subset_flag_t subset_flags[] = {
  {"no-hinting", HB_SUBSET_FLAGS_NO_HINTING},
  {"retain-gids", HB_SUBSET_FLAGS_RETAIN_GIDS},
  {"desubroutinize", HB_SUBSET_FLAGS_DESUBROUTINIZE},
  {"downgrade-cff2", HB_SUBSET_FLAGS_DOWNGRADE_CFF2},
  {"cff-identity-charset", HB_SUBSET_FLAGS_CFF_IDENTITY_CHARSET},
  {"name-legacy", HB_SUBSET_FLAGS_NAME_LEGACY},
  {"set-overlaps-flag", HB_SUBSET_FLAGS_SET_OVERLAPS_FLAG},
  {"notdef-outline", HB_SUBSET_FLAGS_NOTDEF_OUTLINE},
  {"no-prune-unicode-ranges", HB_SUBSET_FLAGS_NO_PRUNE_UNICODE_RANGES},
  {"no-layout-closure", HB_SUBSET_FLAGS_NO_LAYOUT_CLOSURE},
  {"no-bidi-closure", HB_SUBSET_FLAGS_NO_BIDI_CLOSURE},
  {"glyph-names", HB_SUBSET_FLAGS_GLYPH_NAMES},
  {"passthrough-tables", HB_SUBSET_FLAGS_PASSTHROUGH_UNRECOGNIZED},
  {"iftb-requirements", HB_SUBSET_FLAGS_IFTB_REQUIREMENTS},
  {"retain-num-glyphs", HB_SUBSET_FLAGS_RETAIN_NUM_GLYPHS},
  {"optimize", HB_SUBSET_FLAGS_OPTIMIZE_IUP_DELTAS},
};

/* Applies one option with a value; false on a bad value. */
static bool subset_option (subset_args_t *a, const std::string &name, const char *arg)
{
  std::string base = name;
  char suffix = subset_suffix (name.c_str ());
  if (suffix) base.erase (base.size () - 1);
  hb_subset_input_t *in = a->input;
  if (base == "gids")
    return subset_numbers (hb_subset_input_glyph_set (in), suffix, arg, 10, ", ", true);
  if (base == "unicodes")
    return subset_numbers (hb_subset_input_unicode_set (in), suffix, arg, 16,
			   "<+->{},;&#\\xXuUnNiI\n\t\v\f\r ", false);
  if (base == "text") return subset_text (hb_subset_input_unicode_set (in), suffix, arg);
  if (base == "glyphs") return subset_glyphs (a, suffix, arg);
  if (base == "name-IDs")
    return subset_ids (hb_subset_input_set (in, HB_SUBSET_SETS_NAME_ID), suffix, arg);
  if (base == "name-languages")
    return subset_ids (hb_subset_input_set (in, HB_SUBSET_SETS_NAME_LANG_ID), suffix, arg);
  if (base == "layout-features")
    return subset_tags (hb_subset_input_set (in, HB_SUBSET_SETS_LAYOUT_FEATURE_TAG), suffix, arg);
  if (base == "layout-scripts")
    return subset_tags (hb_subset_input_set (in, HB_SUBSET_SETS_LAYOUT_SCRIPT_TAG), suffix, arg);
  if (base == "drop-tables")
    return subset_tags (hb_subset_input_set (in, HB_SUBSET_SETS_DROP_TABLE_TAG), suffix, arg);
  if (name == "gid-map") return subset_gid_map (a, arg);
  if (name == "variations" || name == "instance") return subset_instance (a, arg);
  return subset_fail ("unknown option --%s", name.c_str ());
}

static int dump_subset (int argc, char **argv)
{
  /* The font first, as hb-subset parses the face before the options. */
  const char *font_file = nullptr, *output_file = nullptr;
  unsigned face_index = 0, iterations = 1;
  bool preprocess = false;
  std::vector<std::pair<std::string, const char *>> options;
  for (int i = 2; i < argc; i++)
  {
    std::string a = argv[i];
    const char *value = nullptr;
    std::string name;
    if (a == "-o")
      name = "output-file";
    else if (a.compare (0, 2, "--") == 0)
    {
      size_t eq = a.find ('=');
      name = a.substr (2, eq == std::string::npos ? std::string::npos : eq - 2);
      if (eq != std::string::npos) value = argv[i] + eq + 1;
    }
    else
    {
      subset_fail ("unexpected argument '%s'", argv[i]);
      return 2;
    }
    bool is_flag = name == "keep-everything" || name == "preprocess" || name == "preprocess-face";
    for (const auto &f : subset_flags) is_flag = is_flag || name == f.name;
    if (is_flag)
    {
      if (value) { subset_fail ("--%s takes no value", name.c_str ()); return 2; }
      if (name == "preprocess" || name == "preprocess-face") preprocess = true;
      else options.push_back ({name, nullptr});
      continue;
    }
    if (!value)
    {
      if (i + 1 >= argc) { subset_fail ("--%s needs a value", name.c_str ()); return 2; }
      value = argv[++i];
    }
    if (name == "font-file") font_file = value;
    else if (name == "output-file") output_file = value;
    else if (name == "face-index") face_index = strtoul (value, nullptr, 10);
    else if (name == "num-iterations") iterations = strtoul (value, nullptr, 10);
    else options.push_back ({name, value});
  }
  if (!font_file || !output_file)
  {
    fprintf (stderr, "usage: hb-dump --subset --font-file=FILE --output-file=FILE [OPTION...]\n");
    return 2;
  }
  hb_blob_t *blob = hb_blob_create_from_file_or_fail (font_file);
  if (!blob)
  {
    fprintf (stderr, "hb-dump: cannot read %s\n", font_file);
    return 1;
  }
  subset_args_t a;
  a.face = hb_face_create (blob, face_index);
  hb_blob_destroy (blob);
  a.input = hb_subset_input_create_or_fail ();
  int status = 0;
  for (const auto &o : options)
  {
    bool ok = true;
    if (o.first == "keep-everything")
      hb_subset_input_keep_everything (a.input);
    else if (!o.second)
    {
      for (const auto &f : subset_flags)
	if (o.first == f.name)
	  hb_subset_input_set_flags (a.input, hb_subset_input_get_flags (a.input) | f.flag);
    }
    else
      ok = subset_option (&a, o.first, o.second);
    if (!ok) { status = 2; break; }
  }
  hb_face_t *source = hb_face_reference (a.face);
  if (!status && preprocess)
  {
    hb_face_destroy (source);
    source = hb_subset_preprocess (a.face);
  }
  hb_face_t *out = nullptr;
  for (unsigned i = 0; !status && i < (iterations ? iterations : 1); i++)
  {
    hb_face_destroy (out);
    out = hb_subset_or_fail (source, a.input);
  }
  if (!status && !out)
  {
    fprintf (stderr, "hb-dump --subset: subsetting failed\n");
    status = 1;
  }
  if (!status)
  {
    hb_blob_t *result = hb_face_reference_blob (out);
    unsigned size = 0;
    const char *data = hb_blob_get_data (result, &size);
    FILE *fp = fopen (output_file, "wb");
    bool written = fp && fwrite (data, 1, size, fp) == size;
    if (fp && fclose (fp) != 0) written = false;
    if (!written)
    {
      fprintf (stderr, "hb-dump: cannot write %s\n", output_file);
      status = 1;
    }
    hb_blob_destroy (result);
  }
  hb_face_destroy (out);
  hb_face_destroy (source);
  hb_font_destroy (a.font);
  hb_subset_input_destroy (a.input);
  hb_face_destroy (a.face);
  return status;
}

/* The modes with more arguments. */
struct args_mode_t
{
  const char *name;
  int argc;
  int (*dump) (int argc, char **argv);
};

static const args_mode_t args_modes[] = {
  {"--shape", 6, dump_shape},
  {"--corpus-shape", 10, dump_corpus_shape},
  {"--aat-corpus-shape", 13, dump_corpus_shape},
  {"--shape-random", 5, dump_shape_random},
  {"--syllables", 6, dump_syllables},
  {"--png-digest", 3, dump_png_digest},
  {"--paint-var", 5, dump_paint_var},
  {"--var-metrics", 5, dump_var_metrics},
};

/* The modes that read no file. */
struct fileless_mode_t
{
  const char *name;
  int (*dump) ();
};

static const fileless_mode_t fileless_modes[] = {
  {"--unicode", dump_unicode},
  {"--arabic-pua", dump_arabic_pua},
  {"--ot-tags", dump_ot_tags},
  {"--features", dump_features},
  {"--roundf", dump_roundf},
  {"--mult", dump_mult},
};

struct dump_mode_t
{
  const char *name;
  int (*dump) (hb_blob_t *blob);
};

static const dump_mode_t modes[] = {
  {"--metrics", dump_metrics},
  {"--metrics-variation", dump_metrics_variation},
  {"--names", dump_names},
  {"--cmap", dump_cmap},
  {"--cmap-digest", dump_cmap_digest},
  {"--baselines", dump_baselines},
  {"--vowel-constraints", dump_vowel_constraints},
  {"--layout-map", dump_layout_map},
  {"--paint", dump_paint},
  {"--color", dump_color},
  {"--extents", dump_extents},
  {"--repack", dump_repack},
};

/* The modes that read a font and a second file. */
struct two_file_mode_t
{
  const char *name;
  int (*dump) (hb_blob_t *blob, const char *path);
};

static const two_file_mode_t two_file_modes[] = {
  {"--serialize", dump_serialize},
  {"--deserialize", dump_deserialize},
  {"--sampled-metrics", dump_sampled_metrics},
};

int main (int argc, char **argv)
{
  if (argc >= 2 && (strcmp (argv[1], "--var") == 0 ||
                    strcmp (argv[1], "--var-normalize") == 0))
    return dump_var_reference (argc, argv);
  if (argc >= 2 && strcmp (argv[1], "--subset") == 0)
    return dump_subset (argc, argv);
  if (argc >= 2 && !reference_options (&argc, argv))
  {
    fprintf (stderr, "hb-dump: invalid variation options\n");
    return 2;
  }
  for (unsigned i = 0; i < sizeof (args_modes) / sizeof (args_modes[0]); i++)
    if (argc == args_modes[i].argc && strcmp (argv[1], args_modes[i].name) == 0)
      return args_modes[i].dump (argc, argv);
  if (argc == 4)
    for (unsigned i = 0; i < sizeof (two_file_modes) / sizeof (two_file_modes[0]); i++)
      if (strcmp (argv[1], two_file_modes[i].name) == 0)
      {
	hb_blob_t *blob = hb_blob_create_from_file_or_fail (argv[2]);
	if (!blob)
	{
	  fprintf (stderr, "hb-dump: cannot read %s\n", argv[2]);
	  return 1;
	}
	int status = two_file_modes[i].dump (blob, argv[3]);
	hb_blob_destroy (blob);
	return status;
      }
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
    if (strncmp (argv[1], "--paint=", 8) == 0)
    {
      paint_slant = (float) atof (argv[1] + 8);
      for (unsigned i = 0; i < sizeof (modes) / sizeof (modes[0]); i++)
	if (strcmp (modes[i].name, "--paint") == 0)
	  mode = &modes[i];
    }
  }
  if (!mode || argc < 2 || argc > 3)
  {
    for (unsigned i = 0; i < sizeof (args_modes) / sizeof (args_modes[0]); i++)
      if (argc >= 2 && strcmp (argv[1], args_modes[i].name) == 0)
      {
	fprintf (stderr, "usage: hb-dump %s %s\n", args_modes[i].name,
		 strcmp (args_modes[i].name, "--syllables") == 0 ? "MACHINE ALPHABET COUNT SEED" : args_modes[i].argc == 6 ? "FONT TEXTFILE DIRECTION FEATURES" : "FONT COUNT SEED");
	return 2;
      }
    fprintf (stderr, "usage: hb-dump [MODE] FILE; MODE is one of:");
    for (unsigned i = 0; i < sizeof (modes) / sizeof (modes[0]); i++)
      fprintf (stderr, " %s", modes[i].name);
    fprintf (stderr, "; or hb-dump MODE, MODE one of:");
    for (unsigned i = 0; i < sizeof (fileless_modes) / sizeof (fileless_modes[0]); i++)
      fprintf (stderr, " %s", fileless_modes[i].name);
    fprintf (stderr, "; or hb-dump MODE FONT FILE, MODE one of:");
    for (unsigned i = 0; i < sizeof (two_file_modes) / sizeof (two_file_modes[0]); i++)
      fprintf (stderr, " %s", two_file_modes[i].name);
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
