require 'minitest/autorun'
require 'tmpdir'
require 'fileutils'
require_relative 'inputs'

class InputTests < Minitest::Test
  def setup
    @tmp = Dir.mktmpdir
    FileUtils.mkdir_p(File.join(@tmp, 'fonts'))
    File.binwrite(File.join(@tmp, 'fonts', 'one.ttf'), "OTTO" + "\0" * 8)
    File.binwrite(File.join(@tmp, 'fonts', 'many.ttc'), "ttcf\0\x01\0\0" + [2].pack('N'))
    @manifest = File.join(@tmp, 'fonts.tsv')
  end

  def teardown
    FileUtils.remove_entry(@tmp)
  end

  def test_inv1_gallery_faces
    File.write(@manifest, "# id\tsource\tarchive\tpath\nsource-serif\tsource-serif\t-\tfonts/one.ttf\nsource-han\tsource-han\t-\tfonts/many.ttc\n")
    cases = GalleryInputs.cases(repo: @tmp, manifest: @manifest, font_dir: @tmp, include_variable: false)
    assert_equal [['source-serif', 0], ['source-han', 0], ['source-han', 1]], cases.map { |c| [c.id, c.face] }
    assert_raises(GalleryInputs::InputError) { GalleryInputs.cases(repo: @tmp, manifest: File.join(@tmp, 'missing'), include_variable: false) }
    File.write(@manifest, '# empty')
    assert_raises(GalleryInputs::InputError) { GalleryInputs.cases(repo: @tmp, manifest: @manifest, include_variable: false) }
  end

  def test_inv3_gallery_failure
    bad = File.join(@tmp, 'bad-render')
    File.write(bad, "#!/bin/sh\necho 'sofel-render: unexpected failure' >&2\nexit 1\n")
    File.chmod(0755, bad)
    sample = GalleryInputs::Case.new(id: 'source-serif', face: 0, path: File.join(@tmp, 'fonts/one.ttf'),
                                     ppem: 48, solo_kind: 'char', solo_value: 'U+0067',
                                     text: 'office & affix', lang: 'en', color: false, unsupported: nil)
    assert_raises(GalleryInputs::InputError) do
      GalleryInputs.render_case(sample, render_bin: bad, shape_bin: '/bin/false')
    end
  end
end
