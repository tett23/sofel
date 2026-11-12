// Build with pinned HarfBuzz hb-gpu-paint.cc, hb-gpu-draw.cc and libharfbuzz.
#include "hb-gpu.h"
#include <cstdint>
#include <cstdio>
#include <cstring>

static void word(uint32_t value, unsigned bytes) {
  for (unsigned i = 0; i < bytes; ++i) std::putchar((value >> (8 * i)) & 255);
}

static bool clip(hb_paint_funcs_t *funcs, hb_gpu_paint_t *paint, bool empty) {
  void *data = nullptr;
  auto *draw = hb_paint_push_clip_path_start(funcs, paint, &data);
  if (!draw) return false;
  if (!empty) {
    hb_draw_state_t state = HB_DRAW_STATE_DEFAULT;
    hb_draw_move_to(draw, data, &state, 0, 0);
    hb_draw_line_to(draw, data, &state, 1, 0);
    hb_draw_line_to(draw, data, &state, 0, 1);
    hb_draw_close_path(draw, data, &state);
  }
  hb_paint_push_clip_path_end(funcs, paint);
  return true;
}

static unsigned stops(hb_color_line_t *, void *, unsigned first, unsigned *n,
                      hb_color_stop_t *out, void *) {
  const hb_color_stop_t all[] = {{0.25f, false, HB_COLOR(11,73,234,128)},
                                 {0.75f, true, HB_COLOR(0,0,0,255)}};
  if (n && out) {
    unsigned done = 0;
    while (done < *n && first + done < 2) {out[done] = all[first + done]; ++done;}
    *n = done;
  }
  return 2;
}

int main() {
  for (unsigned test = 0; test <= 6; ++test) {
    auto *paint = hb_gpu_paint_create_or_fail();
    if (!paint) return 1;
    hb_gpu_paint_set_scale(paint, 1000, 1000);
    auto *funcs = hb_gpu_paint_get_funcs(paint);
    unsigned n = test >= 5 ? 1 : test;
    for (unsigned i = 0; i < n; ++i) clip(funcs, paint, test == 5);
    hb_paint_push_group(funcs, paint);
    hb_paint_color(funcs, paint, false, HB_COLOR(11, 73, 234, 128));
    hb_paint_color(funcs, paint, true, HB_COLOR(0, 0, 0, 255));
    if (test == 6) {
      hb_color_line_t line = {};
      line.get_color_stops = stops;
      line.get_extend = [](hb_color_line_t *, void *, void *) { return HB_PAINT_EXTEND_REPEAT; };
      hb_paint_push_transform(funcs,paint,2,0,0,1,8,9);
      hb_paint_linear_gradient(funcs,paint,&line,0,2,20,4,0,12);
      hb_paint_radial_gradient(funcs,paint,&line,4,5,2,12,9,6);
      hb_paint_sweep_gradient(funcs,paint,&line,4,5,-1,2);
      hb_paint_pop_transform(funcs,paint);
    }
    hb_paint_pop_group(funcs, paint, HB_PAINT_COMPOSITE_MODE_MULTIPLY);
    auto *blob = hb_gpu_paint_encode(paint, nullptr);
    if (!blob) {
      if (test != 4) return 1;
      word(UINT32_MAX, 4);
    } else {
      unsigned length = 0;
      const char *buffer = hb_blob_get_data(blob, &length);
      word(length / 8, 4);
      for (unsigned i = 0; i < length; i += 2) {
        uint16_t v;
        std::memcpy(&v, buffer + i, sizeof v);
        word(v, 2);
      }
      hb_blob_destroy(blob);
    }
    hb_gpu_paint_destroy(paint);
  }
  return std::ferror(stdout) ? 1 : 0;
}
