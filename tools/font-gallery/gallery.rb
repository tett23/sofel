# frozen_string_literal: true

require 'cgi'
require 'json'
require 'open3'
require_relative 'inputs'
require_relative 'pixels'

module GallerySheet
  PADDING = 12
  WIDTH = 490
  HEIGHT = 232
  SAMPLE_TOP = 35
  SAMPLE_HEIGHT = 128
  RUN_X = 152
  COLUMNS = 3

  def self.card(solo, run, labels)
    card = GalleryPixels.blank(WIDTH, HEIGHT)
    [[solo, PADDING, RUN_X - 2 * PADDING], [run, RUN_X, WIDTH - RUN_X - PADDING]].each do |image, x, max_width|
      next unless image
      raise 'sample does not fit at its native ppem' if image.width > max_width || image.height > SAMPLE_HEIGHT
      y = SAMPLE_TOP + (SAMPLE_HEIGHT - image.height) / 2
      GalleryPixels.copy!(card, image, x, y)
    end
    labels.each do |label, x, y|
      raise 'label does not fit' if x + label.width > WIDTH || y + label.height > HEIGHT
      GalleryPixels.blit!(card, label, x, y)
    end
    card
  end

  def self.layout(cards)
    raise 'empty gallery' if cards.empty?
    columns = [COLUMNS, cards.length].min
    canvas = GalleryPixels.blank(WIDTH * columns, HEIGHT * ((cards.length + columns - 1) / columns))
    cards.each_with_index { |card, i| GalleryPixels.copy!(canvas, card, WIDTH * (i % columns), HEIGHT * (i / columns)) }
    canvas
  end
end

# Labels use a pinned corpus font so the computer's installed fonts do not affect the sheet.
class GalleryLabels
  def initialize(binaries, path)
    @shape = File.join(binaries, 'sofel-shape')
    @render = File.join(binaries, 'sofel-render')
    @path = path
    @glyphs = {}
    @labels = {}
  end

  def image(text)
    @labels[text] ||= begin
      data, error, status = Open3.capture3(@shape, '-O', 'json', '--no-glyph-names', '--language=en',
                                           '--font-size=1024', '--font-ppem=16', @path, text)
      raise error unless status.success?
      glyphs = JSON.parse(data, symbolize_names: true).map do |g|
        raise "label contains a missing glyph: #{text}" if g[:g].zero?
        g.merge(image: text[g[:cl]] == ' ' ? nil : glyph(g[:g]))
      end
      GalleryPixels.compose(glyphs)
    end
  end

  private

  def glyph(id)
    @glyphs[id] ||= begin
      bytes, error, status = Open3.capture3(@render, '--ppem', '16', '--glyph', id.to_s, @path, binmode: true)
      raise error unless status.success?
      bytes
    end
  end
end

module Gallery
  def self.input_label(text)
    return text if text.ascii_only?
    text.codepoints.map { |c| format('%04X', c) }.join(' ')
  end

  def self.generate(output, binaries)
    raise 'gallery output directory is not writable' unless File.directory?(output) && File.writable?(output)
    cases = GalleryInputs.cases(repo: Dir.pwd)
    labels = GalleryLabels.new(binaries, cases.find { |c| c.id == 'source-serif' }.path)
    rows = []
    cards = cases.map do |sample|
      begin
        result = GalleryInputs.render_case(sample, render_bin: File.join(binaries, 'sofel-render'),
                                           shape_bin: File.join(binaries, 'sofel-shape'))
        solo = GalleryPixels.parse(result[:solo]) if result[:solo]
        run = GalleryPixels.compose(result[:glyphs]) if result[:glyphs].any? { |g| g[:image] }
        heading = "#{sample.id} / face #{sample.face}"
        single = "#{sample.solo_kind} #{sample.solo_value}"
        info = "#{single} | #{input_label(sample.text)}"
        reason, note = case result[:status]
                       when :empty then ['空のグリフ（単字19、文字列26）', 'Unsupported: glyphs 19 and 26 are empty']
                       when :grayscale then ['カラー未対応（グレーの輪郭を表示）', 'Unsupported: color (grayscale outline shown)']
                       end
        captions = [[heading, 9], [info, 170], ["#{sample.ppem} ppem", 189]]
        captions << [note, 208] if reason
        rows << [heading, single, sample.text, sample.ppem.to_s, reason ? "未対応: #{reason}" : '描画済み']
        GallerySheet.card(solo, run, captions.map { |line, y| [labels.image(line), GallerySheet::PADDING, y] })
      rescue StandardError => e
        raise "#{sample.id} face #{sample.face}: #{e.message}"
      end
    end
    png = GalleryPixels.png(GallerySheet.layout(cards))
    File.binwrite(File.join(output, 'all-samples.png'), png)
    table = rows.map { |row| '<tr>' + row.map { |s| "<td>#{CGI.escapeHTML(s)}</td>" }.join + '</tr>' }.join("\n")
    File.write(File.join(output, 'index.html'), <<~HTML)
      <!doctype html><html lang="ja"><meta charset="utf-8"><meta name="viewport" content="width=device-width">
      <title>sofel フォント描画見本</title><style>body{font:1rem system-ui;margin:2rem}img{max-width:100%;height:auto}td,th{text-align:start;padding:.35rem .6rem;border-bottom:1px solid #ddd}table{border-collapse:collapse}</style>
      <h1>フォント描画見本</h1><p>左は単字、右は短い文字列です。画像を開くと等倍で確認できます。</p>
      <a href="all-samples.png"><img src="all-samples.png" alt="全フェイスの単字と短い文字列"></a>
      <table><thead><tr><th>フォント / フェイス</th><th>単字</th><th>文字列</th><th>ppem</th><th>結果</th></tr></thead><tbody>#{table}</tbody></table></html>
    HTML
  end
end

Gallery.generate(*ARGV) if $PROGRAM_NAME == __FILE__
