fn linear_to_srgb(vlinear: vec4<f32>) -> vec4<f32> {
    let color_linear = vlinear.rgb;
    let selector = ceil(color_linear - 0.0031308);
    let under = 12.92 * color_linear;
    let over = 1.055 * pow(color_linear, vec3(0.41666)) - 0.055;
    let result = mix(under, over, selector);
    return vec4(result, vlinear.a);
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    let vertex_idx = i32(in_vertex_index);
    var out : VertexOutput;
    out.uv = vec2(f32((vertex_idx << 1u) & 2), f32(vertex_idx & 2));
    out.position =
        vec4(out.uv.x * 2.0 + -1.0, 1.0 - out.uv.y * 2.0, 0.0, 1.0);
    return out;
}

@group(0) @binding(0)
var src_sampler: sampler;
@group(1) @binding(0)
var src_texture: texture_2d<f32>;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let col = textureSample(src_texture, src_sampler, in.uv);
    let col = linear_to_srgb(col);
    return col;
}
