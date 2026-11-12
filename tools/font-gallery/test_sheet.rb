# frozen_string_literal: true
require 'minitest/autorun'
require_relative 'gallery'

class GallerySheetTest < Minitest::Test
  def test_inv4_gallery_sheet_places_ink_without_rescaling
    image = GalleryPixels.parse("P5\n# left -1 top 2\n2 1\n255\n\x00\x40".b)
    ink = GalleryPixels.compose([{image: "P5\n# left 1 top 2\n2 1\n255\n\x40\x00".b, dx: 64, dy: 64, ax: 192, ay: 0}])
    card = GallerySheet.card(image, ink, [])
    canvas = GallerySheet.layout([card, card])
    x = GallerySheet::PADDING
    y = GallerySheet::SAMPLE_TOP + (GallerySheet::SAMPLE_HEIGHT - 1) / 2
    pixel = ->(xpos) { canvas.pixels.byteslice((y * canvas.width + xpos) * 4, 8) }
    assert_equal image.pixels, pixel.call(x)
    assert_equal image.pixels, pixel.call(card.width + x)
    assert_equal ink.pixels, pixel.call(GallerySheet::RUN_X)
  end
end
