#version 460

// In the beginning, colours never existed. There's nothing that was done before you...

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

layout(std430, push_constant) uniform PushConstant {
    vec3 pos;
    float time;
    vec2 resolution;
    vec2 mouse;
    bool mouse_pressed;
    uint frame;
    float time_delta;
    float record_period;
} pc;

vec3 vignette(vec3 color, vec2 q, float v) {
    color *= 0.3 + 0.8 * pow(16.0 * q.x * q.y * (1.0 - q.x) * (1.0 - q.y), v);
    return color;
}

vec3 desaturate(in vec3 c, in float a) {
    float l = dot(c, vec3(1. / 3.));
    return mix(c, vec3(l), a);
}

void main() {
    time = pc.time;

    vec3 color = vec3(0.);

#ifdef COMPUTE_ROUTINE
    {
        color = texture(float_texture1, in_uv).rgb;
    }
#else
    color = render(in_uv * pc.resolution,  pc.resolution, pc.frame);
#endif

    color = desaturate(color, -0.8);
    color = vignette(color, in_uv, 1.2);
    out_color = vec4(color, 1.0);
}
