[[group(0), binding(0)]]
var src_texture: texture_2d<f32>;
[[group(1), binding(0)]]
var src_sampler: sampler;
[[group(2), binding(0)]]
var dst_texture: texture_storage_2d<rgba32float, write>;

[[block]]
struct Uniforms {
    resolution : vec2<f32>;
    samples : u32;
};

[[group(3), binding(0)]]
var<uniform> params: Uniforms;

[[stage(compute), workgroup_size(16, 16, 1)]]
fn main([[builtin(global_invocation_id)]] global_id: vec3<u32>) {
    // let tpos = vec2<f32>(global_id.xy) / params.resolution;
    // let pix = textureSample(src_texture, src_sampler, tpos);
    // textureStore(dst_texture, global_id.xy, pix);

    // let dims = vec2<f32>(textureDimensions(src_texture, 0));
    // let pix : vec4<f32> = textureSample(src_texture, src_sampler, vec2<f32>(global_id.xy) / dims);
    // textureStore(dst_texture, vec2<i32>(global_id.xy), pix);
}
