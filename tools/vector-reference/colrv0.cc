// Emit the positioned COLRv0 pages from HarfBuzz's test/vector basic.tests.
#include "sofel.h"
#include <cstdio>
#include <fstream>
#include <iterator>
#include <vector>

int main(int argc, char **argv) {
  if (argc != 4) return 2;
  std::ifstream file(argv[1], std::ios::binary);
  std::vector<uint8_t> data((std::istreambuf_iterator<char>(file)), {});
  int32_t status;
  auto *font = sofel_font_create(data.data(), data.size(), 0, &status);
  if (!font) return 1;
  uint16_t upem;
  uint32_t glyph;
  if (sofel_font_units_per_em(font,&upem) || !upem ||
      sofel_font_nominal_glyph(font,0x3299,&glyph)) return 1;
  const float scale = 65536.f / upem;
  sofel_vector_glyph_t positioned = {glyph,{scale,0,0,scale,0,0},{0,0,0,255}};
  sofel_color_t white = {255,255,255,255};
  for (uint32_t format : {SOFEL_VECTOR_SVG, SOFEL_VECTOR_PDF}) {
    const int y = format == SOFEL_VECTOR_PDF ? -8208 : -57360;
    size_t n = 0;
    if (sofel_vector_page(font,nullptr,format,-16,y,65568,65568,&white,&positioned,1,
                          nullptr,0,&n) != SOFEL_ERROR_BUFFER_TOO_SMALL) return 1;
    std::vector<uint8_t> page(n);
    if (sofel_vector_page(font,nullptr,format,-16,y,65568,65568,&white,&positioned,1,
                          page.data(),page.size(),&n)) return 1;
    std::ofstream output(argv[format+2], std::ios::binary);
    output.write(reinterpret_cast<const char *>(page.data()),page.size());
    if (!output) return 1;
  }
  sofel_font_destroy(font);
}
