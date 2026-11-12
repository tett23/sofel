// Emit matching HarfBuzz and sofel pages using the public C APIs.
#include "hb-vector.h"
#include "hb-ot.h"
#include "sofel.h"
#include <fstream>
#include <cstdlib>
#include <cstring>
#include <vector>

int main(int argc, char **argv) {
  if (argc != 5 && argc != 6) return 2;
  const bool empty = argc == 6 && !std::strcmp(argv[5], "empty");
  const bool svg = argc == 6 && !std::strcmp(argv[5], "svg-gid");
  hb_blob_t *blob = hb_blob_create_from_file_or_fail(argv[1]);
  if (!blob) return 1;
  hb_face_t *face = hb_face_create(blob, 0);
  hb_font_t *font = hb_font_create(face);
  hb_ot_font_set_funcs(font);
  hb_codepoint_t gid;
  if (argc == 6) gid = std::strtoul(argv[4], nullptr, 10);
  else if (!hb_font_get_nominal_glyph(font, std::strtoul(argv[4], nullptr, 16), &gid)) return 1;
  auto *paint = hb_vector_paint_create_or_fail(svg ? HB_VECTOR_FORMAT_SVG : HB_VECTOR_FORMAT_PDF);
  hb_glyph_extents_t ink;
  if (empty) ink = {8, 48, 80, -48};
  else if (!hb_font_get_glyph_extents(font, gid, &ink) || ink.width <= 0 || ink.height >= 0) return 1;
  const hb_vector_extents_t extents = {float(ink.x_bearing - 16),
      float(ink.y_bearing + ink.height - 16), float(ink.width + 32), float(32 - ink.height)};
  if (svg) {
    const int scale = int(hb_face_get_upem(face)) * 64;
    hb_font_set_scale(font, scale, scale);
    hb_vector_paint_set_scale_factor(paint, 64, 64);
  }
  const hb_vector_extents_t viewport = svg ? hb_vector_extents_t{
      extents.x * 64, -(extents.y + extents.height) * 64, extents.width * 64, extents.height * 64} : extents;
  hb_vector_paint_set_extents(paint, &viewport);
  hb_vector_paint_set_background(paint, HB_COLOR(255, 255, 255, 255));
  // HarfBuzz returns null for a blank content stream. An empty graphics-state
  // scope creates a PDF with the requested background and no painted glyphs.
  if (empty) {
    auto *funcs = hb_vector_paint_get_funcs(paint);
    hb_paint_push_transform(funcs, paint, 1, 0, 0, 1, 1, 0);
    hb_paint_pop_transform(funcs, paint);
  } else if (!hb_vector_paint_glyph_or_fail(paint, font, gid, HB_VECTOR_EXTENTS_MODE_NONE)) return 1;
  auto *pdf = hb_vector_paint_render(paint);
  if (!pdf) return 1;
  unsigned length;
  const char *data = hb_blob_get_data(pdf, &length);
  std::ofstream reference(argv[2], std::ios::binary);
  reference.write(data, length);
  if (!length || !reference) return 1;
  hb_blob_destroy(pdf);
  hb_vector_paint_destroy(paint);
  hb_font_destroy(font);
  hb_face_destroy(face);

  const char *input = hb_blob_get_data(blob, &length);
  int32_t status;
  auto *sf = sofel_font_create(reinterpret_cast<const uint8_t *>(input), length, 0, &status);
  sofel_vector_glyph_t glyph = {gid, {1, 0, 0, 1, 0, 0}, {0, 0, 0, 255}};
  sofel_color_t white = {255, 255, 255, 255};
  size_t n = 0;
  auto render = [&](uint8_t *out, size_t capacity) {
    return sofel_vector_page(sf, nullptr, svg ? SOFEL_VECTOR_SVG : SOFEL_VECTOR_PDF, extents.x, extents.y,
                             extents.width, extents.height,
                             &white, empty ? nullptr : &glyph, empty ? 0 : 1, out, capacity, &n);
  };
  if (render(nullptr, 0) != SOFEL_ERROR_BUFFER_TOO_SMALL) return 1;
  std::vector<uint8_t> bytes(n);
  if (render(bytes.data(), bytes.size())) return 1;
  std::ofstream actual(argv[3], std::ios::binary);
  actual.write(reinterpret_cast<const char *>(bytes.data()), bytes.size());
  sofel_font_destroy(sf);
  hb_blob_destroy(blob);
  return !actual;
}
