struct VertexOutput {
    [[builtin(position)]] position : vec4<f32>;
    [[location(0)]] uv : vec2<f32>;
};

[[stage(vertex)]]
fn main([[builtin(vertex_index)]] in_vertex_index : u32) -> VertexOutput {
    let vertex_idx = i32(in_vertex_index);
    var out : VertexOutput;
    out.uv = vec2<f32>(f32((vertex_idx << 1u) & 2), f32(vertex_idx & 2));
    out.position = vec4<f32>(out.uv * 2.0 + -1.0, 0.0, 1.0);
    return out;
}
