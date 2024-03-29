#version 460

// Signed distance and line segment by Inigo Quilez
// Segment:              https://www.shadertoy.com/view/3tdSDj
// and many more here:   http://www.iquilezles.org/www/articles/distfunctions2d/distfunctions2d.htm

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform texture2D prev_frame;
layout(set = 0, binding = 1) uniform texture2D generic_texture;
layout(set = 0, binding = 2) uniform texture2D dummy_texture;
layout(set = 0, binding = 3) uniform texture2D float_texture1;
layout(set = 0, binding = 4) uniform texture2D float_texture2;
layout(set = 1, binding = 0) uniform sampler tex_sampler;
#define T(tex, uv_coord) (texture(sampler2D(tex, tex_sampler), uv_coord))
#define Tuv(tex) (T(tex, vec2(in_uv.x, -in_uv.y)))
#define T_off(tex, off) (T(tex, vec2(in_uv.x + off.x, -(in_uv.y + off.y))))


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

float line_segment(in vec2 p, in vec2 a, in vec2 b) {
    vec2 ba = b - a;
    vec2 pa = p - a;
    float h = clamp(dot(pa, ba) / dot(ba, ba), 0., 1.);
    return length(pa - h * ba);
}

void main() {
    vec2 pos = (uv + -0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);

    vec2 v1 = vec2(-2.0, -1.0);
    vec2 v2 = cos(pc.time + vec2(-8., 3.) + 1.1) - 1.;
    float thickness = .2 * (.5 + .5 * sin(pc.time * 1.));

    float d = line_segment(pos, v1, v2) - thickness;

    vec3 color = vec3(1.) - sign(d) * vec3(0., 0., 0.);
    color *= 1.5 - exp(.5 * abs(d));
    color *= .5 + .3 * cos(120. * d);
    color = mix(color, vec3(1.), 1. - smoothstep(.0, .015, abs(d)));

    out_color = vec4(color, 1.);
}
