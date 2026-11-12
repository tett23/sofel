// Run with deno run --allow-read from the repository root.
const adapter = await navigator.gpu.requestAdapter();
if (!adapter) throw new Error("No WebGPU adapter");
const device = await adapter.requestDevice();
const read = (stage: string) =>
  Deno.readTextFile(`crates/sofel-gpu/shaders/${stage}.wgsl`);
const shader = device.createShaderModule({
  code: [
    await read("vertex"),
    await read("outline"),
    `@group(0) @binding(0) var<storage, read> hb_gpu_atlas: array<vec4<i32>>;
   @vertex fn vertex_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
     let points = array(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));
     return vec4f(points[i],0,1);
   }
   @fragment fn fragment_main(@builtin(position) p:vec4f)->@location(0) vec4f {
     return vec4f(hb_gpu_draw(p.xy,0u));
   }`,
  ].join("\n"),
});
const info = await shader.getCompilationInfo();
for (const message of info.messages) {
  if (message.type === "error") {
    throw new Error(`${message.lineNum}: ${message.message}`);
  }
}
await device.createRenderPipelineAsync({
  layout: "auto",
  vertex: { module: shader, entryPoint: "vertex_main" },
  fragment: {
    module: shader,
    entryPoint: "fragment_main",
    targets: [{ format: "rgba8unorm" }],
  },
});
console.log("WGSL vertex and fragment pipeline compiled");
device.destroy();
