use std::fs::File;
use std::io::Write;
use std::path::Path;

use super::utils::create_folder;

pub fn create_default_shaders<P: AsRef<Path>>(name: P) -> std::io::Result<()> {
    create_folder(&name)?;

    let create_file = |filename: &str, content: &str| -> std::io::Result<()> {
        let path = name.as_ref().join(filename);
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())
    };

    create_file("prelude.glsl", PRELUDE)?;
    create_file("shader.frag", FRAG_SHADER)?;
    create_file("shader.vert", VERT_SHADER)?;
    create_file("shader.comp", COMP_SHADER)?;

    Ok(())
}

const FRAG_SHADER: &str = "#version 460

// In the beginning, colours never existed. There's nothing that can be done before you...

#include <prelude.glsl>

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D previous_frame;
layout(set = 0, binding = 1) uniform sampler2D generic_texture;
layout(set = 0, binding = 2) uniform sampler2D dummy_texture;
#define T(t) (texture(t, vec2(in_uv.x, -in_uv.y)))
#define T_off(t,off) (texture(t, vec2(in_uv.x + off.x, -(in_uv.y + off.y))))

layout(set = 0, binding = 3) uniform sampler2D float_texture1;
layout(set = 0, binding = 4) uniform sampler2D float_texture2;

layout(set = 1, binding = 0) uniform sampler1D fft_texture;

layout(std430, push_constant) uniform PushConstant {
    vec3 pos;
    float time;
    vec2 resolution;
    vec2 mouse;
    bool mouse_pressed;
    uint frame;
    float time_delta;
} pc;

float worldSDF(in vec3 pos) {
	float res = -1.0;
	res = sphereSDF(pos);

	return res;
}

void main() {
    vec2 uv = (in_uv + -0.5) * 2.0 * vec2(pc.resolution.x / pc.resolution.y, 1);

	vec3 O = vec3(0.0, 0.0, 3.0);
	vec3 D = normalize(vec3(uv, -2.));

	vec2 path = ray_march(O, D);
	vec3 normal = wnormal(O);
	vec3 at = O + path.x * D;

	float r = 2.0;
	vec2 l = r * vec2(cos(pc.time), sin(pc.time));
	vec3 l_pos = vec3(l.x, 3.0, l.y + 2.0);

	vec3 l_col = vec3(1.0, 1.0, 0.7);
    vec3 diffuse = vec3(0.5, 0.5, 0.5);
	vec3 dlight = enlight(at, wnormal(at), diffuse, l_col, l_pos);

    vec3 col = dlight * 10.;
    out_color = vec4(col, 1.0);
}";

const VERT_SHADER: &str = "#version 460

layout(location = 0) out vec2 out_uv;

layout(std430, push_constant) uniform PushConstant {
    vec3 pos;
    float time;
    vec2 resolution;
    vec2 mouse;
    bool mouse_pressed;
    uint frame;
    float time_delta;
} pc;

void main() {
    out_uv = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
    gl_Position = vec4(out_uv * 2.0f + -1.0f, 0.0, 1.0);
}";

const COMP_SHADER: &str = "#version 460

layout(std430, push_constant) uniform PushConstant {
    vec3 pos;
    float time;
    vec2 resolution;
    vec2 mouse;
    bool mouse_pressed;
    uint frame;
    float time_delta;
} pc;

layout (local_size_x = 16, local_size_y = 16, local_size_z = 1) in;

layout (binding = 0, rgba8) uniform image2D previous_frame;
layout (binding = 1, rgba8) uniform image2D generic_texture;
layout (binding = 2, rgba8) uniform image2D dummy_texture;

layout (binding = 3, rgba32f) uniform image2D float_texture1;
layout (binding = 4, rgba32f) uniform image2D float_texture2;

layout(set = 1, binding = 0) uniform sampler1D fft_texture;

void main() {
    if (gl_GlobalInvocationID.x >= pc.resolution.x ||
        gl_GlobalInvocationID.y >= pc.resolution.y) {
        return;
    }
}";

const PRELUDE: &str = "#define PI 3.14159265359
#define TWOPI 6.28318530718

const vec3 EPS = vec3(0., 0.01, 0.001);
const float HIT_DIST = EPS.y;
const int MAX_STEPS = 100;
const float MISS_DIST = 10.0;

const float WIDTH = 2.0;
const float HALF_WIDTH = 1.0;

float worldSDF(vec3 rayPos);

vec2 ray_march(vec3 rayPos, vec3 rayDir) {
    float dist = 0.0;

    for(int i = 0; i < MAX_STEPS; i++) {
        vec3 pos = rayPos + (dist * rayDir);
        float posToScene = worldSDF(pos);
        dist += posToScene;
        if(abs(posToScene) < HIT_DIST) return vec2(dist, i);
        if(posToScene > MISS_DIST) break;
    }

    return vec2(-dist, MAX_STEPS);
}

float crossSDF(vec3 rayPos) {
    const vec3 corner = vec3(HALF_WIDTH);
    vec3 ray = abs(rayPos);
    vec3 cornerToRay = ray - corner;
    float minComp = min(min(cornerToRay.x, cornerToRay.y), cornerToRay.z);
    float maxComp = max(max(cornerToRay.x, cornerToRay.y), cornerToRay.z);
    float midComp = cornerToRay.x + cornerToRay.y + cornerToRay.z
                                             - minComp - maxComp;
    vec2 closestOutsidePoint = max(vec2(minComp, midComp), 0.0);
    vec2 closestInsidePoint = min(vec2(midComp, maxComp), 0.0);
    return (midComp > 0.0) ? length(closestOutsidePoint) : -length(closestInsidePoint);
}

float cubeSDF(vec3 rayPos) {
    const vec3 corner = vec3(HALF_WIDTH);
    vec3 ray = abs(rayPos);
    vec3 cornerToRay = ray - corner;
    float cornerToRayMaxComponent = max(max(cornerToRay.x, cornerToRay.y), cornerToRay.z);
    float distToInsideRay = min(cornerToRayMaxComponent, 0.0);
    vec3 closestToOusideRay = max(cornerToRay, 0.0);
    return length(closestToOusideRay) + distToInsideRay;
}

float squareSDF(vec2 rayPos) {
    const vec2 corner = vec2(HALF_WIDTH);
    vec2 ray = abs(rayPos.xy);
    vec2 cornerToRay = ray - corner;
    float cornerToRayMaxComponent = max(cornerToRay.x, cornerToRay.y);
    float distToInsideRay = min(cornerToRayMaxComponent, 0.0);
    vec2 closestToOusideRay = max(cornerToRay, 0.0);
    return length(closestToOusideRay) + distToInsideRay;
}

float sphereSDF(vec3 rayPosition, vec3 sphereCenterPosition, float radius) {
    vec3 centerToRay = rayPosition - sphereCenterPosition;
    float distToCenter = length(centerToRay);
    return distToCenter - radius;
}

float sphereSDF(vec3 rayPos, float radius) {
    return length(rayPos) - radius;
}

float sphereSDF(vec3 rayPos) {
    return length(rayPos) - HALF_WIDTH;
}

float yplaneSDF(vec3 rayPos) {
    return abs(rayPos.y);
}

mat2 rotate(float angle) {
    float sine = sin(angle);
    float cosine = cos(angle);
    return mat2(cosine, -sine, sine, cosine);
}

vec3 enlight(in vec3 at, vec3 normal, vec3 diffuse, vec3 l_color, vec3 l_pos) {
  vec3 l_dir = l_pos - at;
  return diffuse * l_color * max(0., dot(normal, normalize(l_dir))) /
         dot(l_dir, l_dir);
}

vec3 wnormal(in vec3 p) {
  return normalize(vec3(worldSDF(p + EPS.yxx) - worldSDF(p - EPS.yxx),
                        worldSDF(p + EPS.xyx) - worldSDF(p - EPS.xyx),
                        worldSDF(p + EPS.xxy) - worldSDF(p - EPS.xxy)));
}";
