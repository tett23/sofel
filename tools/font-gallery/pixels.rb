require 'zlib'

module GalleryPixels
  Image = Struct.new(:width, :height, :pixels, :left, :top, keyword_init: true)

  def self.parse(bytes)
    bytes = bytes.b
    if bytes.start_with?("P5\n".b)
      match = /\AP5\n(?:#[^\n]*\n)*(\d+) (\d+)\n255\n/n.match(bytes)
      raise ArgumentError, 'invalid PGM header' unless match
      w, h = match.captures.map(&:to_i)
      comment = bytes.byteslice(0, match.end(0)).match(/^# left (-?\d+) top (-?\d+)$/)
      left, top = comment ? comment.captures.map(&:to_i) : [0, 0]
      data = bytes.byteslice(match.end(0)..-1)
      raise ArgumentError, 'invalid PGM payload' unless data&.bytesize == w * h
      rgba = data.bytes.flat_map { |v| [v, v, v, 255] }.pack('C*')
    elsif bytes.start_with?("P7\n".b)
      ending = bytes.index("ENDHDR\n".b)
      raise ArgumentError, 'invalid PAM header' unless ending
      header = bytes.byteslice(0, ending + 7)
      params = header.lines.map do |line|
        key, value = line.strip.split(' ', 2)
        [key, value] if value
      end.compact.to_h
      w, h = Integer(params.fetch('WIDTH')), Integer(params.fetch('HEIGHT'))
      raise ArgumentError, 'unsupported PAM format' unless params['DEPTH'] == '4' && params['MAXVAL'] == '255' && params['TUPLTYPE'] == 'RGB_ALPHA'
      comment = header.match(/^# left (-?\d+) top (-?\d+)$/)
      left, top = comment ? comment.captures.map(&:to_i) : [0, 0]
      rgba = bytes.byteslice(ending + 7..-1)
      raise ArgumentError, 'invalid PAM payload' unless rgba&.bytesize == w * h * 4
    else
      raise ArgumentError, 'unsupported image format'
    end
    raise ArgumentError, 'invalid image dimensions' if w <= 0 || h <= 0
    image = Image.new(width: w, height: h, pixels: rgba, left: left, top: top)
    image.instance_variable_set(:@pgm, bytes.start_with?("P5\n".b))
    image
  end

  def self.blank(width, height)
    raise ArgumentError, 'invalid canvas dimensions' if width <= 0 || height <= 0
    Image.new(width: width, height: height, pixels: "\xff".b * (width * height * 4), left: 0, top: 0)
  end

  # Copy the source RGBA bytes exactly, including its alpha channel.
  def self.copy!(canvas, source, x, y)
    raise ArgumentError, 'copy outside canvas' if x < 0 || y < 0 || x + source.width > canvas.width || y + source.height > canvas.height
    stride = source.width * 4
    source.height.times do |row|
      canvas.pixels[((y + row) * canvas.width + x) * 4, stride] = source.pixels.byteslice(row * stride, stride)
    end
    canvas
  end

  # RGBA over a white or previously drawn canvas.
  def self.blit!(canvas, source, x, y, gray_ink: false)
    source.height.times do |sy|
      cy = y + sy
      next if cy.negative? || cy >= canvas.height
      source.width.times do |sx|
        cx = x + sx
        next if cx.negative? || cx >= canvas.width
        si = (sy * source.width + sx) * 4
        di = (cy * canvas.width + cx) * 4
        sr, sg, sb, sa = source.pixels.byteslice(si, 4).bytes
        if gray_ink
          sa = 255 - sr
          sr = sg = sb = 0
        end
        next if sa.zero?
        dr, dg, db, da = canvas.pixels.byteslice(di, 4).bytes
        out_a = sa + (da * (255 - sa) + 127) / 255
        rgb = [sr, sg, sb].zip([dr, dg, db]).map do |s, d|
          ((s * sa * 255 + d * da * (255 - sa) + out_a * 127) / (out_a * 255)).clamp(0, 255)
        end
        canvas.pixels[di, 4] = (rgb + [out_a]).pack('C*')
      end
    end
    canvas
  end

  def self.compose(glyphs)
    pen_x = pen_y = 0
    positioned = glyphs.map do |g|
      image = g.fetch(:image)
      image = parse(image) if image.is_a?(String)
      x = ((pen_x + g.fetch(:dx)) / 64.0).round
      y = -((pen_y + g.fetch(:dy)) / 64.0).round
      pen_x += g.fetch(:ax)
      pen_y += g.fetch(:ay)
      next unless image
      [image, x + image.left, y - image.top]
    end.compact
    return nil if positioned.empty?
    left = positioned.map { |_, x, _| x }.min
    top = positioned.map { |_, _, y| y }.min
    right = positioned.map { |im, x, _| x + im.width }.max
    bottom = positioned.map { |im, _, y| y + im.height }.max
    canvas = blank(right - left, bottom - top)
    positioned.each do |im, x, y|
      blit!(canvas, im, x - left, y - top, gray_ink: im.instance_variable_get(:@pgm))
    end
    canvas.left = left
    canvas.top = -top
    canvas
  end

  def self.chunk(type, payload)
    [payload.bytesize].pack('N') + type + payload + [Zlib.crc32(type + payload)].pack('N')
  end

  def self.png(image)
    raw = image.height.times.map do |y|
      "\0".b + image.pixels.byteslice(y * image.width * 4, image.width * 4)
    end.join.b
    "\x89PNG\r\n\x1a\n".b +
      chunk('IHDR', [image.width, image.height, 8, 6, 0, 0, 0].pack('NNC5')) +
      chunk('IDAT', Zlib.deflate(raw)) +
      chunk('IEND', ''.b)
  end
end
