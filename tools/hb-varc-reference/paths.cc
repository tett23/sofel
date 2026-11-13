#include "hb.h"
#include "hb-ot.h"

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

static uint32_t bits(float value) {
  uint32_t out;
  std::memcpy(&out, &value, sizeof out);
  return out;
}

static void move_to(hb_draw_funcs_t *, void *, hb_draw_state_t *, float x, float y, void *) {
  std::printf("M %08x %08x\n", bits(x), bits(y));
}
static void line_to(hb_draw_funcs_t *, void *, hb_draw_state_t *, float x, float y, void *) {
  std::printf("L %08x %08x\n", bits(x), bits(y));
}
static void quad_to(hb_draw_funcs_t *, void *, hb_draw_state_t *, float cx, float cy,
                    float x, float y, void *) {
  std::printf("Q %08x %08x %08x %08x\n", bits(cx), bits(cy), bits(x), bits(y));
}
static void cubic_to(hb_draw_funcs_t *, void *, hb_draw_state_t *, float ax, float ay,
                     float bx, float by, float x, float y, void *) {
  std::printf("C %08x %08x %08x %08x %08x %08x\n",
              bits(ax), bits(ay), bits(bx), bits(by), bits(x), bits(y));
}
static void close_path(hb_draw_funcs_t *, void *, hb_draw_state_t *, void *) {
  std::puts("Z");
}

static bool number(const char *text, unsigned &out) {
  char *end = nullptr;
  unsigned long parsed = std::strtoul(text, &end, 10);
  if (!text[0] || *end || parsed > UINT32_MAX) return false;
  out = static_cast<unsigned>(parsed);
  return true;
}

int main(int argc, char **argv) {
  if (argc < 4) return 2;
  unsigned face_index;
  if (!number(argv[2], face_index)) return 2;
  hb_blob_t *blob = hb_blob_create_from_file(argv[1]);
  if (!blob || face_index >= hb_face_count(blob)) return 2;
  hb_face_t *face = hb_face_create(blob, face_index);
  hb_font_t *font = hb_font_create(face);
  hb_ot_font_set_funcs(font);
  for (int i = 4; i < argc; i++) {
    const char *named = "--named-instance=";
    const char *variations = "--variations=";
    if (std::strncmp(argv[i], named, std::strlen(named)) == 0) {
      unsigned index;
      if (!number(argv[i] + std::strlen(named), index) || index > UINT16_MAX) return 2;
      hb_font_set_var_named_instance(font, index);
    } else if (std::strncmp(argv[i], variations, std::strlen(variations)) == 0) {
      std::vector<hb_variation_t> values;
      std::string source(argv[i] + std::strlen(variations));
      size_t start = 0;
      while (start < source.size()) {
        size_t end = source.find(',', start);
        if (end == std::string::npos) end = source.size();
        hb_variation_t value;
        if (!hb_variation_from_string(source.data() + start, end - start, &value)) return 2;
        values.push_back(value);
        start = end + 1;
      }
      hb_font_set_variations(font, values.data(), values.size());
    } else {
      return 2;
    }
  }
  hb_draw_funcs_t *funcs = hb_draw_funcs_create();
  hb_draw_funcs_set_move_to_func(funcs, move_to, nullptr, nullptr);
  hb_draw_funcs_set_line_to_func(funcs, line_to, nullptr, nullptr);
  hb_draw_funcs_set_quadratic_to_func(funcs, quad_to, nullptr, nullptr);
  hb_draw_funcs_set_cubic_to_func(funcs, cubic_to, nullptr, nullptr);
  hb_draw_funcs_set_close_path_func(funcs, close_path, nullptr, nullptr);
  std::string gids(argv[3]);
  size_t start = 0;
  while (start < gids.size()) {
    size_t end = gids.find(',', start);
    if (end == std::string::npos) end = gids.size();
    unsigned glyph;
    if (!number(gids.substr(start, end - start).c_str(), glyph)) return 2;
    std::printf("glyph %u\n", glyph);
    if (!hb_font_draw_glyph_or_fail(font, glyph, funcs, nullptr)) std::puts("error");
    std::puts("end");
    start = end + 1;
  }
  hb_draw_funcs_destroy(funcs);
  hb_font_destroy(font);
  hb_face_destroy(face);
  hb_blob_destroy(blob);
  return 0;
}
