// Compare encoded glyphs from the public C API against pinned HarfBuzz GPU.
#include "sofel.h"
#include "hb-gpu.h"
#include <array>
#include <cstdio>
#include <cstring>
#include <vector>

int main(int argc, char **argv) {
  if (argc != 2) return 2;
  auto *source = hb_blob_create_from_file_or_fail(argv[1]);
  if (!source) return 1;
  unsigned size = 0;
  auto *data = hb_blob_get_data(source, &size);
  int32_t error;
  auto *ours = sofel_font_create(reinterpret_cast<const uint8_t *>(data), size, 0, &error);
  if (!ours) return 1;
  auto *face = hb_face_create(source, 0);
  auto *font = hb_font_create(face);
  auto *paint = hb_gpu_paint_create_or_fail();
  if (!paint) return 1;
  unsigned checked = 0, rejected = 0, failed = 0;
  size_t total = 0;
  for (unsigned gid = 0; gid < hb_face_get_glyph_count(face); ++gid) {
    hb_gpu_paint_glyph(paint,font,gid);
    auto *ref = hb_gpu_paint_encode(paint,nullptr);
    size_t n = 0;
    error = sofel_gpu_paint_glyph(ours,nullptr,gid,0,nullptr,0,&n);
    if (!ref) {
      if (error == SOFEL_ERROR_INVALID_ARGUMENT) { ++rejected; continue; }
      std::fprintf(stderr,"glyph %u: expected encoding error, got %d\n",gid,error);
      ++failed; continue;
    }
    unsigned len = 0;
    auto *bytes = hb_blob_get_data(ref,&len);
    std::vector<std::array<int16_t,4>> records(n);
    error = sofel_gpu_paint_glyph(ours,nullptr,gid,0,reinterpret_cast<int16_t (*)[4]>(records.data()),n,&n);
    if (error || n*8 != len) {
      std::fprintf(stderr,"glyph %u: %zu records, expected %u, status %d\n",gid,n,len/8,error);
      ++failed;
    } else {
      ++checked;
      total += n;
      for (size_t i=0; i<n; ++i) {
        if (std::memcmp(records[i].data(),bytes+i*8,8)) {
          std::fprintf(stderr,"glyph %u: first different record %zu\n",gid,i);
          ++failed; break;
        }
      }
    }
    hb_blob_destroy(ref);
  }
  hb_gpu_paint_destroy(paint); hb_font_destroy(font); hb_face_destroy(face);
  hb_blob_destroy(source); sofel_font_destroy(ours);
  std::printf("%s: %u matching counts, %zu records, %u matching rejections, %u differences\n",
              argv[1], checked, total, rejected, failed);
  return failed ? 1 : 0;
}
