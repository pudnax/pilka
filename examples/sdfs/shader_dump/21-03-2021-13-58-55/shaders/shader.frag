#version 460

// In the beginning, colours never existed. There's nothing that can be done before you...

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D previous_frame;
layout(set = 0, binding = 1) uniform sampler2D generic_texture;
layout(set = 0, binding = 2) uniform sampler2D dummy_texture;
layout(set = 0, binding = 3) uniform sampler2D float_texture1;
layout(set = 0, binding = 4) uniform sampler2D float_texture2;
#define T(t) (texture(t, vec2(in_uv.x, -in_uv.y)))
#define T_off(t,off) (texture(t, vec2(in_uv.x + off.x, -(in_uv.y + off.y))))

layout(std430, push_constant) uniform PushConstant {
	vec3 pos;
	float time;
	vec2 resolution;
	vec2 mouse;
	float spectrum;
	bool mouse_pressed;
	uint frame;
} pc;

#define rot(a) mat2(cos(a),sin(a),-sin(a),cos(a))

float map(vec3 p) {
	float time = pc.time;
	p /= 1.9;

    p.xy *= rot(time * .5);
    p.xz *= rot(time * .3);
    float h = sin(time) * .5 + .5;
    p.x -= clamp(p.x, -h, h);
    return length(vec2(length(p.xy) - .5, p.z)) - .1;
}

void main() {
    vec2 uv = (in_uv + -0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);
	/* uv = (in_uv - 0.5*pc.resolution) / pc.resolution.y; */

	vec3 O = vec3(0., 0., -5);
	vec3 D = vec3(uv, 1.);

	float d = 0., i = 0;
	for (; i < 100; ++i) {
		float hit_dist = map(O + D * d);
		d += hit_dist;
		if (abs(hit_dist) < 0.001) break;
	}
	vec3 pos = O + D * d;

	vec3 col = vec3(fract(pos + 0.001));
	/* col = vec3(1.0 - i / 100); */

	out_color = vec4(col, 1.0);
}
