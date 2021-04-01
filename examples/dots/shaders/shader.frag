#version 460

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
} pc;

#define rot(a) mat2(cos(a),-sin(a),sin(a),cos(a))

void main() {
    vec2 uv = (in_uv + -0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);
    vec2 p = vec2(0.01, 0.0);

    float pi = 3.141592,
		  phi = pc.time * 0.0001,
		  a = phi * 2 * PI,
		  d = 1e9;

    for (int i = 0; i < 1400; ++i) {
        d = min(d, length(uv - p) - 0.001);
        p *= rot(a);
        p = normalize(p) * (length(p) + 0.0015);
    }

    out_color = vec4(smoothstep(3/pc.resolution.y, 0.0, d - 0.1));
	out_color = vec4(d - 0.01);
}
