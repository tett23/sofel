require 'zlib'

module GalleryPngReader
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
    raw = Zlib.inflate(data)
    stride = width * 4 + 1
    assert_equal stride * height, raw.bytesize
    rgba = String.new(capacity: width * height * 4, encoding: Encoding::BINARY)
    height.times do |y|
      at = y * stride
      assert_equal 0, raw.getbyte(at)
      rgba << raw.byteslice(at + 1, stride - 1)
    end
    [width, height, rgba]
  end

end
