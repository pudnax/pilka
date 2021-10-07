[[group(0), binding(0)]]
var src_texture: texture_2d<f32>;
[[group(1), binding(0)]]
var usampler: sampler;
[[group(2), binding(0)]]
var dst_texture: [[access(write)]] texture_storage_2d<rgba32float>;

[[stage(compute), workgroup_size(16, 16, 0)]]
fn main([[builtin(global_invocation_id)]] global_invocation_id: vec3<i32>) {
	let pix = textureSample(src_texture, usampler, tpos);
	textureStore(dst_texture, pos, pix);
}
