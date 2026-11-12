/* Compare every sfnt table exposed by a FreeType with WOFF and WOFF2. */
#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_TRUETYPE_TABLES_H
#include <stdlib.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>

static int check_sfnt_sum(FT_Face face)
{
    FT_ULong len = 0;
    if (FT_Load_Sfnt_Table(face, 0, 0, NULL, &len)) return 1;
    FT_Byte *data = malloc(len);
    if (!data || FT_Load_Sfnt_Table(face, 0, 0, data, &len)) {
        free(data); return 1;
    }
    uint32_t sum = 0;
    for (FT_ULong i = 0; i < len; ++i)
        sum += (uint32_t)data[i] << (24 - 8 * (i % 4));
    free(data);
    return sum != UINT32_C(0xb1b0afba);
}

static int compare(FT_Face original, FT_Face encoded, int woff2)
{
    FT_ULong count, out_count, kept = 0;
    if (FT_Sfnt_Table_Info(original, 0, NULL, &count) ||
        FT_Sfnt_Table_Info(encoded, 0, NULL, &out_count)) return 1;
    for (FT_ULong i = 0; i < count; ++i) {
        FT_ULong tag, len, out_len = 0;
        if (FT_Sfnt_Table_Info(original, i, &tag, &len)) return 1;
        if (woff2 && tag == FT_MAKE_TAG('D','S','I','G')) continue;
        if (FT_Load_Sfnt_Table(encoded, tag, 0, NULL, &out_len) || out_len != len) return 1;
        FT_Byte *a = malloc(len), *b = malloc(len);
        if (!a || !b || FT_Load_Sfnt_Table(original, tag, 0, a, &len) ||
            FT_Load_Sfnt_Table(encoded, tag, 0, b, &out_len)) {
            free(a); free(b); return 1;
        }
        if (woff2 && tag == FT_MAKE_TAG('h','e','a','d') && len >= 18) {
            memcpy(a + 8, b + 8, 4);
            a[16] |= 8;
        }
        int differs = memcmp(a, b, len);
        free(a); free(b);
        if (differs) { fprintf(stderr, "mismatched table %08lx\n", tag); return 1; }
        ++kept;
    }
    /* FreeType reconstructs WOFF2 offsets and checkSumAdjustment itself.
       Compare that format table by table, as specified in ADR 0070. */
    return kept != out_count || (!woff2 && check_sfnt_sum(encoded));
}
int main(int argc, char **argv)
{
    FT_Library library;
    FT_Face source, result;
    if (argc != 4 || FT_Init_FreeType(&library)) return 1;
    if (FT_New_Face(library, argv[2], 0, &source)) {
        fprintf(stderr, "cannot open source %s\n", argv[2]); return 1;
    }
    if (FT_New_Face(library, argv[3], 0, &result)) {
        fprintf(stderr, "cannot open result %s\n", argv[3]); return 1;
    }
    int status = compare(source, result, !strcmp(argv[1], "woff2"));
    FT_Done_Face(result); FT_Done_Face(source); FT_Done_FreeType(library);
    return status;
}
