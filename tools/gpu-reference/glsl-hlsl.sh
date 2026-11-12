#!/bin/sh
# Compile the caller's stage entry points together with sofel's shader helpers.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
src=$root/crates/sofel-gpu/shaders
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
compiler=${GLSLANG:-glslangValidator}
{
  echo '#version 450'
  cat "$src/vertex.glsl"
  echo 'void main() { gl_Position = vec4(0, 0, 0, 1); }'
} > "$tmp/outline.vert"
{
  echo '#version 450'
  cat "$src/outline.glsl"
  echo 'layout(location=0) out vec4 color;'
  echo 'void main() { color = vec4(hb_gpu_draw(gl_FragCoord.xy, 0u)); }'
} > "$tmp/outline.frag"
"$compiler" -G --auto-map-bindings "$tmp/outline.vert" -o "$tmp/vert.spv"
"$compiler" -G --auto-map-bindings "$tmp/outline.frag" -o "$tmp/frag.spv"
{
  cat "$src/vertex.hlsl"
  echo 'float4 main(float4 p : POSITION) : SV_Position { return p; }'
} > "$tmp/outline.vert.hlsl"
{
  echo 'StructuredBuffer<int4> hb_gpu_atlas : register(t0);'
  cat "$src/outline.hlsl"
  echo 'float4 main(float4 p : SV_Position) : SV_Target { return hb_gpu_draw(p.xy, 0); }'
} > "$tmp/outline.frag.hlsl"
"$compiler" -D -V -S vert -e main "$tmp/outline.vert.hlsl" -o "$tmp/vert-hlsl.spv"
"$compiler" -D -V -S frag -e main "$tmp/outline.frag.hlsl" -o "$tmp/frag-hlsl.spv"
