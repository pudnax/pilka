pub const FRAG_SHADER: &str = "
struct Uniform {
  pos: vec3<f32>,
  resolution: vec2<f32>,
  mouse: vec2<f32>,
  mouse_pressed: u32,
  time: f32,
  time_delta: f32,
  frame: u32,
  record_period: f32,
};

@group(0) @binding(0)
var prev_frame: texture_2d<f32>;
@group(0) @binding(1) var generic_texture: texture_2d<f32>;
@group(0) @binding(2) var dummy_texture: texture_2d<f32>;
@group(0) @binding(3) var float_texture1: texture_2d<f32>;
@group(0) @binding(4) var float_texture2: texture_2d<f32>;
@group(1) @binding(4) var tex_sampler: sampler;
@group(2) @binding(0) var<uniform> un: Uniform;

struct VertexOutput {
  @builtin(position) pos: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@fragment
fn main(in: VertexOutput) -> @location(0) vec4<f32> {
  let uv =
      (in.uv * 2.0 - 1.0) * vec2<f32>(un.resolution.x / un.resolution.y, 1.);

  var col = vec3<f32>(uv, 1.);
  col.x = col.x + un.mouse.x;
  col.y = col.y + un.mouse.y;

  return vec4<f32>(col, 1.0);
}
";

pub const VERT_SHADER: &str = "
struct VertexOutput {
  @builtin(position)
  pos: vec4<f32>,
  @location(0)
  uv: vec2<f32>,
};

@vertex
fn main(@builtin(vertex_index) vertex_idx: u32) -> VertexOutput {
  let uv = vec2<u32>((vertex_idx << 1u) & 2u, vertex_idx & 2u);
  let out = VertexOutput(vec4<f32>(2.0 * vec2<f32>(uv) - 1.0, 0.0, 1.0), vec2<f32>(uv));
  return out;
}
";

pub const COMP_SHADER: &str = "
struct Uniform {
  pos: vec3<f32>,
  resolution: vec2<f32>,
  mouse: vec2<f32>,
  mouse_pressed: u32,
  time: f32,
  time_delta: f32,
  frame: u32,
  record_period: f32,
};

@group(0) @binding(0) var prev_frame: texture_storage_2d<rgba8unorm, read_write>;
@group(0) @binding(1) var generic_texture: texture_storage_2d<rgba8unorm, read_write>;
@group(0) @binding(2) var dummy_texture: texture_storage_2d<rgba8unorm, read_write>;
@group(0) @binding(3) var float_texture1: texture_storage_2d<rgba32float, read_write>;
@group(0) @binding(4) var float_texture2: texture_storage_2d<rgba32float, read_write>;
@group(1) @binding(0) var<uniform> un: Uniform;

@compute
@workgroup_size(256, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
 if (f32(global_id.x) >= un.resolution.x ||
     f32(global_id.y) >= un.resolution.y) {
   return;
 }
}
";
