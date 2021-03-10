#version 460

// In the beginning, colours never existed. There's nothing that can be done before you...

#include <prelude.glsl>

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D previous_frame;
layout(set = 0, binding = 1) uniform sampler2D generic_texture;
layout(set = 0, binding = 2) uniform sampler2D dummy_texture;
#define T(t) (texture(t, vec2(uv.x, -uv.y)))
#define T_off(t,off) (texture(t, vec2(uv.x + off.x, -(uv.y + off.y))))

layout(std430, push_constant) uniform PushConstant {
	vec3 pos;
	float time;
	vec2 resolution;
	vec2 mouse;
	float spectrum;
} pc;

float worldSDF(in vec3 pos) {
	float res = -1.0;
	res = sphereSDF(pos);

	return res;
}

void main() {
    vec2 uu = (uv + -0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);

    vec3 O = vec3(0.0, 0.0, 3.0) + pc.pos;
    vec3 D = normalize(vec3(uu, -2.));

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

    out_color = vec4(pow(col + T(previous_frame).rgb * 0.1, vec3(1 / 1.12)), 1.0);
}
