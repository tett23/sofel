require 'json'
require 'open3'

module GalleryInputs
  class InputError < StandardError; end

  Case = Struct.new(:id, :face, :path, :ppem, :solo_kind, :solo_value,
                    :text, :lang, :color, :unsupported, keyword_init: true)

  # The chosen words are recorded with measured shaping results in sofel ADR 0069.
  SAMPLES = {
    'arabic' => ['U+0628', 'مرحبا', 'ar'],
    'devanagari' => ['U+0915', 'नमस्ते', 'hi'],
    'khmer' => ['U+1780', 'សួស្តី', 'km'],
    'myanmar' => ['U+1000', 'မင်္ဂလာပါ', 'my'],
    'balinese' => ['U+1B13', 'ᬩᬮᬶ', 'ban'],
    'hebrew' => ['U+05E9', 'שלום', 'he'],
    'source-serif' => ['U+0067', 'office & affix', 'en'],
    'stix-math' => ['U+222B', '∫₀¹ x² dx', 'en'],
    'source-han' => ['U+9AA8', '山水風月', 'und'],
    'amstelvar' => ['U+0067', 'office & affix', 'en'],
    'color-cff' => ['19', 'simple_linear', 'en'],
    'color-sbix' => ['19', 'simple_linear', 'en'],
    'color-svg' => ['19', 'simple_linear', 'en'],
    'hb-variable-colr' => ['92', [0xF0502, 0xF0503].pack('U*'), 'und']
  }.freeze

  COLOR_EMPTY = %w[color-cff color-svg].freeze

  def self.cases(repo:, manifest: File.join(repo, 'conformance/fonts.tsv'),
                 font_dir: File.join(repo, 'target/testdata/extra-fonts'),
                 include_variable: true)
    raise InputError, "missing font manifest: #{manifest}" unless File.file?(manifest)

    rows = File.readlines(manifest, chomp: true).reject { |line| line.empty? || line.start_with?('#') }.map do |line|
      fields = line.split("\t")
      raise InputError, "invalid font manifest row: #{line}" unless fields.size >= 4
      [fields[0], File.join(font_dir, fields[3])]
    end
    raise InputError, 'empty font manifest' if rows.empty?
    rows << ['hb-variable-colr', File.join(repo, 'target/testdata/harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1_variable.ttf')] if include_variable

    rows.flat_map do |id, path|
      sample = SAMPLES.fetch(id) { raise InputError, "no sample for #{id}" }
      raise InputError, "missing font: #{path}" unless File.file?(path)
      header = File.binread(path, 12)
      count = header.start_with?('ttcf') ? header.byteslice(8, 4).unpack1('N') : 1
      raise InputError, "invalid face count: #{path}" if count < 1 || count > 1024
      (0...count).map do |face|
        glyph = sample[0].start_with?('U+') ? 'char' : 'glyph'
        Case.new(id: id, face: face, path: path, ppem: id == 'color-sbix' ? 109 : 48,
                 solo_kind: glyph, solo_value: sample[0], text: sample[1], lang: sample[2],
                 color: id.start_with?('color-', 'hb-variable-colr'),
                 unsupported: COLOR_EMPTY.include?(id) ? 'sofel-render: glyph %d is empty' : nil)
      end
    end
  end

  def self.run!(args)
    out, err, status = Open3.capture3(*args, binmode: true)
    [out, err, status]
  rescue Errno::ENOENT => e
    raise InputError, e.message
  end

  def self.render_case(item, render_bin:, shape_bin:)
    base = [render_bin, '--ppem', item.ppem.to_s, '--face', item.face.to_s]
    base << '--color' if item.color
    solo, err, status = run!(base + ["--#{item.solo_kind}", item.solo_value, item.path])
    if item.unsupported
      check_empty!(solo, err, status, item.solo_value, item)
    elsif !status.success?
      raise InputError, "#{item.id}/#{item.face} solo: #{err.strip}"
    end

    shape_args = [shape_bin, '-O', 'json', '--no-glyph-names',
                  "--language=#{item.lang}", "--face-index=#{item.face}",
                  "--font-size=#{item.ppem * 64}", "--font-ppem=#{item.ppem}",
                  item.path, item.text]
    shaped, err, status = run!(shape_args)
    raise InputError, "#{item.id}/#{item.face} shape: #{err.strip}" unless status.success?
    begin
      records = JSON.parse(shaped)
    rescue JSON::ParserError => e
      raise InputError, "#{item.id}/#{item.face} shape JSON: #{e.message}"
    end
    raise InputError, "#{item.id}/#{item.face} empty shape" if records.empty?
    raise InputError, "#{item.id}/#{item.face} missing glyph" if records.any? { |r| r.fetch('g').zero? }
    if item.unsupported && records.map { |r| r.fetch('g') } != [26]
      raise InputError, "#{item.id}/#{item.face} unexpected color glyphs"
    end

    glyphs = records.map do |r|
      image, err, status = run!(base + ['--glyph', r.fetch('g').to_s, item.path])
      if item.unsupported
        check_empty!(image, err, status, r.fetch('g').to_s, item)
      elsif !status.success?
        unless item.text.each_char.to_a[r.fetch('cl')] == ' ' && err == "sofel-render: glyph #{r.fetch('g')} is empty\n"
          raise InputError, "#{item.id}/#{item.face} glyph #{r.fetch('g')}: #{err.strip}"
        end
        image = nil
      end
      { image: item.unsupported ? nil : image, dx: r.fetch('dx'), dy: r.fetch('dy'),
        ax: r.fetch('ax'), ay: r.fetch('ay'), glyph_id: r.fetch('g') }
    end
    limitation = if item.unsupported
                   :empty
                 elsif item.id == 'hb-variable-colr'
                   unless solo.start_with?("P5\n") && glyphs.all? { |g| g[:image]&.start_with?("P5\n") }
                     raise InputError, 'variable COLR render mode changed; review its registered limitation'
                   end
                   :grayscale
                 end
    { solo: item.unsupported ? nil : solo, glyphs: glyphs, status: limitation }
  end

  def self.check_empty!(out, err, status, glyph, item)
    expected = format(item.unsupported, glyph.to_i) + "\n"
    return if !status.success? && out.empty? && err == expected
    raise InputError, "#{item.id}/#{item.face} expected #{expected.inspect}, got #{err.inspect}"
  end
  private_class_method :check_empty!
end
