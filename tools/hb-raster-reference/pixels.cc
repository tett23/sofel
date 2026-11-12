// Emit a single color glyph using the HarfBuzz raster implementation pinned by testdata.lock.
#include "hb-ot.h"
#include "hb-raster.h"
#include <cstdio>
#include <cstdlib>
#include <limits>

static bool number(const char *arg, unsigned &value) {
  char *end = nullptr;
  unsigned long v = std::strtoul(arg, &end, 10);
  if (!*arg || *arg == '-' || *end || v > std::numeric_limits<unsigned>::max()) return false;
  value = static_cast<unsigned>(v);
  return true;
}

int main(int argc, char **argv) {
  unsigned glyph = 0, ppem = 0;
  if (argc != 4 || !number(argv[2], glyph) || !number(argv[3], ppem) || !ppem || ppem > 65535) return 2;
  hb_blob_t *blob = hb_blob_create_from_file_or_fail(argv[1]);
  if (!blob) return 1;
  hb_face_t *face = hb_face_create(blob, 0);
  hb_font_t *font = hb_font_create(face);
  hb_ot_font_set_funcs(font);
  unsigned upem = hb_face_get_upem(face);
  hb_font_set_scale(font, upem, upem);
  hb_font_set_ppem(font, ppem, ppem);
  hb_raster_paint_t *paint = hb_raster_paint_create_or_fail();
  if (!paint || !upem || glyph >= hb_face_get_glyph_count(face)) return 1;
  float scale = static_cast<float>(ppem) / upem;
  hb_raster_paint_set_transform(paint, scale, 0, 0, scale, 0, 0);
  hb_raster_paint_set_palette(paint, 0);
  hb_raster_paint_set_foreground(paint, HB_COLOR(0, 0, 0, 255));
  if (!hb_raster_paint_glyph_or_fail(paint, font, glyph)) return 1;
  hb_raster_image_t *image = hb_raster_paint_render(paint);
  if (!image || hb_raster_image_get_format(image) != HB_RASTER_FORMAT_BGRA32) return 1;
  hb_raster_extents_t ext;
  hb_raster_image_get_extents(image, &ext);
  if (!ext.width || !ext.height || ext.width > ext.stride / 4) return 1;
  std::printf("BGRA %d %lld %u %u\n", ext.x_origin, static_cast<long long>(ext.y_origin) + ext.height, ext.width, ext.height);
  const uint8_t *pixels = hb_raster_image_get_buffer(image);
  for (unsigned y = 0; y < ext.height; y++) {
    if (std::fwrite(pixels + static_cast<size_t>(ext.height - 1 - y) * ext.stride, 4, ext.width, stdout) != ext.width) return 1;
  }
  hb_raster_image_destroy(image);
  hb_raster_paint_destroy(paint);
  hb_font_destroy(font);
  hb_face_destroy(face);
  hb_blob_destroy(blob);
  return std::fflush(stdout) == 0 ? 0 : 1;
}
