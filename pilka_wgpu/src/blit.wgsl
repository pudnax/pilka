struct VertexOutput {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] uv : vec2<f32>;
};

[[stage(vertex)]]
fn vs_main([[builtin(vertex_index)]] in_vertex_index: u32) -> VertexOutput {
    let vertex_idx = i32(in_vertex_index);
    var out : VertexOutput;
    out.uv = vec2<f32>(f32((vertex_idx << 1u) & 2), f32(vertex_idx & 2));
    out.position =
        vec4<f32>(out.uv.x * 2.0 + -1.0, 1.0 - out.uv.y * 2.0, 0.0, 1.0);
    return out;
}

[[group(0), binding(0)]]
var src_sampler: sampler;
[[group(1), binding(0)]]
var src_texture: texture_2d<f32>;

[[stage(fragment)]]
fn fs_main(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    return textureSample(src_texture, src_sampler, in.uv);
}
