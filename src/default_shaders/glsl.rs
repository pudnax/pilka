pub const FRAG_SHADER: &str = "#version 460
#extension GL_EXT_buffer_reference : require

// In the beginning, colours never existed. There's nothing that was done before you...

#include <prelude.glsl>

layout(set = 0, binding = 0) uniform sampler gsamplers[];
layout(set = 0, binding = 1) uniform texture2D gtextures[];

layout(std430, push_constant) uniform PushConstant {
    vec3 pos;
    float time;
    vec2 resolution;
    vec2 mouse;
    bool mouse_pressed;
    uint frame;
    float time_delta;
    float record_time;
}
pc;

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_color;

void main() {
    vec2 uv = (in_uv + -0.5) * vec2(pc.resolution.x / pc.resolution.y, 1);

    vec3 col = vec3(uv, 1.);
    out_color = vec4(col, 1.0);
}";

pub const VERT_SHADER: &str = "#version 460
#extension GL_EXT_buffer_reference : require

layout(set = 0, binding = 0) uniform sampler gsamplers[];
layout(set = 0, binding = 1) uniform texture2D gtextures[];

layout(std430, push_constant) uniform PushConstant {
    vec3 pos;
    float time;
    vec2 resolution;
    vec2 mouse;
    bool mouse_pressed;
    uint frame;
    float time_delta;
    float record_time;
}
pc;

layout(location = 0) out vec2 out_uv;

void main() {
    out_uv = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
    gl_Position = vec4(out_uv * 2.0f + -1.0f, 0.0, 1.0);
}";

pub const COMP_SHADER: &str = "#version 460
#extension GL_EXT_buffer_reference : require

layout(set = 0, binding = 0) uniform sampler gsamplers[];
layout(set = 0, binding = 1) uniform texture2D gtextures[];

layout(std430, push_constant) uniform PushConstant {
    vec3 pos;
    float time;
    vec2 resolution;
    vec2 mouse;
    bool mouse_pressed;
    uint frame;
    float time_delta;
    float record_time;
}
pc;

layout (local_size_x = 16, local_size_y = 16, local_size_z = 1) in;

void main() {
    if (gl_GlobalInvocationID.x >= pc.resolution.x ||
        gl_GlobalInvocationID.y >= pc.resolution.y) {
        return;
    }
}";

pub const PRELUDE: &str = "const float PI = acos(-1.);
const float TAU = 2. * PI;

const uint PREV_TEX = 0;
const uint GENERIC_TEX1 = 1;
const uint GENERIC_TEX2 = 2;
const uint DITHER_TEX = 3;
const uint NOISE_TEX = 4;
const uint BLUE_TEX = 5;

const uint LINER_SAMPL = 0;
const uint NEAREST_SAMPL = 1;

vec4 ASSERT_COL = vec4(0.);
void assert(bool cond, int v) {
    if (!(cond)) {
        if      (v == 0) ASSERT_COL.x = -1.0;
        else if (v == 1) ASSERT_COL.y = -1.0;
        else if (v == 2) ASSERT_COL.z = -1.0;
        else             ASSERT_COL.w = -1.0;
    }
}
void assert(bool cond) { assert(cond, 0); }
#define catch_assert(out)                                   \
    if (ASSERT_COL.x < 0.0) out = vec4(1.0, 0.0, 0.0, 1.0); \
    if (ASSERT_COL.y < 0.0) out = vec4(0.0, 1.0, 0.0, 1.0); \
    if (ASSERT_COL.z < 0.0) out = vec4(0.0, 0.0, 1.0, 1.0); \
    if (ASSERT_COL.w < 0.0) out = vec4(1.0, 1.0, 0.0, 1.0);

float AAstep(float threshold, float val) {
    return smoothstep(-.5, .5, (val - threshold) / min(0.005, fwidth(val - threshold)));
}
float AAstep(float val) {
    return AAstep(val, 0.);
}

float worldsdf(vec3 rayPos);

vec2 ray_march(vec3 rayPos, vec3 rayDir) {
    const vec3 EPS = vec3(0., 0.001, 0.0001);
    const float HIT_DIST = EPS.y;
    const int MAX_STEPS = 100;
    const float MISS_DIST = 10.0;
    float dist = 0.0;

    for(int i = 0; i < MAX_STEPS; i++) {
        vec3 pos = rayPos + (dist * rayDir);
        float posToScene = worldsdf(pos);
        dist += posToScene;
        if(abs(posToScene) < HIT_DIST) return vec2(dist, i);
        if(posToScene > MISS_DIST) break;
    }

    return vec2(-dist, MAX_STEPS);
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
    const vec3 EPS = vec3(0., 0.01, 0.0001);
    return normalize(vec3(worldsdf(p + EPS.yxx) - worldsdf(p - EPS.yxx),
                        worldsdf(p + EPS.xyx) - worldsdf(p - EPS.xyx),
                        worldsdf(p + EPS.xxy) - worldsdf(p - EPS.xxy)));
}";
