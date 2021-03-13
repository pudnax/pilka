#version 460

// In the beginning, colours never existed. There's nothing that can be done before you...

#include <prelude.glsl>

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D previous_frame;
layout(set = 0, binding = 1) uniform sampler2D generic_texture;
layout(set = 0, binding = 2) uniform sampler2D dummy_texture;
layout(set = 0, binding = 3) uniform sampler2D float_texture1;
layout(set = 0, binding = 4) uniform sampler2D float_texture2;
#define T(t) (texture(t, vec2(uv.x, -uv.y)))
#define T_off(t,off) (texture(t, vec2(uv.x + off.x, -(uv.y + off.y))))

layout(std430, push_constant) uniform PushConstant {
	vec3 pos;
	float time;
	vec2 resolution;
	vec2 mouse;
	float spectrum;
	bool mouse_pressed;
} pc;

float worldSDF(in vec3 pos) {
    float res = -1.0;
    res = sphereSDF(pos);

    return res;
}

void main() {
    vec2 uu = (uv + -0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);

	float tex = T(float_texture1).r;

	float circ = distance(uv, vec2(0.5));
	circ = step(circ, 0.4);

	float col = min(tex, circ);

    out_color = vec4(vec3(col), 1.0);
}
