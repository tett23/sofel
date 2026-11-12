require 'minitest/autorun'
require_relative 'gallery'
require_relative 'test_png_reader'

class SbixGalleryTest < Minitest::Test
  include GalleryPngReader

  def test_i4_i5_published_sbix_card_matches_both_glyphs
    output, binaries = ARGV
    cases = GalleryInputs.cases(repo: Dir.pwd)
    index = cases.index { |s| s.id == 'color-sbix' && s.face.zero? }
    sample = cases.fetch(index)
    assert_nil sample.unsupported
    result = GalleryInputs.render_case(sample, render_bin: File.join(binaries, 'sofel-render'),
                                      shape_bin: File.join(binaries, 'sofel-shape'))
    assert_nil result[:status]
    assert_equal [26], result[:glyphs].map { |g| g[:glyph_id] }
    width, _, sheet = decode_png(File.binread(File.join(output, 'all-samples.png')))
    x = (index % GallerySheet::COLUMNS) * GallerySheet::WIDTH
    y = (index / GallerySheet::COLUMNS) * GallerySheet::HEIGHT
    single = GalleryPixels.parse(result.fetch(:solo))
    glyph = GalleryPixels.parse(result[:glyphs].first.fetch(:image))
    [[single, GallerySheet::PADDING, false], [glyph, GallerySheet::RUN_X, true]].each do |image, dx, over_white|
      assert_equal [128,128], [image.width, image.height]
      assert image.pixels.bytes.each_slice(4).any? { |r,g,b,a| a.positive? && (r != g || b != g) }
      image.height.times do |row|
        src = image.pixels.byteslice(row * 128 * 4, 128 * 4)
        if over_white
          src = src.bytes.each_slice(4).flat_map do |r,g,b,a|
            [r,g,b].map { |v| (v*a + 255*(255-a) + 127) / 255 } + [255]
          end.pack('C*')
        end
        at = ((y + GallerySheet::SAMPLE_TOP + row) * width + x + dx) * 4
        assert_equal src, sheet.byteslice(at, 128 * 4), "glyph at x=#{dx}, row #{row}"
      end
    end
    note_y = y + 208
    (note_y...(y + GallerySheet::HEIGHT)).each do |row|
      assert_equal "\xff".b * (GallerySheet::WIDTH * 4),
                   sheet.byteslice((row * width + x) * 4, GallerySheet::WIDTH * 4)
    end
    html = File.read(File.join(output, 'index.html'))
    assert_match(%r{<tr><td>color-sbix / face 0</td>[^\n]*<td>描画済み</td></tr>}, html)
  end
end
