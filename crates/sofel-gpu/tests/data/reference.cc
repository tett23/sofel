// Rebuild with the pinned HarfBuzz src/hb-gpu-draw.cc and libharfbuzz.
// The binary fixture stores a little-endian texel count and RGBA16I texels per case.
#include "hb-gpu.h"
#include <cstdint>
#include <cstdio>
#include <cstring>

static void little_endian(uint32_t value, unsigned bytes) {
  for (unsigned i = 0; i < bytes; ++i)
    std::putchar((value >> (8 * i)) & 255);
}

int main() {
  for (unsigned test = 0; test < 5; ++test) {
    auto *draw = hb_gpu_draw_create_or_fail();
    if (!draw) return 1;
    auto *funcs = hb_gpu_draw_get_funcs(draw);
    hb_draw_state_t state = HB_DRAW_STATE_DEFAULT;
    if (test == 1 || test == 2) {
      hb_draw_move_to(funcs, draw, &state, 0, 0);
      hb_draw_line_to(funcs, draw, &state, 1, 0);
      hb_draw_line_to(funcs, draw, &state, 0, 1);
      hb_draw_close_path(funcs, draw, &state);
    }
    if (test == 2) {
      hb_draw_move_to(funcs, draw, &state, 0, 0);
      hb_draw_quadratic_to(funcs, draw, &state, -0.75f, 0.25f, -0.25f, -1);
      hb_draw_close_path(funcs, draw, &state);
    }
    if (test == 3) {
      hb_draw_move_to(funcs, draw, &state, 0.24f, 0.24f);
      hb_draw_cubic_to(funcs, draw, &state, 35, 79, 60, -21, 94.8f, 1.7f);
      hb_draw_close_path(funcs, draw, &state);
    }
    if (test == 4) {
      for (unsigned i = 0; i < 34; ++i) {
        hb_draw_move_to(funcs, draw, &state, 0, 0);
        hb_draw_line_to(funcs, draw, &state, 16, 16);
        hb_draw_close_path(funcs, draw, &state);
      }
    }
    auto *blob = hb_gpu_draw_encode(draw, nullptr);
    if (!blob) return 1;
    unsigned bytes = 0;
    auto *raw = hb_blob_get_data(blob, &bytes);
    if (bytes % 8) return 1;
    little_endian(bytes / 8, 4);
    for (unsigned i = 0; i < bytes; i += 2) {
      uint16_t word;
      std::memcpy(&word, raw + i, sizeof word);
      little_endian(word, 2);
    }
    hb_blob_destroy(blob);
    hb_gpu_draw_destroy(draw);
  }
  return std::ferror(stdout) ? 1 : 0;
}
