use sofel_gpu::{ShaderLanguage, ShaderStage, shader_source};

#[test]
fn i2_gpu_records_supply_both_shader_stages_in_four_languages() {
    use ShaderLanguage::{Glsl, Hlsl, Msl, Wgsl};
    for language in [Wgsl, Glsl, Msl, Hlsl] {
        assert!(shader_source(language, ShaderStage::Vertex).contains("hb_gpu_dilate"));
        let frag = shader_source(language, ShaderStage::Fragment);
        assert!(frag.contains("hb_gpu_fetch"));
        assert!(frag.contains("hb_gpu_draw"));
        assert!(frag.contains("hb_gpu_paint"));
    }
}
