struct VertexOutput {
    [[builtin(position)]] position : vec4<f32>;
    [[location(0)]] uv : vec2<f32>;
};

[[block]]
struct Uniforms {
	samples: u32;
};

[[group(0), binding(0)]]
var src_texture: texture_2d<f32>;
[[group(1), binding(0)]]
var r_sampler : sampler;
[[group(2), binding(0)]]
var<uniform> params : Uniforms;

[[stage(fragment)]]
fn main(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    return textureSample(src_texture, r_sampler, in.uv);
}
