//! Caller-assembled shaders from HarfBuzz 15.1.0 (Old MIT license).
/// Source language for the GPU pipeline.
#[derive(Clone, Copy, Debug)]
pub enum ShaderLanguage {
    Wgsl,
    Glsl,
    Msl,
    Hlsl,
}

/// Stage of the outline pipeline.
#[derive(Clone, Copy, Debug)]
pub enum ShaderStage {
    Vertex,
    Fragment,
}

/// Returns source to prepend to the caller's entry point. The vertex stage
/// provides `hb_gpu_dilate`; the fragment stage provides `hb_gpu_draw`.
/// Language-specific atlas bindings are described in each fragment source.
pub fn shader_source(language: ShaderLanguage, stage: ShaderStage) -> &'static str {
    use ShaderLanguage::{Glsl, Hlsl, Msl, Wgsl};
    use ShaderStage::{Fragment, Vertex};
    match (language, stage) {
        (Wgsl, Vertex) => include_str!("../shaders/vertex.wgsl"),
        (Glsl, Vertex) => include_str!("../shaders/vertex.glsl"),
        (Msl, Vertex) => include_str!("../shaders/vertex.msl"),
        (Hlsl, Vertex) => include_str!("../shaders/vertex.hlsl"),
        (Wgsl, Fragment) => include_str!("../shaders/outline.wgsl"),
        (Glsl, Fragment) => include_str!("../shaders/outline.glsl"),
        (Msl, Fragment) => include_str!("../shaders/outline.msl"),
        (Hlsl, Fragment) => include_str!("../shaders/outline.hlsl"),
    }
}
