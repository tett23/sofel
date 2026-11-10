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
 * --outline and --raster open each face with the random seed 0
 * (FT_PARAM_TAG_RANDOM_SEED), so that the CFF `random` operator starts
 * from the font's initialRandomSeed instead of an address (ADR 0021).
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

#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_TRUETYPE_TABLES_H
#include FT_ADVANCES_H
#include FT_TRIGONOMETRY_H
#include FT_OUTLINE_H
#include FT_SFNT_NAMES_H
#include FT_CID_H
#include FT_TYPE1_TABLES_H
#include FT_PARAMETER_TAGS_H
#include FT_OPENTYPE_VALIDATE_H

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

static void dump_outlines(FT_Face face, int unscaled, int ppem)
{
  FT_Int32 flags = FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP;
  FT_Long g;

  if (unscaled)
    flags |= FT_LOAD_NO_SCALE;
  else if (FT_Set_Pixel_Sizes(face, 0, (FT_UInt)ppem) != 0) {
    printf("error size\n");
    return;
  }
  for (g = 0; g < face->num_glyphs; g++) {
    FT_Error error;
    FT_BBox cbox;

    printf("glyph %ld\n", (long)g);
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

static void dump_rasters(FT_Face face, int ppem)
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
    if (error == 0)
      error = FT_Render_Glyph(face->glyph, FT_RENDER_MODE_NORMAL);
    if (error != 0) {
      printf("error %d\n", (int)error);
      continue;
    }
    b = &face->glyph->bitmap;
    printf("bitmap %d %d %u %u\n", face->glyph->bitmap_left, face->glyph->bitmap_top,
           b->width, b->rows);
    for (r = 0; r < b->rows; r++) {
      const unsigned char *row = b->buffer + (long)r * b->pitch;
      for (c = 0; c < b->width; c++)
        printf("%02x", row[c]);
      if (b->width)
        printf("\n");
    }
  }
}

/* Opens face `index` with the random seed 0 (ADR 0021). */
static FT_Error open_seeded(FT_Library library, const char *path, FT_Long index, FT_Face *face)
{
  FT_Int32 seed = 0;
  FT_Parameter param;
  FT_Open_Args args;

  param.tag = FT_PARAM_TAG_RANDOM_SEED;
  param.data = &seed;
  memset(&args, 0, sizeof args);
  args.flags = FT_OPEN_PATHNAME | FT_OPEN_PARAMS;
  args.pathname = (char *)path;
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
    dump(face);
    printf("end\n");
    FT_Done_Face(face);
  }
  FT_Done_FreeType(library);
  return 0;
}

static int outlines(const char *size, const char *path, int raster)
{
  FT_Library library;
  FT_Face face;
  FT_Long num_faces, i;
  int unscaled = strcmp(size, "unscaled") == 0;
  int ppem = atoi(size);

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
  for (i = 0; i < num_faces; i++) {
    FT_Error error;

    printf("face %ld\n", (long)i);
    error = open_seeded(library, path, i, &face);
    if (error != 0) {
      printf("error %d\nend\n", (int)error);
      continue;
    }
    printf("num_glyphs %ld\n", (long)face->num_glyphs);
    if (raster)
      dump_rasters(face, ppem);
    else
      dump_outlines(face, unscaled, ppem);
    printf("end\n");
    FT_Done_Face(face);
  }
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

static void dump_names_and_metrics(FT_Face face)
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
  FT_Error error;

  if (argc == 2 && strcmp(argv[1], "--calc") == 0)
    return calc();
  if (argc == 4 && strcmp(argv[1], "--outline") == 0)
    return outlines(argv[2], argv[3], 0);
  if (argc == 4 && strcmp(argv[1], "--raster") == 0)
    return outlines(argv[2], argv[3], 1);
  if (argc == 3 && strcmp(argv[1], "--cid") == 0)
    return faces(argv[2], dump_cid);
  if (argc == 3 && strcmp(argv[1], "--ps-info") == 0)
    return faces(argv[2], dump_ps_info);
  if (argc == 3 && strcmp(argv[1], "--otvalid") == 0)
    return otvalid(argv[2]);
  if (argc != 2) {
    fprintf(stderr, "usage: ft-dump FILE | ft-dump --calc | ft-dump --outline SIZE FILE | ft-dump --raster PPEM FILE | ft-dump --cid FILE | ft-dump --ps-info FILE | ft-dump --otvalid FILE\n");
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

  for (i = 0; i < num_faces; i++) {
    void *table;

    printf("face %ld\n", (long)i);
    error = FT_New_Face(library, argv[1], i, &face);
    if (error != 0) {
      printf("error %d\nend\n", (int)error);
      continue;
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
    dump_names_and_metrics(face);
    printf("end\n");
    FT_Done_Face(face);
  }
  FT_Done_FreeType(library);
  return 0;
}
