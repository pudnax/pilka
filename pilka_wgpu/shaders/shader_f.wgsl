struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
};

[[stage(fragment)]]
fn main(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    return vec4<f32>(0.2, 0.0, 1.0, 1.0);
}
