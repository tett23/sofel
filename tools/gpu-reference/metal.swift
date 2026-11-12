// Run from the repository root on a Mac with a Metal device.
import Foundation
import Metal

enum CheckError: Error { case unavailable, render, pixel(Int, Int, [UInt8], [UInt8]) }

func source(_ name: String) throws -> String {
    try String(contentsOfFile: "crates/sofel-gpu/shaders/" + name, encoding: .utf8)
}

guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else {
    throw CheckError.unavailable
}
let shader = try "#include <metal_stdlib>\nusing namespace metal;\n" + source("vertex.msl") + source("outline.msl") + """
vertex float4 triangle(uint i [[vertex_id]]) {
    const float2 points[] = {float2(-1,-1),float2(3,-1),float2(-1,3)};
    return float4(points[i],0,1);
}
fragment float4 pixels(float4 p [[position]], device const short4* atlas [[buffer(0)]]) {
    float red = hb_gpu_draw(p.xy / 16.0, 0, atlas);
    float blue = 0.5 * hb_gpu_draw((p.xy - float2(8, 0)) / 16.0, 0, atlas);
    return float4(red * (1-blue), 0, blue, blue + red * (1-blue));
}
"""
let library = try device.makeLibrary(source: shader, options: nil)
let pipeline = MTLRenderPipelineDescriptor()
pipeline.vertexFunction = library.makeFunction(name: "triangle")
pipeline.fragmentFunction = library.makeFunction(name: "pixels")
pipeline.colorAttachments[0].pixelFormat = .rgba8Unorm
let state = try device.makeRenderPipelineState(descriptor: pipeline)
let fixture = try Data(contentsOf: URL(fileURLWithPath: "crates/sofel-gpu/tests/data/outline-records.bin"))
// Skip the empty case and the triangle's count, then read its 36 RGBA16I records.
let triangle = fixture.subdata(in: 8..<(8 + 36 * 8))
let atlas = triangle.withUnsafeBytes { device.makeBuffer(bytes: $0.baseAddress!, length: $0.count) }
let desc = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: 32, height: 16, mipmapped: false)
desc.usage = .renderTarget
desc.storageMode = .shared
guard let texture = device.makeTexture(descriptor: desc), let command = queue.makeCommandBuffer() else {
    throw CheckError.unavailable
}
let pass = MTLRenderPassDescriptor()
pass.colorAttachments[0].texture = texture
pass.colorAttachments[0].loadAction = .clear
pass.colorAttachments[0].storeAction = .store
guard let encoder = command.makeRenderCommandEncoder(descriptor: pass) else { throw CheckError.unavailable }
encoder.setRenderPipelineState(state)
encoder.setFragmentBuffer(atlas, offset: 0, index: 0)
encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
encoder.endEncoding()
command.commit()
command.waitUntilCompleted()
guard command.status == .completed else { throw command.error ?? CheckError.render }
var image = [UInt8](repeating: 0, count: 32 * 16 * 4)
texture.getBytes(&image, bytesPerRow: 32 * 4, from: MTLRegionMake2D(0,0,32,16), mipmapLevel: 0)
// At these pixel centers each triangle covers either the entire pixel or none.
for (x,y) in [(3,3), (9,2), (19,1), (28,4)] {
    let red = x + y + 1 < 16
    let blue = x >= 8 && x - 8 + y + 1 < 16
    let expected: [UInt8] = [red ? (blue ? 128 : 255) : 0, 0, blue ? 128 : 0, red ? 255 : (blue ? 128 : 0)]
    let start = (y * 32 + x) * 4
    let actual = Array(image[start..<(start + 4)])
    guard zip(actual, expected).allSatisfy({ abs(Int($0) - Int($1)) <= 1 }) else {
        throw CheckError.pixel(x,y,actual,expected)
    }
}
print("Metal outline shader: four CPU reference pixels agree on", device.name)
