/*
 * ft-dump: write what FreeType reads from a font, for comparison with
 * sofel (ADR 0008). Only FreeType's public API is used.
 *
 * Usage: ft-dump FILE
 *
 * Output is line-oriented text. All numbers are decimal. For each face
 * index i of the file (a TTC/OTC has several):
 *
 *   face <i>
 *   error <FreeType error code>          (when the face cannot be opened;
 *                                         nothing else follows for it)
 *   num_glyphs <face->num_glyphs>
 *   head <field> <value>...              (when FT_Get_Sfnt_Table gives head)
 *   hhea <field> <value>                 (likewise for hhea)
 *   maxp <field> <value>                 (likewise for maxp)
 *   cmap <platform> <encoding> <format>  (one per charmap FreeType accepted,
 *                                         in FreeType's order)
 *   map <code> <glyph>                   (FT_Get_First_Char/FT_Get_Next_Char
 *                                         on the charmap above; not for
 *                                         format 14)
 *   advance <glyph> <value>              (FT_Get_Advances, FT_LOAD_NO_SCALE)
 *   fs_type <value>                      (FT_Get_FSType_Flags)
 *   ps_name <hex>                        (FT_Get_Postscript_Name; `-` for
 *                                         none)
 *   face_metrics <ascender> <descender> <height> <max_advance_width>
 *                <max_advance_height> <underline_position>
 *                <underline_thickness> <xMin> <yMin> <xMax> <yMax>
 *                                        (the FT_Face fields; bbox last)
 *   sfnt_name <i> <platform> <encoding> <language> <name> <hex>
 *                                        (FT_Get_Sfnt_Name, every record;
 *                                         `-` for an empty string)
 *   v_advance <glyph> <value>            (FT_Get_Advances,
 *                                         FT_LOAD_NO_SCALE |
 *                                         FT_LOAD_VERTICAL_LAYOUT)
 *   glyph_name <glyph> <error> <hex>     (FT_Get_Glyph_Name with 256 bytes;
 *                                         `-` for an empty name)
 *   name_index <glyph> <value>           (FT_Get_Name_Index of that name,
 *                                         for every non-empty name)
 *   end
 *
 * Usage: ft-dump --advances [--named-instance=N] [--variations=TAG=value,...] FILE
 *
 * For each face, query every glyph's horizontal and vertical advance with
 * FT_Get_Advance (a fresh face for each glyph) and FT_Get_Advances (one
 * fresh face for the entire range).  Each face starts at the same location
 * with FT_LOAD_NO_SCALE; a failed query reports its error and value 0:
 *
 *   face <i>
 *   error <code>                        (the face cannot be opened)
 *   num_glyphs <n>
 *   advance <h|v> <single|batch> <glyph> <error> <value>
 *   end
 *
 * Field names and order:
 *   head: version font_revision checksum_adjustment magic_number flags
 *         units_per_em created (two numbers: high and low 32 bits)
 *         modified (likewise) x_min y_min x_max y_max mac_style
 *         lowest_rec_ppem font_direction_hint index_to_loc_format
 *         glyph_data_format
 *   hhea: version ascender descender line_gap advance_width_max
 *         min_left_side_bearing min_right_side_bearing x_max_extent
 *         caret_slope_rise caret_slope_run caret_offset
 *         metric_data_format number_of_h_metrics
 *   maxp: version num_glyphs max_points max_contours max_composite_points
 *         max_composite_contours max_zones max_twilight_points
 *         max_storage max_function_defs max_instruction_defs
 *         max_stack_elements max_size_of_instructions
 *         max_component_elements max_component_depth
 *         (all but version and num_glyphs only for version 1.0)
 *
 * Usage: ft-dump --calc
 *
 * Reads lines from standard input and writes FreeType's result for each
 * (ADR 0009). Arguments and results are decimal:
 *
 *   mul_fix A B     -> FT_MulFix(A, B)
 *   div_fix A B     -> FT_DivFix(A, B)
 *   mul_div A B C   -> FT_MulDiv(A, B, C)
 *   length X Y      -> FT_Vector_Length of (X, Y)
 *
 * Usage: ft-dump --outline SIZE FILE
 *
 * Usage: ft-dump --stroke SIZE GLYPH RADIUS JOIN MITER BORDER FILE
 *
 * Strokes one loaded outline through FT_Glyph_Stroke (BORDER=0), or
 * FT_Glyph_StrokeBorder (BORDER=1 outside, 2 inside), using butt caps.
 * JOIN is round=0, bevel=1, miter-variable=2 or miter-fixed=3.
 * MITER is signed 16.16. The M/L/Q/C/Z lines are FT_Outline_Decompose
 * of the resulting outline, with Z appended after each contour.
 *
 * Writes the outline of every glyph (ADR 0011), loaded with
 * FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP, plus FT_LOAD_NO_SCALE when SIZE
 * is `unscaled`; otherwise SIZE is a pixel size for FT_Set_Pixel_Sizes.
 * For each face:
 *
 *   face <i>
 *   error <code>                        (the face cannot be opened)
 *   num_glyphs <n>
 *   glyph <g>
 *   M x y | L x y | Q cx cy x y | C c1x c1y c2x c2y x y | Z
 *                                       (FT_Outline_Decompose; Z ends a
 *                                       contour)
 *   error <code>                        (loading or decomposing failed;
 *                                       nothing else follows for the glyph)
 *   overlap <0|1>                       (FT_OUTLINE_OVERLAP)
 *   cbox <xMin> <yMin> <xMax> <yMax>    (FT_Outline_Get_CBox)
 *   end
 *
 * Usage: ft-dump --sampled-outline SIZE FILE PLAN
 *
 * PLAN contains sorted "face glyph" decimal pairs, at most 128 glyphs
 * per face. The output has the --outline form for these glyphs only,
 * with an additional "advance <value>" line after each glyph line
 * (FT_Get_Advance with FT_LOAD_NO_SCALE).
 * Add `--sampled-outline-var SIZE FILE PLAN TAG=value[,TAG=value...]` to
 * apply design coordinates before loading the sampled glyphs (ADR 0031).
 *
 * Usage: ft-dump --raster PPEM FILE
 *
 * Renders every glyph (ADR 0012): FT_Set_Pixel_Sizes(PPEM), FT_Load_Glyph
 * with FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP, FT_Render_Glyph with
 * FT_RENDER_MODE_NORMAL. For each face, `face`, `error` and `num_glyphs`
 * as above, then for each glyph:
 *
 *   glyph <g>
 *   error <code>                        (loading or rendering failed)
 *   bitmap <left> <top> <width> <rows>
 *   <row as hex, two digits per pixel>  (one line per row, top first;
 *                                       none when width or rows is 0)
 *   end
 *
 * Usage: ft-dump --raster PPEM --color FILE
 *
 * Renders the COLR version 0 glyphs in color (ADR 0023). For each face,
 * `face`, `error` and `num_glyphs` as above, then for palette 0 and the
 * last palette (only 0 when the font has no CPAL):
 *
 *   palette <i>                         (FT_Palette_Select)
 *   palette_error <code>                (selecting failed; the palette's
 *                                       glyphs are skipped)
 *
 * and for each glyph for which FT_Get_Color_Glyph_Layer gives a layer,
 * FT_Load_Glyph with FT_LOAD_COLOR | FT_LOAD_NO_HINTING |
 * FT_LOAD_NO_BITMAP | FT_LOAD_NO_SVG and FT_Render_Glyph with
 * FT_RENDER_MODE_NORMAL:
 *
 *   glyph <g>
 *   error <code>                        (loading or rendering failed)
 *   bitmap <left> <top> <width> <rows> <bgra|gray>
 *   <row as hex, eight digits (B, G, R, A) per pixel for bgra, two for
 *    gray>                              (one line per row, top first;
 *                                       none when width or rows is 0)
 *   end
 *
 * Usage: ft-dump --cid FILE
 *
 * For each face (ADR 0021), `face` and `error` as above, then:
 *
 *   is_cid <error> <0|1>                (FT_Get_CID_Is_Internally_CID_Keyed)
 *   ros <error> <registry> <ordering> <supplement>
 *                                       (FT_Get_CID_Registry_Ordering_Supplement;
 *                                       strings in hex, `-` for NULL, `""` for empty; only the
 *                                       error code when it fails)
 *   cid <glyph> <error> <cid>           (FT_Get_CID_From_Glyph_Index for every
 *                                       glyph and for num_glyphs itself; the
 *                                       cid is 0 when it fails)
 *   end
 *
 * Usage: ft-dump --ps-info FILE
 *
 * For each face (ADR 0021), `face` and `error` as above, then:
 *
 *   has_ps_glyph_names <0|1>            (FT_Has_PS_Glyph_Names)
 *   ps_font_info <error> <version> <notice> <full_name> <family_name>
 *                <weight> <italic_angle> <is_fixed_pitch>
 *                <underline_position> <underline_thickness>
 *                                       (FT_Get_PS_Font_Info; strings in hex,
 *                                       `-` for NULL; only the error code when
 *                                       it fails)
 *   fs_type_flags <value>               (FT_Get_FSType_Flags: the `/FSType`
 *                                       of the CFF `PostScript` string when
 *                                       it is not 0, else OS/2's fsType;
 *                                       FT_Get_PS_Font_Extra is not public)
 *   end
 *
 * Usage: ft-dump --cff2-private [--named-instance=N | --variations=TAG=value,...] FILE
 *
 * For each CFF2 face and Font DICT, prints the location-evaluated Private
 * DICT scalar fields as `private`, followed by its six array fields as
 * `private_array`. The reference accesses the CFF driver's internal load
 * service from the pinned FreeType source; no public CFF2 Private DICT
 * query exists. Non-CFF2 faces have no `private` lines.
 *
 * --outline and --raster open each face with the random seed 0
 * (FT_PARAM_TAG_RANDOM_SEED), so that the CFF `random` operator starts
 * from the font's initialRandomSeed instead of an address (ADR 0021).
 *
 * Usage: ft-dump --colr FILE
 *
 * The raw COLR and CPAL values (ADR 0023). For each face, `face` and
 * `error` as above, then:
 *
 *   cpal <num_palettes> <num_palette_entries>   (FT_Palette_Data_Get)
 *   palette <i> <name_id> <flags> <colors>      (the palette's name id
 *                                       and flags, and FT_Palette_Select's
 *                                       colors as RRGGBBAA joined by `,`,
 *                                       `-` for none)
 *   color_name <c> <name_id>            (palette_entry_name_ids)
 *   layers <g> <glyph:index,...>        (FT_Get_Color_Glyph_Layer, for
 *                                       every glyph with layers)
 *   paint <g> <tree>                    (FT_Get_Color_Glyph_Paint with
 *                                       FT_COLOR_NO_ROOT_TRANSFORM and
 *                                       FT_Get_Paint, in pre-order, for every
 *                                       glyph with a paint; see paint_tree)
 *   clip <g> <xMin> <yMin> <xMax> <yMax> (FT_Get_Color_Glyph_ClipBox at
 *                                       FT_Set_Pixel_Sizes (upem): the
 *                                       box in 26.6, 64 per font unit)
 *   end
 *
 * Usage: ft-dump --sbit FILE
 *
 * The strikes and embedded bitmaps (ADR 0023). For each face, `face`
 * and `error` as above, then:
 *
 *   strike <i> <height> <width> <size> <x_ppem> <y_ppem>
 *                                       (available_sizes)
 *   then for each strike, after FT_Select_Size (i):
 *   size <i>
 *   glyph <g> <error> <left> <top> <width> <rows> <pitch> <pixel_mode>
 *         <hori_advance> <vert_bearing_x> <vert_bearing_y> <vert_advance>
 *                                       (FT_Load_Glyph with FT_LOAD_COLOR,
 *                                       the slot's bitmap fields and metrics
 *                                       in 26.6; only the error when it
 *                                       fails, -1 when the glyph is not a
 *                                       bitmap; every glyph)
 *   <row as hex, pitch bytes>           (one line per row, top first)
 *   end
 *
 * Usage: ft-dump --otvalid FILE
 *
 * Validates GDEF, GSUB and GPOS with FT_OpenType_Validate (ADR 0020).
 * Each table present in a face is validated on its own; GDEF is
 * validated together with those of GSUB and GPOS that passed, because
 * FreeType reads their lookup flags to decide whether a version 1.0 GDEF
 * has MarkAttachClassDef. For each face, `face` and `error` as above,
 * then:
 *
 *   otvalid <tag> <error>               (one line per table present, in
 *                                       the order GSUB GPOS GDEF; 0 when
 *                                       FreeType accepts it)
 *   end
 *
 * Exit status: 0 on success, 1 when FreeType cannot be initialized or
 * the file cannot be opened at all, 2 on wrong usage.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_TRUETYPE_TABLES_H
#include FT_ADVANCES_H
#include FT_TRIGONOMETRY_H
#include FT_OUTLINE_H
#include FT_GLYPH_H
#include FT_STROKER_H
#include FT_MULTIPLE_MASTERS_H
#include FT_SFNT_NAMES_H
#include FT_CID_H
#include FT_TYPE1_TABLES_H
#include FT_PARAMETER_TAGS_H
#include FT_OPENTYPE_VALIDATE_H
#include FT_COLOR_H
#include <freetype/internal/cffotypes.h>
#include <freetype/internal/services/svcfftl.h>

static FT_UInt reference_named;
static int reference_has_named;
static const char *reference_variations;

static int reference_options(int *argc, char **argv)
{
  int read, write = 2;
  for (read = 2; read < *argc; read++) {
    const char *arg = argv[read];
    if (strncmp(arg, "--named-instance=", 17) == 0) {
      char *end;
      unsigned long value = strtoul(arg + 17, &end, 10);
      if (!arg[17] || *end || value >= 0xFFFF) return 0;
      reference_named = (FT_UInt)value + 1;
      reference_has_named = 1;
    } else if (strncmp(arg, "--variations=", 13) == 0) {
      reference_variations = arg + 13;
      if (!*reference_variations) return 0;
    } else argv[write++] = argv[read];
  }
  *argc = write;
  return 1;
}

static int contour_open;

static void end_contour(void)
{
  if (contour_open)
    printf("Z\n");
  contour_open = 0;
}

static int move_to(const FT_Vector *to, void *user)
{
  (void)user;
  end_contour();
  contour_open = 1;
  printf("M %ld %ld\n", (long)to->x, (long)to->y);
  return 0;
}

static int line_to(const FT_Vector *to, void *user)
{
  (void)user;
  printf("L %ld %ld\n", (long)to->x, (long)to->y);
  return 0;
}

static int conic_to(const FT_Vector *c, const FT_Vector *to, void *user)
{
  (void)user;
  printf("Q %ld %ld %ld %ld\n", (long)c->x, (long)c->y, (long)to->x, (long)to->y);
  return 0;
}

static int cubic_to(const FT_Vector *c1, const FT_Vector *c2,
                    const FT_Vector *to, void *user)
{
  (void)user;
  printf("C %ld %ld %ld %ld %ld %ld\n", (long)c1->x, (long)c1->y,
         (long)c2->x, (long)c2->y, (long)to->x, (long)to->y);
  return 0;
}

static const FT_Outline_Funcs outline_funcs = {
  move_to, line_to, conic_to, cubic_to, 0, 0
};

/* One closed glyph outline, using the public stroker entry points (ADR 0076). */
static int dump_stroke(const char *size, const char *glyph_arg,
                       const char *radius_arg, const char *join_arg,
                       const char *miter_arg, const char *border_arg,
                       const char *path)
{
  FT_Library library;
  FT_Face face;
  FT_Stroker stroker;
  FT_Glyph glyph;
  FT_Error error;
  FT_Int32 flags = FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP;
  FT_UInt glyph_id = (FT_UInt)strtoul(glyph_arg, NULL, 10);
  FT_Fixed miter = (FT_Fixed)strtol(miter_arg, NULL, 10);
  FT_Stroker_LineJoin join = (FT_Stroker_LineJoin)strtoul(join_arg, NULL, 10);
  unsigned long border = strtoul(border_arg, NULL, 10);
  if (FT_Init_FreeType(&library)) return 1;
  error = FT_New_Face(library, path, 0, &face);
  if (error) { FT_Done_FreeType(library); return 1; }
  if (strcmp(size, "unscaled") == 0) flags |= FT_LOAD_NO_SCALE;
  else if (FT_Set_Pixel_Sizes(face, 0, (FT_UInt)strtoul(size, NULL, 10))) {
    FT_Done_Face(face); FT_Done_FreeType(library); return 1;
  }
  error = FT_Load_Glyph(face, glyph_id, flags);
  if (!error) error = FT_Get_Glyph(face->glyph, &glyph);
  if (!error) error = FT_Stroker_New(library, &stroker);
  if (error) { FT_Done_Face(face); FT_Done_FreeType(library); return 1; }
  FT_Stroker_Set(stroker, (FT_Fixed)strtol(radius_arg, NULL, 10),
                 FT_STROKER_LINECAP_BUTT, join, miter);
  if (border == 0) error = FT_Glyph_Stroke(&glyph, stroker, 1);
  else error = FT_Glyph_StrokeBorder(&glyph, stroker, border == 2, 1);
  printf("glyph %u\n", glyph_id);
  if (error) printf("error %d\n", (int)error);
  else {
    FT_OutlineGlyph outline = (FT_OutlineGlyph)glyph;
    contour_open = 0;
    error = FT_Outline_Decompose(&outline->outline, &outline_funcs, NULL);
    end_contour();
    if (error) printf("error %d\n", (int)error);
  }
  FT_Done_Glyph(glyph);
  FT_Stroker_Done(stroker);
  FT_Done_Face(face);
  FT_Done_FreeType(library);
  return error ? 1 : 0;
}

static void dump_outlines(FT_Face face, int unscaled, int ppem,
                          const FT_ULong *selected, size_t selected_count)
{
  FT_Int32 flags = FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP;
  FT_Long g;
  size_t index;

  if (unscaled)
    flags |= FT_LOAD_NO_SCALE;
  else if (FT_Set_Pixel_Sizes(face, 0, (FT_UInt)ppem) != 0) {
    printf("error size\n");
    return;
  }
  for (index = 0; index < (selected ? selected_count : (size_t)face->num_glyphs); index++) {
    FT_Error error;
    FT_BBox cbox;

    g = selected ? (FT_Long)selected[index] : (FT_Long)index;
    if (g < 0 || g >= face->num_glyphs) {
      printf("glyph %ld\nerror range\n", (long)g);
      continue;
    }

    printf("glyph %ld\n", (long)g);
    if (selected) {
      FT_Fixed advance;
      FT_Error advance_error = FT_Get_Advance(face, (FT_UInt)g, FT_LOAD_NO_SCALE, &advance);
      if (advance_error == 0)
        printf("advance %ld\n", (long)advance);
      else
        printf("advance_error %d\n", (int)advance_error);
    }
    error = FT_Load_Glyph(face, (FT_UInt)g, flags);
    if (error == 0 && face->glyph->format != FT_GLYPH_FORMAT_OUTLINE)
      error = -1;
    if (error != 0) {
      printf("error %d\n", (int)error);
      continue;
    }
    contour_open = 0;
    error = FT_Outline_Decompose(&face->glyph->outline, &outline_funcs, NULL);
    if (error != 0) {
      printf("error %d\n", (int)error);
      continue;
    }
    end_contour();
    FT_Outline_Get_CBox(&face->glyph->outline, &cbox);
    printf("overlap %d\n", (face->glyph->outline.flags & FT_OUTLINE_OVERLAP) ? 1 : 0);
    printf("cbox %ld %ld %ld %ld\n", (long)cbox.xMin, (long)cbox.yMin,
           (long)cbox.xMax, (long)cbox.yMax);
  }
}

static void dump_rasters(FT_Face face, int ppem, FT_Render_Mode mode, int from_bitmap)
{
  FT_Long g;

  if (FT_Set_Pixel_Sizes(face, 0, (FT_UInt)ppem) != 0) {
    printf("error size\n");
    return;
  }
  for (g = 0; g < face->num_glyphs; g++) {
    FT_Error error;
    FT_Bitmap *b;
    unsigned int r, c;

    printf("glyph %ld\n", (long)g);
    error = FT_Load_Glyph(face, (FT_UInt)g, FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP);
    if (error == 0 && from_bitmap)
      error = FT_Render_Glyph(face->glyph, FT_RENDER_MODE_NORMAL);
    if (error == 0)
      error = FT_Render_Glyph(face->glyph, mode);
    if (error != 0) {
      printf("error %d\n", (int)error);
      continue;
    }
    b = &face->glyph->bitmap;
    printf("bitmap %d %d %u %u\n", face->glyph->bitmap_left, face->glyph->bitmap_top,
           b->width, b->rows);
    for (r = 0; r < b->rows; r++) {
      const unsigned char *row = b->pitch < 0
          ? b->buffer - (long)(b->rows - 1 - r) * b->pitch
          : b->buffer + (long)r * b->pitch;
      unsigned int bytes = mode == FT_RENDER_MODE_MONO ? (b->width + 7) / 8 : b->width;
      for (c = 0; c < bytes; c++)
        printf("%02x", row[c]);
      if (b->width)
        printf("\n");
    }
  }
}

static void dump_color_glyphs(FT_Face face)
{
  FT_Long g;

  for (g = 0; g < face->num_glyphs; g++) {
    FT_LayerIterator it;
    FT_UInt lg, li;
    FT_Error error;
    FT_Bitmap *b;
    unsigned int r, c, bytes;

    it.p = NULL;
    if (!FT_Get_Color_Glyph_Layer(face, (FT_UInt)g, &lg, &li, &it))
      continue;
    printf("glyph %ld\n", (long)g);
    error = FT_Load_Glyph(face, (FT_UInt)g,
                          FT_LOAD_COLOR | FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP | FT_LOAD_NO_SVG);
    if (error == 0)
      error = FT_Render_Glyph(face->glyph, FT_RENDER_MODE_NORMAL);
    if (error != 0) {
      printf("error %d\n", (int)error);
      continue;
    }
    b = &face->glyph->bitmap;
    bytes = b->pixel_mode == FT_PIXEL_MODE_BGRA ? 4 : 1;
    printf("bitmap %d %d %u %u %s\n", face->glyph->bitmap_left, face->glyph->bitmap_top,
           b->width, b->rows, bytes == 4 ? "bgra" : "gray");
    if (!b->buffer)
      continue;
    for (r = 0; r < b->rows; r++) {
      const unsigned char *row = b->buffer + (long)r * b->pitch;
      for (c = 0; c < b->width * bytes; c++)
        printf("%02x", row[c]);
      if (b->width)
        printf("\n");
    }
  }
}

static void dump_color_rasters(FT_Face face, int ppem)
{
  FT_Palette_Data pd;
  unsigned int palettes[2], n = 1, k;

  if (FT_Set_Pixel_Sizes(face, 0, (FT_UInt)ppem) != 0) {
    printf("error size\n");
    return;
  }
  palettes[0] = 0;
  if (FT_Palette_Data_Get(face, &pd) == 0 && pd.num_palettes > 1)
    palettes[n++] = (unsigned int)pd.num_palettes - 1;
  for (k = 0; k < n; k++) {
    FT_Error error;

    printf("palette %u\n", palettes[k]);
    if (FT_Palette_Data_Get(face, &pd) == 0 && pd.num_palettes > 0) {
      error = FT_Palette_Select(face, (FT_UShort)palettes[k], NULL);
      if (error != 0) {
        printf("palette_error %d\n", (int)error);
        continue;
      }
    }
    dump_color_glyphs(face);
  }
}

/* The pinned driver can free an invalid file-stream frame when closing
 * named instances of variable color fonts without a gvar table.
 * A memory face keeps all table frames in the caller's input buffer. */
static FT_Byte *read_font(const char *path, FT_Long *size)
{
  FILE *file = fopen(path, "rb");
  FT_Byte *bytes = NULL;
  if (!file)
    return NULL;
  if (fseek(file, 0, SEEK_END) == 0 && (*size = ftell(file)) > 0 &&
      fseek(file, 0, SEEK_SET) == 0) {
    bytes = (FT_Byte *)malloc((size_t)*size);
    if (bytes && fread(bytes, 1, (size_t)*size, file) != (size_t)*size) {
      free(bytes);
      bytes = NULL;
    }
  }
  fclose(file);
  return bytes;
}

/* Opens face `index` with the random seed 0 (ADR 0021). */
static FT_Error open_seeded(FT_Library library, const char *path, const FT_Byte *bytes,
                            FT_Long size, FT_Long index, FT_Face *face)
{
  FT_Int32 seed = 0;
  FT_Parameter param;
  FT_Open_Args args;

  param.tag = FT_PARAM_TAG_RANDOM_SEED;
  param.data = &seed;
  memset(&args, 0, sizeof args);
  args.flags = (bytes ? FT_OPEN_MEMORY : FT_OPEN_PATHNAME) | FT_OPEN_PARAMS;
  args.pathname = (char *)path;
  args.memory_base = bytes;
  args.memory_size = size;
  args.num_params = 1;
  args.params = &param;
  return FT_Open_Face(library, &args, index, face);
}

static void print_str(const char *s)
{
  if (!s) {
    printf(" -");
    return;
  }
  printf(" ");
  if (!*s)
    printf("\"\"");
  for (; *s; s++)
    printf("%02x", (unsigned char)*s);
}

static void dump_cid(FT_Face face)
{
  FT_Bool is_cid = 0;
  FT_Error error = FT_Get_CID_Is_Internally_CID_Keyed(face, &is_cid);
  const char *registry = NULL, *ordering = NULL;
  FT_Int supplement = 0;
  FT_Long g;

  printf("is_cid %d %d\n", (int)error, is_cid ? 1 : 0);
  error = FT_Get_CID_Registry_Ordering_Supplement(face, &registry, &ordering, &supplement);
  printf("ros %d", (int)error);
  if (!error) {
    print_str(registry);
    print_str(ordering);
    printf(" %d", (int)supplement);
  }
  printf("\n");
  for (g = 0; g <= face->num_glyphs; g++) {
    FT_UInt cid = 0;
    error = FT_Get_CID_From_Glyph_Index(face, (FT_UInt)g, &cid);
    printf("cid %ld %d %u\n", (long)g, (int)error, error ? 0U : cid);
  }
}

static void dump_ps_info(FT_Face face)
{
  PS_FontInfoRec info;
  FT_Error error;

  printf("has_ps_glyph_names %d\n", FT_Has_PS_Glyph_Names(face) ? 1 : 0);
  memset(&info, 0, sizeof info);
  error = FT_Get_PS_Font_Info(face, &info);
  printf("ps_font_info %d", (int)error);
  if (!error) {
    print_str(info.version);
    print_str(info.notice);
    print_str(info.full_name);
    print_str(info.family_name);
    print_str(info.weight);
    printf(" %ld %d %d %u", (long)info.italic_angle, info.is_fixed_pitch ? 1 : 0,
           (int)info.underline_position, (unsigned)info.underline_thickness);
  }
  printf("\n");
  printf("fs_type_flags %u\n", (unsigned)FT_Get_FSType_Flags(face));
}


/* ADR 0023: the raw COLR paint tree, in pre-order, one token list. */
static int paint_budget;

static void paint_stops(FT_Face face, FT_ColorLine *line)
{
  FT_ColorStop stop;
  printf(" extend %d", (int)line->extend);
  while (FT_Get_Colorline_Stops(face, &stop, &line->color_stop_iterator))
    printf(" stop %ld %u %d", (long)stop.stop_offset,
           (unsigned)stop.color.palette_index, (int)stop.color.alpha);
  printf(" .");
}

static void paint_tree(FT_Face face, FT_OpaquePaint op, int depth)
{
  FT_COLR_Paint p;
  if (depth > 64 || paint_budget-- <= 0) {
    printf(" ...");
    return;
  }
  if (!FT_Get_Paint(face, op, &p)) {
    printf(" error");
    return;
  }
  switch (p.format) {
  case FT_COLR_PAINTFORMAT_COLR_LAYERS: {
    FT_OpaquePaint layer = {NULL, 0};
    printf(" layers");
    while (FT_Get_Paint_Layers(face, &p.u.colr_layers.layer_iterator, &layer))
      paint_tree(face, layer, depth + 1);
    printf(" .");
    break;
  }
  case FT_COLR_PAINTFORMAT_SOLID:
    printf(" solid %u %d", (unsigned)p.u.solid.color.palette_index, (int)p.u.solid.color.alpha);
    break;
  case FT_COLR_PAINTFORMAT_LINEAR_GRADIENT:
    printf(" linear %ld %ld %ld %ld %ld %ld", (long)p.u.linear_gradient.p0.x,
           (long)p.u.linear_gradient.p0.y, (long)p.u.linear_gradient.p1.x,
           (long)p.u.linear_gradient.p1.y, (long)p.u.linear_gradient.p2.x,
           (long)p.u.linear_gradient.p2.y);
    paint_stops(face, &p.u.linear_gradient.colorline);
    break;
  case FT_COLR_PAINTFORMAT_RADIAL_GRADIENT:
    printf(" radial %ld %ld %ld %ld %ld %ld", (long)p.u.radial_gradient.c0.x,
           (long)p.u.radial_gradient.c0.y, (long)p.u.radial_gradient.r0,
           (long)p.u.radial_gradient.c1.x, (long)p.u.radial_gradient.c1.y,
           (long)p.u.radial_gradient.r1);
    paint_stops(face, &p.u.radial_gradient.colorline);
    break;
  case FT_COLR_PAINTFORMAT_SWEEP_GRADIENT:
    printf(" sweep %ld %ld %ld %ld", (long)p.u.sweep_gradient.center.x,
           (long)p.u.sweep_gradient.center.y, (long)p.u.sweep_gradient.start_angle,
           (long)p.u.sweep_gradient.end_angle);
    paint_stops(face, &p.u.sweep_gradient.colorline);
    break;
  case FT_COLR_PAINTFORMAT_GLYPH:
    printf(" glyph %u", (unsigned)p.u.glyph.glyphID);
    paint_tree(face, p.u.glyph.paint, depth + 1);
    break;
  case FT_COLR_PAINTFORMAT_COLR_GLYPH:
    printf(" colr_glyph %u", (unsigned)p.u.colr_glyph.glyphID);
    break;
  case FT_COLR_PAINTFORMAT_TRANSFORM:
    printf(" transform %ld %ld %ld %ld %ld %ld", (long)p.u.transform.affine.xx,
           (long)p.u.transform.affine.yx, (long)p.u.transform.affine.xy,
           (long)p.u.transform.affine.yy, (long)p.u.transform.affine.dx,
           (long)p.u.transform.affine.dy);
    paint_tree(face, p.u.transform.paint, depth + 1);
    break;
  case FT_COLR_PAINTFORMAT_TRANSLATE:
    printf(" translate %ld %ld", (long)p.u.translate.dx, (long)p.u.translate.dy);
    paint_tree(face, p.u.translate.paint, depth + 1);
    break;
  case FT_COLR_PAINTFORMAT_SCALE:
    printf(" scale %ld %ld %ld %ld", (long)p.u.scale.scale_x, (long)p.u.scale.scale_y,
           (long)p.u.scale.center_x, (long)p.u.scale.center_y);
    paint_tree(face, p.u.scale.paint, depth + 1);
    break;
  case FT_COLR_PAINTFORMAT_ROTATE:
    printf(" rotate %ld %ld %ld", (long)p.u.rotate.angle, (long)p.u.rotate.center_x,
           (long)p.u.rotate.center_y);
    paint_tree(face, p.u.rotate.paint, depth + 1);
    break;
  case FT_COLR_PAINTFORMAT_SKEW:
    printf(" skew %ld %ld %ld %ld", (long)p.u.skew.x_skew_angle, (long)p.u.skew.y_skew_angle,
           (long)p.u.skew.center_x, (long)p.u.skew.center_y);
    paint_tree(face, p.u.skew.paint, depth + 1);
    break;
  case FT_COLR_PAINTFORMAT_COMPOSITE:
    printf(" composite %d", (int)p.u.composite.composite_mode);
    paint_tree(face, p.u.composite.source_paint, depth + 1);
    paint_tree(face, p.u.composite.backdrop_paint, depth + 1);
    break;
  default:
    printf(" format %d", (int)p.format);
    break;
  }
}

static void dump_colr(FT_Face face)
{
  FT_Palette_Data pd;
  FT_Long g;
  if (FT_Palette_Data_Get(face, &pd) == 0) {
    FT_UShort i, c;
    printf("cpal %u %u\n", (unsigned)pd.num_palettes, (unsigned)pd.num_palette_entries);
    for (i = 0; i < pd.num_palettes; i++) {
      FT_Color *colors = NULL;
      printf("palette %u %u %u ", (unsigned)i,
             pd.palette_name_ids ? (unsigned)pd.palette_name_ids[i] : 0xFFFFu,
             pd.palette_flags ? (unsigned)pd.palette_flags[i] : 0u);
      if (FT_Palette_Select(face, i, &colors) != 0 || !colors || !pd.num_palette_entries)
        printf("-");
      else
        for (c = 0; c < pd.num_palette_entries; c++)
          printf("%s%02x%02x%02x%02x", c ? "," : "", colors[c].red, colors[c].green,
                 colors[c].blue, colors[c].alpha);
      printf("\n");
    }
    for (i = 0; i < pd.num_palette_entries; i++)
      printf("color_name %u %u\n", (unsigned)i,
             pd.palette_entry_name_ids ? (unsigned)pd.palette_entry_name_ids[i] : 0xFFFFu);
  }
  FT_Set_Pixel_Sizes(face, face->units_per_EM, face->units_per_EM);
  for (g = 0; g < face->num_glyphs; g++) {
    FT_LayerIterator it;
    FT_UInt lg, li;
    int first = 1;
    FT_OpaquePaint op = {NULL, 0};
    FT_ClipBox box;
    it.p = NULL;
    while (FT_Get_Color_Glyph_Layer(face, (FT_UInt)g, &lg, &li, &it)) {
      if (first) printf("layers %ld ", (long)g);
      printf("%s%u:%u", first ? "" : ",", lg, li);
      first = 0;
    }
    if (!first) printf("\n");
    if (FT_Get_Color_Glyph_Paint(face, (FT_UInt)g, FT_COLOR_NO_ROOT_TRANSFORM, &op)) {
      printf("paint %ld", (long)g);
      paint_budget = 100000;
      paint_tree(face, op, 0);
      printf("\n");
    }
    if (FT_Get_Color_Glyph_ClipBox(face, (FT_UInt)g, &box))
      printf("clip %ld %ld %ld %ld %ld\n", (long)g, (long)box.bottom_left.x,
             (long)box.bottom_left.y, (long)box.top_right.x, (long)box.top_right.y);
  }
}

static void dump_sbit(FT_Face face)
{
  FT_Int i;
  FT_Long g;
  for (i = 0; i < face->num_fixed_sizes; i++) {
    FT_Bitmap_Size *b = &face->available_sizes[i];
    printf("strike %d %d %d %ld %ld %ld\n", i, (int)b->height, (int)b->width, (long)b->size,
           (long)b->x_ppem, (long)b->y_ppem);
  }
  for (i = 0; i < face->num_fixed_sizes; i++) {
    if (FT_Select_Size(face, i) != 0)
      continue;
    printf("size %d\n", i);
    for (g = 0; g < face->num_glyphs; g++) {
      FT_GlyphSlot s = face->glyph;
      FT_Error e = FT_Load_Glyph(face, (FT_UInt)g, FT_LOAD_COLOR);
      unsigned r, k;
      if (e != 0 || s->format != FT_GLYPH_FORMAT_BITMAP) {
        printf("glyph %ld %d\n", (long)g, e != 0 ? (int)e : -1);
        continue;
      }
      printf("glyph %ld 0 %d %d %u %u %d %d %ld %ld %ld %ld\n", (long)g, s->bitmap_left,
             s->bitmap_top, s->bitmap.width, s->bitmap.rows, s->bitmap.pitch,
             (int)s->bitmap.pixel_mode, (long)s->metrics.horiAdvance,
             (long)s->metrics.vertBearingX, (long)s->metrics.vertBearingY,
             (long)s->metrics.vertAdvance);
      for (r = 0; r < s->bitmap.rows && s->bitmap.buffer; r++) {
        const unsigned char *row = s->bitmap.buffer + (long)r * s->bitmap.pitch;
        for (k = 0; k < (unsigned)abs(s->bitmap.pitch); k++)
          printf("%02x", row[k]);
        printf("\n");
      }
    }
  }
}

static int set_reference_location(FT_Library library, FT_Face face, const char *variations);

static int faces(const char *path, void (*dump)(FT_Face))
{
  FT_Library library;
  FT_Face face;
  FT_Long num_faces, i;

  if (FT_Init_FreeType(&library) != 0)
    return 1;
  if (FT_New_Face(library, path, -1, &face) != 0) {
    FT_Done_FreeType(library);
    return 1;
  }
  num_faces = face->num_faces;
  FT_Done_Face(face);

  for (i = 0; i < num_faces; i++) {
    FT_Error error;

    printf("face %ld\n", (long)i);
    error = FT_New_Face(library, path, i, &face);
    if (error != 0) {
      printf("error %d\nend\n", (int)error);
      continue;
    }
    if (set_reference_location(library, face, reference_variations) != 0) {
      FT_Done_Face(face);
      FT_Done_FreeType(library);
      return 2;
    }
    dump(face);
    printf("end\n");
    FT_Done_Face(face);
  }
  FT_Done_FreeType(library);
  return 0;
}

static int set_design_coordinates(FT_Library library, FT_Face face, const char *text)
{
  FT_MM_Var *mm = NULL;
  FT_Fixed *coordinates;
  char *copy, *token;
  FT_UInt i;
  FT_Error error;

  if (!text || FT_Get_MM_Var(face, &mm) != 0)
    return text ? 2 : 0;
  coordinates = (FT_Fixed *)malloc((size_t)mm->num_axis * sizeof(*coordinates));
  copy = (char *)malloc(strlen(text) + 1);
  if (!coordinates || !copy) {
    free(coordinates); free(copy); FT_Done_MM_Var(library, mm); return 1;
  }
  strcpy(copy, text);
  if (FT_Get_Var_Design_Coordinates(face, mm->num_axis, coordinates) != 0)
    for (i = 0; i < mm->num_axis; i++)
      coordinates[i] = mm->axis[i].def;
  token = strtok(copy, ",");
  if (!token) {
    free(coordinates); free(copy); FT_Done_MM_Var(library, mm); return 2;
  }
  while (token) {
    char *equal = strchr(token, '=');
    char *end = NULL;
    float value;
    FT_ULong tag;
    if (!equal || equal - token != 4) break;
    tag = ((FT_ULong)(unsigned char)token[0] << 24) |
          ((FT_ULong)(unsigned char)token[1] << 16) |
          ((FT_ULong)(unsigned char)token[2] << 8) |
          (FT_ULong)(unsigned char)token[3];
    value = strtof(equal + 1, &end);
    if (end == equal + 1 || *end || !isfinite(value) ||
        value < -32768.0f || value > 32767.0f) break;
    for (i = 0; i < mm->num_axis && mm->axis[i].tag != tag; i++) {}
    if (i == mm->num_axis) break;
    coordinates[i] = (FT_Fixed)(value * 65536.f);
    token = strtok(NULL, ",");
  }
  error = token ? 2 : FT_Set_Var_Design_Coordinates(face, mm->num_axis, coordinates);
  free(coordinates);
  free(copy);
  FT_Done_MM_Var(library, mm);
  return error ? 2 : 0;
}

static int set_reference_location(FT_Library library, FT_Face face, const char *variations)
{
  if (reference_has_named && FT_Set_Named_Instance(face, reference_named) != 0)
    return 2;
  return set_design_coordinates(library, face, variations);
}

/* A new face is required for each single query: the first query can
 * initialize HVAR/VVAR and change the result of later queries. */
static int dump_advance_queries(const char *path)
{
  FT_Library library;
  FT_Face face;
  FT_Long face_count, index, glyph;
  FT_Error error;
  int vertical;
  FT_Long size = 0;
  FT_Byte *bytes;

  if (FT_Init_FreeType(&library) != 0) return 1;
  bytes = read_font(path, &size);
  if (!bytes) { FT_Done_FreeType(library); return 1; }
  error = open_seeded(library, path, bytes, size, -1, &face);
  if (error != 0) {
    fprintf(stderr, "ft-dump: %s: FreeType error %d\n", path, (int)error);
    free(bytes); FT_Done_FreeType(library);
    return 1;
  }
  face_count = face->num_faces;
  FT_Done_Face(face);

  for (index = 0; index < face_count; index++) {
    FT_Long count;
    printf("face %ld\n", (long)index);
    error = open_seeded(library, path, bytes, size, index, &face);
    if (error != 0) {
      printf("error %d\nend\n", (int)error);
      continue;
    }
    count = face->num_glyphs;
    FT_Done_Face(face);
    printf("num_glyphs %ld\n", (long)count);
    if (count < 0 || (FT_Long)(FT_UInt)count != count) {
      free(bytes); FT_Done_FreeType(library);
      return 1;
    }
    for (vertical = 0; vertical <= 1; vertical++) {
      const char *direction = vertical ? "v" : "h";
      FT_Int32 flags = FT_LOAD_NO_SCALE | (vertical ? FT_LOAD_VERTICAL_LAYOUT : 0);
      FT_Fixed *values;
      for (glyph = 0; glyph < count; glyph++) {
        FT_Fixed value = 0;
        error = open_seeded(library, path, bytes, size, index, &face);
        if (error == 0) {
          if (set_reference_location(library, face, reference_variations) != 0) {
            FT_Done_Face(face);
            free(bytes); FT_Done_FreeType(library);
            return 2;
          }
          error = FT_Get_Advance(face, (FT_UInt)glyph, flags, &value);
          FT_Done_Face(face);
        }
        printf("advance %s single %ld %d %ld\n", direction, (long)glyph,
               (int)error, error ? 0L : (long)value);
      }
      if (count == 0) continue;
      values = (FT_Fixed *)calloc((size_t)count, sizeof(*values));
      if (!values) { free(bytes); FT_Done_FreeType(library); return 1; }
      error = open_seeded(library, path, bytes, size, index, &face);
      if (error == 0) {
        if (set_reference_location(library, face, reference_variations) != 0) {
          FT_Done_Face(face);
          free(values);
          free(bytes); FT_Done_FreeType(library);
          return 2;
        }
        error = FT_Get_Advances(face, 0, (FT_UInt)count, flags, values);
        FT_Done_Face(face);
      }
      for (glyph = 0; glyph < count; glyph++)
        printf("advance %s batch %ld %d %ld\n", direction, (long)glyph,
               (int)error, error ? 0L : (long)values[glyph]);
      free(values);
    }
    printf("end\n");
  }
  free(bytes); FT_Done_FreeType(library);
  return 0;
}

/* raster: 0 for --outline, 1 for --raster, 2 for --raster --color.
 * plan: sorted "face glyph" pairs for --sampled-outline, or NULL. */
static int outlines(const char *size, const char *path, int raster, const char *plan,
                    const char *variations)
{
  FT_Library library;
  FT_Face face;
  FT_Long num_faces, i;
  int unscaled = strcmp(size, "unscaled") == 0;
  int ppem = atoi(size);
  FT_ULong *selected = NULL;
  FT_Long *selected_faces = NULL;
  size_t selected_count = 0, selected_capacity = 0, cursor = 0, face_samples = 0;
  FILE *input = NULL;
  FT_Byte *bytes = NULL;
  FT_Long byte_count = 0;

  if ((raster && unscaled) || (!unscaled && ppem <= 0)) {
    fprintf(stderr, "ft-dump: bad size %s\n", size);
    return 2;
  }
  if (FT_Init_FreeType(&library) != 0)
    return 1;
  if (FT_New_Face(library, path, -1, &face) != 0) {
    FT_Done_FreeType(library);
    return 1;
  }
  num_faces = face->num_faces;
  FT_Done_Face(face);
  if (plan) {
    unsigned long face_number, glyph_number;
    int parsed;
    input = fopen(plan, "r");
    if (!input) {
      FT_Done_FreeType(library);
      return 1;
    }
    while ((parsed = fscanf(input, "%lu %lu", &face_number, &glyph_number)) == 2) {
      if (!selected_count || face_number != (unsigned long)selected_faces[selected_count - 1])
        face_samples = 0;
      face_samples++;
      if (face_number >= (unsigned long)num_faces || selected_count >= 128 * (size_t)num_faces ||
          face_samples > 128 ||
          (selected_count && (face_number < (unsigned long)selected_faces[selected_count - 1] ||
          (face_number == (unsigned long)selected_faces[selected_count - 1] && glyph_number <= selected[selected_count - 1])))) {
        fprintf(stderr, "ft-dump: invalid sample plan\n");
        fclose(input);
        free(selected);
        free(selected_faces);
        FT_Done_FreeType(library);
        return 2;
      }
      if (selected_count == selected_capacity) {
        size_t next = selected_capacity ? selected_capacity * 2 : 128;
        FT_ULong *new_selected = (FT_ULong *)realloc(selected, next * sizeof(*selected));
        FT_Long *new_faces;
        if (!new_selected) {
          fclose(input);
          free(selected);
          free(selected_faces);
          FT_Done_FreeType(library);
          return 1;
        }
        selected = new_selected;
        new_faces = (FT_Long *)realloc(selected_faces, next * sizeof(*selected_faces));
        if (!new_faces) {
          fclose(input);
          free(selected);
          free(selected_faces);
          FT_Done_FreeType(library);
          return 1;
        }
        selected_faces = new_faces;
        selected_capacity = next;
      }
      selected[selected_count] = (FT_ULong)glyph_number;
      selected_faces[selected_count] = (FT_Long)face_number;
      selected_count++;
    }
    if (parsed != EOF) {
      fprintf(stderr, "ft-dump: invalid sample plan\n");
      fclose(input);
      free(selected);
      free(selected_faces);
      FT_Done_FreeType(library);
      return 2;
    }
    fclose(input);
    if (!selected) {
      selected = (FT_ULong *)malloc(sizeof(*selected));
      if (!selected) {
        FT_Done_FreeType(library);
        return 1;
      }
    }
  }
  if (reference_has_named && !(bytes = read_font(path, &byte_count))) {
    free(selected);
    free(selected_faces);
    FT_Done_FreeType(library);
    return 1;
  }
  for (i = 0; i < num_faces; i++) {
    FT_Error error;
    size_t begin = cursor;

    while (cursor < selected_count && selected_faces[cursor] == i)
      cursor++;

    printf("face %ld\n", (long)i);
    error = open_seeded(library, path, bytes, byte_count, i, &face);
    if (error != 0) {
      printf("error %d\nend\n", (int)error);
      continue;
    }
    if (set_reference_location(library, face,
                               variations ? variations : reference_variations) != 0) {
      fprintf(stderr, "ft-dump: invalid variation location\n");
      FT_Done_Face(face);
      free(bytes);
      free(selected);
      free(selected_faces);
      FT_Done_FreeType(library);
      return 2;
    }
    printf("num_glyphs %ld\n", (long)face->num_glyphs);
    if (raster == 2)
      dump_color_rasters(face, ppem);
    else if (raster)
      dump_rasters(face, ppem, raster == 3 ? FT_RENDER_MODE_LCD :
                               raster == 4 ? FT_RENDER_MODE_LCD_V :
                               raster == 5 ? FT_RENDER_MODE_MONO :
                               raster >= 6 ? FT_RENDER_MODE_SDF : FT_RENDER_MODE_NORMAL,
                               raster == 7);
    else
      dump_outlines(face, unscaled, ppem, plan ? selected + begin : NULL, cursor - begin);
    printf("end\n");
    FT_Done_Face(face);
  }
  free(bytes);
  free(selected);
  free(selected_faces);
  FT_Done_FreeType(library);
  return 0;
}

static int calc(void)
{
  char op[32];
  long a, b, c;

  while (scanf("%31s", op) == 1) {
    if (strcmp(op, "mul_fix") == 0 && scanf("%ld %ld", &a, &b) == 2)
      printf("%ld\n", (long)FT_MulFix(a, b));
    else if (strcmp(op, "div_fix") == 0 && scanf("%ld %ld", &a, &b) == 2)
      printf("%ld\n", (long)FT_DivFix(a, b));
    else if (strcmp(op, "mul_div") == 0 && scanf("%ld %ld %ld", &a, &b, &c) == 3)
      printf("%ld\n", (long)FT_MulDiv(a, b, c));
    else if (strcmp(op, "length") == 0 && scanf("%ld %ld", &a, &b) == 2) {
      FT_Vector v;
      v.x = a;
      v.y = b;
      printf("%ld\n", (long)FT_Vector_Length(&v));
    } else if (strcmp(op, "atan2") == 0 && scanf("%ld %ld", &a, &b) == 2)
      printf("%ld\n", (long)FT_Atan2(a, b));
    else if (strcmp(op, "unit") == 0 && scanf("%ld", &a) == 1) {
      FT_Vector v;
      FT_Vector_Unit(&v, a);
      printf("%ld %ld\n", (long)v.x, (long)v.y);
    } else if (strcmp(op, "from_polar") == 0 && scanf("%ld %ld", &a, &b) == 2) {
      FT_Vector v;
      FT_Vector_From_Polar(&v, a, b);
      printf("%ld %ld\n", (long)v.x, (long)v.y);
    } else if (strcmp(op, "tan") == 0 && scanf("%ld", &a) == 1) {
      printf("%ld\n", (long)FT_Tan(a));
    } else if (strcmp(op, "angle_diff") == 0 && scanf("%ld %ld", &a, &b) == 2) {
      printf("%ld\n", (long)FT_Angle_Diff(a, b));
    } else {
      fprintf(stderr, "ft-dump: bad --calc input at %s\n", op);
      return 2;
    }
  }
  return 0;
}

static unsigned long u32(FT_ULong v) { return (unsigned long)(v & 0xFFFFFFFFUL); }

static void dump_head(const TT_Header *h)
{
  printf("head version %lu\n", u32((FT_ULong)h->Table_Version));
  printf("head font_revision %ld\n", (long)h->Font_Revision);
  printf("head checksum_adjustment %lu\n", u32((FT_ULong)h->CheckSum_Adjust));
  printf("head magic_number %lu\n", u32((FT_ULong)h->Magic_Number));
  printf("head flags %u\n", (unsigned)h->Flags);
  printf("head units_per_em %u\n", (unsigned)h->Units_Per_EM);
  printf("head created %lu %lu\n", u32(h->Created[0]), u32(h->Created[1]));
  printf("head modified %lu %lu\n", u32(h->Modified[0]), u32(h->Modified[1]));
  printf("head x_min %d\n", (int)h->xMin);
  printf("head y_min %d\n", (int)h->yMin);
  printf("head x_max %d\n", (int)h->xMax);
  printf("head y_max %d\n", (int)h->yMax);
  printf("head mac_style %u\n", (unsigned)h->Mac_Style);
  printf("head lowest_rec_ppem %u\n", (unsigned)h->Lowest_Rec_PPEM);
  printf("head font_direction_hint %d\n", (int)h->Font_Direction);
  printf("head index_to_loc_format %d\n", (int)h->Index_To_Loc_Format);
  printf("head glyph_data_format %d\n", (int)h->Glyph_Data_Format);
}

static void dump_hhea(const TT_HoriHeader *h)
{
  printf("hhea version %lu\n", u32((FT_ULong)h->Version));
  printf("hhea ascender %d\n", (int)h->Ascender);
  printf("hhea descender %d\n", (int)h->Descender);
  printf("hhea line_gap %d\n", (int)h->Line_Gap);
  printf("hhea advance_width_max %u\n", (unsigned)h->advance_Width_Max);
  printf("hhea min_left_side_bearing %d\n", (int)h->min_Left_Side_Bearing);
  printf("hhea min_right_side_bearing %d\n", (int)h->min_Right_Side_Bearing);
  printf("hhea x_max_extent %d\n", (int)h->xMax_Extent);
  printf("hhea caret_slope_rise %d\n", (int)h->caret_Slope_Rise);
  printf("hhea caret_slope_run %d\n", (int)h->caret_Slope_Run);
  printf("hhea caret_offset %d\n", (int)h->caret_Offset);
  printf("hhea metric_data_format %d\n", (int)h->metric_Data_Format);
  printf("hhea number_of_h_metrics %u\n", (unsigned)h->number_Of_HMetrics);
}

static void dump_maxp(const TT_MaxProfile *m)
{
  printf("maxp version %lu\n", u32((FT_ULong)m->version));
  printf("maxp num_glyphs %u\n", (unsigned)m->numGlyphs);
  if ((FT_ULong)m->version < 0x10000UL)
    return;
  printf("maxp max_points %u\n", (unsigned)m->maxPoints);
  printf("maxp max_contours %u\n", (unsigned)m->maxContours);
  printf("maxp max_composite_points %u\n", (unsigned)m->maxCompositePoints);
  printf("maxp max_composite_contours %u\n", (unsigned)m->maxCompositeContours);
  printf("maxp max_zones %u\n", (unsigned)m->maxZones);
  printf("maxp max_twilight_points %u\n", (unsigned)m->maxTwilightPoints);
  printf("maxp max_storage %u\n", (unsigned)m->maxStorage);
  printf("maxp max_function_defs %u\n", (unsigned)m->maxFunctionDefs);
  printf("maxp max_instruction_defs %u\n", (unsigned)m->maxInstructionDefs);
  printf("maxp max_stack_elements %u\n", (unsigned)m->maxStackElements);
  printf("maxp max_size_of_instructions %u\n", (unsigned)m->maxSizeOfInstructions);
  printf("maxp max_component_elements %u\n", (unsigned)m->maxComponentElements);
  printf("maxp max_component_depth %u\n", (unsigned)m->maxComponentDepth);
}

static void dump_charmaps(FT_Face face)
{
  FT_Int i;
  for (i = 0; i < face->num_charmaps; i++) {
    FT_CharMap cm = face->charmaps[i];
    FT_Long format = FT_Get_CMap_Format(cm);
    FT_ULong code;
    FT_UInt glyph;

    printf("cmap %u %u %ld\n", (unsigned)cm->platform_id, (unsigned)cm->encoding_id, (long)format);
    if (format == 14 || FT_Set_Charmap(face, cm) != 0)
      continue;
    code = FT_Get_First_Char(face, &glyph);
    while (glyph != 0) {
      printf("map %lu %u\n", u32(code), glyph);
      code = FT_Get_Next_Char(face, code, &glyph);
    }
  }
}

static void dump_advances(FT_Face face)
{
  FT_Long n = face->num_glyphs;
  FT_Long g;
  FT_Fixed *advances;

  if (n <= 0)
    return;
  advances = (FT_Fixed *)calloc((size_t)n, sizeof(FT_Fixed));
  if (!advances)
    return;
  if (FT_Get_Advances(face, 0, (FT_UInt)n, FT_LOAD_NO_SCALE, advances) == 0)
    for (g = 0; g < n; g++)
      printf("advance %ld %ld\n", (long)g, (long)advances[g]);
  free(advances);
}

static void print_hex(const unsigned char *s, unsigned long n)
{
  unsigned long i;
  if (n == 0) {
    printf(" -");
    return;
  }
  printf(" ");
  for (i = 0; i < n; i++)
    printf("%02x", s[i]);
}

static void dump_names_and_metrics(FT_Face face, int with_bearings)
{
  FT_Long n = face->num_glyphs, g;
  FT_UInt i, count;
  const char *ps;
  FT_Fixed *advances;

  printf("fs_type %u\n", (unsigned)FT_Get_FSType_Flags(face));
  ps = FT_Get_Postscript_Name(face);
  printf("ps_name");
  print_hex((const unsigned char *)(ps ? ps : ""), ps ? (unsigned long)strlen(ps) : 0);
  printf("\n");
  printf("face_metrics %d %d %d %d %d %d %d %ld %ld %ld %ld\n", (int)face->ascender,
         (int)face->descender, (int)face->height, (int)face->max_advance_width,
         (int)face->max_advance_height, (int)face->underline_position,
         (int)face->underline_thickness, (long)face->bbox.xMin, (long)face->bbox.yMin,
         (long)face->bbox.xMax, (long)face->bbox.yMax);
  count = FT_Get_Sfnt_Name_Count(face);
  for (i = 0; i < count; i++) {
    FT_SfntName name;
    if (FT_Get_Sfnt_Name(face, i, &name) != 0)
      continue;
    printf("sfnt_name %u %u %u %u %u", i, (unsigned)name.platform_id,
           (unsigned)name.encoding_id, (unsigned)name.language_id, (unsigned)name.name_id);
    print_hex(name.string, name.string_len);
    printf("\n");
  }
  if (n > 0) {
    advances = (FT_Fixed *)calloc((size_t)n, sizeof(FT_Fixed));
    if (advances && FT_Get_Advances(face, 0, (FT_UInt)n,
                                    FT_LOAD_NO_SCALE | FT_LOAD_VERTICAL_LAYOUT,
                                    advances) == 0)
      for (g = 0; g < n; g++)
        printf("v_advance %ld %ld\n", (long)g, (long)advances[g]);
    free(advances);
  }
  /* The glyph slot bearing is recomputed from the outline's CBox.  Read the
   * metric tables instead: this is the input of tt_face_get_metrics. */
  if (with_bearings) {
    TT_HoriHeader *h = (TT_HoriHeader *)FT_Get_Sfnt_Table(face, FT_SFNT_HHEA);
    TT_VertHeader *v = (TT_VertHeader *)FT_Get_Sfnt_Table(face, FT_SFNT_VHEA);
    FT_ULong hlen = 0, vlen = 0;
    FT_Byte *hmtx = NULL, *vmtx = NULL;
    if (h && FT_Load_Sfnt_Table(face, FT_MAKE_TAG('h','m','t','x'), 0, NULL, &hlen) == 0 &&
        (hmtx = (FT_Byte *)malloc((size_t)hlen)) != NULL &&
        FT_Load_Sfnt_Table(face, FT_MAKE_TAG('h','m','t','x'), 0, hmtx, &hlen) != 0) {
      free(hmtx);
      hmtx = NULL;
    }
    if (v && FT_Load_Sfnt_Table(face, FT_MAKE_TAG('v','m','t','x'), 0, NULL, &vlen) == 0 &&
        (vmtx = (FT_Byte *)malloc((size_t)vlen)) != NULL &&
        FT_Load_Sfnt_Table(face, FT_MAKE_TAG('v','m','t','x'), 0, vmtx, &vlen) != 0) {
      free(vmtx);
      vmtx = NULL;
    }
    for (g = 0; g < n && hmtx; g++) {
      FT_ULong hm = h->number_Of_HMetrics;
      FT_ULong hoff = (FT_ULong)g < hm ? (FT_ULong)g * 4 + 2 : hm * 4 + ((FT_ULong)g - hm) * 2;
      if (hoff + 2 <= hlen) {
        FT_Short lsb = (FT_Short)((hmtx[hoff] << 8) | hmtx[hoff + 1]);
        if (vmtx) {
          FT_ULong vm = v->number_Of_VMetrics;
          FT_ULong voff = (FT_ULong)g < vm ? (FT_ULong)g * 4 + 2 : vm * 4 + ((FT_ULong)g - vm) * 2;
          if (voff + 2 <= vlen) {
            FT_Short tsb = (FT_Short)((vmtx[voff] << 8) | vmtx[voff + 1]);
            printf("bearing %ld %d %d\n", (long)g, (int)lsb, (int)tsb);
            continue;
          }
        }
        printf("bearing %ld %d -\n", (long)g, (int)lsb);
      }
    }
    free(hmtx);
    free(vmtx);
  }
  for (g = 0; g < n; g++) {
    char buf[256];
    FT_Error error = FT_Get_Glyph_Name(face, (FT_UInt)g, buf, sizeof buf);
    unsigned long len = error ? 0 : (unsigned long)strlen(buf);
    printf("glyph_name %ld %d", (long)g, (int)error);
    print_hex((const unsigned char *)buf, len);
    printf("\n");
    if (!error && len)
      printf("name_index %ld %u\n", (long)g, FT_Get_Name_Index(face, buf));
  }
}

/* Test-only access to FreeType's CFF load service.  The public Type 1
 * private-DICT query is absent for the CFF driver; this uses the same
 * load_private_dict callback as cff_slot_load, once for each Font DICT. */
static void dump_cff2_private(FT_Face face)
{
  FT_ULong table_len = 0;
  CFF_Face cff_face;
  CFF_Font cff;
  const FT_Service_CFFLoadRec *service;
  FT_Fixed *coords = NULL;
  FT_UInt axes, count, fd;

  if (FT_Load_Sfnt_Table(face, FT_MAKE_TAG('C','F','F','2'), 0, NULL, &table_len) != 0)
    return;
  cff_face = (CFF_Face)face;
  cff = (CFF_Font)cff_face->extra.data;
  if (!cff || !cff->cff2 || !cff->cffload)
    return;
  service = (const FT_Service_CFFLoadRec *)cff->cffload;
  if (!service->load_private_dict)
    return;
  axes = cff->vstore.axisCount;
  if (axes) {
    coords = (FT_Fixed *)calloc(axes, sizeof(*coords));
    if (!coords || FT_Get_Var_Blend_Coordinates(face, axes, coords) != 0) {
      free(coords);
      printf("private_error coords\n");
      return;
    }
  }
  count = cff->num_subfonts ? cff->num_subfonts : 1;
  for (fd = 0; fd < count; fd++) {
    CFF_SubFont sub = cff->num_subfonts ? cff->subfonts[fd] : &cff->top_font;
    CFF_Private priv = &sub->private_dict;
    FT_Error error = service->load_private_dict(cff, sub, axes, coords);
    if (error) {
      printf("private_error %u %d\n", (unsigned)fd, (int)error);
      continue;
    }
    printf("private %u %u %ld %ld %ld %ld %ld %ld %ld %ld %ld %u %ld %d %d %lu\n",
           (unsigned)fd, (unsigned)priv->vsindex,
           (long)priv->standard_width, (long)priv->standard_height,
           (long)priv->blue_scale, (long)priv->blue_shift,
           (long)priv->blue_fuzz, (long)priv->expansion_factor,
           (long)priv->initial_random_seed, (long)priv->default_width,
           (long)priv->nominal_width, (unsigned)priv->force_bold,
           (long)priv->force_bold_threshold, (int)priv->lenIV,
           (int)priv->language_group, (unsigned long)priv->local_subrs_offset);
#define DUMP_ARRAY(name, count, values) do {                                  \
      FT_UInt j;                                                               \
      printf("private_array %u %s %u", (unsigned)fd, name, (unsigned)(count)); \
      for (j = 0; j < (FT_UInt)(count); j++) printf(" %ld", (long)(values)[j]); \
      printf("\n");                                                         \
    } while (0)
    DUMP_ARRAY("blue_values", priv->num_blue_values, priv->blue_values);
    DUMP_ARRAY("other_blues", priv->num_other_blues, priv->other_blues);
    DUMP_ARRAY("family_blues", priv->num_family_blues, priv->family_blues);
    DUMP_ARRAY("family_other_blues", priv->num_family_other_blues, priv->family_other_blues);
    DUMP_ARRAY("stem_snap_h", priv->num_snap_widths, priv->snap_widths);
    DUMP_ARRAY("stem_snap_v", priv->num_snap_heights, priv->snap_heights);
#undef DUMP_ARRAY
  }
  free(coords);
}

/* ADR 0027 task 2: public FreeType variation queries and normalization. */
static int dump_var_reference(int argc, char **argv)
{
  FT_Library library;
  FT_Face face;
  FT_Long face_count, index;
  const char *variations = NULL;
  FT_UInt named = 0;
  int has_named = 0, normalize = strcmp(argv[1], "--var-normalize") == 0;
  int arg;
  if (argc < 3) return 2;
  for (arg = 2; arg < argc - 1; arg++) {
    if (strncmp(argv[arg], "--named-instance=", 17) == 0) {
      char *end;
      unsigned long parsed = strtoul(argv[arg] + 17, &end, 10);
      if (!argv[arg][17] || *end || parsed >= 0xFFFF) return 2;
      named = (FT_UInt)parsed + 1;
      has_named = 1;
    } else if (strncmp(argv[arg], "--variations=", 13) == 0) {
      variations = argv[arg] + 13;
      if (!*variations) return 2;
    } else return 2;
  }
  if (FT_Init_FreeType(&library) != 0) return 1;
  if (FT_New_Face(library, argv[argc - 1], -1, &face) != 0) {
    FT_Done_FreeType(library);
    return 1;
  }
  face_count = face->num_faces;
  FT_Done_Face(face);
  for (index = 0; index < face_count; index++) {
    FT_MM_Var *mm = NULL;
    FT_Error error = FT_New_Face(library, argv[argc - 1], index, &face);
    FT_UInt axis;
    printf("face %ld\n", (long)index);
    if (error) {
      printf("error %d\nend\n", (int)error);
      continue;
    }
    if (has_named && FT_Set_Named_Instance(face, named) != 0) {
      FT_Done_Face(face);
      FT_Done_FreeType(library);
      return 2;
    }
    if (set_design_coordinates(library, face, variations) != 0) {
      FT_Done_Face(face);
      FT_Done_FreeType(library);
      return 2;
    }
    error = FT_Get_MM_Var(face, &mm);
    if (error) {
      printf("axis_count 0\nnamed_count 0\ndefault_named 0\ncoords_blend\ncoords_design\nps_name");
      print_hex((const unsigned char *)FT_Get_Postscript_Name(face),
                FT_Get_Postscript_Name(face) ? strlen(FT_Get_Postscript_Name(face)) : 0);
      printf("\nend\n");
      FT_Done_Face(face);
      continue;
    }
    printf("axis_count %u\n", mm->num_axis);
    if (!normalize) {
      for (axis = 0; axis < mm->num_axis; axis++) {
        FT_UInt flags = 0;
        FT_Var_Axis *a = &mm->axis[axis];
        FT_Get_Var_Axis_Flags(mm, axis, &flags);
        printf("axis %u %08lx %u %u %ld %ld %ld\n", axis,
               (unsigned long)a->tag, (unsigned)a->strid, (unsigned)flags,
               (long)a->minimum, (long)a->def, (long)a->maximum);
      }
      printf("named_count %u\n", mm->num_namedstyles);
      for (axis = 0; axis < mm->num_namedstyles; axis++) {
        FT_UInt a;
        FT_Var_Named_Style *style = &mm->namedstyle[axis];
        printf("named %u %u %u", axis, (unsigned)style->strid,
               (unsigned)style->psid);
        for (a = 0; a < mm->num_axis; a++)
          printf(" %ld", (long)style->coords[a]);
        printf("\n");
      }
      {
        FT_UInt def = 0;
        FT_Error def_error = FT_Get_Default_Named_Instance(face, &def);
        printf("default_named %u %d\n", def, (int)def_error);
      }
      {
        FT_Fixed *coords = (FT_Fixed *)calloc(mm->num_axis, sizeof(*coords));
        if (!coords) { FT_Done_MM_Var(library, mm); FT_Done_Face(face); FT_Done_FreeType(library); return 1; }
        error = FT_Get_Var_Blend_Coordinates(face, mm->num_axis, coords);
        printf("coords_blend");
        for (axis = 0; !error && axis < mm->num_axis; axis++)
          printf(" %ld", (long)coords[axis]);
        printf("\n");
        error = FT_Get_Var_Design_Coordinates(face, mm->num_axis, coords);
        printf("coords_design");
        for (axis = 0; !error && axis < mm->num_axis; axis++)
          printf(" %ld", (long)coords[axis]);
        printf("\n");
        free(coords);
      }
      {
        const char *name = FT_Get_Postscript_Name(face);
        printf("ps_name");
        print_hex((const unsigned char *)(name ? name : ""), name ? strlen(name) : 0);
        printf("\n");
      }
    } else {
      FT_Fixed *design = (FT_Fixed *)calloc(mm->num_axis, sizeof(*design));
      FT_Fixed *blend = (FT_Fixed *)calloc(mm->num_axis, sizeof(*blend));
      if (!design || !blend) {
        free(design); free(blend); FT_Done_MM_Var(library, mm);
        FT_Done_Face(face); FT_Done_FreeType(library); return 1;
      }
      for (axis = 0; axis < mm->num_axis; axis++) {
        FT_UInt other, sample;
        long long span = (long long)mm->axis[axis].maximum - mm->axis[axis].minimum;
        for (other = 0; other < mm->num_axis; other++)
          design[other] = mm->axis[other].def;
        for (sample = 0; sample <= 256; sample++) {
          design[axis] = mm->axis[axis].minimum + (FT_Fixed)(span * sample / 256);
          FT_Set_Var_Design_Coordinates(face, mm->num_axis, design);
          FT_Get_Var_Blend_Coordinates(face, mm->num_axis, blend);
          printf("sample %u %u %ld %ld\n", axis, sample,
                 (long)design[axis], (long)blend[axis]);
        }
        {
          const char *labels[] = {"outside_min", "outside_max", "half_min", "half_max"};
          FT_Fixed values[] = {
            mm->axis[axis].minimum - (FT_Fixed)span,
            mm->axis[axis].maximum + (FT_Fixed)span,
            mm->axis[axis].def -
              (FT_Fixed)(((long long)mm->axis[axis].def - mm->axis[axis].minimum) / 32768),
            mm->axis[axis].def +
              (FT_Fixed)(((long long)mm->axis[axis].maximum - mm->axis[axis].def) / 32768)
          };
          for (sample = 0; sample < 4; sample++) {
            design[axis] = values[sample];
            FT_Set_Var_Design_Coordinates(face, mm->num_axis, design);
            FT_Get_Var_Blend_Coordinates(face, mm->num_axis, blend);
            printf("sample %u %s %ld %ld\n", axis, labels[sample],
                   (long)design[axis], (long)blend[axis]);
          }
        }
      }
      free(design);
      free(blend);
    }
    printf("end\n");
    FT_Done_MM_Var(library, mm);
    FT_Done_Face(face);
  }
  FT_Done_FreeType(library);
  return 0;
}

static int table_present(FT_Face face, FT_ULong tag)
{
  FT_ULong len = 0;
  return FT_Load_Sfnt_Table(face, tag, 0, NULL, &len) == 0;
}

/* FT_OpenType_Validate with `flags`; the returned tables are freed. */
static FT_Error ot_validate(FT_Face face, FT_UInt flags)
{
  FT_Bytes base = NULL, gdef = NULL, gpos = NULL, gsub = NULL, jstf = NULL;
  FT_Error error =
      FT_OpenType_Validate(face, flags, &base, &gdef, &gpos, &gsub, &jstf);
  FT_OpenType_Free(face, base);
  FT_OpenType_Free(face, gdef);
  FT_OpenType_Free(face, gpos);
  FT_OpenType_Free(face, gsub);
  FT_OpenType_Free(face, jstf);
  return error;
}

static int otvalid(const char *path)
{
  FT_Library library;
  FT_Face face;
  FT_Long num_faces, i;
  FT_Error error;

  if (FT_Init_FreeType(&library) != 0) {
    fprintf(stderr, "ft-dump: cannot initialize FreeType\n");
    return 1;
  }
  error = FT_New_Face(library, path, -1, &face);
  if (error != 0) {
    fprintf(stderr, "ft-dump: %s: FreeType error %d\n", path, (int)error);
    FT_Done_FreeType(library);
    return 1;
  }
  num_faces = face->num_faces;
  FT_Done_Face(face);
  for (i = 0; i < num_faces; i++) {
    FT_UInt gdef_flags = FT_VALIDATE_GDEF;

    printf("face %ld\n", (long)i);
    error = FT_New_Face(library, path, i, &face);
    if (error != 0) {
      printf("error %d\nend\n", (int)error);
      continue;
    }
    if (table_present(face, FT_MAKE_TAG('G', 'S', 'U', 'B'))) {
      error = ot_validate(face, FT_VALIDATE_GSUB);
      printf("otvalid GSUB %d\n", (int)error);
      if (error == 0)
        gdef_flags |= FT_VALIDATE_GSUB;
    }
    if (table_present(face, FT_MAKE_TAG('G', 'P', 'O', 'S'))) {
      error = ot_validate(face, FT_VALIDATE_GPOS);
      printf("otvalid GPOS %d\n", (int)error);
      if (error == 0)
        gdef_flags |= FT_VALIDATE_GPOS;
    }
    if (table_present(face, FT_MAKE_TAG('G', 'D', 'E', 'F')))
      printf("otvalid GDEF %d\n", (int)ot_validate(face, gdef_flags));
    printf("end\n");
    FT_Done_Face(face);
  }
  FT_Done_FreeType(library);
  return 0;
}

int main(int argc, char **argv)
{
  FT_Library library;
  FT_Face face;
  FT_Long num_faces, i;
  FT_Long byte_count = 0;
  FT_Byte *bytes = NULL;
  int metrics_mode = 0;
  FT_Error error;

  if (argc >= 2 && (strcmp(argv[1], "--var") == 0 ||
                    strcmp(argv[1], "--var-normalize") == 0))
    return dump_var_reference(argc, argv);
  if (argc >= 2 && !reference_options(&argc, argv)) return 2;
  if (argc == 3 && strcmp(argv[1], "--advances") == 0)
    return dump_advance_queries(argv[2]);
  if (argc == 2 && strcmp(argv[1], "--calc") == 0)
    return calc();
  if (argc == 4 && strcmp(argv[1], "--outline") == 0)
    return outlines(argv[2], argv[3], 0, NULL, NULL);
  if (argc == 9 && strcmp(argv[1], "--stroke") == 0)
    return dump_stroke(argv[2], argv[3], argv[4], argv[5], argv[6], argv[7], argv[8]);
  if (argc == 5 && strcmp(argv[1], "--sampled-outline") == 0)
    return outlines(argv[2], argv[3], 0, argv[4], NULL);
  if (argc == 6 && strcmp(argv[1], "--sampled-outline-var") == 0)
    return outlines(argv[2], argv[3], 0, argv[4], argv[5]);
  if (argc == 4 && strcmp(argv[1], "--raster") == 0)
    return outlines(argv[2], argv[3], 1, NULL, NULL);
  if (argc == 5 && strcmp(argv[1], "--raster") == 0 && strcmp(argv[3], "--lcd") == 0)
    return outlines(argv[2], argv[4], 3, NULL, NULL);
  if (argc == 5 && strcmp(argv[1], "--raster") == 0 && strcmp(argv[3], "--lcd-v") == 0)
    return outlines(argv[2], argv[4], 4, NULL, NULL);
  if (argc == 5 && strcmp(argv[1], "--raster") == 0 && strcmp(argv[3], "--mono") == 0)
    return outlines(argv[2], argv[4], 5, NULL, NULL);
  if (argc == 5 && strcmp(argv[1], "--raster") == 0 && strcmp(argv[3], "--sdf") == 0)
    return outlines(argv[2], argv[4], 6, NULL, NULL);
  if (argc == 5 && strcmp(argv[1], "--raster") == 0 && strcmp(argv[3], "--bitmap-sdf") == 0)
    return outlines(argv[2], argv[4], 7, NULL, NULL);
  if (argc == 5 && strcmp(argv[1], "--raster") == 0 && strcmp(argv[3], "--color") == 0)
    return outlines(argv[2], argv[4], 2, NULL, NULL);
  if (argc == 3 && strcmp(argv[1], "--cid") == 0)
    return faces(argv[2], dump_cid);
  if (argc == 3 && strcmp(argv[1], "--ps-info") == 0)
    return faces(argv[2], dump_ps_info);
  if (argc == 3 && strcmp(argv[1], "--cff2-private") == 0)
    return faces(argv[2], dump_cff2_private);
  if (argc == 3 && strcmp(argv[1], "--colr") == 0)
    return faces(argv[2], dump_colr);
  if (argc == 3 && strcmp(argv[1], "--sbit") == 0)
    return faces(argv[2], dump_sbit);
  if (argc == 3 && strcmp(argv[1], "--otvalid") == 0)
    return otvalid(argv[2]);
  if (argc == 3 && strcmp(argv[1], "--metrics") == 0) {
    metrics_mode = 1;
    argv[1] = argv[2];
    argc = 2;
  }
  if (argc != 2) {
    fprintf(stderr, "usage: ft-dump FILE | ft-dump --advances [--named-instance=N] [--variations=TAG=value,...] FILE | ft-dump --calc | ft-dump --outline SIZE FILE | ft-dump --stroke SIZE GLYPH RADIUS JOIN MITER BORDER FILE | ft-dump --sampled-outline SIZE FILE PLAN | ft-dump --sampled-outline-var SIZE FILE PLAN TAG=value[,TAG=value...] | ft-dump --raster PPEM [--color|--lcd|--lcd-v|--mono|--sdf|--bitmap-sdf] FILE | ft-dump --cid FILE | ft-dump --ps-info FILE | ft-dump --cff2-private FILE | ft-dump --otvalid FILE | ft-dump --colr FILE | ft-dump --sbit FILE\n");
    return 2;
  }
  if (FT_Init_FreeType(&library) != 0) {
    fprintf(stderr, "ft-dump: cannot initialize FreeType\n");
    return 1;
  }
  error = FT_New_Face(library, argv[1], -1, &face);
  if (error != 0) {
    fprintf(stderr, "ft-dump: %s: FreeType error %d\n", argv[1], (int)error);
    FT_Done_FreeType(library);
    return 1;
  }
  num_faces = face->num_faces;
  FT_Done_Face(face);
  if (reference_has_named && !(bytes = read_font(argv[1], &byte_count))) {
    FT_Done_FreeType(library);
    return 1;
  }

  for (i = 0; i < num_faces; i++) {
    void *table;

    printf("face %ld\n", (long)i);
    error = bytes ? FT_New_Memory_Face(library, bytes, byte_count, i, &face)
                  : FT_New_Face(library, argv[1], i, &face);
    if (error != 0) {
      printf("error %d\nend\n", (int)error);
      continue;
    }
    if (set_reference_location(library, face, reference_variations) != 0) {
      FT_Done_Face(face);
      free(bytes);
      FT_Done_FreeType(library);
      return 2;
    }
    printf("num_glyphs %ld\n", (long)face->num_glyphs);
    if ((table = FT_Get_Sfnt_Table(face, FT_SFNT_HEAD)) != NULL)
      dump_head((const TT_Header *)table);
    if ((table = FT_Get_Sfnt_Table(face, FT_SFNT_HHEA)) != NULL)
      dump_hhea((const TT_HoriHeader *)table);
    if ((table = FT_Get_Sfnt_Table(face, FT_SFNT_MAXP)) != NULL)
      dump_maxp((const TT_MaxProfile *)table);
    dump_charmaps(face);
    dump_advances(face);
    dump_names_and_metrics(face, metrics_mode);
    printf("end\n");
    FT_Done_Face(face);
  }
  free(bytes);
  FT_Done_FreeType(library);
  return 0;
}
