// Render the positioned abc cases from HarfBuzz test/vector/data/tests/basic.tests.
#include "sofel.h"
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iostream>
#include <iterator>
#include <vector>

int main(int argc, char **argv) {
  if (argc < 6) return 2;
  std::ifstream file(argv[1], std::ios::binary);
  std::vector<uint8_t> bytes((std::istreambuf_iterator<char>(file)), {});
  int32_t status;
  auto *font = sofel_font_create(bytes.data(), bytes.size(), 0, &status);
  if (!font) return 1;
  sofel_location_t *location = nullptr;
  std::vector<sofel_variation_t> axes;
  sofel_segment_t requested = {SOFEL_DIRECTION_LTR, 0, nullptr}, segment;
  for (int i = 6; i < argc; ++i) {
    if (!std::strcmp(argv[i], "ttb")) requested.direction = SOFEL_DIRECTION_TTB;
    else if (!std::strcmp(argv[i], "wght=900")) axes.push_back({SOFEL_TAG('w', 'g', 'h', 't'), 900});
    else if (!std::strcmp(argv[i], "CNTR=100")) axes.push_back({SOFEL_TAG('C', 'N', 'T', 'R'), 100});
    else return 2;
  }
  if (!axes.empty() && sofel_location_create(font, UINT32_MAX, axes.data(), axes.size(), &location)) return 1;
  std::vector<sofel_vector_glyph_t> glyphs;
  auto *buffer = sofel_buffer_create();
  if (sofel_buffer_add_utf8(buffer, "abc", 3, 0, 3) ||
      sofel_segment_guess("abc", 3, &requested, nullptr, &segment)) return 1;
  auto *plan = sofel_shape_plan_create(font, location, &segment, nullptr, 0, &status);
  uint16_t upem;
  if (sofel_font_units_per_em(font, &upem)) return 1;
  auto *scale = sofel_scale_create(font, upem * 64, upem * 64, &status);
  if (!plan || !scale || sofel_shape(font, location, scale, plan, buffer)) return 1;
  uint32_t gids[3];
  int32_t pos[12];
  if (sofel_buffer_glyphs(buffer, gids, nullptr, nullptr, pos, 3) != 3) return 1;
  float x = 0, y = 0;
  for (int i = 0; i < 3; ++i) {
    glyphs.push_back({gids[i], {1, 0, 0, 1, (x + pos[4*i+2]) / 64, (y + pos[4*i+3]) / 64}, {0, 0, 0, 255}});
    x += pos[4*i];
    y += pos[4*i+1];
  }
  sofel_scale_destroy(scale);
  sofel_shape_plan_destroy(plan);
  sofel_buffer_destroy(buffer);
  const sofel_color_t white = {255, 255, 255, 255};
  size_t size = 0;
  auto render = [&](uint8_t *out, size_t capacity) {
    return sofel_vector_page(font, location, SOFEL_VECTOR_SVG, std::atoi(argv[2]), std::atoi(argv[3]),
                             std::atoi(argv[4]), std::atoi(argv[5]), &white, glyphs.data(),
                             glyphs.size(), out, capacity, &size);
  };
  if (render(nullptr, 0) != SOFEL_ERROR_BUFFER_TOO_SMALL) return 1;
  std::vector<uint8_t> svg(size);
  if (render(svg.data(), svg.size())) return 1;
  std::cout.write(reinterpret_cast<const char *>(svg.data()), svg.size());
  sofel_location_destroy(location);
  sofel_font_destroy(font);
  return !std::cout;
}
