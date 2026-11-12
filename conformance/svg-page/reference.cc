// Recreate the fixtures with HarfBuzz 3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372:
// c++ reference.cc -o reference $(pkg-config --cflags --libs harfbuzz-vector harfbuzz)
// ./reference /path/to/harfbuzz/test/api/fonts/Roboto-Regular.abc.ttf
#include <hb.h>
#include <hb-vector.h>
#include <cstdio>
#include <cstdlib>

static void save(const char *name, hb_blob_t *blob) {
  if (!blob) std::exit(1);
  unsigned size;
  const char *data = hb_blob_get_data(blob, &size);
  FILE *out = std::fopen(name, "wb");
  if (!size || !out || std::fwrite(data, 1, size, out) != size ||
      std::fclose(out))
    std::exit(1);
  hb_blob_destroy(blob);
}

int main(int argc, char **argv) {
  if (argc != 2) return 1;
  hb_blob_t *file = hb_blob_create_from_file_or_fail(argv[1]);
  if (!file) return 1;
  hb_face_t *face = hb_face_create(file, 0);
  hb_font_t *font = hb_font_create(face);
  hb_codepoint_t a, b;
  if (!hb_font_get_nominal_glyph(font, 'a', &a) ||
      !hb_font_get_nominal_glyph(font, 'b', &b)) return 1;
  hb_vector_extents_t bounds = {0, 0, 2048, 2048};
  hb_vector_draw_t *draw = hb_vector_draw_create_or_fail(HB_VECTOR_FORMAT_SVG);
  if (!draw) return 1;
  hb_vector_draw_set_extents(draw, &bounds);
  save("empty.svg", hb_vector_draw_render(draw));
  hb_vector_draw_set_extents(draw, &bounds);
  hb_vector_draw_set_foreground(draw, HB_COLOR(0, 0, 255, 51));
  hb_vector_draw_glyph(draw, font, a, HB_VECTOR_EXTENTS_MODE_NONE);
  save("single.svg", hb_vector_draw_render(draw));
  hb_vector_draw_set_extents(draw, &bounds);
  hb_vector_draw_glyph(draw, font, a, HB_VECTOR_EXTENTS_MODE_NONE);
  hb_vector_draw_set_foreground(draw, HB_COLOR(255, 0, 0, 102));
  hb_vector_draw_set_transform(draw, 1, 0, 0, 1, 160, 64);
  hb_vector_draw_glyph(draw, font, b, HB_VECTOR_EXTENTS_MODE_NONE);
  save("translucent-overlap.svg", hb_vector_draw_render(draw));
  hb_vector_draw_destroy(draw);
  hb_font_destroy(font);
  hb_face_destroy(face);
  hb_blob_destroy(file);

  hb_vector_paint_t *paint = hb_vector_paint_create_or_fail(HB_VECTOR_FORMAT_SVG);
  if (!paint) return 1;
  hb_vector_paint_set_extents(paint, &bounds);
  auto *funcs = hb_vector_paint_get_funcs(paint);
  hb_paint_push_clip_rectangle(funcs, paint, 2, 4, 12, 14);
  hb_paint_push_transform(funcs, paint, 2, 0, 0, 3, 1, 2);
  auto *image = hb_blob_create_from_file_or_fail("red.png");
  if (!image) return 1;
  hb_glyph_extents_t extents = {0, 4, 4, -4};
  hb_paint_image(funcs, paint, image, 1, 1, HB_TAG('p','n','g',' '), 0, &extents);
  hb_paint_pop_transform(funcs, paint);
  hb_paint_pop_clip(funcs, paint);
  save("clipped-image.svg", hb_vector_paint_render(paint));
  hb_blob_destroy(image);
  hb_vector_paint_destroy(paint);
}
