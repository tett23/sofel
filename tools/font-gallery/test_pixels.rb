require 'minitest/autorun'
require 'zlib'
require_relative 'pixels'

class TestPixels < Minitest::Test
  def decode_png(bytes)
    assert_equal "\x89PNG\r\n\x1a\n".b, bytes.byteslice(0, 8)
    offset = 8
    data = ''.b
    width = height = nil
    until offset == bytes.bytesize
      length = bytes.byteslice(offset, 4).unpack1('N')
      type = bytes.byteslice(offset + 4, 4)
      payload = bytes.byteslice(offset + 8, length)
      assert_equal Zlib.crc32(type + payload), bytes.byteslice(offset + 8 + length, 4).unpack1('N')
      width, height = payload.unpack('NN') if type == 'IHDR'
      data << payload if type == 'IDAT'
      offset += length + 12
    end
    rows = Zlib.inflate(data).bytes.each_slice(width * 4 + 1).to_a
    assert_equal height, rows.length
    [width, height, rows.flat_map { |row| assert_equal 0, row.shift; row }.pack('C*')]
  end

  def test_inv2_gallery_pixels
    gray = GalleryPixels.parse("P5\n# left -2 top 3\n2 1\n255\n".b + [0, 128].pack('C*'))
    assert_equal [-2, 3, 2, 1], [gray.left, gray.top, gray.width, gray.height]
    assert_equal [2, 1, [0,0,0,255,128,128,128,255].pack('C*')], decode_png(GalleryPixels.png(gray))

    rgba = GalleryPixels.parse("P7\n# left 1 top 4\nWIDTH 1\nHEIGHT 1\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n".b + [240, 12, 80, 128].pack('C*'))
    assert_equal [1, 4, 1, 1], [rgba.left, rgba.top, rgba.width, rgba.height]
    assert_equal [1, 1, [240,12,80,128].pack('C*')], decode_png(GalleryPixels.png(rgba))
  end

  def test_inv4_gallery_sheet
    red = GalleryPixels.parse("P7\n# left 0 top 2\nWIDTH 1\nHEIGHT 1\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n".b + [255,0,0,128].pack('C*'))
    black = GalleryPixels.parse("P5\n# left 0 top 2\n1 1\n255\n".b + "\0")
    glyphs = [
      {image: red, dx: 0, dy: 0, ax: 64, ay: 0},
      {image: black, dx: -64, dy: 0, ax: 64, ay: 0}
    ]
    run = GalleryPixels.compose(glyphs)
    assert_equal [1, 1], [run.width, run.height]
    assert_equal [0,0,0,255].pack('C*'), run.pixels
    sheet = GalleryPixels.blank(3, 2)
    GalleryPixels.blit!(sheet, run, 1, 1)
    assert_equal [3, 2, ([255,255,255,255] * 4 + [0,0,0,255] + [255,255,255,255]).pack('C*')], decode_png(GalleryPixels.png(sheet))
  end


  def test_inv2_transparent_pixel_copy
    source = GalleryPixels.parse("P7\n# left 0 top 0\nWIDTH 1\nHEIGHT 1\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n".b + [20,40,80,64].pack('C*'))
    canvas = GalleryPixels.blank(2, 1)
    GalleryPixels.copy!(canvas, source, 1, 0)
    assert_equal [20,40,80,64].pack('C*'), canvas.pixels.byteslice(4,4)
    assert_equal [2, 1, canvas.pixels], decode_png(GalleryPixels.png(canvas))
  end

  def test_inv4_gray_white_does_not_erase_earlier_ink
    ink = GalleryPixels.parse("P5\n# left 0 top 1\n2 1\n255\n".b + [0, 255].pack('C*'))
    glyphs = [{image: ink, dx: 0, dy: 0, ax: 0, ay: 0},
              {image: ink, dx: 64, dy: 0, ax: 0, ay: 0}]
    composed = GalleryPixels.compose(glyphs)
    assert_equal [3, 1], [composed.width, composed.height]
    assert_equal [0,0,0,255,0,0,0,255,255,255,255,255].pack('C*'), composed.pixels
  end

  def test_invalid_payload
    assert_raises(ArgumentError) { GalleryPixels.parse("P5\n1 1\n255\n".b) }
  end
end
